//! Actual prepared Clients retain native replay, usage and failure evidence.
//! Synthetic handwritten HTTP/SSE fixtures are independent of the peer encoder.
//! Restored no-effect history is a caller/adapter handoff, not a root restore
//! or combined memory proof. The separate malformed-call story drives original
//! root discovery, byte peers, one corrected host effect and cancellation.
//! Contract: scratch/client.md, sections 1–7; domain/run.md, sections 5.2,
//! 10, 13 and 14; testing-strategy.md, sections 2.3, 2.4, 4.1 and 6.

use skein_fake_llm_domain::api::{Finish, Line, Part, Query, Script, Turn};
use skein_lib::{Duration, Time, Token, Wall};
use skein_llm::{self as shared, client};
use skein_llm_world::{World as RawWorld, events, response, text_response};
use skein_world::domain::Span;
use smith_domain::{Event, llm, run, session::llm as recorded, tools};
use smith_protocol_llm_world::adapter::{self as adapter, Context, Input, Limits, Receiving};
use smith_protocol_llm_world::{
    Job, Settings, World,
    wire::{self, Configuration, Observed},
};

const CODEX_REASONING: &[u8] = br#"{"id":"rs_c","type":"reasoning","encrypted_content":"exact-opaque","summary":[],"future":{"proof":[1,true,null]}}"#;
const CODEX_TEXT_REPLAY: &[u8] = br#"{"id":"msg_c","phase":"commentary"}"#;
const CODEX_REFUSAL_REPLAY: &[u8] = br#"{"id":"msg_r"}"#;
// The native stream retains unknown head fields before its assembled final
// thinking/signature values. This independent literal keeps that documented
// order and every extension; no encoder constructs the expected value.
const THINKING: &[u8] =
    br#"{"type":"thinking","future":{"proof":[1,true,null]},"thinking":"Plan carefully","signature":"signed-opaque"}"#;
const REDACTED: &[u8] = br#"{"type":"redacted_thinking","data":"hidden-opaque"}"#;
const ROOT_REPLAY: &[u8] = br#"{"item_id":"item_1_0"}"#;
const RAW: &[u8] = b"{broken\n\"\\";
const BODY: &[u8] = br#"{ "opaque" : {"future":[1,true,null]}, "extra":"unchanged" }"#;
const CALL_ID: &[u8] = b"call_0000000000000001";
const INVALID: &[u8] = b"Error: input is not a complete JSON object";
const HOST_RESULT: &[u8] = b"opaque host answer: first decision";

const CODEX_RESPONSE: &[&str] = &[
    r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"rs_c","type":"reasoning"}}"#,
    r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_c","type":"reasoning","encrypted_content":"exact-opaque","summary":[],"future":{"proof":[1,true,null]}}}"#,
    r#"{"type":"response.output_item.added","output_index":1,"item":{"id":"msg_c","type":"message"}}"#,
    r#"{"type":"response.output_item.done","output_index":1,"item":{"id":"msg_c","type":"message","phase":"commentary","content":[{"type":"output_text","text":"Reading"}]}}"#,
    r#"{"type":"response.output_item.added","output_index":2,"item":{"id":"msg_r","type":"message"}}"#,
    r#"{"type":"response.output_item.done","output_index":2,"item":{"id":"msg_r","type":"message","content":[{"type":"refusal","refusal":"Cannot comply"}]}}"#,
    r#"{"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"content_filter"},"usage":{"input_tokens":12,"output_tokens":3,"input_tokens_details":{"cached_tokens":4}}}}"#,
];

const ANTHROPIC_RESPONSE: &[u8] = br#"event: message_start
data: {"type":"message_start","message":{"id":"msg_a","type":"message","role":"assistant","model":"fixture-model","content":[],"stop_reason":null,"usage":{"input_tokens":7,"cache_read_input_tokens":11,"cache_creation_input_tokens":13,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"Plan carefully","signature":"signed-opaque","future":{"proof":[1,true,null]}}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"hidden-opaque"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"text","text":"Answer"}}

event: content_block_stop
data: {"type":"content_block_stop","index":2}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":9}}

event: message_stop
data: {"type":"message_stop"}

"#;

const CODEX_HISTORY: &[u8] = br#""input":[{"id":"rs_c","type":"reasoning","encrypted_content":"exact-opaque","summary":[],"future":{"proof":[1,true,null]}},{"type":"message","role":"assistant","id":"msg_c","phase":"commentary","content":[{"type":"output_text","text":"Reading"}]},{"type":"message","role":"assistant","id":"msg_r","content":[{"type":"refusal","refusal":"Cannot comply"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"Continue."}]}]"#;
const ANTHROPIC_HISTORY: &[u8] = br#""messages":[{"role":"assistant","content":[{"type":"thinking","future":{"proof":[1,true,null]},"thinking":"Plan carefully","signature":"signed-opaque"},{"type":"redacted_thinking","data":"hidden-opaque"},{"type":"text","text":"Answer"}]},{"role":"user","content":[{"type":"text","text":"Continue."}]}]"#;

fn limits() -> Limits {
    Limits {
        client: skein_llm_world::limits(),
        tool_bytes: 32768,
        rendered_result: skein_llm_world::limits().dialect.string_bytes,
    }
}

fn receiving(bounds: &Limits) -> Receiving {
    Receiving {
        max_completion_bytes: adapter::completion_worst_case(&bounds.client, 4096).expect("full receiving contract"),
        max_completion_blocks: bounds.client.dialect.parts,
        decoded_call_bytes: 4096,
        max_failure_bytes: bounds.client.dialect.detail_bytes,
    }
}

fn prompt(messages: Box<[llm::Message]>) -> llm::Prompt {
    llm::Prompt {
        endpoint: llm::Endpoint(1),
        model: b"fixture-model".as_slice().into(),
        system: b"Literal continuation control.".as_slice().into(),
        tools: tools::Grants { inspect: false, modify: false, shell: false },
        served: Box::new([]),
        messages,
        max_tokens: 128,
    }
}

fn user(text: &[u8]) -> llm::Message {
    llm::Message { role: llm::Role::User, content: Box::new([llm::Block::Text { text: text.into(), replay: None }]) }
}

fn adopt(
    configuration: &Configuration,
    owner: Token,
    messages: Box<[llm::Message]>,
    source: Vec<u8>,
) -> (Context, RawWorld) {
    let bounds = limits();
    let adapter::Prepared { client, context } = adapter::prepare(
        Input {
            owner,
            prompt: prompt(messages),
            endpoint_name: llm::Endpoint(1),
            endpoint: configuration.endpoint.clone(),
            credential: shared::Credential {
                access_token: configuration.credential.access_token.clone(),
                account_id: configuration.credential.account_id.clone(),
            },
            application: Box::new([]),
            receiving: receiving(&bounds),
        },
        &bounds,
    )
    .expect("actual adapter prepares exactly one Client");
    let mut peer = RawWorld::prepared(client, bounds.client, source, 71);
    peer.env.now = Time::from_nanos(3000);
    peer.env.wall = Wall::from_nanos(9000);
    peer.fragmentation(1, 1);
    assert!(peer.seen.is_empty() && peer.sent.is_empty());
    (context, peer)
}

fn take_terminal(peer: &mut RawWorld) -> client::Event {
    peer.assert_once();
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
    let [position] = positions.as_slice() else { panic!("one actual terminal") };
    peer.seen.remove(*position)
}

fn completed(context: Context, peer: &mut RawWorld, expected_owner: Token) -> llm::Completion {
    peer.request(client::Request::Start);
    peer.run();
    let client::Event::Completed { owner, completion } = take_terminal(peer) else {
        panic!("actual native completion")
    };
    assert_eq!(owner, expected_owner);
    let Event::Completed { owner, completion } =
        adapter::completion(context, owner, completion, Box::new([])).expect("actual owned terminal translates")
    else {
        panic!("completed translation")
    };
    assert_eq!(owner, expected_owner);
    completion
}

fn close(peer: &mut RawWorld) {
    assert_eq!(peer.machine.waiting(), client::Waiting::Idle);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Reusable)).count(), 1);
    peer.request(client::Request::Close);
    assert_eq!(peer.machine.waiting(), client::Waiting::Closing);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Close)).count(), 1);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 0);
    peer.settle();
    assert_eq!(peer.machine.waiting(), client::Waiting::Nothing);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 1);
    let settled = peer.seen.len();
    peer.settle();
    assert_eq!(peer.seen.len(), settled);
    assert_eq!(peer.terminals(), 0, "consumed terminal is never emitted a second time");
}

fn envelope(bytes: &[u8], header: [u8; 7], payload: &[u8]) -> bool {
    bytes.get(..7) == Some(header.as_slice()) && bytes.get(7..) == Some(payload)
}

fn usage(actual: llm::Usage, expected: [u64; 4]) {
    let observed = [actual.input_tokens, actual.output_tokens, actual.cache_read_tokens, actual.cache_write_tokens];
    assert_eq!(observed, expected, "all four actual translated usage fields");
    for index in 0..4 {
        let mut changed = observed;
        changed[index] += 1;
        assert_ne!(changed, expected, "usage field {index} participates in the outside oracle");
    }
}

fn codex_evidence(completion: &llm::Completion) -> bool {
    let [
        llm::Said::Opaque { bytes },
        llm::Said::Text { text, replay: Some(text_replay) },
        llm::Said::Refusal { text: refusal, replay: Some(refusal_replay) },
    ] = completion.content.as_ref()
    else {
        return false;
    };
    completion.stop == llm::Stop::Refusal
        && envelope(bytes, *b"\x00\x01\x01\x00\x00\x00\x71", CODEX_REASONING)
        && text.as_ref() == b"Reading"
        && envelope(&text_replay.bytes, *b"\x00\x01\x01\x00\x00\x00\x23", CODEX_TEXT_REPLAY)
        && refusal.as_ref() == b"Cannot comply"
        && envelope(&refusal_replay.bytes, *b"\x00\x01\x01\x00\x00\x00\x0e", CODEX_REFUSAL_REPLAY)
}

fn anthropic_evidence(completion: &llm::Completion) -> bool {
    let [llm::Said::Opaque { bytes }, llm::Said::Opaque { bytes: redacted }, llm::Said::Text { text, replay: None }] =
        completion.content.as_ref()
    else {
        return false;
    };
    completion.stop == llm::Stop::EndTurn
        && envelope(bytes, *b"\x00\x01\x02\x00\x00\x00\x6c", THINKING)
        && envelope(redacted, *b"\x00\x01\x02\x00\x00\x00\x33", REDACTED)
        && text.as_ref() == b"Answer"
}

// Mechanical transfer of actual no-effect data into caller-owned continuation
// history. This fixture supplies no application classification or root decoder.
fn history(completion: llm::Completion) -> Box<[llm::Message]> {
    let blocks: Vec<_> = completion
        .content
        .into_iter()
        .map(|said| match said {
            llm::Said::Opaque { bytes } => llm::Block::Opaque { bytes },
            llm::Said::Text { text, replay } => llm::Block::Text { text, replay },
            llm::Said::Refusal { text, replay } => llm::Block::Refusal { text, replay },
            llm::Said::ToolCall { .. } => panic!("this history handoff grants no tool effects"),
        })
        .collect();
    Box::new([llm::Message { role: llm::Role::Assistant, content: blocks.into() }, user(b"Continue.")])
}

fn native_body(peer: &RawWorld) -> &[u8] {
    let at = peer.sent.windows(4).position(|bytes| bytes == b"\r\n\r\n").expect("actual HTTP request") + 4;
    &peer.sent[at..]
}

fn native_evidence(bytes: &[u8], expected: &[u8]) -> bool {
    bytes.windows(expected.len()).filter(|part| *part == expected).count() == 1
}

fn native_corruptions(peer: &RawWorld, expected: &[u8], markers: &[&[u8]]) {
    let body = native_body(peer);
    assert!(native_evidence(body, expected), "whole literal continued history: {body:?}");
    for marker in markers {
        let at = body.windows(marker.len()).position(|part| part == *marker).expect("positive literal field");
        let mut changed = body.to_vec();
        changed[at] = b'X';
        assert!(!native_evidence(&changed, expected), "changed literal {marker:?}");
    }
    assert!(!native_evidence(b"{}", expected));
    let mut duplicated = body.to_vec();
    duplicated.extend_from_slice(expected);
    assert!(!native_evidence(&duplicated, expected), "duplicate complete history is not the outside expectation");
}

#[test]
fn codex_reasoning_refusal_and_message_identity_continue_exactly() {
    let configuration = wire::configurations().into_iter().next().expect("literal Codex fixture");
    let source = response(200, "Content-Type: text/event-stream\r\n", &events(CODEX_RESPONSE), true);
    let (context, mut peer) = adopt(&configuration, Token::new(0), Box::new([user(b"Start.")]), source);
    let mut completion = completed(context, &mut peer, Token::new(0));
    assert!(codex_evidence(&completion), "actual translated ordered replay/refusal: {completion:?}");
    usage(completion.usage, [8, 3, 4, 0]);
    completion.content.swap(0, 1);
    assert!(!codex_evidence(&completion));
    completion.content.swap(0, 1);
    let llm::Said::Refusal { replay, .. } = &mut completion.content[2] else { panic!("actual refusal") };
    let retained = replay.take();
    assert!(!codex_evidence(&completion));
    let llm::Said::Refusal { replay, .. } = &mut completion.content[2] else { panic!("actual refusal") };
    *replay = retained;
    assert!(codex_evidence(&completion));
    close(&mut peer);
    let (context, mut next) = adopt(&configuration, Token::new(99), history(completion), text_response(false));
    let continued = completed(context, &mut next, Token::new(99));
    assert_eq!(continued.stop, llm::Stop::EndTurn);
    native_corruptions(
        &next,
        CODEX_HISTORY,
        &[b"exact-opaque", b"proof", b"msg_c", b"commentary", b"msg_r", b"refusal", b"Cannot comply"],
    );
    close(&mut next);
}

#[test]
fn anthropic_all_usage_signed_and_redacted_replay_continue_exactly() {
    let configuration = wire::configurations().into_iter().nth(1).expect("literal Anthropic fixture");
    let source = response(200, "Content-Type: text/event-stream\r\n", ANTHROPIC_RESPONSE, true);
    let (context, mut peer) = adopt(&configuration, Token::new(99), Box::new([user(b"Start.")]), source.clone());
    let mut completion = completed(context, &mut peer, Token::new(99));
    assert!(anthropic_evidence(&completion), "actual translated ordered thinking: {completion:?}");
    usage(completion.usage, [7, 9, 11, 13]);
    completion.content.swap(0, 1);
    assert!(!anthropic_evidence(&completion));
    completion.content.swap(0, 1);
    let llm::Said::Opaque { bytes } = &mut completion.content[0] else { panic!("actual thinking") };
    bytes[2] = 1;
    assert!(!anthropic_evidence(&completion));
    let llm::Said::Opaque { bytes } = &mut completion.content[0] else { panic!("actual thinking") };
    bytes[2] = 2;
    assert!(anthropic_evidence(&completion));
    close(&mut peer);
    let (context, mut next) = adopt(&configuration, Token::new(7), history(completion), source);
    let continued = completed(context, &mut next, Token::new(7));
    assert!(anthropic_evidence(&continued));
    usage(continued.usage, [7, 9, 11, 13]);
    native_corruptions(
        &next,
        ANTHROPIC_HISTORY,
        &[b"Plan carefully", b"signed-opaque", b"proof", b"redacted_thinking", b"hidden-opaque", b"Answer"],
    );
    close(&mut next);
}

fn correction_script() -> Box<[Script]> {
    Box::new([Script {
        cue: b"@hosttools".as_slice().into(),
        turns: Box::new([
            Turn {
                lines: Box::new([Line::Call { name: b"host_action".as_slice().into(), arguments: RAW.into() }]),
                finish: Finish::ToolCalls,
                tokens: 8,
            },
            Turn {
                lines: Box::new([Line::Call { name: b"host_action".as_slice().into(), arguments: BODY.into() }]),
                finish: Finish::ToolCalls,
                tokens: 8,
            },
            Turn {
                lines: Box::new([Line::Text { text: b"Corrected continuation.".as_slice().into() }]),
                finish: Finish::Stop,
                tokens: 4,
            },
        ]),
    }])
}

fn pair(assistant: &[Part], user: &[Part], arguments: &[u8], feedback: &[u8]) -> bool {
    let [Part::ToolCall { id, name, arguments: actual }] = assistant else { return false };
    let [Part::ToolOutput { id: returned, output, is_error }] = user else { return false };
    id.as_ref() == CALL_ID
        && returned.as_ref() == CALL_ID
        && name.as_ref() == b"host_action"
        && actual.as_ref() == arguments
        && output.as_ref() == feedback
        && !is_error
}

fn invalid_feedback(query: &Query) -> bool {
    let [_, assistant, user] = query.messages.as_ref() else { return false };
    pair(&assistant.parts, &user.parts, RAW, INVALID)
}

fn corrected_feedback(query: &Query) -> bool {
    let [_, first, invalid, second, result] = query.messages.as_ref() else { return false };
    pair(&first.parts, &invalid.parts, RAW, INVALID) && pair(&second.parts, &result.parts, BODY, HOST_RESULT)
}

fn feedback_corruptions(query: &Query) {
    assert!(invalid_feedback(query), "actual full positive malformed history");
    for field in 0..6 {
        let mut changed = query.clone();
        let [Part::ToolCall { id, name, arguments }] = changed.messages[1].parts.as_mut() else {
            panic!("actual malformed call")
        };
        match field {
            0 => *id = b"different".as_slice().into(),
            1 => *name = b"different".as_slice().into(),
            2 => *arguments = b"{}".as_slice().into(),
            3..=5 => {}
            _ => panic!("bounded corruption field"),
        }
        let [Part::ToolOutput { id, output, is_error }] = changed.messages[2].parts.as_mut() else {
            panic!("actual feedback")
        };
        match field {
            0..=2 => {}
            3 => *id = b"different".as_slice().into(),
            4 => *output = b"input is not a complete JSON object".as_slice().into(),
            5 => *is_error = true,
            _ => panic!("bounded corruption field"),
        }
        assert!(!invalid_feedback(&changed), "corruption {field}");
    }
    let mut missing = query.clone();
    missing.messages[2].parts = Box::new([]);
    assert!(!invalid_feedback(&missing));
    let mut duplicate = query.clone();
    duplicate.messages[2].parts = Box::new([query.messages[2].parts[0].clone(), query.messages[2].parts[0].clone()]);
    assert!(!invalid_feedback(&duplicate));
}

fn await_query(world: &mut World, count: usize) {
    for _ in 0..100_000 {
        assert!(!world.drive(1), "actual query precedes root answer");
        if world.prompts().len() == count {
            return;
        }
    }
    panic!("actual query {count} not observed: {:?}", world.trace());
}

fn discovered(world: &World) {
    let read = world.trace().iter().position(|line| line.contains("agent -> Read {")).expect("original discovery read");
    let returned =
        world.trace().iter().position(|line| line.contains("agent <- Read {")).expect("actual discovery terminal");
    let complete =
        world.trace().iter().position(|line| line.contains("agent -> Complete {")).expect("actual root Complete");
    assert!(read < returned && returned < complete);
    assert!(
        world.prompts()[0]
            .system
            .windows(b"The answer is in src/lib.rs; the checks want it to be 43.\n".len())
            .any(|bytes| bytes == b"The answer is in src/lib.rs; the checks want it to be 43.\n")
    );
}

fn corrected_host(world: &World) {
    assert!(corrected_feedback(&world.prompts()[2]));
    let [submission] = world.host_submissions() else { panic!("one corrected host effect") };
    assert_eq!(submission.tool.as_ref(), b"host_action");
    assert_eq!(submission.effect, run::HostEffect::Write);
    assert_eq!(submission.input.bytes(), BODY);
    let [(_, _, run::HostReply::Answered(answer))] = world.host_terminals() else { panic!("one actual host result") };
    assert_eq!(answer.text(), HOST_RESULT);
    assert!(!answer.error());
    assert!(
        world.turns().iter().flat_map(|turn| &turn.messages).flat_map(|message| &message.content).any(
            |block| matches!(block,
        recorded::Block::ToolCall { id, name, input, replay: Some(replay), .. }
        if id.as_ref() == CALL_ID && name.as_ref() == b"host_action" && input.as_ref() == RAW
        && envelope(&replay.bytes, *b"\x00\x01\x01\x00\x00\x00\x16", ROOT_REPLAY))
        ),
        "actual root Turn retains original malformed call replay"
    );
}

fn retired(world: &World) {
    assert!(matches!(world.answer(), run::Answer::Failed { failure: run::Failure::Cancelled, .. }));
    let [first, second, third] = world.wire_bindings() else { panic!("three actual Clients") };
    for binding in [first, second] {
        assert_eq!(
            binding.observed.iter().map(|(_, _, event)| *event).collect::<Vec<_>>(),
            [Observed::Completed(binding.owner), Observed::Reusable, Observed::Close, Observed::Closed]
        );
        assert!(binding.retired.is_some());
    }
    assert_eq!(
        third.observed.iter().map(|(_, _, event)| *event).collect::<Vec<_>>(),
        [Observed::Close, Observed::Cancelled(third.owner), Observed::Closed]
    );
    assert!(third.retired.is_some());
    assert!(corrected_feedback(&world.prompts()[2]));
    assert_eq!(world.host_submissions().len(), 1);
}

#[test]
fn root_malformed_call_has_no_effect_and_corrected_call_runs_once() {
    let configuration = wire::configurations().into_iter().next().expect("raw-string Codex fixture");
    let mut bounds = limits();
    bounds.client.http.request = 16384;
    bounds.client.dialect.request_bytes = 16384;
    let mut settings =
        Settings { job: Job::HostTools, writable: false, network: Span::millis(1, 1), ..Settings::calm(71) };
    settings.limits.session.completion_bytes =
        adapter::completion_worst_case(&bounds.client, settings.limits.decoded_call_bytes)
            .expect("full translated reservation");
    settings.limits.session.completion_blocks = bounds.client.dialect.parts;
    let mut world = World::with_wire(settings, None, configuration, bounds, correction_script());
    world.wall_at(Wall::from_nanos(9000));
    await_query(&mut world, 2);
    assert!(world.host_submissions().is_empty(), "malformed actual call confers no host effect");
    feedback_corruptions(&world.prompts()[1]);
    discovered(&world);
    await_query(&mut world, 3);
    corrected_host(&world);
    world.cancel_run();
    world.run(100_000);
    retired(&world);
}

fn failure_evidence(
    event: &Event,
    owner: Token,
    failure: llm::Failure,
    evidence: llm::Evidence,
    detail: &[u8],
) -> bool {
    matches!(event, Event::Failed { owner: actual, failure: actual_failure, evidence: actual_evidence, detail: actual_detail }
        if *actual == owner && *actual_failure == failure && *actual_evidence == evidence && actual_detail.as_ref() == detail)
}

fn failure_corruptions(event: &mut Event, owner: Token, failure: llm::Failure, evidence: llm::Evidence, detail: &[u8]) {
    assert!(failure_evidence(event, owner, failure, evidence, detail), "full actual failure: {event:?}");
    let Event::Failed { owner: actual, .. } = event else { panic!("actual failure") };
    *actual = Token::new(999);
    assert!(!failure_evidence(event, owner, failure, evidence, detail));
    let Event::Failed { owner: actual, detail: actual_detail, .. } = event else { panic!("actual failure") };
    *actual = owner;
    let retained = std::mem::replace(actual_detail, b"rewritten".as_slice().into());
    assert!(!failure_evidence(event, owner, failure, evidence, detail));
    let Event::Failed { detail: actual_detail, evidence: actual_evidence, .. } = event else {
        panic!("actual failure")
    };
    *actual_detail = retained;
    *actual_evidence =
        if evidence == llm::Evidence::Response { llm::Evidence::Unsent } else { llm::Evidence::Response };
    assert!(!failure_evidence(event, owner, failure, evidence, detail));
    let Event::Failed { evidence: actual_evidence, failure: actual_failure, .. } = event else {
        panic!("actual failure")
    };
    *actual_evidence = evidence;
    *actual_failure = llm::Failure::Invalid;
    assert!(!failure_evidence(event, owner, failure, evidence, detail));
    let Event::Failed { failure: actual_failure, .. } = event else { panic!("actual failure") };
    *actual_failure = failure;
    assert!(failure_evidence(event, owner, failure, evidence, detail));
}

fn observe_failure(
    context: Context,
    peer: &mut RawWorld,
    owner: Token,
    failure: llm::Failure,
    evidence: llm::Evidence,
    detail: &[u8],
) {
    let client::Event::Failed {
        owner: actual,
        failure: actual_failure,
        evidence: actual_evidence,
        detail: actual_detail,
    } = take_terminal(peer)
    else {
        panic!("actual failure entrance")
    };
    assert_eq!(actual, owner);
    let mut event = adapter::failed(context, actual, actual_failure, actual_evidence, actual_detail)
        .expect("actual failure consumes retained Context");
    failure_corruptions(&mut event, owner, failure, evidence, detail);
    if peer.machine.waiting() != client::Waiting::Nothing {
        assert_eq!(peer.machine.waiting(), client::Waiting::Closing);
        assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 0);
        peer.settle();
    }
    assert_eq!(peer.machine.waiting(), client::Waiting::Nothing);
    assert_eq!(peer.seen.iter().filter(|event| matches!(event, client::Event::Closed)).count(), 1);
    let settled = peer.seen.len();
    peer.settle();
    assert_eq!(peer.seen.len(), settled);
    assert_eq!(peer.terminals(), 0);
}

#[test]
fn response_unknown_and_unsent_failures_preserve_full_evidence() {
    let configuration = wire::configurations().into_iter().next().expect("literal failure fixture");
    let source = response(
        429,
        "Content-Type: application/json\r\nRetry-After: 4\r\n",
        br#"{"error":{"type":"rate_limit_error","message":"literal full detail"}}"#,
        false,
    );
    let (context, mut peer) = adopt(&configuration, Token::new(71), Box::new([user(b"Start.")]), source);
    peer.request(client::Request::Start);
    peer.run();
    assert!(!peer.sent.is_empty(), "actual request preceded native refusal");
    observe_failure(
        context,
        &mut peer,
        Token::new(71),
        llm::Failure::RateLimited { retry_after: Duration::from_secs(4) },
        llm::Evidence::Response,
        b"literal full detail",
    );
    let (context, mut peer) = adopt(&configuration, Token::new(72), Box::new([user(b"Start.")]), Vec::new());
    peer.request(client::Request::Start);
    for _ in 0..100_000 {
        if !peer.sent.is_empty() {
            break;
        }
        assert!(peer.tick(false));
    }
    assert!(!peer.sent.is_empty());
    assert_eq!(peer.source_at, 0, "no response bytes precede actual transport loss");
    assert_eq!(peer.terminals(), 0);
    peer.transport_failed();
    observe_failure(
        context,
        &mut peer,
        Token::new(72),
        llm::Failure::Unavailable,
        llm::Evidence::Unknown,
        b"provider transport or HTTP framing failed",
    );
    let (context, mut peer) = adopt(&configuration, Token::new(73), Box::new([user(b"Start.")]), Vec::new());
    peer.settle();
    assert!(peer.sent.is_empty(), "lower closure preceded Start and every send");
    observe_failure(
        context,
        &mut peer,
        Token::new(73),
        llm::Failure::Unavailable,
        llm::Evidence::Unsent,
        b"stream closed before completion",
    );
}
