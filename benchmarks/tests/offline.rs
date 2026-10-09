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
fn every_committed_suite_and_configuration_pin_is_checked_offline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let counts = smith_bench::check_benchmark_tree(&root, &design()).expect("all benchmark manifests validate");
    assert!(counts.configurations > 0, "committed pins are actually checked");
}

#[test]
fn the_recorded_suite_resolves_its_frozen_task_and_pin() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let suite_file = root.join("tests/recorded/suites/sample.toml");
    let suite = smith_bench::read_suite(&suite_file).expect("recorded suite");
    let tasks =
        smith_bench::catalogue(&root.join("tests/recorded/tasks/valid"), &design()).expect("recorded catalogue");
    let models = smith_bench::read_model_tiers(&root.join("agents/models.toml")).expect("committed models");
    assert_eq!(
        smith_bench::validate_suite(&suite_file, &suite, &tasks, &root.join("agents"), &models)
            .expect("references resolve"),
        ["probes/recorded"]
    );
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

#[test]
fn a_recorded_attempt_round_trips_without_erasing_measurement_gaps() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/results/attempt.json");
    let result = smith_bench::read_result(&file).expect("recorded result");
    let rendered = smith_bench::render_result(&result).expect("versioned JSON");
    assert_eq!(smith_bench::parse_result(&file, &rendered).expect("round-trip"), result);
    assert!(matches!(result.spend.usd, smith_bench::Measure::Unavailable { .. }));
    assert!(matches!(result.resources.cpu_ms, smith_bench::Measure::LowerBound { .. }));
    assert_eq!(result.outcome.end, smith_bench::End::Completed);
    assert_eq!(result.outcome.forced, smith_bench::Forced::Killed);
}
