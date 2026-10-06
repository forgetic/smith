//! Positive-first adapter boundary controls over actual shared Client terminals.
//! Caller schemas, restored results and a literal Wait attestation are fixture
//! data; no application decoder or Finish policy lives in this test.
//! Contract: scratch/client.md, sections 1–6; testing-strategy.md, sections 2.5 and 6.

use skein_fake_llm_domain::{self as fake, api};
use skein_lib::{Env, Queue, Time, Token, Wall};
use skein_llm::{self as shared, client};
use skein_llm_world::fake::Exchange;
use smith_agent_world::wire::{self, Configuration};
use smith_domain::{Event, llm, run, tools};
use smith_protocol_llm::{
    self as adapter, Context, Input, Limits, Receiving, ResolvedCall, ResultText, ToolKind, ToolSchema,
};

const OWNER: Token = Token::new(71);
const OTHER: Token = Token::new(72);
const WAIT_SCHEMA: &[u8] = br#"{"type":"object","properties":{},"additionalProperties":false}"#;

fn limits() -> Limits {
    Limits { client: skein_llm_world::limits(), tool_bytes: 32_768, result_bytes: 32_768 }
}

fn prompt() -> llm::Prompt {
    llm::Prompt {
        endpoint: llm::Endpoint(1),
        model: b"fixture-model".as_slice().into(),
        system: b"@adapter-boundary".as_slice().into(),
        tools: tools::Grants { inspect: false, modify: false, shell: false },
        served: Box::new([llm::Served::Wait]),
        messages: Box::new([llm::Message {
            role: llm::Role::User,
            content: Box::new([llm::Block::Text { text: b"Start.".as_slice().into(), replay: None }]),
        }]),
        max_tokens: 128,
    }
}

fn schema(kind: ToolKind, name: &[u8]) -> ToolSchema {
    ToolSchema {
        kind,
        name: name.into(),
        description: b"Boundary caller declaration.".as_slice().into(),
        schema: WAIT_SCHEMA.into(),
    }
}

fn input(configuration: &Configuration) -> Input {
    let bounds = limits();
    let decoded_call_bytes = 4096;
    Input {
        owner: OWNER,
        prompt: prompt(),
        endpoint_name: llm::Endpoint(1),
        endpoint: configuration.endpoint.clone(),
        credential: shared::Credential {
            access_token: configuration.credential.access_token.clone(),
            account_id: configuration.credential.account_id.clone(),
        },
        application: Box::new([schema(ToolKind::Wait, b"wait")]),
        results: Box::new([]),
        receiving: Receiving {
            max_completion_bytes: adapter::completion_worst_case(&bounds.client, decoded_call_bytes)
                .expect("compatible full completion receiving allowance"),
            max_completion_blocks: bounds.client.dialect.parts,
            decoded_call_bytes,
            max_failure_bytes: bounds.client.dialect.detail_bytes,
        },
    }
}

fn scripts(calls: bool) -> Box<[api::Script]> {
    let lines: Box<[api::Line]> = if calls {
        Box::new([
            api::Line::Text { text: b"before".as_slice().into() },
            api::Line::Call { name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() },
            api::Line::Text { text: b"after".as_slice().into() },
        ])
    } else {
        Box::new([api::Line::Text { text: b"admitted exact".as_slice().into() }])
    };
    Box::new([api::Script {
        cue: b"@adapter-boundary".as_slice().into(),
        turns: Box::new([api::Turn {
            lines,
            finish: if calls { api::Finish::ToolCalls } else { api::Finish::Stop },
            tokens: 7,
        }]),
    }])
}

fn adopt(input: Input, calls: bool) -> (Context, Exchange) {
    let endpoint = input.endpoint.clone();
    let credential = shared::Credential {
        access_token: input.credential.access_token.clone(),
        account_id: input.credential.account_id.clone(),
    };
    let adapter::Prepared { client, context } = adapter::prepare(input, &limits()).expect("positive admission");
    let peer = Exchange::prepared(client, endpoint, credential, limits().client, scripts(calls));
    assert!(peer.queries.is_empty(), "preparation starts no wire request");
    assert!(peer.requests.is_empty());
    (context, peer)
}

fn quiesce(peer: &mut Exchange) {
    for _ in 0..100_000_u32 {
        if !peer.tick(true) {
            return;
        }
    }
    panic!("bounded actual exchange did not quiesce");
}

fn take_terminal(peer: &mut Exchange) -> client::Event {
    let positions: Vec<_> = peer
        .seen
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            client::Event::Completed { .. } | client::Event::Failed { .. } | client::Event::Cancelled { .. } => {
                Some(index)
            }
            client::Event::Delta { .. }
            | client::Event::Block { .. }
            | client::Event::Reusable
            | client::Event::Close
            | client::Event::Closed => None,
        })
        .collect();
    let [position] = positions.as_slice() else {
        panic!("one actual native terminal: {:?}", peer.seen);
    };
    peer.seen.remove(*position)
}

fn close_won(peer: &mut Exchange) {
    peer.request(client::Request::Close);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Close)).count(), 1);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 0);
    peer.settle();
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 1);
    assert!(peer.seen.iter().all(|event| !matches!(
        event,
        client::Event::Completed { .. } | client::Event::Failed { .. } | client::Event::Cancelled { .. }
    )));
    let events = peer.seen.len();
    peer.settle();
    assert_eq!(peer.seen.len(), events, "repeated actual settlement emits nothing");
    assert_eq!(peer.machine.waiting(), client::Waiting::Nothing);
}

fn completed(configuration: &Configuration, calls: bool) -> (Context, Exchange, Token, shared::Completion) {
    let (context, mut peer) = adopt(input(configuration), calls);
    peer.start();
    peer.run();
    assert_eq!(peer.queries.len(), 1);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Reusable)).count(), 1);
    let client::Event::Completed { owner, completion } = take_terminal(&mut peer) else {
        panic!("actual scripted successful terminal");
    };
    assert_eq!(owner, OWNER);
    (context, peer, owner, completion)
}

#[test]
fn actual_completed_owner_is_preserved_and_a_different_owner_is_refused() {
    for configuration in &wire::configurations() {
        for wrong_owner in [false, true] {
            let (context, mut peer, owner, completion) = completed(configuration, false);
            let returned =
                adapter::completion(context, if wrong_owner { OTHER } else { owner }, completion, Box::new([]));
            if wrong_owner {
                assert!(matches!(returned, Err(adapter::Error::Invalid)));
            } else {
                let Ok(Event::Completed { owner, completion }) = returned else {
                    panic!("the real successful terminal translates once");
                };
                assert_eq!(owner, OWNER);
                assert_eq!(completion.stop, llm::Stop::EndTurn);
                assert_eq!(completion.usage.output_tokens, 7);
                let [llm::Said::Text { text, .. }] = completion.content.as_ref() else {
                    panic!("literal actual text completion");
                };
                assert_eq!(text.as_ref(), b"admitted exact");
            }
            close_won(&mut peer);
        }
    }
}

fn failed(configuration: &Configuration) -> (Context, Exchange, client::Event) {
    let (context, mut peer) = adopt(input(configuration), false);
    peer.manual_replies = true;
    peer.start();
    quiesce(&mut peer);
    assert_eq!(peer.pending.len(), 1, "one actual HTTP request issued a domain reply right");
    let config = fake::Config { overloaded: 1000, ..skein_llm_world::fake::config() };
    let mut domain = fake::Domain::new(&config, 7);
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: config };
    let mut out = Queue::with_capacity(fake::MAX_OUT);
    fake::step(&mut domain, &env, peer.pending.pop().expect("actual independent peer request"), &mut out);
    assert!(out.pop().is_none(), "the actual fake decision awaits its timer entrance");
    fake::fire(&mut domain, &env, &mut out);
    peer.reply(out.pop().expect("actual configured fake overloaded terminal"));
    assert!(out.pop().is_none());
    domain.reclaim();
    assert_eq!(domain.calls(), 0);
    assert_eq!(domain.next_deadline(), None);
    peer.run();
    let terminal = take_terminal(&mut peer);
    assert!(matches!(
        &terminal,
        client::Event::Failed {
            owner,
            failure: shared::Failure::Overloaded,
            evidence: client::Evidence::Response,
            detail
        } if *owner == OWNER && detail.as_ref() == b"overloaded_error"
    ));
    (context, peer, terminal)
}

#[test]
fn actual_failed_owner_class_evidence_and_detail_survive_only_the_matching_context() {
    for configuration in &wire::configurations() {
        for wrong_owner in [false, true] {
            let (context, mut peer, terminal) = failed(configuration);
            let client::Event::Failed { owner, failure, evidence, detail } = terminal else {
                panic!("actual byte-peer failure established above");
            };
            let returned = adapter::failed(context, if wrong_owner { OTHER } else { owner }, failure, evidence, detail);
            if wrong_owner {
                assert!(matches!(returned, Err(adapter::Error::Invalid)));
            } else {
                let Ok(Event::Failed { owner, failure, evidence, detail }) = returned else {
                    panic!("real failure translates once");
                };
                assert_eq!(owner, OWNER);
                assert_eq!(failure, llm::Failure::Overloaded);
                assert_eq!(evidence, llm::Evidence::Response);
                assert_eq!(detail.as_ref(), b"overloaded_error");
            }
            close_won(&mut peer);
        }
    }
}

#[test]
fn actual_cancelled_owner_is_checked_only_after_genuine_lower_settlement() {
    for configuration in &wire::configurations() {
        for wrong_owner in [false, true] {
            let (context, mut peer) = adopt(input(configuration), false);
            peer.start();
            peer.request(client::Request::Cancel);
            assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Close)).count(), 1);
            assert!(!peer.seen.iter().any(|event| matches!(event, client::Event::Cancelled { .. })));
            peer.settle();
            let client::Event::Cancelled { owner } = take_terminal(&mut peer) else {
                panic!("real lower settlement yields the native cancellation terminal");
            };
            assert_eq!(owner, OWNER);
            let returned = adapter::cancelled(context, if wrong_owner { OTHER } else { owner });
            if wrong_owner {
                assert!(matches!(returned, Err(adapter::Error::Invalid)));
            } else {
                assert!(matches!(returned, Ok(Event::Cancelled { owner }) if owner == OWNER));
            }
            assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 1);
            let events = peer.seen.len();
            peer.settle();
            assert_eq!(peer.seen.len(), events, "no second cancellation or close terminal");
            assert_eq!(peer.machine.waiting(), client::Waiting::Nothing);
        }
    }
}

#[test]
fn actual_schema_inventory_admits_first_then_refuses_missing_duplicate_and_unoffered_entries() {
    for configuration in &wire::configurations() {
        let (context, mut peer, owner, completion) = completed(configuration, false);
        let [query] = peer.queries.as_slice() else { panic!("one observed actual query") };
        let [declaration] = query.tools.as_ref() else { panic!("one actual offered application schema") };
        assert_eq!(declaration.name.as_ref(), b"wait");
        assert_eq!(declaration.description.as_ref(), b"Boundary caller declaration.");
        assert_eq!(declaration.parameters.as_ref(), WAIT_SCHEMA);
        assert!(matches!(adapter::completion(context, owner, completion, Box::new([])), Ok(Event::Completed { .. })));
        close_won(&mut peer);

        let mut missing = input(configuration);
        missing.application = Box::new([]);
        assert!(matches!(adapter::prepare(missing, &limits()), Err(adapter::Error::Invalid)));
        let mut duplicate = input(configuration);
        duplicate.application = Box::new([schema(ToolKind::Wait, b"wait"), schema(ToolKind::Wait, b"wait")]);
        assert!(matches!(adapter::prepare(duplicate, &limits()), Err(adapter::Error::Invalid)));
        let mut unoffered = input(configuration);
        unoffered.application =
            Box::new([schema(ToolKind::Wait, b"wait"), schema(ToolKind::Owned(tools::Tool::Shell), b"shell")]);
        assert!(matches!(adapter::prepare(unoffered, &limits()), Err(adapter::Error::Invalid)));
    }
}

fn result_input(configuration: &Configuration) -> Input {
    let mut input = input(configuration);
    input.prompt.tools.shell = true;
    input.application =
        Box::new([schema(ToolKind::Wait, b"wait"), schema(ToolKind::Owned(tools::Tool::Shell), b"shell")]);
    input.prompt.messages = Box::new([
        llm::Message {
            role: llm::Role::Assistant,
            content: Box::new([llm::Block::ToolCall {
                id: b"prior-shell".as_slice().into(),
                name: b"shell".as_slice().into(),
                input: br#"{"command":"echo prior"}"#.as_slice().into(),
                replay: None,
            }]),
        },
        llm::Message {
            role: llm::Role::User,
            content: Box::new([llm::Block::ToolResult {
                id: b"prior-shell".as_slice().into(),
                result: llm::Returned::Owned {
                    outcome: tools::Outcome::Exited {
                        exit: tools::Exit::Code { code: 0 },
                        head: b"prior".as_slice().into(),
                        tail: Box::new([]),
                        dropped: 0,
                    },
                },
            }]),
        },
    ]);
    input.results = Box::new([result_text()]);
    input
}

fn result_text() -> ResultText {
    ResultText {
        message: 1,
        block: 0,
        id: b"prior-shell".as_slice().into(),
        text: b"caller rendered result unchanged".as_slice().into(),
        error: false,
    }
}

#[test]
fn owned_result_renderings_admit_first_then_refuse_missing_duplicate_unused_or_mispaired_entries() {
    for configuration in &wire::configurations() {
        let (context, mut peer) = adopt(result_input(configuration), false);
        peer.start();
        peer.run();
        let [query] = peer.queries.as_slice() else { panic!("one actual restored-result query") };
        let [_, user] = query.messages.as_ref() else { panic!("original ordered caller history") };
        let [api::Part::ToolOutput { id, output, is_error }] = user.parts.as_ref() else {
            panic!("actual owned result crossed the byte peer");
        };
        assert_eq!(id.as_ref(), b"prior-shell");
        assert_eq!(output.as_ref(), b"caller rendered result unchanged");
        assert!(!*is_error);
        let client::Event::Completed { owner, completion } = take_terminal(&mut peer) else {
            panic!("positive actual restored-history completion");
        };
        assert!(matches!(adapter::completion(context, owner, completion, Box::new([])), Ok(Event::Completed { .. })));
        close_won(&mut peer);

        let mut missing = result_input(configuration);
        missing.results = Box::new([]);
        assert!(matches!(adapter::prepare(missing, &limits()), Err(adapter::Error::Invalid)));
        let mut duplicate = result_input(configuration);
        duplicate.results = Box::new([result_text(), result_text()]);
        assert!(matches!(adapter::prepare(duplicate, &limits()), Err(adapter::Error::Invalid)));
        let mut unused = result_input(configuration);
        let mut extra = result_text();
        extra.message = 0;
        unused.results = Box::new([result_text(), extra]);
        assert!(matches!(adapter::prepare(unused, &limits()), Err(adapter::Error::Invalid)));
        let mut mispaired = result_input(configuration);
        mispaired.results[0].id = b"other-result-id".as_slice().into();
        assert!(matches!(adapter::prepare(mispaired, &limits()), Err(adapter::Error::Invalid)));
    }
}

fn resolution() -> ResolvedCall {
    ResolvedCall {
        position: 1,
        name: b"wait".as_slice().into(),
        input: b"{}".as_slice().into(),
        call: llm::Decoded::Served { ask: run::Ask::Wait },
    }
}

#[derive(Clone, Copy, Debug)]
enum Attestation {
    Exact,
    Missing,
    Position,
    Name,
    Input,
    Kind,
    Duplicate,
    Unused,
}

fn resolutions(case: Attestation) -> Box<[ResolvedCall]> {
    let mut exact = resolution();
    match case {
        Attestation::Exact => {}
        Attestation::Missing => return Box::new([]),
        Attestation::Position => exact.position = 0,
        Attestation::Name => exact.name = b"another-name".as_slice().into(),
        Attestation::Input => exact.input = b"{".as_slice().into(),
        Attestation::Kind => {
            exact.call = llm::Decoded::Owned { call: tools::Call::Shell { command: Box::new([]), timeout: None } };
        }
        Attestation::Duplicate => return Box::new([resolution(), resolution()]),
        Attestation::Unused => {
            let mut unused = resolution();
            unused.position = 2;
            return Box::new([exact, unused]);
        }
    }
    Box::new([exact])
}

#[test]
fn actual_call_attestation_checks_position_name_literal_input_kind_uniqueness_and_consumption() {
    for configuration in &wire::configurations() {
        for case in [
            Attestation::Exact,
            Attestation::Missing,
            Attestation::Position,
            Attestation::Name,
            Attestation::Input,
            Attestation::Kind,
            Attestation::Duplicate,
            Attestation::Unused,
        ] {
            let (context, mut peer, owner, completion) = completed(configuration, true);
            let [
                shared::Block::Text { text: first, .. },
                shared::Block::ToolCall { id, name, arguments, .. },
                shared::Block::Text { text: last, .. },
            ] = completion.content.as_ref()
            else {
                panic!("the real native call has its independent text/call/text positions");
            };
            assert_eq!(first.as_ref(), b"before");
            assert_eq!(last.as_ref(), b"after");
            assert_eq!(name.as_ref(), b"wait");
            assert_eq!(arguments.as_ref(), b"{}");
            assert_eq!(id.as_ref(), b"call_0000000000000001");
            let returned = adapter::completion(context, owner, completion, resolutions(case));
            match case {
                Attestation::Exact => {
                    let Ok(Event::Completed { owner, completion }) = returned else {
                        panic!("the exact actual Wait attestation is admitted first");
                    };
                    assert_eq!(owner, OWNER);
                    assert_eq!(completion.stop, llm::Stop::ToolUse);
                    assert_eq!(completion.usage.output_tokens, 7);
                    let [
                        llm::Said::Text { text: first, .. },
                        llm::Said::ToolCall { id, name, input, call, .. },
                        llm::Said::Text { text: last, .. },
                    ] = completion.content.as_ref()
                    else {
                        panic!("adapter preserves original positions and fields");
                    };
                    assert_eq!(first.as_ref(), b"before");
                    assert_eq!(last.as_ref(), b"after");
                    assert_eq!(id.as_ref(), b"call_0000000000000001");
                    assert_eq!(name.as_ref(), b"wait");
                    assert_eq!(input.as_ref(), b"{}");
                    assert!(matches!(call, llm::Decoded::Served { ask: run::Ask::Wait }));
                }
                Attestation::Missing
                | Attestation::Position
                | Attestation::Name
                | Attestation::Input
                | Attestation::Kind
                | Attestation::Duplicate
                | Attestation::Unused => {
                    assert!(matches!(returned, Err(adapter::Error::Invalid)), "bad attestation {case:?}");
                }
            }
            close_won(&mut peer);
        }
    }
}
