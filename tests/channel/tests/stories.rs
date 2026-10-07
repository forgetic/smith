//! Channel opening stories (protocol/channel.md, sections 2 and 10).
use skein_channel::{Closed, StreamMode};
use smith_channel::CEILINGS;
use smith_channel_world::{Observation, World};

#[test]
fn the_halves_open_on_the_highest_version_both_speak() {
    for mode in [StreamMode::One, StreamMode::Two] {
        let mut world = World::new(CEILINGS, CEILINGS, mode);
        world.settle();
        assert!(world.observations().contains(&Observation::HostOpened(1)));
        assert!(world.observations().contains(&Observation::AgentOpened(1)));
    }
}

#[test]
fn no_version_in_common_ends_the_channel_before_a_start() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.agent_hears_foreign_version();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(1))));
    assert!(!world.observations().contains(&Observation::AgentOpened(1)));
}

#[test]
fn a_host_whose_terms_are_too_small_is_refused_at_the_opening() {
    let mut host = CEILINGS;
    host.turn_body = 1;
    let mut world = World::new(host, CEILINGS, StreamMode::Two);
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(2))));
}

#[test]
fn a_start_that_decodes_reaches_the_agent() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    assert!(world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_charter_in_an_unread_version_is_refused_as_invalid_with_no_turns() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from([0, 2]));
    world.settle();
    assert!(world.observations().contains(&Observation::InvalidStart {
        why: smith_channel::InvalidStart::CharterVersion,
        turns: 0,
        spent: 0,
    }));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_malformed_charter_is_refused_with_its_own_reason() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from([0, 1]));
    world.settle();
    assert!(world.observations().contains(&Observation::InvalidStart {
        why: smith_channel::InvalidStart::MalformedCharter,
        turns: 0,
        spent: 0,
    }));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_start_record_that_does_not_decode_breaks_the_channel_rules() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.agent_hears_malformed_start();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(256))));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn the_agent_resolves_the_charters_endpoint_before_admission() {
    let mut entries = skein_lib::List::with_capacity(1);
    entries
        .push(smith_protocol_channel::Endpoint { name: Box::default(), number: 17, dialect: 23, account: 29 })
        .expect("one endpoint");
    let endpoints = smith_protocol_channel::Endpoints::new(entries);
    let bytes = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin");
    let charter =
        smith_protocol_channel::decode_charter(bytes, &smith_charter::CEILINGS, &endpoints).expect("v1 charter");
    assert_eq!(charter.llm.endpoint.0, 17);
    assert_eq!(charter.llm.dialect, 23);
    assert_eq!(charter.llm.account, 29);
    assert!(charter.brief.sections.is_empty());
    assert!(charter.models.is_empty());
    let unknown = smith_protocol_channel::decode_charter(
        bytes,
        &smith_charter::CEILINGS,
        &smith_protocol_channel::Endpoints::new(skein_lib::List::with_capacity(0)),
    );
    assert!(matches!(unknown, Err(smith_domain::run::Invalid::Endpoint)));
}

#[test]
fn the_full_charter_keeps_contracts_tools_and_model_prices() {
    let bytes = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_full.bin");
    let wire = smith_charter::Charter::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(bytes))
        .expect("full charter");
    let mut entries = skein_lib::List::with_capacity(1 + wire.models().len());
    entries
        .push(smith_protocol_channel::Endpoint {
            name: Box::from(wire.main().endpoint()),
            number: 17,
            dialect: 23,
            account: 29,
        })
        .expect("main endpoint");
    for model in wire.models() {
        entries
            .push(smith_protocol_channel::Endpoint {
                name: Box::from(model.endpoint()),
                number: 17,
                dialect: 23,
                account: 29,
            })
            .expect("model endpoint");
    }
    let charter = smith_protocol_channel::decode_charter(
        bytes,
        &smith_charter::CEILINGS,
        &smith_protocol_channel::Endpoints::new(entries),
    )
    .expect("translated charter");
    assert_eq!(charter.instructions.as_ref(), wire.instructions());
    assert_eq!(charter.brief.sections.len(), usize::try_from(wire.brief().len()).expect("bounded count"));
    assert_eq!(charter.grants.host_tools.len(), usize::try_from(wire.tools().host().len()).expect("bounded count"));
    assert_eq!(
        charter.outcome.verdicts.len(),
        usize::try_from(wire.contract().verdicts().len()).expect("bounded count")
    );
    assert_eq!(charter.models.len(), usize::try_from(wire.models().len()).expect("bounded count"));
    assert_eq!(charter.llm.prices.input, wire.main().prices().input());
    assert_eq!(charter.llm.prices.cached, wire.main().prices().cached());
    assert_eq!(charter.llm.prices.output, wire.main().prices().output());
    assert_eq!(charter.llm.prices.unit, wire.main().prices().unit());
    assert_eq!(charter.budget.turns, wire.budget().turns());
    assert_eq!(charter.budget.spend, wire.budget().spend());
    assert_eq!(charter.budget.time, wire.budget().time());
}

#[test]
fn an_accepted_report_keeps_its_fields_in_charter_v1() {
    let result = smith_domain::run::outcome::Declared::Report(smith_domain::run::outcome::Report {
        text: Box::from(*b"done"),
        fields: Box::from([smith_domain::run::outcome::Field {
            name: Box::from(*b"source"),
            value: Box::from(*b"agent"),
        }]),
    });
    let bytes = smith_protocol_channel::encode_result(&result, &smith_charter::CEILINGS).expect("bounded result");
    let record = smith_charter::RunResult::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(&bytes))
        .expect("result decodes");
    assert!(matches!(record.form(), smith_charter::Form::Report));
    assert_eq!(record.text(), b"done");
    assert_eq!(record.fields().get(0).expect("one field").name(), b"source");
    assert_eq!(record.fields().get(0).expect("one field").text(), b"agent");
}

#[test]
fn an_accepted_verdict_keeps_its_label_and_items() {
    let result = smith_domain::run::outcome::Declared::Verdict(smith_domain::run::outcome::Verdict {
        name: Box::from(*b"ready"),
        text: Box::from(*b"reviewed"),
        fields: Box::default(),
        items: Box::from([smith_domain::run::outcome::Item {
            kind: Box::from(*b"path"),
            fields: Box::from([smith_domain::run::outcome::Field {
                name: Box::from(*b"name"),
                value: Box::from(*b"file"),
            }]),
        }]),
    });
    let bytes = smith_protocol_channel::encode_result(&result, &smith_charter::CEILINGS).expect("bounded result");
    let record = smith_charter::RunResult::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(&bytes))
        .expect("result decodes");
    assert!(matches!(record.form(), smith_charter::Form::Verdict));
    assert_eq!(record.label().as_deref(), Some(&b"ready"[..]));
    assert_eq!(record.items().get(0).expect("one item").kind(), b"path");
    assert_eq!(record.items().get(0).expect("one item").fields().get(0).expect("one field").text(), b"file");
}

#[test]
fn a_run_goes_from_start_to_answer() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    assert!(world.observations().contains(&Observation::AgentStart));
    world.agent_admits_and_parks();
    world.settle();
    assert!(world.observations().contains(&Observation::HostAdmitted));
    assert!(world.observations().contains(&Observation::HostParked { turns: 0, spent: 0 }));
}

#[test]
fn a_model_failure_keeps_transport_evidence_and_cooldown() {
    let answer = smith_domain::run::Answer::Failed {
        failure: smith_domain::run::Failure::Model(smith_domain::run::Fault::Completion {
            failure: smith_domain::run::CompletionFailure::RateLimited {
                retry_after: skein_lib::Duration::from_nanos(42),
            },
            evidence: smith_domain::run::CompletionEvidence::Response,
        }),
        spent: smith_domain::run::Spend::ZERO,
        turns: 2,
    };
    let record =
        smith_protocol_channel::answer_record(answer, &CEILINGS, &smith_charter::CEILINGS).expect("bounded failure");
    assert_eq!(record.turns(), 2);
    let smith_channel::RunResult::Failed(failed) = record.result() else {
        panic!("expected failure");
    };
    let smith_channel::RunFailure::Model(model) = failed.reason() else {
        panic!("expected model failure");
    };
    let smith_channel::ModelFault::Completion(fault) = model.value() else {
        panic!("expected completion");
    };
    assert!(matches!(fault.failure(), smith_channel::CompletionFailure::RateLimited));
    assert!(matches!(fault.evidence(), smith_channel::CompletionEvidence::Response));
    assert_eq!(fault.retry_after(), &Some(skein_lib::Duration::from_nanos(42)));
}

#[test]
fn the_start_keeps_workspace_paths_saved_answers_and_grant_values_below_the_domain() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start_with_context(
        Box::from(&include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..]),
        smith_host_domain::SavedReply::Host { error: false, body: Box::from(*b"ok") },
    );
    world.settle();
    assert!(world.observations().contains(&Observation::AgentContext {
        path: Box::from(*b"/tmp/src"),
        answered: 1,
        window: 2,
    }));
    assert_eq!(world.agent_grant_value(3, 4), Some(Box::from(*b"secret")));
    assert!(world.observations().contains(&Observation::AgentSavedHost {
        activation: 6,
        tool: Box::from(*b"check"),
        text: Box::from(*b"ok"),
        error: false,
    }));
}

#[test]
fn a_saved_oversized_answer_reaches_the_agent_as_a_settled_decision() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start_with_context(
        Box::from(&include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..]),
        smith_host_domain::SavedReply::TooLarge,
    );
    world.settle();
    assert!(world.observations().contains(&Observation::AgentSavedTooLarge));
}

fn saved_turn(place: u32, dialect: &[u8]) -> Box<[u8]> {
    let limits = smith_transcript::CEILINGS;
    let turn = smith_transcript::Turn::new(
        &limits,
        smith_transcript::TurnParts {
            endpoint: Box::default(),
            dialect: Box::from(dialect),
            place,
            usage: smith_transcript::Usage::new(
                &limits,
                smith_transcript::UsageParts { input: 1, output: 2, cache_read: 0, cache_write: 0 },
            )
            .expect("usage"),
            spent: 3,
            messages: skein_lib::List::with_capacity(0),
        },
    )
    .expect("turn");
    let mut writer = skein_lib::Writer::new(usize::try_from(turn.measure()).expect("turn length"));
    turn.encode(&mut writer).expect("measured turn");
    writer.finish()
}

#[test]
fn a_saved_turn_reaches_the_agent_as_concrete_history() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start_with_turns(
        Box::from(&include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..]),
        Some(Box::from([saved_turn(1, b"00000000")])),
    );
    world.settle();
    assert!(world.observations().contains(&Observation::AgentHistory { turns: 1, place: 1 }));
}

#[test]
fn a_saved_turn_from_another_dialect_fails_as_transcript() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start_with_turns(
        Box::from(&include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..]),
        Some(Box::from([saved_turn(1, b"other")])),
    );
    world.settle();
    assert!(world.observations().contains(&Observation::HostAdmitted));
    assert!(world.observations().contains(&Observation::TranscriptFailed(smith_channel::TranscriptRefusal::Dialect)));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_run_resumed_from_a_transcript_restores_the_calls_answered_after_its_last_turn() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start_with_context_and_turns(
        Box::from(&include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..]),
        smith_host_domain::SavedReply::Host { error: true, body: Box::from(*b"retry") },
        Some(Box::from([saved_turn(1, b"00000000")])),
    );
    world.settle();
    assert!(world.observations().contains(&Observation::AgentHistory { turns: 1, place: 1 }));
    assert!(world.observations().contains(&Observation::AgentSavedHost {
        activation: 6,
        tool: Box::from(*b"check"),
        text: Box::from(*b"retry"),
        error: true,
    }));
}

#[test]
fn a_message_reaches_the_llm_with_its_senders_label() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let name = skein_lib::Token::new(31);
    world.send_message(name, Box::from(*b"Ada"), Box::from(*b"Please retry"));
    world.settle();
    assert!(world.observations().contains(&Observation::AgentMessage { name, text: Box::from(*b"Ada: Please retry") }));
    world.agent_waits(Some(name));
    world.settle();
    assert!(world.observations().contains(&Observation::HostWaiting { read: Some(name) }));
}

#[test]
fn a_long_operation_stretches_progress_until_its_end() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let span = skein_lib::Duration::from_nanos(42);
    world.agent_starts_long(span);
    world.settle();
    world.agent_ends_long();
    world.settle();
    assert!(world.observations().contains(&Observation::HostLong { span }));
    assert!(world.observations().contains(&Observation::HostLongDone));
}

#[test]
#[expect(clippy::wildcard_enum_match_arm, reason = "test selects one observed wire turn")]
fn a_concrete_turn_reaches_the_host_and_its_acknowledgement_returns() {
    use smith_domain_session::{llm, record};

    let turn = record::Turn {
        version: record::VERSION,
        endpoint: llm::Endpoint(0),
        dialect: 0,
        sequence: 1,
        usage: llm::Usage { input_tokens: 3, output_tokens: 5, cache_read_tokens: 1, cache_write_tokens: 2 },
        spent: 11,
        messages: Box::from([llm::Message {
            role: llm::Role::Assistant,
            content: Box::from([
                llm::Block::Text { text: Box::from(*b"hello"), replay: None },
                llm::Block::ToolCall {
                    id: Box::from(*b"id"),
                    name: Box::from(*b"check"),
                    input: Box::from(*b"{}"),
                    call: llm::Decoded::Historical,
                    replay: None,
                },
                llm::Block::ToolResult {
                    id: Box::from(*b"id"),
                    result: llm::Returned::Invalid { problem: llm::Problem::UnknownTool },
                },
            ]),
        }]),
    };
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let read = Some(skein_lib::Token::new(31));
    world.agent_tells_turn(1, read, &turn);
    world.settle();
    let body = world
        .observations()
        .iter()
        .find_map(|observation| match observation {
            Observation::HostTurn { number: 1, spent: 11, read: actual, body } if *actual == read => Some(body.clone()),
            _ => None,
        })
        .expect("one forwarded turn");
    let mut endpoints = skein_lib::List::with_capacity(1);
    endpoints
        .push(smith_protocol_channel::Endpoint { name: Box::default(), number: 0, dialect: 0, account: 0 })
        .expect("one endpoint");
    let transcript = smith_protocol_channel::decode_transcript(
        &[body],
        &smith_transcript::CEILINGS,
        &smith_protocol_channel::Endpoints::new(endpoints),
    )
    .expect("decoded turn")
    .expect("one turn");
    assert_eq!(transcript.turns.as_ref(), std::slice::from_ref(&turn));
    let mut second = turn.clone();
    second.sequence = 2;
    assert_eq!(world.try_agent_tells_turn(2, read, &second), Err(smith_protocol_channel::Error::Window));
    world.host_acknowledges(1);
    world.settle();
    assert!(world.observations().contains(&Observation::AgentAcknowledged { turn: 1 }));
    world.agent_tells_turn(2, read, &second);
    world.settle();
    assert!(
        world.observations().iter().any(|observation| matches!(observation, Observation::HostTurn { number: 2, .. }))
    );
}

#[test]
fn facts_are_dropped_under_pressure_and_waiting_still_passes() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let elapsed = skein_lib::Duration::from_nanos(7);
    let mut kept = 0_u64;
    for _ in 0..12 {
        let fact = smith_domain::Fact::Run {
            fact: smith_domain::run::facts::Fact::Admitted { run: skein_lib::Token::new(1) },
        };
        if world.agent_sends_fact(fact, elapsed) {
            kept += 1;
        }
    }
    assert!(kept > 0);
    assert_eq!(world.lost_facts(), 12 - kept);
    assert!(world.lost_facts() > 0);
    world.agent_waits(None);
    world.settle();
    assert!(world.observations().contains(&Observation::HostWaiting { read: None }));
    assert_eq!(
        world
            .observations()
            .iter()
            .filter(|observation| matches!(
                observation, Observation::HostFact { kind: smith_channel::FactKind::Admitted, elapsed: seen, count: 1 }
                    if *seen == elapsed
            ))
            .count(),
        usize::try_from(kept).expect("small fact count")
    );
}

#[test]
fn grants_refreshed_while_a_call_is_in_flight_reach_the_table() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start_with_context(
        Box::from(&include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..]),
        smith_host_domain::SavedReply::TooLarge,
    );
    world.settle();
    world.agent_admits();
    world.settle();
    world.agent_calls_host(
        smith_domain::run::CallName { activation: 7, completion: 1, position: 0 },
        smith_domain::run::RelayName { owner: skein_lib::Token::new(10), attempt: 1 },
    );
    world.settle();
    world.host_refreshes_grant(3, 5, Box::from(*b"new-secret"));
    world.settle();
    assert_eq!(world.agent_grant_value(3, 4), Some(Box::from(*b"secret")));
    assert_eq!(world.agent_grant_value(3, 5), Some(Box::from(*b"new-secret")));
    assert!(world.observations().contains(&Observation::AgentGrant {
        account: 3,
        generation: 5,
        valid: skein_lib::Duration::from_nanos(50),
    }));
    world.host_refreshes_grant(3, 6, Box::from(*b"latest"));
    world.settle();
    assert_eq!(world.agent_grant_value(3, 4), None);
    assert_eq!(world.agent_grant_value(3, 5), Some(Box::from(*b"new-secret")));
    assert_eq!(world.agent_grant_value(3, 6), Some(Box::from(*b"latest")));
}

#[test]
fn rejected_and_exhausted_notices_reach_the_host_without_values() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    world.agent_rejects_grant(3, 4);
    world.agent_exhausts_account(3, skein_lib::Duration::from_nanos(42));
    world.settle();
    assert!(world.observations().contains(&Observation::HostRejected { account: 3, generation: 4 }));
    assert!(
        world
            .observations()
            .contains(&Observation::HostExhausted { account: 3, retry_after: skein_lib::Duration::from_nanos(42) })
    );
}

#[test]
fn a_cancel_with_a_host_call_in_flight_keeps_its_terminal_and_one_answer() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let name = smith_domain::run::CallName { activation: 1, completion: 1, position: 0 };
    let relay = smith_domain::run::RelayName { owner: skein_lib::Token::new(41), attempt: 1 };
    world.agent_calls_host(name, relay);
    world.settle();
    world.host_cancels();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentCancel));
    world.host_answers(skein_lib::Token::new(1), smith_host_domain::channel::Reply::Busy);
    world.settle();
    assert!(world.observations().iter().any(|observation| matches!(
        observation, Observation::AgentHostReturned { relay: seen, .. } if *seen == relay
    )));
    world.agent_parks();
    world.settle();
    assert_eq!(
        world.observations().iter().filter(|observation| matches!(observation, Observation::HostParked { .. })).count(),
        1
    );
}

#[test]
fn host_tools_answered_busy_are_asked_again_under_their_names() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let name = smith_domain::run::CallName { activation: 1, completion: 1, position: 0 };
    let first = smith_domain::run::RelayName { owner: skein_lib::Token::new(40), attempt: 1 };
    world.agent_calls_host(name, first);
    world.settle();
    assert!(world.observations().contains(&Observation::HostCall {
        call: skein_lib::Token::new(1),
        name: smith_host_domain::channel::CallName { activation: 1, completion: 1, position: 0 },
        deadline: skein_lib::Time::from_nanos(100),
        ask: Box::new(smith_host_domain::channel::Ask::Host {
            tool: Box::from(*b"check"),
            effect: smith_host_domain::channel::Effect::Read,
            body: Box::from(*b"{}"),
        }),
    }));
    world.host_answers(skein_lib::Token::new(1), smith_host_domain::channel::Reply::Busy);
    world.settle();
    assert!(
        world
            .observations()
            .contains(&Observation::AgentHostReturned { relay: first, reply: smith_domain::run::HostReply::Busy })
    );
    let second = smith_domain::run::RelayName { owner: skein_lib::Token::new(40), attempt: 2 };
    world.agent_calls_host(name, second);
    world.settle();
    assert!(world.observations().contains(&Observation::HostCall {
        call: skein_lib::Token::new(2),
        name: smith_host_domain::channel::CallName { activation: 1, completion: 1, position: 0 },
        deadline: skein_lib::Time::from_nanos(100),
        ask: Box::new(smith_host_domain::channel::Ask::Host {
            tool: Box::from(*b"check"),
            effect: smith_host_domain::channel::Effect::Read,
            body: Box::from(*b"{}"),
        }),
    }));
    world.host_answers(
        skein_lib::Token::new(2),
        smith_host_domain::channel::Reply::Host { error: false, body: Box::from(*b"ok") },
    );
    world.settle();
    assert!(world.observations().contains(&Observation::AgentHostReturned {
        relay: second,
        reply: smith_domain::run::HostReply::Answered(
            smith_domain::run::HostAnswer::new(Box::from(*b"ok"), false).expect("bounded answer"),
        ),
    }));
}

#[test]
fn a_withdrawn_host_call_still_returns_under_its_live_relay() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let name = smith_domain::run::CallName { activation: 1, completion: 1, position: 0 };
    let relay = smith_domain::run::RelayName { owner: skein_lib::Token::new(40), attempt: 1 };
    world.agent_calls_host(name, relay);
    world.settle();
    world.agent_withdraws(relay);
    world.settle();
    assert!(world.observations().contains(&Observation::HostWithdraw { call: skein_lib::Token::new(1) }));
    world.host_answers(skein_lib::Token::new(1), smith_host_domain::channel::Reply::Withdrawn);
    world.settle();
    assert!(
        world
            .observations()
            .contains(&Observation::AgentHostReturned { relay, reply: smith_domain::run::HostReply::Withdrawn })
    );
}

#[test]
#[expect(clippy::wildcard_enum_match_arm, reason = "test extracts one observation from many event kinds")]
fn a_delivery_keeps_fields_and_a_settled_landing_or_stale_terminal() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let owner = skein_lib::Token::new(50);
    world.agent_delivers(smith_domain::run::CallName { activation: 1, completion: 1, position: 0 }, owner);
    world.settle();
    let fields = world.observations().iter().find_map(|observation| match observation {
        Observation::HostCall { call, ask: boxed, .. } if *call == skein_lib::Token::new(1) => match boxed.as_ref() {
            smith_host_domain::channel::Ask::Deliver { fields } => Some(fields.as_ref()),
            smith_host_domain::channel::Ask::Host { .. } => None,
        },
        _ => None,
    });
    let fields = fields.expect("host receives generic delivery fields");
    let decoded = smith_channel::DeliverAsk::decode(&CEILINGS, &mut skein_lib::Reader::new(fields))
        .expect("bounded structured fields");
    assert_eq!(decoded.fields().get(0).expect("title").name(), b"title");
    assert_eq!(decoded.fields().get(0).expect("title").text(), b"Fix");
    let receipt = smith_host_domain::Receipt::new(0, Box::from(*b"commit-1")).expect("bounded receipt");
    let landed = smith_host_domain::Delivered::new(Box::from([receipt])).expect("one receipt");
    world.host_answers(
        skein_lib::Token::new(1),
        smith_host_domain::channel::Reply::Delivery(smith_host_domain::Delivery::Delivered(landed)),
    );
    world.settle();
    let expected =
        smith_domain::run::Delivered::new(Box::from([
            smith_domain::run::Receipt::new(0, Box::from(*b"commit-1")).expect("bounded receipt")
        ]))
        .expect("one receipt");
    assert!(world.observations().contains(&Observation::AgentDelivered {
        owner,
        delivery: Box::new(smith_domain::run::Delivery::Delivered(expected)),
    }));
    let later = skein_lib::Token::new(51);
    world.agent_delivers(smith_domain::run::CallName { activation: 1, completion: 2, position: 0 }, later);
    world.settle();
    world.host_answers(
        skein_lib::Token::new(2),
        smith_host_domain::channel::Reply::Delivery(smith_host_domain::Delivery::Stale),
    );
    world.settle();
    assert!(world.observations().contains(&Observation::AgentDelivered {
        owner: later,
        delivery: Box::new(smith_domain::run::Delivery::Stale),
    }));
}
