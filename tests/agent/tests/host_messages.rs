//! Live composition: each loop round drives the real root/run/session/shared
//! fake, then translates its actual outputs into the real host kit. Opaque turn
//! bytes use a test-only full Debug record, not a claimed production codec.
//! Parent commitment and lower IO rights remain independent of agent settlement.
//! Contract: domain/host.md, sections 6 and 9; domain/run.md, sections 6 and 13;
//! testing-strategy.md, sections 2.3, 6 and 7.

use skein_lib::{Duration, Time, Token};
use skein_world::domain::Span;
use smith_agent_world::{BUDGET, CompletionTerminal, Job, Settings, World as Agent, messages_referee::Seen};
use smith_domain::run;
use smith_host_domain::{self as host, Down, Event, RunResult, Up};
use smith_host_world::{Lower, World as Host};

fn bridge(host: &mut Host, agent: &mut Agent, seen: &(Time, Seen), woke: &mut bool) {
    let (at, seen) = seen;
    host.stage.tick(*at);
    match seen {
        Seen::Admitted => host.up(Up::Admitted),
        Seen::Turn { number, read, spent, turn } => {
            host.up(Up::Turn {
                turn: host::Turn {
                    number: *number,
                    read: *read,
                    spent: spent.units,
                    spend_overflow: spent.units_overflow,
                    usage_overflow: spent.usage_overflow,
                    body: format!("{turn:?}").into_bytes().into(),
                },
            });
            if *number == 2 {
                assert!(host.seen.turns.contains_key(&1));
                host.event(Event::Acknowledge { agent: host.agent(), turn: 2 });
                assert!(host.seen.turns.contains_key(&1), "ACK2 never commits the earlier parent-owned payload");
                host.sent();
            }
            if *number == 4 {
                host.event(Event::Acknowledge { agent: host.agent(), turn: 4 });
                assert!(host.lower.contains(Lower::Send), "actual final ACK send right stays outstanding");
            }
        }
        Seen::Waiting { read } => {
            host.up(Up::Waiting { read: *read });
            if !*woke {
                *woke = true;
                host.event(Event::Message {
                    agent: host.agent(),
                    name: Token::new(0),
                    body: b"person: live host".as_slice().into(),
                });
                let Down::Message { name, body } = host.seen.down.last().expect("actual downlink write") else {
                    panic!("parent input must become the actual Message Send")
                };
                agent.message_at(*at, *name, body.clone());
                host.sent();
            }
        }
        Seen::Answer { turns, parked, spent } => {
            assert!(*parked, "this actual agent finishes by idle parking");
            host.up(Up::Answer { answer: final_accounting(*turns, *spent, RunResult::Parked) });
        }
        Seen::Input { .. }
        | Seen::Bounced { .. }
        | Seen::Prompt { .. }
        | Seen::Completed { .. }
        | Seen::CompletionEnded => {}
    }
    assert!(host.seen.fault.is_none(), "actual composed metadata satisfies the host boundary");
}

#[test]
fn real_host_turn_ack_and_send_rights_survive_root_parking_and_exit_tree_empty_eof() {
    let calm = Settings::calm(940);
    let mut agent = Agent::new(Settings {
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
    let mut host = Host::new(
        940,
        host::Limits {
            turns: 16,
            turn_bytes: 65_536,
            unacknowledged_bytes: 16 * 65_536,
            no_progress: Duration::from_secs(10),
            ..smith_host_world::limits()
        },
    );
    let mut start = smith_host_world::start();
    start.transcript = None;
    start.answered = Box::new([]);
    host.spawn(start);
    host.spawned();
    assert!(!host.seen.admitted, "physical Started does not invent agent admission");
    host.sent();
    let mut next = 0;
    let mut woke = false;
    let mut done = false;
    for _ in 0..2000 {
        done = agent.drive(1);
        let new = agent.messages_seen()[next..].to_vec();
        next += new.len();
        for seen in &new {
            bridge(&mut host, &mut agent, seen, &mut woke);
        }
        if done {
            break;
        }
    }
    assert!(done && woke);
    assert_eq!(agent.turns().len(), 4);
    assert!(host.seen.answer.is_some(), "actual root answer was accepted after all actual Turns");
    let run::Answer::Parked { spent, turns } = agent.answer() else { panic!("the actual final root word is Parked") };
    assert_accounting(host.seen.answer.as_ref().expect("actual forwarded answer"), *turns, *spent);
    assert_eq!(host.seen.turns.keys().copied().collect::<Vec<_>>(), [1, 3]);
    let actual_first = format!("{:?}", agent.turns()[0]).into_bytes();
    assert_eq!(host.seen.turns[&1].as_ref(), actual_first.as_slice(), "parent retains the complete actual record");

    let owner = host.owner();
    host.event(Event::Exited { owner });
    assert!(host.seen.gone.is_none());
    host.event(Event::Reaped { owner, detail: b"actual tree empty".as_slice().into() });
    assert!(host.seen.gone.is_none());
    host.event(Event::Hangup { owner });
    assert!(host.seen.exited && host.seen.empty && host.seen.eof);
    assert!(host.seen.gone.is_none(), "tree/EOF cannot consume parent commitment or pending ACK Send rights");
    for turn in [1, 3] {
        host.event(Event::Acknowledge { agent: host.agent(), turn });
    }
    assert!(host.seen.turns.is_empty());
    assert!(host.seen.gone.is_none(), "parent committed all payloads, but actual ACK writes remain owed");
    for _ in 0..16 {
        if !host.lower.contains(Lower::Send) {
            break;
        }
        host.sent();
    }
    assert_eq!(host.seen.gone, Some(host::End::Stopped));
    host.settled();
}

/// Compare the forwarded final fields with the actual root, never `Turn` metadata.
/// Contract: domain/host.md, sections 6 and 9; domain/run.md, section 9.4.
fn assert_accounting(answer: &host::Answer, turns: u32, spent: run::Spend) {
    assert_eq!(answer.turns, turns, "only actual main Turns count as transmitted turns");
    assert_eq!(answer.completions, spent.turns, "global main and child completions move once");
    assert_eq!(answer.input, spent.input);
    assert_eq!(answer.output, spent.output);
    assert_eq!(answer.cache_read, spent.cache_read);
    assert_eq!(answer.cache_write, spent.cache_write);
    assert_eq!(answer.spent, spent.units);
    assert_eq!(answer.spend_overflow, spent.units_overflow);
    assert_eq!(answer.usage_overflow, spent.usage_overflow);
}

/// Translate the actual root final `Spend` without session usage or `Turn` metadata.
/// Contract: domain/host.md, sections 6 and 9; domain/run.md, section 9.4.
fn final_accounting(turns: u32, spent: run::Spend, result: RunResult) -> host::Answer {
    host::Answer {
        turns,
        completions: spent.turns,
        input: spent.input,
        output: spent.output,
        cache_read: spent.cache_read,
        cache_write: spent.cache_write,
        spent: spent.units,
        spend_overflow: spent.units_overflow,
        usage_overflow: spent.usage_overflow,
        result,
    }
}

#[test]
fn actual_child_completions_and_raw_usage_cross_the_host_final_answer_once() {
    let mut agent =
        Agent::new(Settings { job: Job::Spending, budget: run::Budget { turns: 8, ..BUDGET }, ..Settings::calm(4) });
    agent.run(20_000);
    let run::Answer::Failed { failure: run::Failure::Budget(run::Exhausted::Turns), spent, turns } = agent.answer()
    else {
        panic!("the original parallel-child budget story settles with its actual global prefix")
    };
    let spent = *spent;
    let turns = *turns;
    assert!(!spent.usage_overflow && !spent.units_overflow);
    assert!(spent.turns > turns, "actual child completions exceed transmitted main Turns");
    assert_raw_terminals(&agent, spent);
    let main = agent.turns().last().expect("actual settled main Turn");
    assert!(spent.input > main.usage.input_tokens && spent.output > main.usage.output_tokens);

    let mut host = Host::new(
        942,
        host::Limits {
            turns: 16,
            turn_bytes: 65_536,
            unacknowledged_bytes: 16 * 65_536,
            no_progress: Duration::from_secs(20),
            ..smith_host_world::limits()
        },
    );
    let mut start = smith_host_world::start();
    start.transcript = None;
    start.answered = Box::new([]);
    host.spawn(start);
    host.spawned();
    host.sent();
    for (at, seen) in agent.messages_seen() {
        host.stage.tick(*at);
        match seen {
            Seen::Admitted => host.up(Up::Admitted),
            Seen::Turn { number, read, spent, turn } => {
                host.up(Up::Turn {
                    turn: host::Turn {
                        number: *number,
                        read: *read,
                        spent: spent.units,
                        spend_overflow: spent.units_overflow,
                        usage_overflow: spent.usage_overflow,
                        body: format!("{turn:?}").into_bytes().into(),
                    },
                });
                host.event(Event::Acknowledge { agent: host.agent(), turn: *number });
                host.sent();
            }
            Seen::Answer { turns: observed, spent: actual, parked } => {
                assert_eq!((*observed, *actual, *parked), (turns, spent, false));
                let result = RunResult::Failed { failure: host::RunFailure::Budget(host::Exhausted::Turns) };
                host.up(Up::Answer { answer: final_accounting(turns, spent, result) });
            }
            Seen::Waiting { read } => host.up(Up::Waiting { read: *read }),
            Seen::Input { .. }
            | Seen::Bounced { .. }
            | Seen::Prompt { .. }
            | Seen::Completed { .. }
            | Seen::CompletionEnded => {}
        }
    }
    assert_accounting(host.seen.answer.as_ref().expect("actual global final reaches host parent"), turns, spent);
    assert_eq!(host.seen.fault, None);
    assert!(host.seen.turns.is_empty());
    host.cleanup();
    host.settled();
}

/// Sum actual provider winners independently of root/session accounting.
/// Failed/cancelled terminals contribute neither invented tokens nor completions.
/// Contract: domain/run.md, section 9; testing-strategy.md, sections 6 and 7.
fn assert_raw_terminals(agent: &Agent, spent: run::Spend) {
    let mut completions = 0;
    let mut input = 0;
    let mut output = 0;
    let mut cache_read = 0;
    let mut cache_write = 0;
    for completion in agent.completions() {
        match completion.terminal.expect("every actual provider right settled").1 {
            CompletionTerminal::Completed(usage) => {
                completions += 1;
                input += usage.input_tokens;
                output += usage.output_tokens;
                cache_read += usage.cache_read_tokens;
                cache_write += usage.cache_write_tokens;
            }
            CompletionTerminal::Failed | CompletionTerminal::Cancelled => {}
        }
    }
    assert_eq!(
        (spent.turns, spent.input, spent.output, spent.cache_read, spent.cache_write),
        (completions, input, output, cache_read, cache_write),
        "every main/child provider winner contributes once"
    );
}

/// Total producer mapping of the actual root reason, without payload inference.
/// Contract: domain/host.md, section 4.2; domain/run.md, section 6.1.
fn message_reason(reason: run::MessageRefusal) -> host::MessageRefusal {
    match reason {
        run::MessageRefusal::Busy => host::MessageRefusal::Busy,
        run::MessageRefusal::TooLarge => host::MessageRefusal::TooLarge,
        run::MessageRefusal::Inactive => host::MessageRefusal::Inactive,
        run::MessageRefusal::ReusedName => host::MessageRefusal::ReusedName,
    }
}

fn refusal_agent(seed: u64) -> Agent {
    let calm = Settings::calm(seed);
    let limits =
        smith_domain::Limits { run: run::Limits { messages: 1, message_bytes: 4, ..calm.limits.run }, ..calm.limits };
    Agent::new(Settings {
        job: Job::Waiting,
        waiting: Duration::from_secs(1),
        limits,
        network: Span::millis(0, 0),
        provider: skein_fake_llm_domain::Config {
            latency_min: Duration::from_millis(100),
            latency_max: Duration::from_millis(100),
            ..calm.provider
        },
        ..calm
    })
}

fn refusal_host(seed: u64) -> Host {
    let mut host = Host::new(
        seed,
        host::Limits { turns: 16, turn_bytes: 65_536, unacknowledged_bytes: 16 * 65_536, ..smith_host_world::limits() },
    );
    let mut start = smith_host_world::start();
    start.transcript = None;
    start.answered = Box::new([]);
    host.spawn(start);
    host.spawned();
    host.sent();
    host
}

/// Replay emitted root order and correlate each notice with its actual reason.
/// A preissued name was genuinely sent by this host before the delayed root input.
fn forward_refusal_record(
    host: &mut Host,
    agent: &Agent,
    record: &(Time, Seen),
    cursor: &mut usize,
    preissued: Option<Token>,
) {
    let (at, seen) = record;
    host.stage.tick(*at);
    match seen {
        Seen::Admitted => host.up(Up::Admitted),
        Seen::Input { name, text } => {
            if preissued == Some(*name) {
                assert!(host.seen.down.iter().any(
                    |message| matches!(message, Down::Message { name: actual, body } if actual == name && body == text)
                ));
            } else {
                host.event(Event::Message { agent: host.agent(), name: *name, body: text.clone() });
                assert!(
                    matches!(host.seen.down.last(), Some(Down::Message { name: actual, body }) if actual == name && body == text)
                );
                host.sent();
            }
        }
        Seen::Bounced { name } => {
            let (actual_name, reason) = agent.bounces()[*cursor];
            assert_eq!(actual_name, *name, "bounded cursor preserves actual emitted refusal order");
            *cursor += 1;
            host.up(Up::MessageBounced { name: *name, reason: message_reason(reason) });
        }
        Seen::Turn { number, read, spent, turn } => {
            host.up(Up::Turn {
                turn: host::Turn {
                    number: *number,
                    read: *read,
                    spent: spent.units,
                    spend_overflow: spent.units_overflow,
                    usage_overflow: spent.usage_overflow,
                    body: format!("{turn:?}").into_bytes().into(),
                },
            });
            host.event(Event::Acknowledge { agent: host.agent(), turn: *number });
            host.sent();
        }
        Seen::Waiting { read } => host.up(Up::Waiting { read: *read }),
        Seen::Answer { turns, parked, spent } => {
            assert!(*parked, "the genuine waiting scenario's final root word is Parked");
            host.up(Up::Answer { answer: final_accounting(*turns, *spent, RunResult::Parked) });
        }
        Seen::Prompt { .. } | Seen::Completed { .. } | Seen::CompletionEnded => {}
    }
}

#[test]
fn real_root_busy_and_too_large_settle_actual_host_issued_names_and_keep_accepted_input() {
    let mut agent = refusal_agent(943);
    for (millis, name, body) in [(50, 0, b"full".as_slice()), (51, 5, b"next".as_slice()), (53, 6, b"large".as_slice())]
    {
        agent.message_at(Time::ZERO.saturating_add(Duration::from_millis(millis)), Token::new(name), body.into());
    }
    agent.run(2000);
    assert_eq!(
        agent.bounces(),
        [(Token::new(5), run::MessageRefusal::Busy), (Token::new(6), run::MessageRefusal::TooLarge)]
    );
    let mut host = refusal_host(943);
    let mut cursor = 0;
    for record in agent.messages_seen() {
        forward_refusal_record(&mut host, &agent, record, &mut cursor, None);
    }
    assert_eq!(cursor, 2);
    assert_eq!(
        host.seen.message_bounces,
        [
            (Token::new(1), Token::new(5), host::MessageRefusal::Busy),
            (Token::new(1), Token::new(6), host::MessageRefusal::TooLarge),
        ]
    );
    assert!(host.seen.bounces.is_empty() && host.seen.fault.is_none());
    let run::Answer::Parked { spent, turns } = agent.answer() else { panic!("actual root parks") };
    assert_accounting(host.seen.answer.as_ref().expect("actual final forwarded"), *turns, *spent);
    assert_eq!(*turns, 4, "accepted name zero wakes main and its exact payload remains in the real transcript");
    host.cleanup();
    host.settled();
}

#[test]
fn an_actually_issued_delayed_input_keeps_its_real_inactive_refusal_after_root_answer() {
    let mut agent = refusal_agent(944);
    agent.message_at(Time::ZERO.saturating_add(Duration::from_secs(2)), Token::new(77), b"late".as_slice().into());
    agent.run(2000);
    assert_eq!(agent.bounces(), [(Token::new(77), run::MessageRefusal::Inactive)]);
    let mut host = refusal_host(944);
    let mut issued = false;
    let mut cursor = 0;
    for record in agent.messages_seen() {
        forward_refusal_record(&mut host, &agent, record, &mut cursor, Some(Token::new(77)));
        if matches!(&record.1, Seen::Waiting { .. }) && !issued {
            host.event(Event::Message { agent: host.agent(), name: Token::new(77), body: b"late".as_slice().into() });
            assert!(host.seen.answer.is_none(), "the real host issues before learning the final Answer");
            assert!(matches!(host.seen.down.last(), Some(Down::Message { name, .. }) if name.raw() == 77));
            host.sent();
            issued = true;
        }
        if matches!(&record.1, Seen::Bounced { .. }) {
            assert!(host.seen.answer.is_some(), "the actual ordered root refusal follows its final word");
        }
    }
    assert!(issued);
    assert_eq!(cursor, 1);
    assert_eq!(host.seen.message_bounces, [(Token::new(1), Token::new(77), host::MessageRefusal::Inactive)]);
    assert!(host.seen.fault.is_none() && host.seen.bounces.is_empty());
    let run::Answer::Parked { spent, turns } = agent.answer() else { panic!("actual root parks before delayed input") };
    assert_eq!(*turns, 2, "late rejected input never creates another completion or Turn");
    assert_accounting(host.seen.answer.as_ref().expect("unchanged final accounting"), *turns, *spent);
    host.cleanup();
    host.settled();
}

#[test]
fn actual_root_reused_name_maps_exactly_without_claiming_a_valid_parent_duplicate() {
    let mut agent = refusal_agent(945);
    for (millis, body) in [(50, b"full".as_slice()), (52, b"same".as_slice())] {
        agent.message_at(Time::ZERO.saturating_add(Duration::from_millis(millis)), Token::new(0), body.into());
    }
    agent.run(2000);
    assert_eq!(agent.bounces(), [(Token::new(0), run::MessageRefusal::ReusedName)]);
    let (_, actual) = agent.bounces()[0];
    assert_eq!(message_reason(actual), host::MessageRefusal::ReusedName);
    // A correct host namespace blocks this duplicate before it can be issued.
    // The separate host boundary controls cover all four issued-name reasons.
    let mut host = refusal_host(945);
    host.up(Up::Admitted);
    for body in [b"full".as_slice(), b"same".as_slice()] {
        host.event(Event::Message { agent: host.agent(), name: Token::new(0), body: body.into() });
    }
    assert_eq!(host.seen.bounces, [host::Bounce::ReusedName]);
    assert!(host.seen.message_bounces.is_empty());
    host.cleanup();
    host.settled();
}
