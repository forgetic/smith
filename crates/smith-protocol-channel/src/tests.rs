//! Opening entry points (protocol/channel.md, section 2).
use skein_channel::{Lower, StreamMode};
use skein_lib::{Queue, stream};

use crate::{Component, Limits, OpenEvent};

#[test]
fn the_agent_demands_the_open_before_the_domain_hears_anything() {
    let limits = Limits {
        bodies: smith_channel::CEILINGS,
        charter: smith_charter::CEILINGS,
        transcript: smith_transcript::CEILINGS,
        endpoints: 0,
        calls: 0,
        turns: 0,
        fact_reserve_frames: 1,
        fact_reserve_bytes: 128,
        channel: skein_channel::Limits {
            chunk: 8,
            credential: 0,
            skip: 8,
            output_bytes: u32::MAX,
            output_frames: 2,
            kinds: 17,
        },
    };
    let mut component =
        Component::new(&limits, StreamMode::Two, crate::Endpoints::new(skein_lib::List::with_capacity(0)))
            .expect("checked schema");
    let mut to_service = Queue::<OpenEvent>::with_capacity(2);
    let mut below = Queue::<Lower>::with_capacity(8);
    component.fire(&mut to_service, &mut below);
    assert!(to_service.is_empty());
    match below.pop() {
        Some(Lower::Read(stream::Down::Demand { read: stream::Read::Fill(1), room: 0 })) => {}
        other => panic!("expected Open read: {other:?}"),
    }
}

fn saved_turn(place: u32, dialect: &[u8]) -> Box<[u8]> {
    use smith_transcript as wire;
    let limits = wire::CEILINGS;
    let result = wire::ToolResult::new(
        &limits,
        wire::ToolResultParts {
            id: Box::from(*b"call-1"),
            returned: wire::Returned::Invalid(
                wire::Invalid::new(&limits, wire::InvalidParts { problem: wire::CallProblem::UnknownTool })
                    .expect("bounded invalid result"),
            ),
        },
    )
    .expect("bounded result");
    let mut blocks = skein_lib::List::with_capacity(2);
    blocks
        .push(wire::Block::Call(
            wire::Call::new(
                &limits,
                wire::CallParts {
                    id: Box::from(*b"call-1"),
                    name: Box::from(*b"missing"),
                    input: Box::from(*b"{}"),
                    replay: None,
                },
            )
            .expect("bounded call"),
        ))
        .expect("first block");
    blocks.push(wire::Block::Result(result)).expect("second block");
    let mut messages = skein_lib::List::with_capacity(1);
    messages
        .push(
            wire::Message::new(&limits, wire::MessageParts { role: wire::Role::Assistant, blocks })
                .expect("bounded message"),
        )
        .expect("first message");
    let turn = wire::Turn::new(
        &limits,
        wire::TurnParts {
            endpoint: Box::default(),
            dialect: Box::from(dialect),
            place,
            usage: wire::Usage::new(&limits, wire::UsageParts { input: 2, output: 3, cache_read: 4, cache_write: 5 })
                .expect("bounded usage"),
            spent: 7,
            messages,
        },
    )
    .expect("bounded turn");
    let mut writer = skein_lib::Writer::new(usize::try_from(turn.measure()).expect("bounded turn length"));
    turn.encode(&mut writer).expect("measured turn");
    writer.finish()
}

#[test]
#[expect(clippy::wildcard_enum_match_arm, reason = "test reports the observed value")]
fn transcript_decodes_history_with_one_invalid_call_problem() {
    use smith_domain_session::{llm, record};
    let mut configured = skein_lib::List::with_capacity(1);
    configured.push(crate::Endpoint { name: Box::default(), number: 3, dialect: 0, account: 5 }).expect("one endpoint");
    let endpoints = crate::Endpoints::new(configured);
    let bytes = Box::from([saved_turn(1, b"00000000")]);
    let transcript = crate::decode_transcript(&bytes, &smith_transcript::CEILINGS, &endpoints)
        .expect("valid transcript")
        .expect("saved history");
    assert_eq!(transcript.version, record::VERSION);
    assert_eq!(transcript.endpoint, llm::Endpoint(3));
    assert_eq!(transcript.turns[0].sequence, 1);
    assert_eq!(transcript.turns[0].spent, 7);
    assert_eq!(transcript.turns[0].usage.cache_read_tokens, 4);
    match &transcript.turns[0].messages[0].content[0] {
        llm::Block::ToolCall { call: llm::Decoded::Historical, .. } => {}
        other => panic!("expected historical call: {other:?}"),
    }
    match &transcript.turns[0].messages[0].content[1] {
        llm::Block::ToolResult { result: llm::Returned::Invalid { problem: llm::Problem::UnknownTool }, .. } => {}
        other => panic!("expected single invalid problem: {other:?}"),
    }
}

#[test]
fn transcript_refuses_a_gap_and_a_different_dialect() {
    use smith_domain_session::record;
    let mut configured = skein_lib::List::with_capacity(1);
    configured.push(crate::Endpoint { name: Box::default(), number: 3, dialect: 0, account: 5 }).expect("one endpoint");
    let endpoints = crate::Endpoints::new(configured);
    assert_eq!(
        crate::decode_transcript(&[saved_turn(2, b"00000000")], &smith_transcript::CEILINGS, &endpoints),
        Err(record::Refusal::Malformed)
    );
    assert_eq!(
        crate::decode_transcript(&[saved_turn(1, b"other")], &smith_transcript::CEILINGS, &endpoints),
        Err(record::Refusal::Dialect)
    );
}
