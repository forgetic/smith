//! Offline checks keep no process state and never execute setup or graders.
//! `check_tree` finds task manifests; `check_task` verifies prompts, frozen
//! seeds and design guards, following benchmarks.md, sections 5.2 and 14.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::task::{Kind, OutcomeCheck, Refusal, Task, read_task};

/// Verify all task documents below a directory without running an agent.
pub fn check_tree(directory: &Path, design: &Path) -> Result<usize, Refusal> {
    let mut manifests = Vec::new();
    find_manifests(directory, &mut manifests)?;
    manifests.sort();
    for manifest in &manifests {
        check_task(manifest, design)?;
    }
    Ok(manifests.len())
}

/// Counts returned by the offline checker to its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManifestCounts {
    pub tasks: usize,
    pub suites: usize,
    pub configurations: usize,
    pub summaries: usize,
    pub baselines: usize,
}

/// Validate a benchmark tree's tasks, suites, model tiers and configuration pins.
pub fn check_benchmark_tree(root: &Path, design: &Path) -> Result<ManifestCounts, Refusal> {
    let tasks = crate::catalogue(&root.join("tasks"), design)?;
    let agents = root.join("agents");
    let models = crate::read_model_tiers(&agents.join("models.toml"))?;
    for provider in [crate::Provider::Codex, crate::Provider::Anthropic] {
        models.lookup(&agents.join("models.toml"), "small", provider)?;
    }
    let mut configurations = 0;
    for agent in fs::read_dir(&agents).map_err(|error| Refusal::new(&agents, "agents", error.to_string()))? {
        let agent = agent.map_err(|error| Refusal::new(&agents, "agents", error.to_string()))?;
        if !agent.file_type().map_err(|error| Refusal::new(&agent.path(), "agents", error.to_string()))?.is_dir() {
            continue;
        }
        for file in
            fs::read_dir(agent.path()).map_err(|error| Refusal::new(&agent.path(), "agents", error.to_string()))?
        {
            let file = file.map_err(|error| Refusal::new(&agent.path(), "agents", error.to_string()))?;
            if file.file_name().to_str().is_some_and(|name| name.ends_with(".pin.toml")) {
                let pin = crate::read_configuration(&file.path())?;
                let expected = agents.join(pin.pin.agent.directory()).join(format!("{}.pin.toml", pin.pin.name));
                if expected != file.path() {
                    return Err(Refusal::new(&file.path(), "name/agent", "pin path disagrees with its name or agent"));
                }
                configurations += 1;
            }
        }
    }
    let mut suites = 0;
    let directory = root.join("suites");
    for file in fs::read_dir(&directory).map_err(|error| Refusal::new(&directory, "suites", error.to_string()))? {
        let file = file.map_err(|error| Refusal::new(&directory, "suites", error.to_string()))?;
        if file.path().extension().is_some_and(|extension| extension == "toml") {
            let suite = crate::read_suite(&file.path())?;
            crate::validate_suite(&file.path(), &suite, &tasks, &agents, &models)?;
            suites += 1;
        }
    }
    let mut summaries = 0;
    let mut baselines = 0;
    for (name, count) in [("summaries", &mut summaries), ("baselines", &mut baselines)] {
        let directory = root.join(name);
        for file in fs::read_dir(&directory).map_err(|error| Refusal::new(&directory, name, error.to_string()))? {
            let file = file.map_err(|error| Refusal::new(&directory, name, error.to_string()))?;
            if file.path().extension().is_some_and(|extension| extension == "json") {
                if name == "summaries" {
                    crate::read_summary(&file.path())?;
                } else {
                    crate::read_baseline(&file.path())?;
                }
                *count += 1;
            }
        }
    }
    Ok(ManifestCounts { tasks: tasks.len(), suites, configurations, summaries, baselines })
}

pub(crate) fn find_manifests(directory: &Path, manifests: &mut Vec<PathBuf>) -> Result<(), Refusal> {
    let entries = fs::read_dir(directory).map_err(|error| Refusal::new(directory, "tasks", error.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|error| Refusal::new(directory, "tasks", error.to_string()))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| Refusal::new(&path, "tasks", error.to_string()))?;
        if kind.is_symlink() {
            return Err(Refusal::new(&path, "tasks", "symlinks are not frozen task inputs"));
        }
        if kind.is_dir() {
            // Hidden graders and solutions are never task catalogues.
            if !matches!(entry.file_name().to_str(), Some("seed" | "grader" | "reference")) {
                find_manifests(&path, manifests)?;
            }
        } else if entry.file_name() == "task.toml" {
            manifests.push(path);
        }
    }
    Ok(())
}

/// Verify one task's frozen inputs and return its parsed specification.
pub fn check_task(file: &Path, design: &Path) -> Result<Task, Refusal> {
    let task = read_task(file)?;
    if !valid_name(&task.id) {
        return Err(Refusal::new(file, "id", "expected a nonempty lowercase task name"));
    }
    if task.version == 0 || task.deadline_seconds == 0 || task.repetitions == Some(0) {
        return Err(Refusal::new(file, "version/deadline_seconds/repetitions", "counts must be positive"));
    }
    check_prompt(file, "prompt", &task.prompt)?;
    check_budget(file, "budget", &task.budget)?;
    for path in &task.protected {
        check_path(file, "protected", path)?;
    }
    check_outcomes(file, "outcome", &task.outcome)?;
    for (index, grade) in task.grade.iter().enumerate() {
        if grade.command.is_empty() || grade.deadline_seconds == 0 || grade.max_output_bytes == 0 {
            return Err(Refusal::new(
                file,
                &format!("grade[{index}]"),
                "command, deadline and log bound must be nonempty",
            ));
        }
    }
    for (index, command) in task.setup.iter().enumerate() {
        if command.is_empty() {
            return Err(Refusal::new(file, &format!("setup[{index}]"), "command must be nonempty"));
        }
    }
    let mut variants = BTreeSet::new();
    for (index, variant) in task.variant.iter().enumerate() {
        let key = format!("variant[{index}]");
        if !valid_name(&variant.name) || !variants.insert(&variant.name) {
            return Err(Refusal::new(file, &format!("{key}.name"), "expected a unique lowercase variant name"));
        }
        if let Some(prompt) = &variant.prompt {
            check_prompt(file, &format!("{key}.prompt"), prompt)?;
        }
        if let Some(budget) = &variant.budget {
            check_budget(file, &format!("{key}.budget"), budget)?;
        }
        check_outcomes(file, &format!("{key}.outcome"), &variant.outcome)?;
    }
    match task.kind {
        Kind::Probe => {}
        Kind::Fixture | Kind::Repository => {
            if task.grade.is_empty() {
                return Err(Refusal::new(file, "grade", "graded task requires a grader"));
            }
        }
    }
    if let Some(calibration) = &task.calibration {
        let unique: BTreeSet<&String> = calibration.passes.iter().collect();
        if calibration.passes.len() != 10
            || unique.len() != 10
            || calibration.binary.is_empty()
            || calibration.failed_before.is_empty()
            || calibration.passes.iter().any(String::is_empty)
        {
            return Err(Refusal::new(
                file,
                "calibration",
                "requires a binary, ten distinct passing run IDs and a failing run ID",
            ));
        }
    }
    for (index, guard) in task.guards.iter().enumerate() {
        check_guard(file, &format!("guards[{index}]"), guard, design)?;
    }
    if !valid_digest(&task.seed_sha256) {
        return Err(Refusal::new(file, "seed_sha256", "expected 64 lowercase hexadecimal digits"));
    }
    let directory = file.parent().ok_or_else(|| Refusal::new(file, "seed", "task needs a directory"))?;
    let observed = seed_digest(&directory.join("seed"))?;
    if observed != task.seed_sha256 {
        return Err(Refusal::new(
            file,
            "seed_sha256",
            format!("seed digest differs: expected {}, observed {observed}", task.seed_sha256),
        ));
    }
    Ok(task)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn check_prompt(file: &Path, key: &str, prompt: &str) -> Result<(), Refusal> {
    let Some(paragraph) = prompt.strip_suffix('\n') else {
        return Err(Refusal::new(file, key, "whole prompt must end in a newline"));
    };
    if paragraph.trim().is_empty() || paragraph.contains(['\n', '\r', '\0']) || prompt.len() >= 4088 {
        return Err(Refusal::new(file, key, "expected one nonempty paragraph under 4,088 bytes"));
    }
    Ok(())
}

fn check_budget(file: &Path, key: &str, budget: &crate::Budget) -> Result<(), Refusal> {
    if budget.turns == Some(0)
        || budget.seconds == Some(0)
        || budget.spend.is_some_and(|spend| !spend.is_finite() || spend <= 0.0)
    {
        return Err(Refusal::new(file, key, "budget quantities must be finite and positive"));
    }
    Ok(())
}

pub(crate) fn check_path(file: &Path, key: &str, path: &Path) -> Result<(), Refusal> {
    if path.as_os_str().is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(Refusal::new(file, key, "expected a relative workspace path without traversal"));
    }
    Ok(())
}

fn check_outcomes(file: &Path, key: &str, outcomes: &[OutcomeCheck]) -> Result<(), Refusal> {
    for (index, outcome) in outcomes.iter().enumerate() {
        let key = format!("{key}[{index}]");
        let path = match outcome {
            OutcomeCheck::FileContent { path, content: _ }
            | OutcomeCheck::Absent { path }
            | OutcomeCheck::Pattern { path, pattern: _ }
            | OutcomeCheck::Protected { path } => path,
            OutcomeCheck::FileDigest { path, sha256 } => {
                if !valid_digest(sha256) {
                    return Err(Refusal::new(
                        file,
                        &format!("{key}.sha256"),
                        "expected 64 lowercase hexadecimal digits",
                    ));
                }
                path
            }
        };
        check_path(file, &format!("{key}.path"), path)?;
    }
    Ok(())
}

fn check_guard(file: &Path, key: &str, guard: &str, design: &Path) -> Result<(), Refusal> {
    let Some((name, section)) = guard.split_once(", section ") else {
        return Err(Refusal::new(file, key, "expected '<design file>, section <number>'"));
    };
    check_path(file, key, Path::new(name))?;
    let text =
        fs::read_to_string(design.join(name)).map_err(|error| Refusal::new(file, key, format!("{guard}: {error}")))?;
    let found = text.lines().any(|line| {
        let Some(heading) = line.strip_prefix('#') else {
            return false;
        };
        let heading = heading.trim_start_matches('#').trim_start();
        let Some(rest) = heading.strip_prefix(section) else {
            return false;
        };
        rest.starts_with(". ") || rest.starts_with(' ')
    });
    if section.is_empty()
        || !section.split('.').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        || !found
    {
        return Err(Refusal::new(file, key, format!("guarded section does not exist: {guard}")));
    }
    Ok(())
}

/// Hash a frozen seed by sorted relative names, executable bits and file bytes.
pub fn seed_digest(directory: &Path) -> Result<String, Refusal> {
    let mut files = Vec::new();
    seed_files(directory, directory, &mut files)?;
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(directory).expect("walked files remain under seed");
        let name = relative.to_str().ok_or_else(|| Refusal::new(&path, "seed", "file name must be UTF-8"))?;
        let bytes = fs::read(&path).map_err(|error| Refusal::new(&path, "seed", error.to_string()))?;
        let metadata = fs::metadata(&path).map_err(|error| Refusal::new(&path, "seed", error.to_string()))?;
        digest.update(u64::try_from(name.len()).expect("file name fits u64").to_be_bytes());
        digest.update(name.as_bytes());
        digest.update([u8::from(metadata.permissions().mode() & 0o111 != 0)]);
        digest.update(u64::try_from(bytes.len()).expect("file size fits u64").to_be_bytes());
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn seed_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), Refusal> {
    let metadata =
        fs::symlink_metadata(directory).map_err(|error| Refusal::new(directory, "seed", error.to_string()))?;
    if !metadata.is_dir() || metadata.is_symlink() {
        return Err(Refusal::new(directory, "seed", "expected an ordinary seed directory"));
    }
    for entry in fs::read_dir(directory).map_err(|error| Refusal::new(directory, "seed", error.to_string()))? {
        let entry = entry.map_err(|error| Refusal::new(directory, "seed", error.to_string()))?;
        let path = entry.path();
        let relative = path.strip_prefix(root).expect("walk remains under seed");
        if relative.components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some("benchmarks" | "grader" | "grading" | "reference" | "reference-solutions" | ".git")
            )
        }) {
            return Err(Refusal::new(
                &path,
                "seed",
                "benchmark, grader, reference and git files stay outside the seed",
            ));
        }
        let kind = entry.file_type().map_err(|error| Refusal::new(&path, "seed", error.to_string()))?;
        if kind.is_dir() {
            seed_files(root, &path, files)?;
        } else if kind.is_file() {
            files.push(path);
        } else {
            return Err(Refusal::new(&path, "seed", "symlinks and special files are not frozen inputs"));
        }
    }
    Ok(())
}
