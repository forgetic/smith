//! Production rendering of typed workspace outcomes in live and saved prompts.

use smith_domain::{llm, tools};
use smith_protocol_llm_world::adapter::{self as adapter, Limits};

fn limits() -> Limits {
    Limits {
        client: skein_llm_world::limits(),
        tool_bytes: 32768,
        rendered_result: skein_llm_world::limits().dialect.string_bytes,
    }
}

#[test]
fn workspace_successes_render_the_retained_content_and_counts() {
    let cases = [
        (
            tools::Outcome::Read {
                content: b"alpha\nbeta\n".as_slice().into(),
                skipped: 2,
                lines: 2,
                total: 9,
                cut: false,
            },
            b"3: alpha\n4: beta\n(2 of 9 lines; skipped 2)".as_slice(),
        ),
        (
            tools::Outcome::Listed {
                entries: Box::new([tools::Entry {
                    name: tools::Name::new(b"src".as_slice().into()).expect("valid entry"),
                    kind: tools::Kind::Directory,
                }]),
                more: 3,
            },
            b"src\tdirectory\n... 3 more entries",
        ),
        (
            tools::Outcome::Found {
                hits: Box::new([tools::Hit {
                    path: b"a.rs".as_slice().into(),
                    line: 5,
                    text: b"found".as_slice().into(),
                }]),
                more: 2,
                timed_out: false,
            },
            b"a.rs:5: found\n... 2 more matches\n",
        ),
        (tools::Outcome::Written { created: true }, b"file created"),
        (tools::Outcome::Edited { replaced: 2 }, b"replaced 2 occurrence(s)"),
        (
            tools::Outcome::Exited {
                exit: tools::Exit::Code { code: 0 },
                head: b"first".as_slice().into(),
                tail: b"last".as_slice().into(),
                dropped: 42,
            },
            b"exit code 0\nfirst\n... 42 bytes dropped ...\nlast",
        ),
    ];
    for (outcome, expected) in cases {
        let (text, error) = adapter::render_outcome(&outcome, 4096).expect("bounded rendering");
        assert_eq!(text.as_ref(), expected);
        assert!(!error);
    }
}

#[test]
fn workspace_failures_and_invalid_bytes_stay_visible_and_bounded() {
    let failures = [
        tools::Outcome::NotGranted,
        tools::Outcome::Outside,
        tools::Outcome::ReadOnly,
        tools::Outcome::TooLong,
        tools::Outcome::NotFound,
        tools::Outcome::NotFile,
        tools::Outcome::Linked,
        tools::Outcome::Protected,
        tools::Outcome::NotDirectory,
        tools::Outcome::TooLarge { size: 10_000 },
        tools::Outcome::NotRead,
        tools::Outcome::Stale,
        tools::Outcome::NoMatch,
        tools::Outcome::Ambiguous { count: 2, lines: Box::new([3, 7]) },
        tools::Outcome::Unchanged,
        tools::Outcome::Failed { fault: tools::Fault::Denied },
        tools::Outcome::Failed { fault: tools::Fault::NoSpace },
        tools::Outcome::Failed { fault: tools::Fault::Other },
        tools::Outcome::TimedOut,
        tools::Outcome::Cancelled,
        tools::Outcome::Busy,
        tools::Outcome::NulByte,
        tools::Outcome::Exited {
            exit: tools::Exit::Code { code: 3 },
            head: b"failure\xff".as_slice().into(),
            tail: Box::new([]),
            dropped: 0,
        },
    ];
    for outcome in failures {
        let (text, error) = adapter::render_outcome(&outcome, 4096).expect("bounded failure rendering");
        assert!(error, "{outcome:?}");
        assert!(!text.is_empty());
        assert!(text.iter().all(u8::is_ascii), "{text:?}");
    }
}

#[test]
fn the_same_typed_result_renders_after_a_saved_turn_is_replayed() {
    let outcome =
        tools::Outcome::Read { content: b"saved line\n".as_slice().into(), skipped: 4, lines: 1, total: 6, cut: false };
    let (live_text, live_error) = adapter::render_outcome(&outcome, 4096).expect("live feedback");
    let history = llm::Prompt {
        endpoint: llm::Endpoint(1),
        model: b"model".as_slice().into(),
        system: Box::new([]),
        tools: tools::Grants { inspect: false, modify: false, shell: false },
        served: Box::new([]),
        messages: Box::new([llm::Message {
            role: llm::Role::User,
            content: Box::new([llm::Block::ToolResult {
                id: b"saved-id".as_slice().into(),
                result: llm::Returned::Owned { outcome },
            }]),
        }]),
        max_tokens: 32,
    };
    let translated = adapter::prompt(history, &[], &limits()).expect("saved typed result translated");
    let [message] = translated.messages.as_ref() else { panic!("one saved message") };
    let [skein_llm::Block::ToolResult { id, text, is_error }] = message.content.as_ref() else {
        panic!("one saved result");
    };
    assert_eq!(id.as_ref(), b"saved-id");
    assert_eq!(text, &live_text);
    assert_eq!(*is_error, live_error);
}

#[test]
fn written_text_and_mechanical_feedback_keep_their_error_flags() {
    let history = llm::Prompt {
        endpoint: llm::Endpoint(1),
        model: b"model".as_slice().into(),
        system: Box::new([]),
        tools: tools::Grants { inspect: false, modify: false, shell: false },
        served: Box::new([]),
        messages: Box::new([llm::Message {
            role: llm::Role::User,
            content: Box::new([
                llm::Block::ToolResult {
                    id: b"text".as_slice().into(),
                    result: llm::Returned::Text { text: b"domain words".as_slice().into(), error: false, replay: None },
                },
                llm::Block::ToolResult {
                    id: b"invalid".as_slice().into(),
                    result: llm::Returned::Invalid {
                        problem: llm::Problem::Missing { field: b"path".as_slice().into() },
                    },
                },
                llm::Block::ToolResult { id: b"not-run".as_slice().into(), result: llm::Returned::NotRun },
                llm::Block::ToolResult { id: b"withdrawn".as_slice().into(), result: llm::Returned::Withdrawn },
            ]),
        }]),
        max_tokens: 32,
    };
    let translated = adapter::prompt(history, &[], &limits()).expect("bounded feedback translated");
    let [message] = translated.messages.as_ref() else { panic!("one message") };
    let [first, second, third, fourth] = message.content.as_ref() else { panic!("four result blocks") };
    let skein_llm::Block::ToolResult { text, is_error, .. } = first else { panic!("text result") };
    assert_eq!(text.as_ref(), b"domain words");
    assert!(!is_error);
    for (block, expected) in
        [(second, b"missing field \"path\"".as_slice()), (third, b"not run"), (fourth, b"withdrawn")]
    {
        let skein_llm::Block::ToolResult { text, is_error, .. } = block else { panic!("error result") };
        assert_eq!(text.as_ref(), expected);
        assert!(*is_error);
    }
}
