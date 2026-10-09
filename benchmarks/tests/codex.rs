use smith_bench::agent::codex::{Observer, parse_line};
use smith_bench::{End, Exit, Forced, Measure};

const COMPLETED: &str = include_str!("recorded/codex/completed.jsonl");
const CHILDREN: &str = include_str!("recorded/codex/children.jsonl");
const FAILED: &str = include_str!("recorded/codex/failed.jsonl");
const REPEATED: &str = include_str!("recorded/codex/repeated.jsonl");

fn observed(stream: &str) -> Observer {
    let mut observer = Observer::default();
    for line in stream.lines() {
        observer.observe_line(line).expect("fixture record");
    }
    observer
}

#[test]
fn archived_and_explicitly_synthetic_streams_preserve_root_only_evidence() {
    let completed = observed(COMPLETED);
    assert_eq!(completed.classify(Exit::Code(0), Forced::No).end, End::Completed);
    assert_eq!(completed.token_records().expect("usage"), observed(REPEATED).token_records().expect("repeat usage"));
    let tokens = completed.token_records().expect("root-only usage");
    assert_eq!(tokens[0].usage.fresh, Measure::Observed { value: 18_802 });
    assert_eq!(tokens[0].usage.cache_write, Measure::Observed { value: 0 });
    assert_eq!(tokens[0].usage.reasoning, Measure::Observed { value: 697 });
    assert!(matches!(tokens[1].usage.output, Measure::LowerBound { value: 2255, .. }));
    let children = observed(CHILDREN);
    assert_eq!(children.classify(Exit::Code(0), Forced::No).end, End::Completed);
    assert!(matches!(
        children.token_records().expect("children usage")[1].usage.output,
        Measure::LowerBound { value: 13_067, .. }
    ));
    assert!(matches!(children.counts().responses, Measure::Unavailable { .. }));
    assert!(matches!(children.counts().children, Measure::Unavailable { .. }));
    assert!(matches!(observed(FAILED).classify(Exit::Code(1), Forced::No).end, End::Failed { .. }));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("recorded/codex/manifest.json")).expect("manifest");
    for record in manifest["records"].as_array().expect("records") {
        let file = record["file"].as_str().expect("file");
        assert_eq!(
            record["provenance"],
            if file == "failed.jsonl" || file == "repeated.jsonl" { "synthetic" } else { "archived" }
        );
    }
}

#[test]
fn known_shapes_and_conflicting_usage_fail_with_named_evidence() {
    assert!(parse_line(r#"{"type":"future.record","payload":17}"#).expect("unknown kind").is_none());
    assert!(parse_line(r#"{"type":"item.completed","item":{"type":"future.item"}}"#).expect("unknown item").is_none());
    for (line, field) in [
        (
            r#"{"type":"item.completed","item":{"type":"command_execution","id":"bad","command":"true","aggregated_output":"","exit_code":0,"status":"renamed"}}"#,
            "item.status",
        ),
        (r#"{"type":"item.completed","item":{"type":"agent_message","id":"bad","text":12}}"#, "item.text"),
        (r#"{"type":"thread.started","thread":"renamed"}"#, "thread"),
        (
            r#"{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"renamed":1}}"#,
            "renamed",
        ),
        (r#"{"type":"turn.failed","error":{}}"#, "message"),
    ] {
        assert!(parse_line(line).expect_err("known shape").to_string().contains(field));
    }
    let mut turns = observed(COMPLETED);
    turns.observe_line(r#"{"type":"turn.started"}"#).expect("new turn");
    let usage = r#"{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}"#;
    turns.observe_line(usage).expect("second aggregate");
    turns.observe_line(usage).expect("repeat second aggregate");
    let combined = turns.token_records().expect("turn sum");
    assert_eq!(combined[0].usage.fresh, Measure::Observed { value: 18_803 });
    assert!(matches!(combined[0].usage.cache_write, Measure::LowerBound { value: 0, .. }));
    let mut observer = observed(COMPLETED);
    let conflict = COMPLETED.lines().last().expect("usage").replace("130418", "130419");
    assert!(observer.observe_line(&conflict).expect_err("conflict").to_string().contains("usage"));
    assert!(matches!(observer.classify(Exit::Code(0), Forced::No).end, End::HarnessError { .. }));
    assert!(parse_line("{").is_err());
}

#[test]
fn latest_turn_exit_deadline_and_fatal_error_decide_the_end() {
    let completed = observed(COMPLETED);
    assert_eq!(completed.classify(Exit::Signal(9), Forced::Killed).end, End::Completed);
    assert_eq!(completed.classify(Exit::Signal(9), Forced::Killed).warnings.len(), 1);
    assert!(matches!(completed.classify(Exit::Code(1), Forced::No).end, End::HarnessError { .. }));
    let mut active = observed(COMPLETED);
    active.observe_line(r#"{"type":"turn.started"}"#).expect("next turn");
    assert!(active.answer().is_none());
    assert_eq!(active.classify(Exit::Signal(9), Forced::Killed).end, End::Timeout);
    active
        .observe_line(r#"{"type":"turn.failed","error":{"message":"synthetic second-turn failure"}}"#)
        .expect("failed");
    assert!(matches!(active.classify(Exit::Code(1), Forced::No).end, End::Failed { .. }));
    let mut fatal = completed;
    fatal.observe_line(r#"{"type":"error","message":"synthetic fatal stream error"}"#).expect("fatal");
    assert!(matches!(fatal.classify(Exit::Code(1), Forced::No).end, End::Failed { .. }));
    let no_usage = observed("{\"type\":\"thread.started\",\"thread_id\":\"synthetic\"}\n{\"type\":\"turn.started\"}\n");
    assert!(matches!(no_usage.token_records().expect("missing")[1].usage.output, Measure::Unavailable { .. }));
    assert!(matches!(no_usage.classify(Exit::Code(0), Forced::No).end, End::HarnessError { .. }));
}
