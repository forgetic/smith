//! Recorded task acceptance and each independent manifest refusal.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use smith_bench::{check_task, check_tree};

fn recorded(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/tasks").join(name).join("task.toml")
}

fn design() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/design")
}

fn refuses(name: &str, key: &str, reason: &str) {
    let error = check_task(&recorded(name), &design()).expect_err("recorded task must refuse");
    assert!(error.key.contains(key), "{error}");
    assert!(error.reason.contains(reason), "{error}");
}

#[test]
fn every_catalogue_manifest_is_checked_offline() {
    let tasks = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tasks");
    check_tree(&tasks, &design()).expect("all committed manifests validate");
}

#[test]
fn a_complete_recorded_task_is_accepted() {
    check_task(&recorded("valid"), &design()).expect("valid frozen task");
}

#[test]
fn a_prompt_without_its_final_newline_is_refused() {
    refuses("missing-newline", "prompt", "end in a newline");
}

#[test]
fn a_prompt_with_two_paragraphs_is_refused() {
    refuses("multiple-paragraphs", "prompt", "one nonempty paragraph");
}

#[test]
fn a_prompt_at_the_legacy_line_bound_is_refused() {
    refuses("oversize-prompt", "prompt", "4,088 bytes");
}

#[test]
fn changed_seed_bytes_are_refused() {
    refuses("wrong-digest", "seed_sha256", "seed digest differs");
}

#[test]
fn a_missing_design_section_is_refused() {
    refuses("missing-guard", "guards[0]", "does not exist");
}

#[test]
fn a_benchmark_tree_in_a_seed_is_refused() {
    refuses("benchmark-seed", "seed", "stay outside");
}

#[test]
fn a_grader_file_in_a_seed_is_refused() {
    refuses("grader-seed", "seed", "stay outside");
}

#[test]
fn a_reference_file_in_a_seed_is_refused() {
    refuses("reference-seed", "seed", "stay outside");
}

#[test]
fn an_unknown_outcome_check_is_refused() {
    refuses("unknown-check", "outcome", "unknown variant");
}

#[test]
fn an_unknown_nested_key_is_refused() {
    refuses("unknown-key", "budget.turnz", "unknown field");
}

#[test]
fn a_check_cannot_escape_the_workspace() {
    refuses("outside-path", "outcome[0].path", "without traversal");
}

#[test]
fn duplicate_variant_names_are_refused() {
    refuses("duplicate-variant", "variant[1].name", "unique");
}
