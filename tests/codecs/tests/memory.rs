//! The largest text payloads admitted by the charter and turn codecs stay
//! within their generated heap bounds (codec.md, sections 4 and 6).

use skein_heap::{Counting, Meter};
use skein_lib::{List, Reader, Writer};

use smith_charter::v1 as charter;
use smith_transcript::v2 as transcript;

#[global_allocator]
static HEAP: Counting = Counting;

fn charter_limits() -> charter::Limits {
    charter::Limits {
        section_title: 0,
        section_body: 0,
        host_tool_name: 0,
        host_tool_description: 0,
        host_tool_input: 0,
        field_rule_name: 0,
        text_rule_fields: 0,
        item_kind_kind: 0,
        item_kind_fields: 0,
        verdict_rule_label: 0,
        verdict_rule_fields: 0,
        verdict_rule_kinds: 0,
        change_rule_fields: 0,
        contract_verdicts: 0,
        tools_host: 0,
        conventions_guide: 0,
        conventions_checks: 0,
        llm_endpoint: 0,
        llm_model: 0,
        charter_instructions: charter::CEILINGS.charter_instructions,
        charter_brief: 0,
        charter_models: 0,
        field_name: 0,
        field_text: 0,
        item_kind: 0,
        item_fields: 0,
        run_result_label_some: 0,
        run_result_text: 0,
        run_result_fields: 0,
        run_result_items: 0,
    }
}

fn transcript_limits() -> transcript::Limits {
    transcript::Limits {
        replay_dialect: 0,
        replay_bytes: 0,
        text_text: transcript::CEILINGS.text_text,
        opaque_dialect: 0,
        opaque_bytes: 0,
        call_id: 0,
        call_name: 0,
        call_input: 0,
        entry_name: 0,
        hit_path: 0,
        hit_text: 0,
        read_content: 0,
        listed_entries: 0,
        found_hits: 0,
        command_end_head: 0,
        command_end_last: 0,
        ambiguous_lines: 0,
        field_problem_field: 0,
        said_text: 0,
        tool_result_id: 0,
        message_blocks: 1,
        turn_endpoint: 0,
        turn_dialect: 0,
        turn_messages: 1,
    }
}

#[test]
fn charter_at_its_largest_instruction_payload_stays_within_the_configured_heap_bound() {
    let limits = charter_limits();
    let smallest = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/smith-charter/golden/v1/record_charter_smallest.bin"),
    )
    .expect("smallest charter");
    let size = limits.charter_instructions;
    let mut wire = Vec::with_capacity(smallest.len() + usize::try_from(size).expect("ceiling fits"));
    wire.extend_from_slice(smallest.get(..2).expect("version prefix"));
    wire.extend_from_slice(&size.to_be_bytes());
    wire.extend(std::iter::repeat_n(b'x', usize::try_from(size).expect("ceiling fits")));
    wire.extend_from_slice(smallest.get(6..).expect("rest of charter"));

    let bound = charter::Charter::worst_case_heap(&limits).expect("charter heap bound");
    let meter = Meter::new();
    meter.start();
    let decoded =
        charter::Charter::decode(&limits, &mut Reader::new(&wire)).expect("charter with ceiling instructions");
    let measured = meter.end();
    assert!(measured.peak() <= bound, "charter decoder peak within generated bound");
    assert!(measured.held() <= bound, "charter decoder retained heap within bound");
    assert_eq!(decoded.instructions().len(), usize::try_from(size).expect("ceiling fits"));
    drop(decoded);
    assert_eq!(meter.held(), 0, "charter allocations released");
}

#[test]
fn turn_at_its_largest_text_payload_stays_within_the_configured_heap_bound() {
    let limits = transcript_limits();
    let text = transcript::Text::new(
        &limits,
        transcript::TextParts {
            text: vec![b'x'; usize::try_from(limits.text_text).expect("ceiling fits")].into_boxed_slice(),
            replay: None,
        },
    )
    .expect("ceiling text");
    let mut blocks = List::with_capacity(1);
    blocks.push(transcript::Block::Text(text)).expect("one block");
    let message =
        transcript::Message::new(&limits, transcript::MessageParts { role: transcript::Role::Assistant, blocks })
            .expect("one message");
    let mut messages = List::with_capacity(1);
    messages.push(message).expect("one message slot");
    let usage =
        transcript::Usage::new(&limits, transcript::UsageParts { input: 0, output: 0, cache_read: 0, cache_write: 0 })
            .expect("usage");
    let value = transcript::Turn::new(
        &limits,
        transcript::TurnParts {
            endpoint: Box::from(&b""[..]),
            dialect: Box::from(&b""[..]),
            place: 1,
            usage,
            spent: 0,
            messages,
        },
    )
    .expect("turn");
    let mut writer = Writer::new(usize::try_from(value.measure()).expect("measured length"));
    value.encode(&mut writer).expect("measured room");
    let wire = writer.finish();
    drop(value);

    let bound = transcript::Turn::worst_case_heap(&limits).expect("turn heap bound");
    let meter = Meter::new();
    meter.start();
    let decoded = transcript::Turn::decode(&limits, &mut Reader::new(&wire)).expect("turn with ceiling text");
    let measured = meter.end();
    assert!(measured.peak() <= bound, "turn decoder peak within generated bound");
    assert!(measured.held() <= bound, "turn decoder retained heap within bound");
    assert_eq!(decoded.messages().len(), 1);
    drop(decoded);
    assert_eq!(meter.held(), 0, "turn allocations released");
}

#[path = "support/events_max.rs"]
mod events;

#[test]
fn every_event_at_its_largest_payload_stays_within_its_encoded_and_heap_bounds() {
    let limits = smith_events::Limits { string: 32, content: 128, items: 3 };
    let heap = smith_events::worst_case(&limits).expect("codec heap bound");
    for record in events::largest_samples(&limits) {
        for capture in [smith_events::Capture::None, smith_events::Capture::Calls, smith_events::Capture::Everything] {
            let bound = smith_events::largest_size(&record.event, &capture, &limits).expect("record bound");
            let meter = Meter::new();
            meter.start();
            let line = smith_events::write(&record, &capture, &limits).expect("largest record");
            if let Some(line) = line {
                assert!(line.len() <= usize::try_from(bound).expect("bound"), "largest line within its bound");
                let decoded = smith_events::read(&line, &limits).expect("largest line reads").expect("known event");
                let measured = meter.end();
                assert!(measured.peak() <= heap, "codec peak {} beyond bound {heap}", measured.peak());
                drop(decoded);
                drop(line);
                assert_eq!(meter.held(), 0, "codec released allocations");
            } else {
                let measured = meter.end();
                assert_eq!(measured.held(), 0, "omitted live text holds nothing");
            }
        }
    }
}
