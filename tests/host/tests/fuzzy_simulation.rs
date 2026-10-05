//! Seeded full-V2 endings and crossed terminal paths, with shared schedules and
//! exactly-once ledgers (testing-strategy.md, sections 3 and 6; domain/host.md, 10).
use skein_lib::{Duration, Rng, Token};
use smith_host_domain::{
    Answer, Ask, CallName, Delivery, Effect, Event, Fault, ModelFault, Reply, RunFailure, RunResult, Up,
};
use smith_host_world::{World, limits, start};
use std::collections::BTreeSet;

#[test]
fn random_worlds_settle_and_reach_every_ending() {
    let mut reached = BTreeSet::new();
    for seed in 1..=240 {
        let mut rng = Rng::new(seed);
        let fate = rng.between(0, 10);
        let mut bounds = limits();
        bounds.wall_time = Duration::from_secs(20);
        let mut world = World::new(seed, bounds);
        world.spawn(start());
        if fate == 0 {
            world.event(Event::Unspawned { owner: world.owner(), detail: Box::new([]) });
        } else {
            world.spawned();
            world.sent();
            if fate == 1 {
                world.up(Up::Answer {
                    answer: Answer { turns: 0, spent: 0, result: RunResult::Refused { detail: Box::new([]) } },
                });
            } else {
                world.up(Up::Admitted);
                // Stable names, generic effects and actual late terminals all
                // cross the real host boundary; tree exit cannot erase them.
                if seed.is_multiple_of(3) && ![2, 3, 4, 7, 8, 10].contains(&fate) {
                    world.up(Up::Call {
                        call: Token::new(20),
                        name: CallName { completion: 1, position: 2 },
                        deadline: world.stage.env.now.saturating_add(Duration::from_secs(5)),
                        ask: Ask::Deliver { fields: Box::new([]) },
                    });
                    world.up(Up::Withdraw { call: Token::new(20) });
                } else {
                    world.up(Up::Call {
                        call: Token::new(20),
                        name: CallName { completion: 1, position: 2 },
                        deadline: world.stage.env.now.saturating_add(Duration::from_secs(5)),
                        ask: Ask::Host { tool: Box::from(&b"tool"[..]), effect: Effect::Read, body: Box::new([]) },
                    });
                }
                match fate {
                    2 => world.up(Up::Answer {
                        answer: Answer { turns: 0, spent: 2, result: RunResult::Accepted { outcome: Box::new([]) } },
                    }),
                    3 => world.up(Up::Answer { answer: Answer { turns: 0, spent: 3, result: RunResult::Parked } }),
                    4 => world.up(Up::Answer {
                        answer: Answer {
                            turns: 0,
                            spent: 4,
                            result: RunResult::Failed { failure: RunFailure::Model(ModelFault::Provider) },
                        },
                    }),
                    5 => world.event(Event::Malformed { owner: world.owner() }),
                    6 => {
                        world.at(5);
                        world.at(15);
                        assert_eq!(world.seen.fault, Some(Fault::NoProgress));
                    }
                    7 => {
                        world.up(Up::Waiting { read: None });
                        world.at(20);
                        world.sent();
                        world.up(Up::Answer {
                            answer: Answer {
                                turns: 0,
                                spent: 7,
                                result: RunResult::Failed { failure: RunFailure::Cancelled },
                            },
                        });
                        assert_eq!(world.seen.fault, Some(Fault::WallTime));
                    }
                    8 => {
                        world.event(Event::Stop { agent: world.agent() });
                        world.sent();
                        world.up(Up::Answer {
                            answer: Answer {
                                turns: 0,
                                spent: 8,
                                result: RunResult::Failed { failure: RunFailure::Cancelled },
                            },
                        });
                    }
                    9 => world.event(Event::Hangup { owner: world.owner() }),
                    10 => {
                        world.event(Event::Exited { owner: world.owner() });
                        world.up(Up::Answer { answer: Answer { turns: 0, spent: 10, result: RunResult::Parked } });
                    }
                    0 | 1 => unreachable!("handled pre-admission"),
                    _ => unreachable!("bounded fate"),
                }
                settle_actual_parent_call(&mut world, seed, fate);
            }
            world.cleanup();
        }
        world.settled();
        assert_ending(&world, fate, seed);
        reached.insert(fate);
    }
    assert_eq!(reached, (0..=10).collect());
}

fn assert_ending(world: &World, fate: u64, seed: u64) {
    match fate {
        0 => assert_eq!(world.seen.gone, Some(smith_host_domain::End::Unspawned)),
        1 | 2 | 3 | 4 | 8 | 10 => {
            assert_eq!(world.seen.fault, None, "positive fate {fate}, seed {seed}");
            let result = &world.seen.answer.as_ref().expect("real agent last word").result;
            match result {
                RunResult::Refused { .. } => assert_eq!(fate, 1),
                RunResult::Accepted { .. } => assert_eq!(fate, 2),
                RunResult::Parked => assert!([3, 10].contains(&fate)),
                RunResult::Failed { failure } => {
                    assert!([4, 8].contains(&fate), "Failed cannot replace another positive ending");
                    assert_eq!(
                        *failure,
                        if fate == 4 { RunFailure::Model(ModelFault::Provider) } else { RunFailure::Cancelled }
                    );
                }
                RunResult::Delivered { .. } => panic!("no landing in this scripted fate"),
            }
        }
        5 => assert_eq!(world.seen.fault, Some(Fault::Rules)),
        6 => assert_eq!(world.seen.fault, Some(Fault::NoProgress)),
        7 => assert_eq!(world.seen.fault, Some(Fault::WallTime)),
        9 => assert_eq!(world.seen.fault, Some(Fault::Exited)),
        _ => unreachable!("bounded fates"),
    }
}

fn settle_actual_parent_call(world: &mut World, seed: u64, fate: u64) {
    world.cleanup();
    assert_eq!(world.seen.gone, None, "actual parent call terminal survives all tree cleanup");
    let reply = if seed.is_multiple_of(3) && ![2, 3, 4, 7, 8, 10].contains(&fate) {
        Reply::Delivery(Delivery::Nothing)
    } else {
        Reply::Unavailable
    };
    world.event(Event::Answer { agent: world.agent(), call: Token::new(20), reply });
}
