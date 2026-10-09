//! Actual shared-fake root/run/session messages and V2 replay histories.
//! Contract: domain/run.md, sections 6, 10 and 13; domain/session.md, section 3;
//! testing-strategy.md, sections 2.3 and 7.

use skein_fake_llm_domain::api::Part;
use skein_lib::{Duration, Time, Token};
use skein_world::domain::Span;
use smith_agent_world::{Job, Settings, World};
use smith_domain::{Answered, AnsweredCall, run, session};

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
    }
}

fn text_seen(world: &World, expected: &[u8]) -> bool {
    world.prompts().iter().flat_map(|query| &query.messages).flat_map(|message| &message.parts).any(|part| match part {
        Part::Text { text } => text.as_ref() == expected,
        Part::Opaque { .. } | Part::ToolCall { .. } | Part::ToolOutput { .. } => false,
    })
}

#[test]
fn wait_result_settles_before_waiting_then_opaque_fifo_names_cross_only_turns() {
    let mut world = World::new(waiting(900));
    world.message_at(
        Time::from_nanos(500_000_000),
        Token::new(0),
        b"person".as_slice().into(),
        b"first".as_slice().into(),
    );
    world.message_at(
        Time::from_nanos(501_000_000),
        Token::new(99),
        b"person".as_slice().into(),
        b"second".as_slice().into(),
    );
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
    assert!(result, "wait terminal is saved as exact canonical feedback");
    assert_eq!(world.judged().1, 1);
}

#[test]
fn bounded_message_keeps_the_zero_name_and_wall_time_runs_while_waiting() {
    let calm = waiting(901);
    let limits =
        smith_domain::Limits { run: run::Limits { messages: 1, message_bytes: 12, ..calm.limits.run }, ..calm.limits };
    let mut world = World::new(Settings { limits, ..calm });
    world.message_at(
        Time::ZERO.saturating_add(Duration::from_millis(50)),
        Token::new(0),
        b"person".as_slice().into(),
        b"full".as_slice().into(),
    );
    world.run(2000);
    assert!(text_seen(&world, b"person: full"));
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
fn parked_transcript_resumes_without_recharging_history_or_reusing_activation_numbers() {
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
fn a_full_window_holds_the_next_completion_until_a_turn_is_acknowledged() {
    let mut world = World::with_window(waiting(923), smith_domain::Window { turns: 1, bytes: u64::MAX });
    let acknowledged = Time::ZERO.saturating_add(Duration::from_millis(500));
    world.acknowledge_at(acknowledged, 1);
    world.run(2000);
    assert!(matches!(world.answer(), run::Answer::Parked { turns: 2, .. }));
    assert_eq!(world.turns().len(), 2, "the told first turn remains in the transcript");
    let mut prompts = Vec::new();
    for (at, seen) in world.messages_seen() {
        if matches!(seen, smith_agent_world::messages_referee::Seen::Prompt { .. }) {
            prompts.push(*at);
        }
    }
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0] < acknowledged && prompts[1] >= acknowledged);
}

#[test]
fn a_window_too_small_for_one_turn_refuses_the_start() {
    let settings = waiting(924);
    let largest = smith_domain::max_turn_bytes(&settings.limits).expect("bounded turn");
    let mut world = World::with_window(settings, smith_domain::Window { turns: 1, bytes: largest - 1 });
    world.run(2000);
    assert_eq!(world.answer(), &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Window)));
    assert!(world.turns().is_empty() && world.prompts().is_empty());
}

#[test]
fn a_host_that_keeps_no_turns_acknowledges_each_at_once_and_never_holds_the_run() {
    let settings = waiting(925);
    let largest = smith_domain::max_turn_bytes(&settings.limits).expect("bounded turn");
    let mut world = World::with_window(settings, smith_domain::Window { turns: 1, bytes: largest });
    world.acknowledge_each_turn();
    world.run(2000);
    assert!(matches!(world.answer(), run::Answer::Parked { turns: 2, .. }));
    assert_eq!(world.turns().len(), 2);
    assert_eq!(world.prompts().len(), 2);
}

#[test]
fn a_run_parked_resumed_and_parked_again_numbers_turns_from_one_each_time_without_a_gap_in_the_conversation() {
    let mut first = World::new(waiting(920));
    first.run(2000);
    assert!(matches!(first.answer(), run::Answer::Parked { turns: 2, .. }));
    let first_numbers = first.turn_metadata().iter().map(|(number, _, _)| *number).collect::<Vec<_>>();
    assert_eq!(first_numbers, [1, 2]);
    assert_eq!(first.turns().iter().map(|turn| turn.sequence).collect::<Vec<_>>(), [1, 2]);

    let mut second = World::with_history(Settings { resume: true, ..waiting(921) }, Some(history(&first)));
    second.run(2000);
    assert!(matches!(second.answer(), run::Answer::Parked { turns: 2, .. }));
    assert_eq!(second.turn_metadata().iter().map(|(number, _, _)| *number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(second.turns().iter().map(|turn| turn.sequence).collect::<Vec<_>>(), [3, 4]);

    let mut continued = history(&first);
    let mut turns = continued.turns.into_vec();
    turns.extend_from_slice(second.turns());
    continued.turns = turns.into_boxed_slice();
    let mut third = World::with_history(Settings { resume: true, ..waiting(922) }, Some(continued));
    third.run(2000);
    assert!(matches!(third.answer(), run::Answer::Parked { turns: 2, .. }));
    assert_eq!(third.turn_metadata().iter().map(|(number, _, _)| *number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(third.turns().iter().map(|turn| turn.sequence).collect::<Vec<_>>(), [5, 6]);
}

#[test]
fn a_resumed_run_is_told_of_each_answer_its_transcript_lacks() {
    let mut first = World::new(Settings { job: Job::HostTools, ..Settings::calm(905) });
    first.run(2000);
    let turn = first.turns()[0].clone();
    let saved = smith_domain::Transcript {
        version: turn.version,
        endpoint: turn.endpoint,
        dialect: turn.dialect,
        turns: Box::new([turn]),
    };
    let mut next = World::with_history_answers(
        Settings { job: Job::HostTools, resume: true, ..Settings::calm(906) },
        Some(saved),
        Box::new([
            AnsweredCall {
                name: run::CallName { activation: 1, completion: 2, position: 0 },
                tool: b"message".as_slice().into(),
                answer: Answered::Host(
                    run::HostAnswer::new(b"opaque host answer: first decision".as_slice().into(), false)
                        .expect("saved result fits"),
                ),
            },
            AnsweredCall {
                name: run::CallName { activation: 1, completion: 3, position: 0 },
                tool: b"deliver".as_slice().into(),
                answer: Answered::Delivery(Box::new(run::Delivery::Nothing)),
            },
        ]),
    );
    next.run(2000);
    let prompt = &next.prompts()[0];
    assert!(next.host_submissions().is_empty(), "saved host calls are not decided again");
    assert!(prompt.messages.iter().any(|message| message.parts.iter().any(|part| {
        matches!(part, Part::Text { text } if text.windows(b"opaque host answer: first decision".len())
            .any(|window| window == b"opaque host answer: first decision"))
    })));
    assert!(prompt.messages.iter().any(|message| message.parts.iter().any(|part| {
        matches!(part, Part::Text { text } if text.windows(b"tool=deliver error: nothing".len())
            .any(|window| window == b"tool=deliver error: nothing"))
    })));
}

#[test]
fn a_resumed_run_is_told_its_saved_answer_was_too_large() {
    let mut world = World::with_history_answers(
        Settings { job: Job::HostTools, resume: true, ..Settings::calm(908) },
        None,
        Box::from([AnsweredCall {
            name: run::CallName { activation: 1, completion: 1, position: 0 },
            tool: b"check".as_slice().into(),
            answer: Answered::TooLarge,
        }]),
    );
    world.run(2000);
    let prompt = &world.prompts()[0];
    assert!(prompt.messages.iter().any(|message| message.parts.iter().any(|part| {
        matches!(part, Part::Text { text } if text.windows(b"host decided this call".len())
            .any(|window| window == b"host decided this call"))
    })));
    assert!(prompt.messages.iter().any(|message| message.parts.iter().any(|part| {
        matches!(part, Part::Text { text } if text.windows(b"answer was too large to give".len())
            .any(|window| window == b"answer was too large to give"))
    })));
}

#[test]
fn answers_too_many_or_too_long_are_refused_before_a_completion() {
    let answer = AnsweredCall {
        name: run::CallName { activation: 1, completion: 1, position: 0 },
        tool: b"deliver".as_slice().into(),
        answer: Answered::Delivery(Box::new(run::Delivery::Nothing)),
    };
    let calm = Settings::calm(907);
    for limits in [
        smith_domain::Limits { run: run::Limits { answered_calls: 0, ..calm.limits.run }, ..calm.limits },
        smith_domain::Limits { run: run::Limits { answered_bytes: 4, ..calm.limits.run }, ..calm.limits },
    ] {
        let mut world =
            World::with_history_answers(Settings { resume: true, limits, ..calm }, None, Box::new([answer.clone()]));
        world.run(2000);
        assert!(matches!(
            world.answer(),
            run::Answer::Failed { failure: run::Failure::Transcript(run::TranscriptRefusal::TooLarge), .. }
        ));
        assert!(world.prompts().is_empty());
    }
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
                                    source: smith_domain::session::ToolSource::Run,
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
                let text = saved
                    .turns
                    .iter_mut()
                    .flat_map(|turn| &mut turn.messages)
                    .flat_map(|message| &mut message.content)
                    .find_map(|block| match block {
                        session::llm::Block::Text { text, .. } => Some(text),
                        session::llm::Block::Refusal { .. }
                        | session::llm::Block::Opaque { .. }
                        | session::llm::Block::ToolCall { .. }
                        | session::llm::Block::ToolResult { .. } => None,
                    })
                    .expect("saved text");
                *text = vec![
                    b'x';
                    usize::try_from(waiting(907).limits.session.session_bytes + 1).expect("bounded fixture")
                ]
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
fn maximum_opaque_delivery_receipt_survives_continue_saved_turn_and_resumed_prompt_exactly() {
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
        "the host produced a maximum successful receipt"
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
