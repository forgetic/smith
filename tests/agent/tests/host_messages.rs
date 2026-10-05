//! Live composition: each loop round drives the real root/run/session/shared
//! fake, then translates its actual outputs into the real host kit. Opaque turn
//! bytes use a test-only full Debug record, not a claimed production codec.
//! Parent commitment and lower IO rights remain independent of agent settlement.
//! Contract: domain/host.md, sections 6 and 9; domain/run.md, sections 6 and 13;
//! testing-strategy.md, sections 2.3, 6 and 7.

use skein_lib::{Duration, Time, Token};
use skein_world::domain::Span;
use smith_agent_world::{Job, Settings, World as Agent, messages_referee::Seen};
use smith_host_domain::{self as host, Down, Event, RunResult, Up};
use smith_host_world::{Lower, World as Host};

fn bridge(host: &mut Host, agent: &mut Agent, seen: &(Time, Seen), woke: &mut bool) {
    let (at, seen) = seen;
    host.stage.tick(*at);
    match seen {
        Seen::Admitted => host.up(Up::Admitted),
        Seen::Turn { number, read, turn } => {
            host.up(Up::Turn {
                turn: host::Turn {
                    number: *number,
                    read: *read,
                    spent: turn.spent,
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
        Seen::Answer { turns, parked } => {
            assert!(*parked, "this actual agent finishes by idle parking");
            host.up(Up::Answer { answer: host::Answer { turns: *turns, spent: 0, result: RunResult::Parked } });
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
