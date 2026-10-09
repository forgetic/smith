use sha2::{Digest, Sha256};
use smith_bench::agent::smith::legacy::{Event, Observation, Observer, Value, parse_debug, parse_line};
use smith_bench::{BudgetLimit, End, Exit, Forced, Measure};

const COMPLETED: &str = include_str!("recorded/smith-legacy/completed.jsonl");
const TOOLS: &str = include_str!("recorded/smith-legacy/tools.jsonl");

fn observed(stream: &str) -> (Observer, Vec<Observation>) {
    let mut observer = Observer::default();
    let records = stream.lines().filter_map(|line| observer.observe_line(line).expect("fixture")).collect();
    (observer, records)
}

fn fact(text: &str) -> String {
    serde_json::json!({"type":"fact","at_ns":17,"fact":text}).to_string()
}

#[test]
fn debug_notation_preserves_nested_named_fields_tuple_variants_bytes_and_unicode() {
    let tree = parse_debug(
        r#"Wrapper { token: Token(5), result: Some((Accepted, [1, 2])), data: b"\0\xff\n", label: "a\u{2764}\"\\" }"#,
    )
    .expect("Debug");
    let Value::Struct { name, fields } = tree else { panic!("struct") };
    assert_eq!(name, "Wrapper");
    assert_eq!(fields["data"], Value::Bytes(vec![0, 255, b'\n']));
    assert_eq!(fields["label"], Value::String("a❤\"\\".into()));
    assert_eq!(fields["token"], Value::Tuple { name: "Token".into(), values: vec![Value::Number(5)] });
    assert_eq!(
        parse_debug("Some(7,)").expect("trailing comma"),
        Value::Tuple { name: "Some".into(), values: vec![Value::Number(7)] }
    );
    for invalid in [
        "Token(1) junk",
        "Foo { renamed: 1, renamed: 2 }",
        "18446744073709551616",
        "b\"\\u{1234}\"",
        "\"\\u{d800}\"",
        "[1",
        "\"\\x80\"",
    ] {
        assert!(parse_debug(invalid).is_err(), "{invalid}");
    }
    assert!(parse_debug(&format!("{}0{}", "Some(".repeat(80), ")".repeat(80))).is_err());
}

#[test]
fn baseline_recordings_map_calls_completion_attempts_tools_and_main_terminals() {
    for stream in [COMPLETED, TOOLS] {
        let (observer, records) = observed(stream);
        assert_eq!(observer.classify(Exit::Code(0), Forced::No).end, End::Completed);
        assert!(observer.accepted().is_some());
        assert!(matches!(observer.usage(), Measure::Unavailable { .. }));
        assert!(matches!(observer.loss(), Measure::Unavailable { .. }));
        assert!(records.iter().any(|record| matches!(
            record,
            Observation::Fact { event: Event::CompletionStarted { attempt: 0, .. }, .. }
        )));
        assert!(records.iter().any(
            |record| matches!(record, Observation::Call { name, input, .. } if name == b"finish" && !input.is_empty())
        ));
    }
    let (_, records) = observed(TOOLS);
    assert!(records.iter().any(|record| matches!(record, Observation::Fact { event: Event::ToolAnswered { verdict: Value::Struct { name, .. }, .. }, .. } if name == "Written")));
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("recorded/smith-legacy/manifest.json")).expect("manifest");
    for (record, stream) in manifest["records"].as_array().expect("records").iter().zip([COMPLETED, TOOLS]) {
        assert_eq!(record["provenance"], "recorded");
        assert_eq!(record["sha256"], format!("{:x}", Sha256::digest(stream)));
        assert!(!stream.contains("refresh-new") && !stream.contains("access-new"));
    }
}

#[test]
fn child_finish_is_never_main_and_authoritative_run_answers_survive_missing_call_facts() {
    let mut child = Observer::default();
    for text in [
        "Run { fact: Opened { run: Token(1), conversation: Token(2), child: false } }",
        "Run { fact: Opened { run: Token(1), conversation: Token(3), child: true } }",
        "Run { fact: Called { run: Token(1), conversation: Token(3), call: Token(4), ask: Finish } }",
        "Run { fact: Returned { run: Token(1), call: Token(4), result: Accepted } }",
    ] {
        child.observe_line(&fact(text)).expect("child");
    }
    assert!(child.accepted().is_none());
    assert!(matches!(child.classify(Exit::Code(0), Forced::No).end, End::HarnessError { .. }));
    child.observe_line(&fact("Run { fact: Answered { run: Token(1), answer: Accepted } }")).expect("run terminal");
    assert_eq!(child.classify(Exit::Code(0), Forced::No).end, End::Completed);
    for (answer, end) in [
        ("Failed(Budget(Turns))", End::Budget { which: BudgetLimit::Turns }),
        ("Failed(Budget(Time))", End::Budget { which: BudgetLimit::Time }),
        ("Failed(Budget(Spend))", End::Budget { which: BudgetLimit::Spend }),
    ] {
        let mut observer = Observer::default();
        observer
            .observe_line(&fact(&format!("Run {{ fact: Answered {{ run: Token(1), answer: {answer} }} }}")))
            .expect("budget");
        assert_eq!(observer.classify(Exit::Code(0), Forced::No).end, end);
    }
}

#[test]
fn known_shape_changes_poison_classification_and_unknown_record_types_are_skipped() {
    assert!(parse_line(r#"{"type":"future","unexpected":2}"#).expect("unknown").is_none());
    for (text, field) in [
        ("Run { fact: Opened { run: Token(1), renamed: Token(2), child: false } }", "conversation"),
        ("Run { fact: Opened { run: Token(1), conversation: Token(2), child: No } }", "child"),
        ("Run { fact: Called { run: Token(1), conversation: Token(2), call: Token(4), ask: Finish(2) } }", "ask"),
        ("Run { fact: Answered { run: Token(1), answer: Accepted { unknown: 0 } } }", "answer"),
        (
            "Session { fact: Used { opener: Token(1), usage: Usage { input_tokens: 1, output_tokens: 2, cache_read_tokens: 3, renamed: 4 } } }",
            "cache_write_tokens",
        ),
    ] {
        let mut observer = observed(COMPLETED).0;
        assert!(observer.observe_line(&fact(text)).expect_err("shape").to_string().contains(field));
        assert!(matches!(observer.classify(Exit::Code(0), Forced::No).end, End::HarnessError { .. }));
    }
    for line in [
        r#"{"type":"prompt","at_ns":0,"owner":1,"encoding":"utf8","bytes":"aa"}"#,
        r#"{"type":"tool","at_ns":0,"owner":1}"#,
        r#"{"type":"call","at_ns":0,"owner":1,"encoding":"hex","id":"a","name":"66","input":""}"#,
    ] {
        assert!(parse_line(line).is_err());
    }
}

#[test]
fn everything_content_preserves_bytes_and_legacy_exit_uses_observed_terminals() {
    assert!(
        matches!(parse_line(r#"{"type":"prompt","at_ns":2,"owner":0,"encoding":"hex","bytes":"00ff"}"#).expect("prompt"), Some(Observation::Prompt { bytes, .. }) if bytes == [0, 255])
    );
    assert!(matches!(parse_line(r#"{"type":"usage","at_ns":2,"owner":0,"usage":"Usage { input_tokens: 3, output_tokens: 2, cache_read_tokens: 1, cache_write_tokens: 0 }"}"#).expect("usage"), Some(Observation::Usage { .. })));
    assert!(
        matches!(parse_line(r#"{"type":"completion","at_ns":2,"owner":0,"text":"done"}"#).expect("text"), Some(Observation::Completion { text, .. }) if text == "done")
    );
    let observer = observed(COMPLETED).0;
    assert_eq!(observer.classify(Exit::Signal(9), Forced::Killed).end, End::Completed);
    assert_eq!(observer.classify(Exit::Signal(9), Forced::Killed).warnings.len(), 1);
    assert!(matches!(observer.classify(Exit::Code(1), Forced::No).end, End::HarnessError { .. }));
    assert_eq!(Observer::default().classify(Exit::Signal(9), Forced::Killed).end, End::Timeout);
    assert!(matches!(Observer::default().classify(Exit::Code(1), Forced::No).end, End::Failed { .. }));
}

#[test]
fn message_facts_keep_opaque_names_and_fences_without_changing_run_classification() {
    // Explicit synthetic renderings of the current run fact variants; the frozen
    // baseline recordings remain untouched.
    for text in [
        "MessageReceived { run: Token(1), name: Token(99), bytes: 18 }",
        "MessageRead { run: Token(1), name: Token(99), turn: 1 }",
        "MessageFence { run: Token(1), turn: 1, read: Some(Token(99)) }",
        "MessageReceived { run: Token(1), name: Token(0), bytes: 16 }",
        "MessageUnread { run: Token(1), name: Token(0) }",
        "MessageFence { run: Token(1), turn: 2, read: None }",
        "MessageRefused { run: Token(1), name: Token(7), bytes: 4194304, reason: TooLarge }",
        "MessageRefused { run: Token(1), name: Token(7), bytes: 16, reason: Full }",
        "MessageRefused { run: Token(1), name: Token(7), bytes: 16, reason: NameInUse }",
        "MessageRefused { run: Token(1), name: Token(7), bytes: 16, reason: Ending }",
    ] {
        let mut observer = observed(COMPLETED).0;
        let event = observer.observe_line(&fact(&format!("Run {{ fact: {text} }}"))).expect("known message fact");
        assert!(matches!(event, Some(Observation::Fact { event: Event::Other { domain, value }, .. })
            if domain == "Run" && value == parse_debug(text).expect("exact neutral payload")));
        assert_eq!(observer.classify(Exit::Code(0), Forced::No).end, End::Completed);
    }
}

#[test]
fn malformed_message_facts_poison_the_legacy_observer() {
    for text in [
        "MessageReceived { run: Token(1), name: 0, bytes: 16 }",
        "MessageReceived { run: Token(1), name: Token(0), bytes: Many }",
        "MessageReceived { run: Token(1), name: Token(0), bytes: 16, text: 4 }",
        "MessageRead { run: Token(1), name: Token(0), turn: 0 }",
        "MessageRead { run: Token(1), name: Token(0), turn: 4294967296 }",
        "MessageUnread { run: Token(1) }",
        "MessageFence { run: Token(1), turn: 1, read: Some(0) }",
        "MessageFence { run: Token(1), turn: 1, read: Some(Token(0), Token(1)) }",
        "MessageRefused { run: Token(1), name: Token(0), bytes: 16, reason: Future }",
        "MessageRefused { run: Token(1), name: Token(0), bytes: 16, reason: Full(0) }",
        "MessageLost { run: Token(1), name: Token(0) }",
    ] {
        let mut observer = observed(COMPLETED).0;
        assert!(observer.observe_line(&fact(&format!("Run {{ fact: {text} }}"))).is_err(), "{text}");
        assert!(matches!(observer.classify(Exit::Code(0), Forced::No).end, End::HarnessError { .. }));
    }
}
