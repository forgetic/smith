//! Offline manifest checking; no attempt starts from this command yet.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [command] if command == "check" => check_all(),
        [command, directory] if command == "check" => check(Path::new(directory)),
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
                "usage: smith-bench check [TASKS_DIRECTORY]\n       smith-bench guards --suite SUITE [--tasks TASKS_DIRECTORY] SECTION..."
            );
            ExitCode::from(2)
        }
    }
}

fn check_all() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    match smith_bench::check_benchmark_tree(&root, &root.join("../docs/design")) {
        Ok(counts) => {
            println!(
                "checked {} tasks, {} suites, {} configuration pins",
                counts.tasks, counts.suites, counts.configurations
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
    Ok(smith_bench::choose_guards(&suite, &tasks, sections))
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
