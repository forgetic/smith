use std::path::{Path, PathBuf};

use crate::task::parse_task;
use crate::{Kind, check_task, seed_digest};

const DOCUMENT: &str = r#"
id = "recorded"
version = 1
kind = "probe"
tier = "small"
deadline_seconds = 10
prompt = "Read the README and finish.\n"
seed_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
"#;

#[test]
fn a_task_retains_all_of_its_typed_parts() {
    let document = format!(
        r#"{DOCUMENT}
title = "Recorded probe"
guards = ["domain/tools.md, section 3"]
behaviours = ["read"]
effort = "low"
setup = [["cargo", "test", "--offline"]]
protected = ["Cargo.toml"]
repetitions = 3
waives = ["limit"]
[[outcome]]
check = "file-content"
path = "answer.txt"
content = "ok\n"
[[outcome]]
check = "file-digest"
path = "README.md"
sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
[[outcome]]
check = "absent"
path = "error.txt"
[[outcome]]
check = "pattern"
path = "answer.txt"
pattern = "ok"
[[outcome]]
check = "protected"
path = "Cargo.toml"
[[grade]]
command = ["cargo", "test", "--offline"]
deadline_seconds = 30
max_output_bytes = 4096
[[variant]]
name = "short"
prompt = "Read one file and finish.\n"
[variant.budget]
turns = 2
[budget]
turns = 3
seconds = 10
spend = 0.02
[estimate]
seconds = 5
tokens = 1000
[smith]
"profile.concurrent_conversations" = 2
[environment]
BENCH_MARKER = "nonce"
[calibration]
binary = "digest"
passes = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
failed_before = "before"
"#
    );
    let task = parse_task(Path::new("task.toml"), &document).expect("complete task parses");
    assert_eq!(task.kind, Kind::Probe);
    assert_eq!(task.outcome.len(), 5);
    assert_eq!(task.variant[0].budget.as_ref().expect("variant budget").turns, Some(2));
    assert_eq!(task.environment["BENCH_MARKER"], "nonce");
}

#[test]
fn unknown_top_level_keys_name_the_document_and_key() {
    let error = parse_task(Path::new("sample/task.toml"), &format!("{DOCUMENT}\nmisspelled = true"))
        .expect_err("unknown key is refused");
    assert!(error.key.contains("misspelled"), "{error}");
    assert_eq!(error.file, Path::new("sample/task.toml"));
}

#[test]
fn unknown_nested_keys_name_their_full_path() {
    for (suffix, path) in [
        ("[budget]\nturnz = 3", "budget.turnz"),
        ("[[variant]]\nname = 'short'\n[variant.budget]\nturnz = 3", "variant[0].budget.turnz"),
        ("[[outcome]]\ncheck = 'absent'\npath = 'x'\nextra = true", "outcome[0].extra"),
    ] {
        let error = parse_task(Path::new("task.toml"), &format!("{DOCUMENT}\n{suffix}"))
            .expect_err("unknown nested key is refused");
        assert_eq!(error.key, path, "{error}");
    }
}

#[test]
fn kinds_and_checks_are_closed() {
    for document in [
        DOCUMENT.replace("kind = \"probe\"", "kind = \"chat\""),
        format!("{DOCUMENT}\n[[outcome]]\ncheck = 'unknown'"),
        format!("{DOCUMENT}\n[[event]]\ncheck = 'unknown'"),
    ] {
        assert!(parse_task(Path::new("task.toml"), &document).is_err());
    }
}

#[test]
fn seed_digest_matches_the_committed_known_bytes() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/tasks/valid/seed");
    assert_eq!(
        seed_digest(&directory).expect("seed hashes"),
        "452c822eb37147a50a5ddf1271245fad2f5884ded6513b8f813e81629627fb85"
    );
}

#[test]
fn a_calibration_requires_ten_distinct_passes() {
    let task_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/tasks/bad-calibration/task.toml");
    let design = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/design");
    let error = check_task(&task_root, &design).expect_err("nine passes do not calibrate");
    assert_eq!(error.key, "calibration");
}

#[test]
fn a_seed_digest_freezes_names_content_and_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("smith-bench-digest-{}", std::process::id()));
    std::fs::create_dir(&root).expect("fresh test directory");
    let original = root.join("a.txt");
    std::fs::write(&original, "one").expect("test seed");
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o600)).expect("nonexecutable seed");
    let baseline = seed_digest(&root).expect("initial digest");
    std::fs::write(&original, "two").expect("changed content");
    assert_ne!(seed_digest(&root).expect("changed digest"), baseline);
    std::fs::write(&original, "one").expect("restored content");
    assert_eq!(seed_digest(&root).expect("restored digest"), baseline);
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700)).expect("executable seed");
    assert_ne!(seed_digest(&root).expect("executable digest"), baseline);
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o600)).expect("restored mode");
    std::fs::rename(&original, root.join("b.txt")).expect("changed name");
    assert_ne!(seed_digest(&root).expect("renamed digest"), baseline);
    std::fs::remove_dir_all(&root).expect("test directory removed");
}

#[test]
fn a_seed_symlink_cannot_reach_hidden_inputs() {
    let root = std::env::temp_dir().join(format!("smith-bench-link-{}", std::process::id()));
    std::fs::create_dir(&root).expect("fresh test directory");
    std::os::unix::fs::symlink("../grader", root.join("hidden")).expect("seed symlink");
    let error = seed_digest(&root).expect_err("seed must not follow symlinks");
    assert!(error.reason.contains("symlinks"), "{error}");
    std::fs::remove_dir_all(&root).expect("test directory removed");
}

fn sample_suite() -> crate::Suite {
    crate::read_suite(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/suites/sample.toml"))
        .expect("recorded suite")
}

fn sample_catalogue() -> Vec<crate::CatalogueTask> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = root.join("tests/recorded/tasks/valid/task.toml");
    let mut task = check_task(&manifest, &root.join("../docs/design")).expect("recorded task");
    task.behaviours.push("read".into());
    task.variant.push(crate::Variant {
        name: "short".into(),
        prompt: None,
        smith: std::collections::BTreeMap::new(),
        environment: std::collections::BTreeMap::new(),
        outcome: Vec::new(),
        event: Vec::new(),
        waives: Vec::new(),
        budget: None,
    });
    vec![crate::CatalogueTask { name: "probes/recorded".into(), manifest, task }]
}

#[test]
fn a_suite_resolves_task_variants_selectors_and_configuration_pins() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let models = crate::read_model_tiers(&root.join("agents/models.toml")).expect("model tiers");
    let mut suite = sample_suite();
    suite.tasks.push("probes/recorded/short".into());
    suite.select.push(crate::Selection { kind: Kind::Probe, behaviours: vec!["read".into()] });
    let selected =
        crate::validate_suite(Path::new("suite.toml"), &suite, &sample_catalogue(), &root.join("agents"), &models)
            .expect("suite references resolve");
    assert_eq!(selected, ["probes/recorded", "probes/recorded/short"]);
}

#[test]
fn a_suite_refuses_unknown_tasks_agents_tiers_and_empty_selectors() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let models = crate::read_model_tiers(&root.join("agents/models.toml")).expect("model tiers");
    let tasks = sample_catalogue();
    let agents = root.join("agents");
    let mut suite = sample_suite();
    suite.tasks[0] = "probes/recorded/unknown".into();
    assert!(
        crate::validate_suite(Path::new("suite.toml"), &suite, &tasks, &agents, &models)
            .expect_err("unknown task")
            .reason
            .contains("unknown task")
    );
    suite = sample_suite();
    suite.agents[0].config = "missing".into();
    assert!(crate::validate_suite(Path::new("suite.toml"), &suite, &tasks, &agents, &models).is_err());
    suite = sample_suite();
    suite.tier = "unregistered".into();
    assert!(crate::validate_suite(Path::new("suite.toml"), &suite, &tasks, &agents, &models).is_err());
    suite = sample_suite();
    suite.select.push(crate::Selection { kind: Kind::Probe, behaviours: vec!["absent".into()] });
    assert!(
        crate::validate_suite(Path::new("suite.toml"), &suite, &tasks, &agents, &models)
            .expect_err("unmatched selector")
            .reason
            .contains("no existing task")
    );
}

#[test]
fn suite_keys_and_arms_are_strict() {
    let document =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/suites/sample.toml"))
            .expect("sample suite");
    let error =
        crate::formats::parse_document::<crate::Suite>(Path::new("suite.toml"), &format!("{document}\nextra = true"))
            .expect_err("unknown nested agent key");
    assert_eq!(error.key, "agents[0].extra");
    let suite = crate::formats::parse_document::<crate::Suite>(Path::new("suite.toml"), &format!("{document}\n[[arm]]\nsource='binary'\nname='before'\ncommit='30259d8'\n[[arm]]\nsource='override'\nname='control'\n[arm.smith]\naffinity=false\n[[arm]]\nsource='agent'\nname='codex'\nagent='codex'\nprovider='codex'\nconfig='standard'\n")).expect("three arm kinds");
    assert_eq!(suite.arms.len(), 3);
    assert!(
        crate::formats::parse_document::<crate::Suite>(
            Path::new("suite.toml"),
            &format!("{document}\n[[arm]]\nsource='unknown'\nname='new'\n")
        )
        .is_err()
    );
}

#[test]
fn the_small_tier_resolves_and_the_working_tier_never_guesses() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents/models.toml");
    let models = crate::read_model_tiers(&file).expect("committed model tiers");
    let codex = models.lookup(&file, "small", crate::Provider::Codex).expect("codex small tier");
    assert_eq!(codex.model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(codex.effort.as_deref(), Some("low"));
    let anthropic = models.lookup(&file, "small", crate::Provider::Anthropic).expect("anthropic small tier");
    assert_eq!(anthropic.model.as_deref(), Some("claude-haiku-5-5"));
    let mut unresolved = models.clone();
    unresolved
        .0
        .get_mut("working")
        .expect("working tier")
        .insert(crate::Provider::Codex, crate::ModelChoice { model: None, effort: None });
    assert!(
        unresolved
            .lookup(&file, "working", crate::Provider::Codex)
            .expect_err("unresolved choice refuses")
            .reason
            .contains("await")
    );
}

#[test]
fn guards_are_cheapest_first_and_report_budget_and_unknown_costs() {
    let mut tasks = sample_catalogue();
    let mut fast = tasks[0].clone();
    fast.name = "probes/fast".into();
    fast.task.estimate = Some(crate::Estimate { seconds: 2, tokens: 20 });
    let mut expensive = fast.clone();
    expensive.name = "probes/expensive".into();
    expensive.task.estimate = Some(crate::Estimate { seconds: 20, tokens: 100 });
    let mut overflow = fast.clone();
    overflow.name = "probes/overflow".into();
    overflow.task.estimate = Some(crate::Estimate { seconds: u64::MAX, tokens: 1 });
    tasks.extend([expensive, fast, overflow]);
    let mut suite = sample_suite();
    suite.max_wall_seconds = 10;
    let chosen = crate::choose_guards(&suite, &tasks, &["benchmarks.md, section 5.2".into()]);
    assert_eq!(chosen.selected, ["probes/fast", "probes/fast/short"]);
    assert_eq!((chosen.estimated_seconds, chosen.estimated_tokens), (8, 80));
    assert!(chosen.omitted.contains(&("probes/recorded".into(), "cost unavailable".into())));
    assert!(chosen.omitted.contains(&("probes/overflow".into(), "cost overflow".into())));
    assert!(chosen.omitted.contains(&("probes/expensive".into(), "suite wall or token budget".into())));
}
