//! Smith's production schema and decoder, exercised independently of the
//! scripted provider's finite fixture language.

use smith_domain::{llm, run, tools};
use smith_protocol_llm::{self as adapter, Limits, ToolKind};

fn limits() -> Limits {
    Limits { client: skein_llm_world::limits(), tool_bytes: 32768, result_bytes: 32768 }
}

fn grants() -> tools::Grants {
    tools::Grants { inspect: true, modify: true, shell: true }
}

fn prompt() -> llm::Prompt {
    llm::Prompt {
        endpoint: llm::Endpoint(1),
        model: b"test".as_slice().into(),
        system: Box::new([]),
        tools: grants(),
        served: Box::new([llm::Served::Wait, llm::Served::SubAgent]),
        messages: Box::new([]),
        max_tokens: 16,
    }
}

#[test]
fn production_schemas_cover_every_offered_workspace_and_run_tool() {
    let schemas = adapter::schemas(&prompt());
    assert_eq!(schemas.len(), 8);
    for kind in [
        ToolKind::Owned(tools::Tool::Read),
        ToolKind::Owned(tools::Tool::List),
        ToolKind::Owned(tools::Tool::Search),
        ToolKind::Owned(tools::Tool::Write),
        ToolKind::Owned(tools::Tool::Edit),
        ToolKind::Owned(tools::Tool::Shell),
        ToolKind::Wait,
        ToolKind::SubAgent,
    ] {
        let schema = schemas.iter().find(|schema| schema.kind == kind).expect("offered schema");
        assert!(schema.description.len() > 20);
        let document = skein_llm::Json::from_bytes(&schema.schema, &limits().client.dialect)
            .expect("production schema is bounded valid JSON");
        assert_eq!(document.as_tokens().first(), Some(&skein_json::Token::ObjectStart));
    }
}

#[test]
fn each_workspace_call_decodes_escaped_input_and_ignores_unknown_members() {
    let limits = limits();
    let cases = [
        (
            b"read".as_slice(),
            br#"{"path":"src/a.rs","first_line":3,"lines":3,"future":[1,{"x":true}]}"#.as_slice(),
            tools::Tool::Read,
        ),
        (b"list", br#"{"path":".","future":false}"#, tools::Tool::List),
        (b"search", br#"{"pattern":"a\\nb","glob":"*.rs"}"#, tools::Tool::Search),
        (b"write", br#"{"path":"a","content":"line\nnext"}"#, tools::Tool::Write),
        (b"edit", br#"{"path":"a","old":"before","new":"after","all":true}"#, tools::Tool::Edit),
        (b"shell", br#"{"command":"printf ok","timeout":5}"#, tools::Tool::Shell),
    ];
    for (name, input, expected) in cases {
        let decoded = adapter::decode(name, input, grants(), &[], &limits);
        match decoded {
            llm::Decoded::Owned { call } => {
                assert_eq!(tools::tool(&call), expected, "{}", String::from_utf8_lossy(name));
            }
            other @ (llm::Decoded::Served { .. } | llm::Decoded::Invalid { .. }) => {
                panic!("{}: {other:?}", String::from_utf8_lossy(name));
            }
        }
    }
}

#[test]
fn malformed_workspace_calls_have_field_named_typed_problems_then_correct() {
    let limits = limits();
    let cases = [
        (b"read".as_slice(), br#"{"path":12}"#.as_slice(), b"path".as_slice()),
        (b"list", br"{}", b"path"),
        (b"search", br#"{"pattern":false}"#, b"pattern"),
        (b"write", br#"{"path":"a"}"#, b"content"),
        (b"edit", br#"{"path":"a","old":"x"}"#, b"new"),
        (b"shell", br#"{"command":"x","timeout":-1}"#, b"timeout"),
    ];
    for (name, input, field) in cases {
        let decoded = adapter::decode(name, input, grants(), &[], &limits);
        match decoded {
            llm::Decoded::Invalid {
                problem:
                    llm::Problem::Missing { field: found }
                    | llm::Problem::WrongType { field: found }
                    | llm::Problem::BadValue { field: found },
            } => assert_eq!(found.as_ref(), field),
            other @ (llm::Decoded::Owned { .. } | llm::Decoded::Served { .. } | llm::Decoded::Invalid { .. }) => {
                panic!("{}: {other:?}", String::from_utf8_lossy(name));
            }
        }
    }
    assert!(matches!(
        adapter::decode(b"read", b"{bad}", grants(), &[], &limits),
        llm::Decoded::Invalid { problem: llm::Problem::NotAnObject }
    ));
    assert!(matches!(
        adapter::decode(b"unknown", b"{}", grants(), &[], &limits),
        llm::Decoded::Invalid { problem: llm::Problem::UnknownTool }
    ));
}

#[test]
fn wait_and_sub_agent_decode_into_run_asks() {
    let limits = limits();
    let served = [llm::Served::Wait, llm::Served::SubAgent];
    assert!(matches!(
        adapter::decode(b"wait", b"{}", grants(), &served, &limits),
        llm::Decoded::Served { ask: run::Ask::Wait }
    ));
    let decoded = adapter::decode(
        b"sub_agent",
        br#"{"brief":"look","tools":["inspect","shell"],"agents":true,"llm":"small"}"#,
        grants(),
        &served,
        &limits,
    );
    match decoded {
        llm::Decoded::Served { ask: run::Ask::SubAgent { brief, families, llm, share: None } } => {
            assert_eq!(brief.as_ref(), b"look");
            assert!(families.tools.inspect && families.tools.shell && !families.tools.modify && families.agents);
            assert_eq!(llm.as_deref(), Some(b"small".as_slice()));
        }
        other @ (llm::Decoded::Owned { .. } | llm::Decoded::Served { .. } | llm::Decoded::Invalid { .. }) => {
            panic!("{other:?}");
        }
    }
}

#[test]
fn sub_agent_own_turn_limit_is_optional_positive_and_checked_before_dispatch() {
    let served = [llm::Served::SubAgent];
    for (input, expected) in [
        (br#"{"brief":"look","tools":["inspect"]}"#.as_slice(), None),
        (br#"{"brief":"look","tools":["inspect"],"max_turns":1}"#, Some(1)),
        (br#"{"brief":"look","tools":["inspect"],"max_turns":32}"#, Some(32)),
        (br#"{"brief":"look","tools":["inspect"],"max_turns":4294967295}"#, Some(u32::MAX)),
    ] {
        let decoded = adapter::decode(b"sub_agent", input, grants(), &served, &limits());
        let llm::Decoded::Served { ask: run::Ask::SubAgent { share, .. } } = decoded else {
            panic!("valid own quota must become a bounded run ask: {decoded:?}");
        };
        assert_eq!(share, expected.map(|turns| run::Share { turns, spend: u64::MAX }));
    }
    for invalid in ["0", "-1", "1.5", "4294967296", "null", "true", "\"2\""] {
        let input = format!(r#"{{"brief":"look","tools":["inspect"],"max_turns":{invalid}}}"#);
        match adapter::decode(b"sub_agent", input.as_bytes(), grants(), &served, &limits()) {
            llm::Decoded::Invalid { problem: llm::Problem::BadValue { field } | llm::Problem::WrongType { field } } => {
                assert_eq!(field.as_ref(), b"max_turns");
            }
            other @ (llm::Decoded::Owned { .. } | llm::Decoded::Served { .. } | llm::Decoded::Invalid { .. }) => {
                panic!("invalid quota must refuse with its field before dispatch: {invalid}: {other:?}");
            }
        }
    }
}
