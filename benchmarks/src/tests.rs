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

fn codex_usage(input: u64) -> crate::Usage {
    crate::Usage::Codex { input, cached_input: 20, output: 10 }
}

fn ledger() -> crate::TokenLedger {
    let mut ledger = crate::TokenLedger::default();
    ledger.declare("root", crate::ScopeKind::Root, crate::Convention::Codex, "root usage not observed").expect("root");
    ledger
}

#[test]
fn token_conventions_preserve_cache_and_reasoning_boundaries() {
    use crate::{Measure, Usage};

    let codex = codex_usage(100).normalise().expect("Codex mapping");
    assert_eq!(codex.fresh, Measure::Observed { value: 80 });
    assert_eq!(codex.cache_read, Measure::Observed { value: 20 });
    assert_eq!(codex.output, Measure::Observed { value: 10 });
    assert!(matches!(codex.cache_write, Measure::Unavailable { .. }));
    assert!(matches!(codex.reasoning, Measure::Unavailable { .. }));
    let claude =
        Usage::ClaudeCode { input: 30, cache_read: 70, cache_write: 5, output: 9 }.normalise().expect("Claude mapping");
    assert_eq!(claude.fresh, Measure::Observed { value: 35 });
    assert_eq!(claude.cache_read, Measure::Observed { value: 70 });
    assert_eq!(claude.cache_write, Measure::Observed { value: 5 });
    assert!(matches!(claude.reasoning, Measure::Unavailable { .. }));
    let smith = Usage::SmithEvents {
        input: Some(30),
        cache_read: Some(70),
        cache_write: Some(5),
        output: Some(9),
        reasoning: Some(4),
    }
    .normalise()
    .expect("events mapping");
    assert_eq!(smith.fresh, Measure::Observed { value: 35 });
    assert_eq!(smith.reasoning, Measure::Observed { value: 4 });
    let partial =
        Usage::SmithEvents { input: Some(30), cache_read: None, cache_write: None, output: None, reasoning: None }
            .normalise()
            .expect("nulls preserved");
    assert!(matches!(partial.fresh, Measure::LowerBound { value: 30, .. }));
    assert!(matches!(partial.output, Measure::Unavailable { .. }));
}

#[test]
fn impossible_and_overflowing_token_records_are_errors() {
    assert!(crate::Usage::Codex { input: 1, cached_input: 2, output: 0 }.normalise().is_err());
    assert!(
        crate::Usage::ClaudeCode { input: u64::MAX, cache_read: 0, cache_write: 1, output: 0 }.normalise().is_err()
    );
    let mut ledger = ledger();
    ledger.record("root", "one", codex_usage(u64::MAX)).expect("first response");
    ledger.record("root", "two", codex_usage(100)).expect("second response");
    assert!(ledger.records().expect_err("sum must not wrap").contains("overflow"));
}

#[test]
fn usage_counts_once_per_response_even_when_repeated_in_another_scope() {
    let mut ledger = ledger();
    ledger
        .declare(
            "child",
            crate::ScopeKind::Child { parent: "root".into() },
            crate::Convention::Codex,
            "child has no own usage",
        )
        .expect("child");
    assert!(ledger.record("root", "response-a", codex_usage(100)).expect("first response"));
    assert!(!ledger.record("root", "response-a", codex_usage(100)).expect("repeat"));
    assert!(!ledger.record("child", "response-a", codex_usage(100)).expect("inherited repeat"));
    assert!(ledger.record("child", "response-b", codex_usage(50)).expect("own child response"));
    assert!(
        ledger
            .record("root", "response-a", codex_usage(101))
            .expect_err("conflicting duplicate")
            .contains("conflicting")
    );
    let records = ledger.records().expect("scoped tokens");
    assert_eq!(records.len(), 3);
    assert_eq!(records.last().expect("total").usage.fresh, crate::Measure::Observed { value: 110 });
    assert_eq!(records.last().expect("total").usage.output, crate::Measure::Observed { value: 20 });
}

#[test]
fn token_totals_name_missing_children_and_never_substitute_for_a_missing_root() {
    let mut ledger = ledger();
    ledger
        .declare(
            "unlinked-child",
            crate::ScopeKind::Child { parent: "root".into() },
            crate::Convention::Codex,
            "rollout does not link child usage",
        )
        .expect("missing child");
    ledger.record("root", "root-response", codex_usage(100)).expect("root usage");
    let records = ledger.records().expect("partial total");
    match &records.last().expect("total").usage.fresh {
        crate::Measure::LowerBound { value, missing } => {
            assert_eq!(*value, 80);
            assert!(missing.iter().any(|reason| reason.contains("unlinked-child")));
        }
        other @ (crate::Measure::Observed { .. } | crate::Measure::Unavailable { .. }) => {
            panic!("missing child must remain a lower bound: {other:?}");
        }
    }
    let mut missing_root = crate::TokenLedger::default();
    missing_root
        .declare("root", crate::ScopeKind::Root, crate::Convention::Codex, "root unavailable")
        .expect("missing root");
    missing_root
        .declare("child", crate::ScopeKind::Child { parent: "root".into() }, crate::Convention::Codex, "not read")
        .expect("child");
    missing_root.record("child", "child-response", codex_usage(100)).expect("child usage");
    assert!(
        matches!(&missing_root.records().expect("total").last().expect("total").usage.fresh, crate::Measure::Unavailable { reason } if reason.contains("root"))
    );
}

#[test]
fn observed_totals_include_children_compactions_and_helpers_in_their_own_scopes() {
    use crate::{Convention, Measure, ScopeKind, TokenLedger, Usage};

    let mut ledger = TokenLedger::default();
    let scopes = [
        ("root", ScopeKind::Root),
        ("child", ScopeKind::Child { parent: "root".into() }),
        ("compact", ScopeKind::Compaction { parent: "root".into() }),
        ("helper", ScopeKind::Helper { parent: "root".into(), model: "helper-model".into() }),
    ];
    for (id, role) in scopes {
        ledger.declare(id, role, Convention::SmithEvents, "not observed").expect("scope");
        ledger
            .record(
                id,
                &format!("response-{id}"),
                Usage::SmithEvents {
                    input: Some(2),
                    cache_read: Some(3),
                    cache_write: Some(4),
                    output: Some(5),
                    reasoning: Some(0),
                },
            )
            .expect("own response");
    }
    let records = ledger.records().expect("complete total");
    assert_eq!(records.len(), 5);
    let total = &records.last().expect("total").usage;
    assert_eq!(total.fresh, Measure::Observed { value: 24 });
    assert_eq!(total.cache_read, Measure::Observed { value: 12 });
    assert_eq!(total.cache_write, Measure::Observed { value: 16 });
    assert_eq!(total.output, Measure::Observed { value: 20 });
    assert_eq!(total.reasoning, Measure::Observed { value: 0 });
}

#[test]
fn a_null_usage_field_keeps_its_response_gap_when_later_responses_report_it() {
    use crate::{Convention, Measure, ScopeKind, TokenLedger, Usage};

    let mut ledger = TokenLedger::default();
    ledger.declare("root", ScopeKind::Root, Convention::SmithEvents, "not observed").expect("root");
    for (id, output) in [("missing-response", None), ("reported-response", Some(10))] {
        ledger
            .record(
                "root",
                id,
                Usage::SmithEvents {
                    input: Some(2),
                    cache_read: Some(3),
                    cache_write: Some(4),
                    output,
                    reasoning: None,
                },
            )
            .expect("partial response");
    }
    let records = ledger.records().expect("partial total");
    assert!(
        matches!(&records.last().expect("total").usage.output, Measure::LowerBound { value: 10, missing } if missing.iter().any(|reason| reason.contains("missing-response")))
    );
}

#[test]
fn measurements_require_reasons_and_the_ledger_requires_a_root_and_one_convention() {
    use crate::{Convention, Measure, ScopeKind, TokenLedger};

    for text in [r#"{"state":"unavailable","reason":""}"#, r#"{"state":"lower-bound","value":1,"missing":[]}"#] {
        assert!(serde_json::from_str::<Measure<u64>>(text).is_err());
    }
    assert!(TokenLedger::default().records().is_err());
    let mut ledger = ledger();
    assert!(ledger.declare("second-root", ScopeKind::Root, Convention::Codex, "not observed").is_err());
    ledger
        .declare("child", ScopeKind::Child { parent: "root".into() }, Convention::SmithEvents, "not observed")
        .expect("scope");
    assert!(ledger.records().expect_err("mixed conventions").contains("mix"));
}

#[test]
fn result_readers_refuse_versions_unknown_shapes_and_trailing_documents() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/results/attempt.json");
    let text = std::fs::read_to_string(&file).expect("recorded result");
    let mut document: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    document["version"] = 2.into();
    assert_eq!(crate::parse_result(&file, &document.to_string()).expect_err("another version").key, "version");
    document["version"] = crate::RESULT_VERSION.into();
    document["counts"]["extra"] = 1.into();
    assert!(crate::parse_result(&file, &document.to_string()).is_err());
    assert!(crate::parse_result(&file, &format!("{text}{{}}")).is_err());
    let mut result = crate::read_result(&file).expect("recorded result");
    result.spend.usd = crate::Measure::Observed { value: f64::INFINITY };
    assert!(crate::render_result(&result).is_err());
}

#[test]
fn smith_exit_codes_reconcile_answers_deadlines_and_own_budgets() {
    use crate::{BudgetLimit, End, Exit, FailureReason, Forced, classify_smith_exit};

    assert_eq!(classify_smith_exit(Exit::Code(0), true, Forced::No, None).end, End::Completed);
    assert!(matches!(
        classify_smith_exit(Exit::Code(1), false, Forced::No, None).end,
        End::Failed { reason: FailureReason::Agent, .. }
    ));
    for code in [2, 3] {
        let end = classify_smith_exit(Exit::Code(code), false, Forced::No, None).end;
        assert!(matches!(end, End::Refused { .. }));
        assert!(!end.counts_in_rate());
    }
    for which in [BudgetLimit::Turns, BudgetLimit::Time, BudgetLimit::Spend] {
        let end = classify_smith_exit(Exit::Code(4), false, Forced::No, Some(which)).end;
        assert_eq!(end, End::Budget { which });
        assert!(!end.counts_in_rate());
    }
    assert!(matches!(
        classify_smith_exit(Exit::Code(5), false, Forced::No, None).end,
        End::Failed { reason: FailureReason::InputNeeded, .. }
    ));
    assert!(matches!(
        classify_smith_exit(Exit::Code(130), false, Forced::No, None).end,
        End::Failed { reason: FailureReason::Cancelled, .. }
    ));
    for exit in [Exit::Code(130), Exit::Signal(9)] {
        assert_eq!(classify_smith_exit(exit, false, Forced::Killed, None).end, End::Timeout);
        let accepted = classify_smith_exit(exit, true, Forced::Killed, None);
        assert_eq!(accepted.end, End::Completed);
        assert_eq!(accepted.warnings.len(), 1);
    }
    for (code, accepted) in [(4, false), (0, false), (77, false), (1, true)] {
        assert!(matches!(
            classify_smith_exit(Exit::Code(code), accepted, Forced::No, None).end,
            End::HarnessError { .. }
        ));
    }
}

fn recorded_result() -> crate::AttemptResult {
    crate::read_result(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/results/attempt.json"))
        .expect("recorded result")
}

#[test]
fn the_probe_rerun_rule_reports_regressions_and_quarantines_disagreement() {
    use crate::{ProbeVerdict, probe_verdict};
    assert_eq!(probe_verdict(true, None), ProbeVerdict::Passed);
    assert_eq!(probe_verdict(false, None), ProbeVerdict::Rerun);
    assert_eq!(probe_verdict(false, Some(false)), ProbeVerdict::Regression);
    assert_eq!(probe_verdict(false, Some(true)), ProbeVerdict::Quarantined);
    assert_eq!(probe_verdict(true, Some(false)), ProbeVerdict::Quarantined);
}

#[test]
fn seeded_blocks_and_audit_samples_reproduce_and_keep_every_disagreement() {
    let blocks = crate::interleave(4, 5, 123);
    assert_eq!(blocks, crate::interleave(4, 5, 123));
    for mut block in blocks {
        block.sort_unstable();
        assert_eq!(block, [0, 1, 2, 3]);
    }
    let sample = crate::audit_sample(20, 3, &[0, 19], 45).expect("sample");
    assert_eq!(sample, crate::audit_sample(20, 3, &[0, 19], 45).expect("replay"));
    assert!(sample.contains(&0) && sample.contains(&19));
    assert_eq!(crate::audit_sample(20, 0, &[2, 8], 45).expect("mandatory only"), [2, 8]);
    assert!(crate::audit_sample(2, 3, &[2], 1).is_err());
}

#[test]
fn bootstrap_intervals_and_effects_reproduce_from_the_seed() {
    let left = [10.0, 11.0, 12.0, 13.0, 14.0];
    let right = [20.0, 21.0, 22.0, 23.0, 24.0];
    let comparison = crate::bootstrap_ratio(&left, &right, 99, 1000).expect("comparison");
    assert_eq!(comparison, crate::bootstrap_ratio(&left, &right, 99, 1000).expect("replay"));
    assert!(comparison.interval_95.lower > 1.0 && comparison.detected);
    let equal = crate::bootstrap_ratio(&[10.0; 5], &[10.0; 5], 99, 1000).expect("equal arms");
    assert!(!equal.detected);
    assert!((equal.ratio - 1.0).abs() < f64::EPSILON);
    assert!(crate::bootstrap_ratio(&[1.0; 3], &[2.0; 3], 99, 1000).is_err());
    assert!(crate::bootstrap_ratio(&[0.0; 5], &[2.0; 5], 99, 1000).is_err());
    let effect = crate::minimum_detectable_effect(0.1, 5).expect("MDE");
    assert!((effect - 0.177_087_548_969_429_24).abs() < 1e-12);
    assert!(crate::coefficient_of_variation(&[f64::MAX; 5]).expect("scaled CV") < f64::EPSILON);
}

#[test]
fn exact_pass_intervals_hold_at_empty_and_extreme_counts() {
    assert!(crate::pass_interval(0, 0).is_err());
    assert!(crate::pass_interval(2, 1).is_err());
    let single = crate::pass_interval(1, 1).expect("one pass");
    assert!((single.lower - 0.025).abs() < 1e-12);
    assert!((single.upper - 1.0).abs() < f64::EPSILON);
    let all = crate::pass_interval(10, 10).expect("ten passes");
    assert!((all.lower - 0.691_502_892_181_239_2).abs() < 1e-10);
    let middle = crate::pass_interval(5, 10).expect("balanced");
    assert!((middle.lower - 0.187_086_028_447_398_5).abs() < 1e-10);
    assert!((middle.upper - 0.812_913_971_552_601_5).abs() < 1e-10);
    let zero = crate::pass_interval(0, 10).expect("no passes");
    assert!((zero.upper + all.lower - 1.0).abs() < 1e-10);
}

#[test]
fn summaries_include_failed_attempt_metrics_and_report_budget_and_setup_apart() {
    let template = recorded_result();
    let ends = [
        crate::End::Completed,
        crate::End::Completed,
        crate::End::Timeout,
        crate::End::Budget { which: crate::BudgetLimit::Time },
        crate::End::Refused { setup: "missing login".into() },
    ];
    let results: Vec<_> = ends
        .into_iter()
        .zip([10, 20, 100, 50, 30])
        .enumerate()
        .map(|(index, (end, wall))| {
            let mut result = template.clone();
            result.identity.attempt = format!("attempt-{index}");
            result.outcome.end = end;
            result.timing.task_wall_ms = crate::Measure::Observed { value: wall };
            result
        })
        .collect();
    let summary = crate::summarise(&results, "sample", "small", crate::Design::Single, 1, None).expect("summary");
    let group = &summary.groups[0];
    assert_eq!(group.attempts.len(), 5);
    assert_eq!(group.passes.passed, 2);
    assert_eq!(group.passes.failed, 1);
    assert_eq!((group.passes.budget, group.passes.setup), (1, 1));
    let metric = &group.metrics["task_wall_ms"];
    assert_eq!(metric.observed, 5);
    assert_eq!(metric.median, crate::Measure::Observed { value: 30.0 });
    assert_eq!(group.metrics["cpu_ms/pidfd-walk"].observed, 0);
    assert_eq!(group.metrics["cpu_ms/pidfd-walk"].missing.len(), 5);
    assert!(summary.comparisons.is_empty());
}

#[test]
fn summaries_audit_disagreements_and_keep_unknown_grades_out_of_observed_rates() {
    let mut results: Vec<_> = (0..10)
        .map(|index| {
            let mut result = recorded_result();
            result.identity.attempt = format!("attempt-{index:02}");
            result
        })
        .collect();
    results[8].outcome.grade = Some(crate::Measure::Observed { value: false });
    results[9].outcome.grade = Some(crate::Measure::unavailable("grader crashed"));
    let summary = crate::summarise(&results, "sample", "small", crate::Design::Single, 3, None).expect("summary");
    assert!(summary.audits.contains(&"attempt-08".into()));
    assert_eq!(
        (summary.groups[0].passes.passed, summary.groups[0].passes.failed, summary.groups[0].passes.unavailable),
        (8, 1, 1)
    );
    assert!(matches!(summary.groups[0].passes.interval_95, crate::Measure::Unavailable { .. }));
    let mut changed = results.clone();
    changed[0].identity.task_version += 1;
    assert!(crate::summarise(&changed, "sample", "small", crate::Design::Single, 3, None).is_err());
    let mut duplicate = results.clone();
    duplicate.push(results[0].clone());
    assert!(crate::summarise(&duplicate, "sample", "small", crate::Design::Single, 3, None).is_err());
}

#[test]
fn baselines_raise_drift_notices_and_refuse_mixed_configurations() {
    let results: Vec<_> = (0..5)
        .map(|index| {
            let mut result = recorded_result();
            result.identity.attempt = format!("attempt-{index}");
            result
        })
        .collect();
    let summary = crate::summarise(&results, "sample", "small", crate::Design::Single, 3, None).expect("summary");
    let baseline = crate::baseline(&summary).expect("one configuration");
    let changed: Vec<_> = results
        .into_iter()
        .map(|mut result| {
            result.timing.task_wall_ms = crate::Measure::Observed { value: 60 };
            result
        })
        .collect();
    let drift =
        crate::summarise(&changed, "sample", "small", crate::Design::Single, 3, Some(&baseline)).expect("drift");
    assert!(
        drift
            .drift_notices
            .iter()
            .any(|notice| notice.contains("task_wall_ms") && notice.contains("compare in one session"))
    );
    let mut mixed = summary.clone();
    let mut other = summary.groups[0].clone();
    other.arm = "other".into();
    mixed.groups.push(other);
    assert!(crate::baseline(&mixed).is_err());
}

#[test]
fn matching_committed_medians_replace_estimates_and_stale_pins_do_not() {
    use sha2::{Digest, Sha256};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let agents = root.join("agents");
    let models = crate::read_model_tiers(&agents.join("models.toml")).expect("tiers");
    let tasks = sample_catalogue();
    let suite = sample_suite();
    let mut result = recorded_result();
    let pin = crate::read_configuration(&agents.join("codex/standard.pin.toml")).expect("pin");
    result.identity.configuration_sha256 = pin.sha256;
    result.identity.prompt_sha256 = format!("{:x}", Sha256::digest(tasks[0].task.prompt.as_bytes()));
    let summary = crate::summarise(&[result], "sample", "small", crate::Design::Single, 1, None).expect("summary");
    let costs =
        crate::committed_costs(&suite, &tasks, std::slice::from_ref(&summary), &agents, &models).expect("costs");
    assert_eq!((costs["probes/recorded"].seconds, costs["probes/recorded"].tokens), (1, 110));
    let chosen = crate::choose_guards_with_costs(&suite, &tasks, &["benchmarks.md, section 5.2".into()], &costs);
    assert_eq!(chosen.selected, ["probes/recorded"]);
    assert_eq!((chosen.estimated_seconds, chosen.estimated_tokens), (2, 220));
    let mut stale = summary;
    stale.groups[0].configuration_sha256 = "old-pin".into();
    assert!(crate::committed_costs(&suite, &tasks, &[stale], &agents, &models).expect("stale ignored").is_empty());
}

#[test]
fn summaries_compare_complete_interleaved_arms_and_keep_single_runs_apart() {
    let mut results = Vec::new();
    for arm in ["a-reference", "b-candidate"] {
        for index in 0..5 {
            let mut result = recorded_result();
            result.identity.attempt = format!("{arm}-{index}");
            result.identity.arm = arm.into();
            result.timing.task_wall_ms = crate::Measure::Observed { value: if arm == "a-reference" { 30 } else { 60 } };
            results.push(result);
        }
    }
    let comparison = crate::summarise(&results, "sample", "small", crate::Design::Interleaved, 17, None)
        .expect("interleaved summary");
    let wall =
        comparison.comparisons.iter().find(|comparison| comparison.metric == "task_wall_ms").expect("wall comparison");
    assert!(
        matches!(&wall.result, crate::Measure::Observed { value } if value.detected && (value.ratio - 2.0).abs() < f64::EPSILON)
    );
    assert!(
        crate::summarise(&results, "sample", "small", crate::Design::Single, 17, None)
            .expect("single summary")
            .comparisons
            .is_empty()
    );
    results[0].timing.task_wall_ms = crate::Measure::unavailable("missing task end");
    let partial =
        crate::summarise(&results, "sample", "small", crate::Design::Interleaved, 17, None).expect("partial summary");
    assert!(matches!(
        partial
            .comparisons
            .iter()
            .find(|comparison| comparison.metric == "task_wall_ms")
            .expect("wall comparison")
            .result,
        crate::Measure::Unavailable { .. }
    ));
}
