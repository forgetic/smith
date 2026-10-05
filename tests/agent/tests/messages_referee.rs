//! Positive-first mutations of actual root/shared-fake chat output. The oracle
//! uses public input, provider terminals and emitted Turn/Waiting/Answer only.
//! Contract: domain/run.md, sections 6/13; testing-strategy.md, section 7.

use skein_fake_llm_domain::api::Part;
use skein_lib::{Duration, Time, Token};
use skein_world::domain::Span;
use skein_world::domain::{Referee, Verdict};
use smith_agent_world::{
    Job, Settings, World,
    messages_referee::{Meeting, Seen},
};

fn observed() -> Vec<(Time, Seen)> {
    let calm = Settings::calm(920);
    let mut world = World::new(Settings {
        job: Job::Waiting,
        waiting: Duration::from_secs(1),
        network: Span::millis(0, 0),
        provider: skein_fake_llm_domain::Config {
            latency_min: Duration::from_millis(100),
            latency_max: Duration::from_millis(100),
            ..calm.provider
        },
        ..calm
    });
    world.message_at(Time::from_nanos(500_000_000), Token::new(0), b"person: first".as_slice().into());
    world.message_at(Time::from_nanos(501_000_000), Token::new(99), b"person: second".as_slice().into());
    world.run(2000);
    let actual = world.messages_seen().to_vec();
    assert_eq!(judge(&actual), Verdict::Passed, "positive comes from actual forwarded root output");
    actual
}

fn judge(observations: &[(Time, Seen)]) -> Verdict {
    let mut referee = Referee::new(Meeting::new(0, Duration::from_secs(1)));
    for (at, seen) in observations {
        referee.observe(*at, seen.clone(), &mut Vec::new());
    }
    referee.verdict()
}

#[test]
fn actual_chat_positive_and_omitted_reordered_or_corrupted_turns() {
    let good = observed();
    for corruption in 0..5 {
        let mut bad = good.clone();
        let turn =
            bad.iter().position(|(_, seen)| matches!(seen, Seen::Turn { number: 3, .. })).expect("real third output");
        if corruption == 0 {
            bad.remove(turn);
        } else {
            let (number, read, body) = turn_fields(&mut bad[turn].1);
            match corruption {
                1 => *number = 2,
                2 => *read = Some(Token::new(99)),
                3 => body.sequence += 1,
                4 => {
                    let assistant = body
                        .messages
                        .iter_mut()
                        .find(|message| message.role == smith_domain::llm::Role::Assistant)
                        .expect("actual assistant");
                    match &mut assistant.content[0] {
                        smith_domain::session::llm::Block::ToolCall { id, .. } => {
                            *id = b"invented-id".as_slice().into()
                        }
                        smith_domain::session::llm::Block::Text { .. }
                        | smith_domain::session::llm::Block::Refusal { .. }
                        | smith_domain::session::llm::Block::Opaque { .. }
                        | smith_domain::session::llm::Block::ToolResult { .. } => {
                            panic!("third completion actually calls wait")
                        }
                    }
                }
                _ => unreachable!("five precise changes"),
            }
        }
        assert!(matches!(judge(&bad), Verdict::Failed(_)), "turn corruption {corruption} rejected after real positive");
    }
}

#[test]
fn premature_waiting_wrong_wake_text_and_wrong_final_count_or_park_time_are_rejected() {
    let good = observed();
    for corruption in 0..5 {
        let mut bad = good.clone();
        match corruption {
            0 => {
                let pending = bad
                    .iter()
                    .position(|(_, seen)| matches!(seen, Seen::Completed { .. }))
                    .expect("actual provider terminal");
                bad.insert(pending, (bad[pending].0, Seen::Waiting { read: None }));
            }
            1 => {
                let prompt = bad.iter_mut().find_map(|(_, seen)| match seen {
                    Seen::Prompt { query } if query.messages.last().is_some_and(|message| matches!(message.parts.as_slice(), [Part::Text { text }] if text.as_ref() == b"person: first")) => Some(query),
                    Seen::Admitted | Seen::Input { .. } | Seen::Bounced { .. } | Seen::Prompt { .. } | Seen::Completed { .. }
                    | Seen::CompletionEnded | Seen::Turn { .. } | Seen::Waiting { .. } | Seen::Answer { .. } => None,
                }).expect("actual first wake request");
                prompt.messages.last_mut().expect("wake").parts[0] =
                    Part::Text { text: b"person: second".as_slice().into() };
            }
            2 => match &mut bad.last_mut().expect("actual final word").1 {
                Seen::Answer { turns, .. } => *turns -= 1,
                Seen::Admitted
                | Seen::Input { .. }
                | Seen::Bounced { .. }
                | Seen::Prompt { .. }
                | Seen::Completed { .. }
                | Seen::CompletionEnded
                | Seen::Turn { .. }
                | Seen::Waiting { .. } => {
                    panic!("actual final output")
                }
            },
            3 => {
                let waiting =
                    bad.iter().rfind(|(_, seen)| matches!(seen, Seen::Waiting { .. })).expect("actual final wait").0;
                bad.last_mut().expect("actual final word").0 = waiting;
            }
            4 => {
                let turn = bad
                    .iter()
                    .position(|(_, seen)| matches!(seen, Seen::Turn { number: 1, .. }))
                    .expect("actual first wait result");
                let (_, _, turn) = turn_fields(&mut bad[turn].1);
                let block = turn
                    .messages
                    .iter_mut()
                    .flat_map(|message| &mut message.content)
                    .find(|block| matches!(block, smith_domain::session::llm::Block::ToolResult { .. }))
                    .expect("actual settled wait");
                match block {
                    smith_domain::session::llm::Block::ToolResult { result, .. } => {
                        *result = smith_domain::session::llm::Returned::NotRun;
                    }
                    smith_domain::session::llm::Block::Text { .. }
                    | smith_domain::session::llm::Block::Refusal { .. }
                    | smith_domain::session::llm::Block::Opaque { .. }
                    | smith_domain::session::llm::Block::ToolCall { .. } => panic!("selected result"),
                }
            }
            _ => unreachable!("five changes"),
        }
        assert!(matches!(judge(&bad), Verdict::Failed(_)), "chronology corruption {corruption} rejected");
    }
}

fn turn_fields(seen: &mut Seen) -> (&mut u32, &mut Option<Token>, &mut smith_domain::session::record::Turn) {
    match seen {
        Seen::Turn { number, read, turn } => (number, read, turn),
        Seen::Admitted
        | Seen::Input { .. }
        | Seen::Bounced { .. }
        | Seen::Prompt { .. }
        | Seen::Completed { .. }
        | Seen::CompletionEnded
        | Seen::Waiting { .. }
        | Seen::Answer { .. } => {
            panic!("selected observed turn")
        }
    }
}
