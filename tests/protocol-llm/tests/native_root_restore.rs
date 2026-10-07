//! Original Start/discovery, actual native replay, parking and root restoration.
//! Caller fixtures supply opaque native data and handwritten outside expectations;
//! Skein alone owns their grammar. Every historical body comes from an actual
//! root Turn, including the independently saved concrete post-transcript result.
//! Contract: scratch/client.md, sections 1, 3–7; domain/run.md, sections 6,
//! 9, 10 and 13; domain/session.md, section 3; testing-strategy.md, sections 2.3 and 6.

use skein_fake_llm_domain::api::{Finish, Line, Message, Part, Query, Role, Script, Turn};
use skein_lib::Duration;
use skein_world::domain::Span;
use smith_domain::{Answered, AnsweredCall, Transcript, run, session::llm};
use smith_protocol_llm::{self as adapter, Limits};
use smith_protocol_llm_world::{
    Job, Settings, World,
    wire::{self, Configuration, Observed},
};

const BEGIN: &[u8] = b"Begin the work your brief describes.";
const ID: &[u8] = b"call_0000000000000001";
const FIRST: &[u8] = b"First activation parked.";
const RESUMED: &[u8] = b"Actual resumed activation.";
const LAST: &[u8] = b"Second activation parked.";
const CODEX_OPAQUE: &[u8] = br#"{"type":"reasoning","id":"opaque_root","encrypted_content":"encrypted-root","summary":[],"future":{"proof":[1,true,null],"nonce":"root-proof"}}"#;
const ANTHROPIC_OPAQUE: &[u8] = br#"{"type":"thinking","future":{"proof":[1,true,null],"nonce":"root-proof"},"thinking":"Plan for parking","signature":"signed-root"}"#;
const CODEX_ENVELOPE: &[u8] = b"\x00\x01\x01\x00\x00\x00\x8f{\"type\":\"reasoning\",\"id\":\"opaque_root\",\"encrypted_content\":\"encrypted-root\",\"summary\":[],\"future\":{\"proof\":[1,true,null],\"nonce\":\"root-proof\"}}";
const ANTHROPIC_ENVELOPE: &[u8] = b"\x00\x01\x02\x00\x00\x00\x81{\"type\":\"thinking\",\"future\":{\"proof\":[1,true,null],\"nonce\":\"root-proof\"},\"thinking\":\"Plan for parking\",\"signature\":\"signed-root\"}";
const CALL_REPLAY: &[u8] = b"\x00\x01\x01\x00\x00\x00\x16{\"item_id\":\"item_1_1\"}";
const TEXT_REPLAY: &[u8] = b"\x00\x01\x01\x00\x00\x00\x28{\"id\":\"item_1_0\",\"phase\":\"final_answer\"}";

struct Fixture {
    opaque: &'static [u8],
    envelope: &'static [u8],
    call_replay: Option<&'static [u8]>,
    text_replay: Option<&'static [u8]>,
    seal: &'static [u8],
    cache_write: bool,
}

fn fixtures() -> [Fixture; 2] {
    [
        Fixture {
            opaque: CODEX_OPAQUE,
            envelope: CODEX_ENVELOPE,
            call_replay: Some(CALL_REPLAY),
            text_replay: Some(TEXT_REPLAY),
            seal: b"encrypted-root",
            cache_write: false,
        },
        Fixture {
            opaque: ANTHROPIC_OPAQUE,
            envelope: ANTHROPIC_ENVELOPE,
            call_replay: None,
            text_replay: None,
            seal: b"signed-root",
            cache_write: true,
        },
    ]
}

fn waiting_turn(opaque: &[u8]) -> Turn {
    Turn {
        lines: Box::new([
            Line::Opaque { bytes: opaque.into() },
            Line::Call { name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() },
        ]),
        finish: Finish::ToolCalls,
        tokens: 17,
    }
}

fn resumed_turn() -> Turn {
    Turn {
        lines: Box::new([
            Line::Text { text: RESUMED.into() },
            Line::Call { name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() },
        ]),
        finish: Finish::ToolCalls,
        tokens: 11,
    }
}

fn text_turn(text: &[u8], tokens: u64) -> Turn {
    Turn { lines: Box::new([Line::Text { text: text.into() }]), finish: Finish::Stop, tokens }
}

fn scripts(fixture: &Fixture, tail: bool) -> Box<[Script]> {
    let mut turns = vec![waiting_turn(fixture.opaque)];
    if !tail {
        turns.push(text_turn(FIRST, 3));
    }
    turns.extend([resumed_turn(), text_turn(LAST, 5)]);
    Box::new([Script { cue: b"@waiting".as_slice().into(), turns: turns.into() }])
}

fn bounds() -> Limits {
    let mut bounds = Limits { client: skein_llm_world::limits(), tool_bytes: 32768, result_bytes: 32768 };
    bounds.client.http.request = 16384;
    bounds.client.dialect.request_bytes = 16384;
    bounds
}

fn settings(seed: u64, resume: bool, bounds: &Limits) -> Settings {
    let mut settings = Settings {
        job: Job::Waiting,
        resume,
        writable: false,
        waiting: Duration::from_secs(1),
        network: Span::millis(1, 1),
        ..Settings::calm(seed)
    };
    settings.limits.session.completion_bytes =
        adapter::completion_worst_case(&bounds.client, settings.limits.decoded_call_bytes)
            .expect("complete actual translated reservation before Start");
    settings.limits.session.completion_blocks = bounds.client.dialect.parts;
    assert!(bounds.client.dialect.detail_bytes <= settings.limits.session.failure_bytes);
    settings
}

fn message(role: Role, parts: Vec<Part>) -> Message {
    Message { role, parts: parts.into() }
}

fn text(text: &[u8]) -> Part {
    Part::Text { text: text.into() }
}

fn call() -> Part {
    Part::ToolCall { id: ID.into(), name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() }
}

fn feedback() -> Part {
    Part::ToolOutput { id: ID.into(), output: b"waiting".as_slice().into(), is_error: false }
}

fn prefix(fixture: &Fixture) -> Vec<Message> {
    vec![
        message(Role::User, vec![text(BEGIN)]),
        message(Role::Assistant, vec![Part::Opaque { bytes: fixture.opaque.into() }, call()]),
        message(Role::User, vec![feedback()]),
    ]
}

fn resumed_prefix(fixture: &Fixture, tail: bool) -> Vec<Message> {
    let mut messages = prefix(fixture);
    if tail {
        if fixture.call_replay.is_some() {
            messages[2].parts = vec![feedback(), text(&waking_prompt())].into();
        } else {
            messages.push(message(Role::User, vec![text(&waking_prompt())]));
        }
    } else {
        messages.push(message(Role::Assistant, vec![text(FIRST)]));
        messages.push(message(Role::User, vec![text(BEGIN)]));
    }
    messages
}

fn waking_prompt() -> Vec<u8> {
    let mut text = b"Earlier host answers absent from the saved transcript:\ncall activation=1 completion=2 position=1 tool=wait result: waiting\n\n".to_vec();
    text.extend_from_slice(BEGIN);
    text
}

fn exact_prefix(query: &Query, expected: &[Message]) -> bool {
    query.model.as_ref() == b"fake-1" && query.messages.as_ref() == expected
}

fn prefix_corruptions(query: &Query, fixture: &Fixture, expected: &[Message]) {
    assert!(exact_prefix(query, expected), "whole positive actual native prefix: {query:?}");
    for marker in [b"root-proof".as_slice(), b"proof", b"type", fixture.seal] {
        let mut changed = query.clone();
        let Part::Opaque { bytes } = &mut changed.messages[1].parts[0] else { panic!("actual opaque") };
        let at = bytes.windows(marker.len()).position(|part| part == marker).expect("positive opaque metadata");
        bytes[at] = b'X';
        assert!(!exact_prefix(&changed, expected), "rewritten native metadata {marker:?}");
    }
    let mut rewritten = query.clone();
    rewritten.messages[1].parts[0] = Part::Opaque { bytes: b"{}".as_slice().into() };
    assert!(!exact_prefix(&rewritten, expected));
    let mut missing = query.clone();
    missing.messages[1].parts = vec![call()].into();
    assert!(!exact_prefix(&missing, expected));
    let mut duplicate = query.clone();
    duplicate.messages[1].parts = vec![Part::Opaque { bytes: fixture.opaque.into() }, call(), call()].into();
    assert!(!exact_prefix(&duplicate, expected));
    for field in 0..6 {
        let mut changed = query.clone();
        match field {
            0 => changed.messages[1].role = Role::User,
            1 => changed.messages[2].role = Role::Assistant,
            2 => {
                let Part::ToolCall { id, .. } = &mut changed.messages[1].parts[1] else { panic!("actual call") };
                *id = b"rewritten-id".as_slice().into();
            }
            3 => {
                let Part::ToolOutput { id, .. } = &mut changed.messages[2].parts[0] else { panic!("actual feedback") };
                *id = b"rewritten-id".as_slice().into();
            }
            4 => {
                let Part::ToolOutput { output, .. } = &mut changed.messages[2].parts[0] else {
                    panic!("actual feedback")
                };
                *output = b"rewritten feedback".as_slice().into();
            }
            5 => changed.messages[2].parts = Box::new([]),
            _ => panic!("bounded independent corruption"),
        }
        assert!(!exact_prefix(&changed, expected), "native prefix corruption {field}");
    }
}

fn replay(actual: Option<&llm::Replay>, expected: Option<&[u8]>) -> bool {
    actual.map(|replay| replay.bytes.as_ref()) == expected
}

fn recorded_wait(turn: &smith_domain::Turn, fixture: &Fixture, opaque: bool) -> bool {
    let [wake, assistant, result] = turn.messages.as_ref() else { return false };
    let wake_matches = matches!(wake.content.as_ref(), [llm::Block::Text { text, replay: None }] if text.as_ref() == BEGIN)
        || (!opaque
            && matches!(wake.content.as_ref(), [llm::Block::ToolResult {
            id, result: llm::Returned::Text { text, error: false, replay: None }
        }, llm::Block::Text { text: prompt, replay: None }]
            if id.as_ref() == ID && text.as_ref() == b"waiting" && prompt.as_ref() == waking_prompt()));
    if wake.role != llm::Role::User || assistant.role != llm::Role::Assistant || !wake_matches {
        return false;
    }
    let [first, llm::Block::ToolCall { id, name, input, call: llm::Decoded::Historical, replay: metadata }] =
        assistant.content.as_ref()
    else {
        return false;
    };
    let first_matches = if opaque {
        matches!(first, llm::Block::Opaque { bytes } if bytes.as_ref() == fixture.envelope)
    } else {
        matches!(first, llm::Block::Text { text, replay: metadata }
            if text.as_ref() == RESUMED && replay(metadata.as_ref(), fixture.text_replay))
    };
    first_matches
        && id.as_ref() == ID
        && name.as_ref() == b"wait"
        && input.as_ref() == b"{}"
        && replay(metadata.as_ref(), fixture.call_replay)
        && result.role == llm::Role::User
        && matches!(result.content.as_ref(), [llm::Block::ToolResult {
            id, result: llm::Returned::Text { text, error: false, replay: None }
        }] if id.as_ref() == ID && text.as_ref() == b"waiting")
}

fn recorded_corruptions(turn: &smith_domain::Turn, fixture: &Fixture) {
    assert!(recorded_wait(turn, fixture, true), "positive full root-owned replay and concrete feedback");
    for at in [0, 1, 2, 6, 7] {
        let mut changed = turn.clone();
        let llm::Block::Opaque { bytes } = &mut changed.messages[1].content[0] else { panic!("actual envelope") };
        bytes[at] ^= 1;
        assert!(!recorded_wait(&changed, fixture, true), "version/tag/length/body corruption {at}");
    }
    let mut missing = turn.clone();
    missing.messages[1].content = Box::new([missing.messages[1].content[1].clone()]);
    assert!(!recorded_wait(&missing, fixture, true));
    let mut unresolved = turn.clone();
    let llm::Block::ToolCall { call, .. } = &mut unresolved.messages[1].content[1] else { panic!("actual historical") };
    *call = llm::Decoded::Delegated { ticket: skein_lib::Token::new(99), effect: smith_domain::tools::Effect::Write };
    assert!(!recorded_wait(&unresolved, fixture, true));
    if fixture.call_replay.is_some() {
        let mut missing = turn.clone();
        let llm::Block::ToolCall { replay, .. } = &mut missing.messages[1].content[1] else { panic!("actual call") };
        *replay = None;
        assert!(!recorded_wait(&missing, fixture, true));
    }
}

fn history(world: &World) -> Transcript {
    let first = world.turns().first().expect("actual root Turn");
    Transcript { version: first.version, endpoint: first.endpoint, dialect: first.dialect, turns: world.turns().into() }
}

// The parent retains an actual prefix and its concrete result separately;
// no completion, application classification, feedback or usage is manufactured.
fn post_tail(world: &World) -> (Transcript, AnsweredCall) {
    let mut saved = history(world);
    let first = saved.turns[0].clone();
    let result = first.messages.last().expect("actual saved Wait terminal").clone();
    saved.turns = Box::new([first]);
    let llm::Block::ToolResult { result: llm::Returned::Text { text, error, .. }, .. } = &result.content[0] else {
        panic!("saved host answer")
    };
    let answered = AnsweredCall {
        tool: b"wait".as_slice().into(),
        name: run::CallName { activation: 1, completion: saved.turns[0].sequence + 1, position: 1 },
        answer: Answered::Host(run::HostAnswer::new(text.clone(), *error).expect("saved host answer fits")),
    };
    (saved, answered)
}

fn part_bytes(part: &Part) -> usize {
    match part {
        Part::Text { text } | Part::Opaque { bytes: text } => text.len(),
        Part::ToolCall { name, arguments, .. } => name.len() + arguments.len(),
        Part::ToolOutput { output, .. } => output.len(),
    }
}

fn expected_usage(system_bytes: usize, expected: &[Message], output: u64, fixture: &Fixture) -> [u64; 4] {
    let (last, earlier) = expected.split_last().expect("handwritten positive user-ending prefix");
    let last_bytes = last.parts.iter().map(part_bytes).sum::<usize>();
    let (fresh, cached) = if earlier.is_empty() {
        (system_bytes + last_bytes, 0)
    } else {
        (last_bytes, system_bytes + earlier.iter().flat_map(|message| &message.parts).map(part_bytes).sum::<usize>())
    };
    let input = u64::try_from(fresh / 4).expect("bounded caller fixture");
    [
        input,
        output,
        u64::try_from(cached / 4).expect("bounded caller fixture"),
        if fixture.cache_write { input } else { 0 },
    ]
}

fn accounting(world: &World, fixture: &Fixture, expected_prompts: &[Vec<Message>], outputs: &[u64], sequence: u32) {
    assert_eq!(world.prompts().len(), expected_prompts.len());
    assert_eq!(world.turns().len(), expected_prompts.len());
    let mut total = run::Spend::ZERO;
    for (index, ((query, turn), expected_prompt)) in
        world.prompts().iter().zip(world.turns()).zip(expected_prompts).enumerate()
    {
        assert!(exact_prefix(query, expected_prompt), "actual query {index}: {query:?}");
        let wanted = expected_usage(query.system.len(), expected_prompt, outputs[index], fixture);
        let actual = [
            turn.usage.input_tokens,
            turn.usage.output_tokens,
            turn.usage.cache_read_tokens,
            turn.usage.cache_write_tokens,
        ];
        assert_eq!(actual, wanted, "all four actual SDK usage fields at completion {index}");
        for field in 0..4 {
            let mut changed = actual;
            changed[field] += 1;
            assert_ne!(changed, wanted, "usage field {field} participates in the oracle");
        }
        total.turns += 1;
        total.input += wanted[0];
        total.output += wanted[1];
        total.cache_read += wanted[2];
        total.cache_write += wanted[3];
        assert_eq!(world.turn_metadata()[index], (total.turns, None, total));
        assert_eq!(turn.version, 2);
        assert_eq!(turn.endpoint, llm::Endpoint(0));
        assert_eq!(turn.dialect, 1);
        assert_eq!(turn.sequence, sequence + total.turns);
        assert_eq!(turn.spent, 0, "the root's current zero record prices retain separate typed token accounting");
    }
    assert!(matches!(world.answer(), run::Answer::Parked { spent, turns } if *spent == total && *turns == total.turns));
}

fn settled(world: &World, settings: &Settings) {
    assert_eq!(world.waiting().len(), 1);
    assert_eq!(world.waiting()[0].1, None);
    assert!(world.answered_at() >= world.waiting()[0].0.saturating_add(settings.waiting));
    assert!(world.host_submissions().is_empty() && world.host_terminals().is_empty());
    assert_eq!(world.host_decisions(), 0);
    assert!(world.checked().is_empty() && world.pushes().is_empty());
    assert_eq!(world.wire_bindings().len(), world.turns().len());
    for binding in world.wire_bindings() {
        assert_eq!(binding.receiving.max_completion_bytes, settings.limits.session.completion_bytes);
        assert_eq!(binding.receiving.max_completion_blocks, settings.limits.session.completion_blocks);
        assert_eq!(binding.receiving.max_failure_bytes, settings.limits.session.failure_bytes);
        assert_eq!(binding.receiving.decoded_call_bytes, settings.limits.decoded_call_bytes);
        assert_eq!(
            binding.observed.iter().map(|(_, _, event)| *event).collect::<Vec<_>>(),
            [Observed::Completed(binding.owner), Observed::Reusable, Observed::Close, Observed::Closed]
        );
        assert!(binding.retired.is_some(), "actual lower Closed permits physical retirement");
    }
    let read = world.trace().iter().position(|line| line.contains("agent -> Read {")).expect("original discovery");
    let returned =
        world.trace().iter().position(|line| line.contains("agent <- Read {")).expect("actual guide terminal");
    let complete = world.trace().iter().position(|line| line.contains("agent -> Complete {")).expect("actual Complete");
    assert!(read < returned && returned < complete);
    assert!(
        world.prompts()[0]
            .system
            .windows(b"The answer is in src/lib.rs; the checks want it to be 43.\n".len())
            .any(|bytes| bytes == b"The answer is in src/lib.rs; the checks want it to be 43.\n")
    );
}

fn configuration(index: usize) -> Configuration {
    wire::configurations().into_iter().nth(index).expect("two caller native fixtures")
}

fn resume(first: &World, fixture: &Fixture, index: usize, tail: bool) {
    let (saved, answered) = if tail {
        let (saved, answered) = post_tail(first);
        (saved, Box::new([answered]) as Box<[_]>)
    } else {
        (history(first), Box::default())
    };
    let sequence = saved.turns.last().expect("actual persisted prefix").sequence;
    let bounds = bounds();
    let settings = settings(920, true, &bounds);
    let mut world =
        World::with_wire_answers(settings, Some(saved), answered, configuration(index), bounds, scripts(fixture, tail));
    world.run(100_000);
    let prefix = resumed_prefix(fixture, tail);
    prefix_corruptions(&world.prompts()[0], fixture, &prefix);
    let mut second = prefix.clone();
    second.push(message(Role::Assistant, vec![text(RESUMED), call()]));
    second.push(message(Role::User, vec![feedback()]));
    accounting(&world, fixture, &[prefix, second], &[11, 5], sequence);
    if !tail {
        assert!(recorded_wait(&world.turns()[0], fixture, false));
    }
    assert!(matches!(world.turns()[1].messages[0].content.as_ref(), [llm::Block::Text { text, replay: metadata }]
        if text.as_ref() == LAST && replay(metadata.as_ref(), fixture.text_replay)));
    settled(&world, &settings);
    assert_eq!(first.turn_metadata()[1].2.output, 20, "saved activation is independently charged");
    assert_eq!(world.turn_metadata()[1].2.output, 16, "history is not recharged in the new activation");
}

fn header_refusals(first: &World, fixture: &Fixture, index: usize) {
    for reason in [run::TranscriptRefusal::Version, run::TranscriptRefusal::Endpoint, run::TranscriptRefusal::Dialect] {
        let mut saved = history(first);
        match reason {
            run::TranscriptRefusal::Version => saved.version = 0,
            run::TranscriptRefusal::Endpoint => saved.endpoint = llm::Endpoint(999),
            run::TranscriptRefusal::Dialect => saved.dialect = 999,
            run::TranscriptRefusal::Malformed
            | run::TranscriptRefusal::Unresolved
            | run::TranscriptRefusal::TooLarge => {
                panic!("only independent header refusals in this native story")
            }
        }
        let bounds = bounds();
        let settings = settings(921, true, &bounds);
        let mut world = World::with_wire(settings, Some(saved), configuration(index), bounds, scripts(fixture, false));
        world.run(100_000);
        assert!(
            matches!(world.answer(), run::Answer::Failed { failure: run::Failure::Transcript(actual), spent, turns: 0 }
            if *actual == reason && *spent == run::Spend::ZERO)
        );
        assert!(world.wire_bindings().is_empty() && world.prompts().is_empty() && world.turns().is_empty());
        assert!(world.host_submissions().is_empty() && world.host_terminals().is_empty());
        assert!(world.checked().is_empty() && world.pushes().is_empty());
    }
}

#[test]
fn native_root_parks_then_restores_whole_opaque_prefix_and_real_post_tail() {
    for (index, fixture) in fixtures().into_iter().enumerate() {
        let bounds = bounds();
        let settings = settings(919, false, &bounds);
        let mut first = World::with_wire(settings, None, configuration(index), bounds, scripts(&fixture, false));
        first.run(100_000);
        accounting(&first, &fixture, &[vec![message(Role::User, vec![text(BEGIN)])], prefix(&fixture)], &[17, 3], 0);
        recorded_corruptions(&first.turns()[0], &fixture);
        assert!(matches!(first.turns()[1].messages[0].content.as_ref(), [llm::Block::Text { text, replay: metadata }]
            if text.as_ref() == FIRST && replay(metadata.as_ref(), fixture.text_replay)));
        settled(&first, &settings);
        resume(&first, &fixture, index, false);
        resume(&first, &fixture, index, true);
        header_refusals(&first, &fixture, index);
    }
}
