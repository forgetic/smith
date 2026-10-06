//! Actual shared-fake root/run/session messages and V2 replay histories.
//! Contract: domain/run.md, sections 6, 10 and 13; domain/session.md, section 3;
//! testing-strategy.md, sections 2.3 and 7.

use skein_fake_llm_domain::api::Part;
use skein_lib::{Duration, Time, Token};
use skein_world::domain::Span;
use smith_agent_world::{Job, Settings, World};
use smith_domain::{run, session};

fn waiting(seed: u64) -> Settings {
    let calm = Settings::calm(seed);
    Settings {
        job: Job::Waiting,
        waiting: Duration::from_secs(1),
        provider: skein_fake_llm_domain::Config {
            latency_min: Duration::from_millis(100),
            latency_max: Duration::from_millis(100),
            ..calm.provider
        },
        network: Span::millis(0, 0),
        ..calm
    }
}

fn history(world: &World) -> smith_domain::Transcript {
    let first = world.turns().first().expect("actual main output");
    smith_domain::Transcript {
        version: first.version,
        endpoint: first.endpoint,
        dialect: first.dialect,
        turns: world.turns().into(),
        after: Box::new([]),
    }
}

fn text_seen(world: &World, expected: &[u8]) -> bool {
    world.prompts().iter().flat_map(|query| &query.messages).flat_map(|message| &message.parts).any(|part| match part {
        Part::Text { text } => text.as_ref() == expected,
        Part::Opaque { .. } | Part::ToolCall { .. } | Part::ToolOutput { .. } => false,
    })
}

#[test]
fn actual_wait_result_settles_before_waiting_then_opaque_fifo_names_cross_only_real_turns() {
    let mut world = World::new(waiting(900));
    world.message_at(Time::from_nanos(500_000_000), Token::new(0), b"person: first".as_slice().into());
    world.message_at(Time::from_nanos(501_000_000), Token::new(99), b"person: second".as_slice().into());
    world.run(2000);
    assert!(matches!(world.answer(), run::Answer::Parked { turns: 6, .. }));
    assert_eq!(
        world.turn_metadata().iter().map(|(_, read, _)| *read).collect::<Vec<_>>(),
        [None, None, Some(Token::new(0)), Some(Token::new(0)), Some(Token::new(99)), Some(Token::new(99))]
    );
    assert_eq!(world.waiting().len(), 2, "fresh waiting and final empty-inbox waiting");
    assert_eq!(world.waiting()[0].1, None);
    assert_eq!(world.waiting()[1].1, Some(Token::new(99)));
    assert!(world.answered_at() >= world.waiting()[1].0.saturating_add(Duration::from_secs(1)));
    assert!(text_seen(&world, b"person: first") && text_seen(&world, b"person: second"));
    let result =
        world.turns().iter().flat_map(|turn| &turn.messages).flat_map(|message| &message.content).any(|block| {
            matches!(block, session::llm::Block::ToolResult {
            result: session::llm::Returned::Text { text, error: false, replay: None }, ..
        } if text.as_ref() == b"waiting")
        });
    assert!(result, "real wait terminal is saved as exact canonical feedback");
    assert_eq!(world.judged().1, 1);
}

#[test]
fn bounded_message_keeps_the_zero_name_and_wall_time_runs_while_waiting() {
    let calm = waiting(901);
    let limits =
        smith_domain::Limits { run: run::Limits { messages: 1, message_bytes: 4, ..calm.limits.run }, ..calm.limits };
    let mut world = World::new(Settings { limits, ..calm });
    world.message_at(Time::ZERO.saturating_add(Duration::from_millis(50)), Token::new(0), b"full".as_slice().into());
    world.run(2000);
    assert!(text_seen(&world, b"full"));
    assert!(matches!(world.answer(), run::Answer::Parked { turns: 4, .. }));
    let mut wall = World::new(Settings {
        waiting: Duration::from_secs(20),
        budget: run::Budget { time: Duration::from_secs(2), ..calm.budget },
        ..calm
    });
    wall.run(2000);
    assert_eq!(wall.waiting().len(), 1);
    assert!(matches!(
        wall.answer(),
        run::Answer::Failed { failure: run::Failure::Budget(run::Exhausted::Time), turns: 2, .. }
    ));
}

#[test]
fn actual_parked_transcript_resumes_without_recharging_history_or_reusing_activation_numbers() {
    let mut first = World::new(waiting(902));
    first.run(2000);
    let saved = history(&first);
    let mut next = World::with_history(Settings { resume: true, ..waiting(903) }, Some(saved.clone()));
    next.run(2000);
    assert!(matches!(next.answer(), run::Answer::Parked { turns: 2, .. }));
    assert_eq!(next.turn_metadata().iter().map(|(number, _, _)| *number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(next.turns().iter().map(|turn| turn.sequence).collect::<Vec<_>>(), [3, 4]);
    assert_eq!(next.prompts().len(), 2, "shared fake continues after restored assistant prefix");
    assert!(text_seen(&next, b"Ready for a person."));
    assert_eq!(
        next.turns()[0].usage.input_tokens,
        next.turn_metadata()[0].2.input,
        "token budget charges only this activation's actual completion"
    );
    let mut ignored = World::with_history(waiting(904), Some(saved));
    ignored.run(2000);
    assert_eq!(ignored.turns()[0].sequence, 1, "false resume explicitly starts fresh");
}

#[test]
fn real_host_result_committed_after_last_transcript_restores_before_wake_without_repeating_effect() {
    let mut first = World::new(Settings { job: Job::HostTools, ..Settings::calm(905) });
    first.run(2000);
    let mut turn = first.turns()[0].clone();
    let mut messages = turn.messages.into_vec();
    let actual_result = messages.pop().expect("actual terminal user message");
    assert!(actual_result.content.iter().any(|block| matches!(block, session::llm::Block::ToolResult {
        result: session::llm::Returned::Text { text, error: false, .. }, ..
    } if text.as_ref() == b"opaque host answer: first decision")));
    turn.messages = messages.into();
    let saved = smith_domain::Transcript {
        version: turn.version,
        endpoint: turn.endpoint,
        dialect: turn.dialect,
        turns: Box::new([turn]),
        after: Box::new([actual_result.clone()]),
    };
    let mut next =
        World::with_history(Settings { job: Job::HostTools, resume: true, ..Settings::calm(906) }, Some(saved));
    next.run(2000);
    assert!(matches!(
        next.answer(),
        run::Answer::Accepted { outcome: run::outcome::Declared::Report(_), turns: 1, .. }
    ));
    assert!(next.host_submissions().is_empty(), "restored real answer prevents duplicate host effect");
    assert!(next.prompts()[0].messages.iter().any(|message| {
        message.parts.iter().any(|part| {
            matches!(part,
        Part::ToolOutput { output, is_error: false, .. } if output.as_ref() == b"opaque host answer: first decision")
        })
    }));
    assert_eq!(next.turns()[0].sequence, 2);
}

#[test]
fn every_transient_history_refusal_is_exact_and_starts_no_provider_or_tool_effect() {
    let mut first = World::new(waiting(907));
    first.run(2000);
    for reason in [
        run::TranscriptRefusal::Version,
        run::TranscriptRefusal::Endpoint,
        run::TranscriptRefusal::Dialect,
        run::TranscriptRefusal::Malformed,
        run::TranscriptRefusal::Unresolved,
        run::TranscriptRefusal::TooLarge,
    ] {
        let mut saved = history(&first);
        match reason {
            run::TranscriptRefusal::Version => saved.version = 0,
            run::TranscriptRefusal::Endpoint => saved.endpoint = session::llm::Endpoint(999),
            run::TranscriptRefusal::Dialect => saved.dialect = 999,
            run::TranscriptRefusal::Malformed => saved.turns[0].sequence = 0,
            run::TranscriptRefusal::Unresolved => {
                for message in &mut saved.turns[0].messages {
                    for block in &mut message.content {
                        match block {
                            session::llm::Block::ToolCall { call, .. } => {
                                *call = session::llm::Decoded::Delegated {
                                    ticket: Token::new(80),
                                    effect: smith_domain::tools::Effect::Write,
                                }
                            }
                            session::llm::Block::Text { .. }
                            | session::llm::Block::Refusal { .. }
                            | session::llm::Block::Opaque { .. }
                            | session::llm::Block::ToolResult { .. } => {}
                        }
                    }
                }
            }
            run::TranscriptRefusal::TooLarge => {
                saved.after = vec![session::llm::Message {
                    role: session::llm::Role::User,
                    content: Box::new([session::llm::Block::Text {
                        text: vec![
                            b'x';
                            usize::try_from(waiting(907).limits.session.session_bytes + 1)
                                .expect("bounded fixture")
                        ]
                        .into(),
                        replay: None,
                    }]),
                }]
                .into();
            }
        }
        let mut world = World::with_history(Settings { resume: true, ..waiting(908) }, Some(saved));
        world.run(2000);
        assert!(
            matches!(world.answer(), run::Answer::Failed { failure: run::Failure::Transcript(actual), turns: 0, .. } if *actual == reason)
        );
        assert!(world.prompts().is_empty() && world.turns().is_empty() && world.host_submissions().is_empty());
        assert!(world.checked().is_empty() && world.pushes().is_empty());
    }
}

#[test]
fn maximum_actual_opaque_delivery_receipt_survives_continue_saved_turn_and_resumed_prompt_exactly() {
    let mut world = World::new(Settings {
        job: Job::MidReport,
        push: smith_agent_world::HostReply::OpaqueDelivered,
        ..Settings::calm(909)
    });
    world.run(2000);
    assert!(matches!(world.answer(), run::Answer::Accepted { outcome: run::outcome::Declared::Report(_), .. }));
    let mut expected = b"delivered\nreceipt directory=0 text=\"".to_vec();
    for _ in 0..run::Receipt::CAPACITY {
        expected.extend_from_slice(b"\\xff");
    }
    expected.push(b'"');
    assert!(
        world.pushes().iter().any(|push| match push {
            run::Delivery::Delivered(receipts) => receipts.receipts()[0].text() == vec![0xff; run::Receipt::CAPACITY],
            run::Delivery::Nothing | run::Delivery::Refused(_) | run::Delivery::Failed(_) | run::Delivery::Stale =>
                false,
        }),
        "the host produced a real maximum successful receipt"
    );
    assert!(world.prompts().iter().flat_map(|query| &query.messages).flat_map(|message| &message.parts).any(|part| {
        matches!(part, Part::ToolOutput { output, is_error: false, .. } if output.as_ref() == expected)
    }));
    let saved = history(&world);
    assert!(saved.turns.iter().flat_map(|turn| &turn.messages).flat_map(|message| &message.content).any(|block| {
        matches!(block, session::llm::Block::ToolResult { result: session::llm::Returned::Text { text, error: false, replay: None }, .. } if text.as_ref() == expected)
    }));
    let mut resumed =
        World::with_history(Settings { job: Job::MidReport, resume: true, ..Settings::calm(910) }, Some(saved));
    resumed.run(2000);
    assert!(
        resumed.prompts()[0].messages.iter().flat_map(|message| &message.parts).any(|part| {
            matches!(part, Part::ToolOutput { output, is_error: false, .. } if output.as_ref() == expected)
        }),
        "exact ASCII expansion remains in the actual resumed provider prompt"
    );
    assert!(resumed.pushes().is_empty(), "restored actual result does not repeat its delivery effect");
}
