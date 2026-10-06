//! Live composition: each loop round drives the root/run/session/shared
//! fake, then translates its actual outputs into the host kit. Opaque turn
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
        Seen::Input { .. } | Seen::Prompt { .. } | Seen::Completed { .. } | Seen::CompletionEnded => {}
    }
    assert!(host.seen.fault.is_none(), "actual composed metadata satisfies the host boundary");
}

#[test]
fn host_turn_ack_and_send_rights_survive_root_parking_and_exit_tree_empty_eof() {
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
/// Contract: domain/host.md, sections 6 and 9; domain/run.md, section 9.
fn assert_accounting(answer: &host::Answer, turns: u32, spent: run::Spend) {
    assert_eq!(answer.turns, turns, "only actual main Turns count as transmitted turns");
    assert_eq!(answer.spent, spent.units);
}

/// Translate the actual root final `Spend` without session usage or `Turn` metadata.
/// Contract: domain/host.md, sections 6 and 9; domain/run.md, section 9.
fn final_accounting(turns: u32, spent: run::Spend, result: RunResult) -> host::Answer {
    host::Answer { turns, spent: spent.units, result }
}

#[test]
fn child_completions_and_raw_usage_cross_the_host_final_answer_once() {
    let mut agent =
        Agent::new(Settings { job: Job::Spending, budget: run::Budget { turns: 8, ..BUDGET }, ..Settings::calm(4) });
    agent.run(20_000);
    let run::Answer::Failed { failure: run::Failure::Budget(run::Exhausted::Turns), spent, turns } = agent.answer()
    else {
        panic!("the original parallel-child budget story settles with its actual global prefix")
    };
    let spent = *spent;
    let turns = *turns;
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
            Seen::Input { .. } | Seen::Prompt { .. } | Seen::Completed { .. } | Seen::CompletionEnded => {}
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
