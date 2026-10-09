//! Offline checks, summaries and frozen arm builds; no agent starts yet.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [command, commit] if command == "build" => build_command(commit, None),
        [command, commit, flag, repository] if command == "build" && flag == "--repository" => {
            build_command(commit, Some(Path::new(repository)))
        }
        [command] if command == "check" => check_all(),
        [command, directory] if command == "check" => check(Path::new(directory)),
        [command, results, suite, tier, design, seed, reference @ ..]
            if command == "summarise" && reference.len() <= 1 =>
        {
            summarize_command(Path::new(results), suite, tier, design, seed, reference.first().map(Path::new))
        }
        [command, summary, name] if command == "baseline" => baseline_command(Path::new(summary), name),
        [command, flag, suite, tasks_flag, directory, sections @ ..]
            if command == "guards" && flag == "--suite" && tasks_flag == "--tasks" && !sections.is_empty() =>
        {
            guards(Path::new(suite), Some(Path::new(directory)), sections)
        }
        [command, flag, suite, sections @ ..] if command == "guards" && flag == "--suite" && !sections.is_empty() => {
            guards(Path::new(suite), None, sections)
        }
        _ => {
            eprintln!(
                "usage: smith-bench check [TASKS_DIRECTORY]\n       smith-bench guards --suite SUITE [--tasks TASKS_DIRECTORY] SECTION...\n       smith-bench summarise RESULTS SUITE TIER DESIGN SEED [BASELINE]\n       smith-bench baseline SUMMARY NAME\n       smith-bench build COMMIT [--repository REPOSITORY]"
            );
            ExitCode::from(2)
        }
    }
}

fn build_command(commit: &str, repository: Option<&Path>) -> ExitCode {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("benchmark workspace");
    report_output(smith_bench::arm::build(repository.unwrap_or(source), commit).map(|built| {
        println!("built {} sha256 {}", built.arm.commit, built.arm.sha256);
        built.directory
    }))
}

fn committed_output(directory: &str, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
        return Err("committed output name must use letters, digits, '-' or '_'".into());
    }
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(directory);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(format!("{name}.json")))
}

fn report_output(result: Result<PathBuf, String>) -> ExitCode {
    match result {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn summarize_command(
    results: &Path,
    suite: &str,
    tier: &str,
    design: &str,
    seed: &str,
    reference: Option<&Path>,
) -> ExitCode {
    report_output((|| {
        let design = match design {
            "single" => smith_bench::Design::Single,
            "interleaved" => smith_bench::Design::Interleaved,
            _ => return Err("design must be single or interleaved".into()),
        };
        let seed = seed.parse::<u64>().map_err(|error| error.to_string())?;
        let results = smith_bench::read_results(results).map_err(|error| error.to_string())?;
        let reference = reference.map(smith_bench::read_baseline).transpose().map_err(|error| error.to_string())?;
        let summary = smith_bench::summarise(&results, suite, tier, design, seed, reference.as_ref())?;
        let output = committed_output("summaries", &summary.run)?;
        smith_bench::write_summary(&output, &summary)?;
        Ok(output)
    })())
}

fn baseline_command(summary: &Path, name: &str) -> ExitCode {
    report_output((|| {
        let summary = smith_bench::read_summary(summary).map_err(|error| error.to_string())?;
        let reference = smith_bench::baseline(&summary)?;
        let output = committed_output("baselines", name)?;
        smith_bench::write_summary(&output, &reference)?;
        Ok(output)
    })())
}

fn check_all() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match smith_bench::check_benchmark_tree(&root, &root.join("../docs/design")) {
        Ok(counts) => {
            println!(
                "checked {} tasks, {} suites, {} configuration pins, {} summaries, {} baselines",
                counts.tasks, counts.suites, counts.configurations, counts.summaries, counts.baselines
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn guards(file: &Path, tasks: Option<&Path>, sections: &[String]) -> ExitCode {
    match guarded_tasks(file, tasks, sections) {
        Ok(selection) => {
            for name in selection.selected {
                println!("{name}");
            }
            for (name, reason) in selection.omitted {
                println!("omitted {name}: {reason}");
            }
            println!("estimated {} seconds, {} tokens", selection.estimated_seconds, selection.estimated_tokens);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn guarded_tasks(
    file: &Path,
    tasks_directory: Option<&Path>,
    sections: &[String],
) -> Result<smith_bench::GuardSelection, smith_bench::Refusal> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let suite = smith_bench::read_suite(file)?;
    let default_tasks = root.join("tasks");
    let tasks = smith_bench::catalogue(tasks_directory.unwrap_or(&default_tasks), &root.join("../docs/design"))?;
    let agents = root.join("agents");
    let models = smith_bench::read_model_tiers(&agents.join("models.toml"))?;
    smith_bench::validate_suite(file, &suite, &tasks, &agents, &models)?;
    let summaries_directory = root.join("summaries");
    let mut summaries = Vec::new();
    if summaries_directory.exists() {
        for entry in std::fs::read_dir(&summaries_directory)
            .map_err(|error| smith_bench::Refusal::new(&summaries_directory, "summaries", error.to_string()))?
        {
            let entry = entry
                .map_err(|error| smith_bench::Refusal::new(&summaries_directory, "summaries", error.to_string()))?;
            if entry.path().extension().is_some_and(|extension| extension == "json") {
                summaries.push(smith_bench::read_summary(&entry.path())?);
            }
        }
    }
    let costs = smith_bench::committed_costs(&suite, &tasks, &summaries, &agents, &models)
        .map_err(|error| smith_bench::Refusal::new(file, "summaries", error))?;
    Ok(smith_bench::choose_guards_with_costs(&suite, &tasks, sections, &costs))
}

fn check(directory: &Path) -> ExitCode {
    let design = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/design");
    match smith_bench::check_tree(directory, &design) {
        Ok(count) => {
            println!("checked {count} tasks");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
