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
