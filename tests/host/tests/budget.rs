//! Outside checks of scalar/overflow metadata fencing through actual host
//! channel rights (domain/host.md, section 6; testing-strategy.md, sections 6–7).
use smith_host_domain::{Answer, Event, Fault, RunFailure, RunResult, Turn, Up};
use smith_host_world::{Lower, World, limits, start};

fn commit(world: &mut World, number: u32) {
    world.event(Event::Acknowledge { agent: world.agent(), turn: number });
    while world.lower.contains(Lower::Send) {
        world.sent();
    }
}

fn recorded(number: u32, spent: u64, spend_overflow: bool, usage_overflow: bool) -> Up {
    Up::Turn {
        turn: Turn { number, spent, spend_overflow, usage_overflow, read: None, body: Box::from(&b"actual turn"[..]) },
    }
}

fn ended(turns: u32, spent: u64, spend_overflow: bool, usage_overflow: bool) -> Up {
    let failure = if spend_overflow { RunFailure::PriceOverflow } else { RunFailure::UsageOverflow };
    Up::Answer {
        answer: Answer {
            turns,
            completions: turns,
            input: u64::from(turns) * 11,
            output: u64::from(turns) * 13,
            cache_read: u64::from(turns) * 17,
            cache_write: u64::from(turns) * 19,
            spent,
            spend_overflow,
            usage_overflow,
            result: RunResult::Failed { failure },
        },
    }
}

#[test]
fn independent_unknown_prefixes_survive_commit_and_actual_final_word() {
    for (spend_overflow, usage_overflow) in [(true, false), (false, true), (true, true)] {
        let mut world = World::new(301, limits());
        world.live();
        world.up(recorded(1, 42, spend_overflow, usage_overflow));
        assert_eq!(world.seen.turn_metadata, [(1, 42, spend_overflow, usage_overflow)]);
        commit(&mut world, 1);
        assert!(world.seen.turns.is_empty());
        let final_spend = if spend_overflow { 42 } else { 57 };
        world.up(recorded(2, final_spend, spend_overflow, usage_overflow));
        assert_eq!(world.seen.turn_metadata[1], (2, final_spend, spend_overflow, usage_overflow));
        commit(&mut world, 2);
        world.up(ended(2, final_spend, spend_overflow, usage_overflow));
        let answer = world.seen.answer.as_ref().expect("actual final word reaches parent");
        assert_eq!(
            (answer.spent, answer.spend_overflow, answer.usage_overflow),
            (final_spend, spend_overflow, usage_overflow)
        );
        assert_eq!(world.seen.fault, None);
        world.cleanup();
        world.settled();
    }
}

#[test]
fn committed_unknown_prefix_cannot_regress_or_clear_in_turn_or_answer() {
    for financial in [false, true] {
        for final_word in [false, true] {
            for decrease in [false, true] {
                let mut world = World::new(302, limits());
                world.live();
                world.up(recorded(1, 42, financial, !financial));
                commit(&mut world, 1);
                let spent = if decrease { 41 } else { 42 };
                let currency = financial && decrease;
                let raw = !financial && decrease;
                let invalid =
                    if final_word { ended(1, spent, currency, raw) } else { recorded(2, spent, currency, raw) };
                world.up(invalid);
                assert_eq!(world.seen.fault, Some(Fault::Rules));
                assert!(world.seen.answer.is_none());
                assert_eq!(world.seen.turn_metadata.len(), 1, "no malformed next record reaches parent");
                world.cleanup();
                world.settled();
            }
        }
    }
}

#[test]
fn refused_start_cannot_claim_unknown_spend_or_usage() {
    for (spend_overflow, usage_overflow) in [(true, false), (false, true), (true, true)] {
        let mut world = World::new(303, limits());
        world.spawn(start());
        world.spawned();
        world.sent();
        world.up(Up::Answer {
            answer: Answer {
                turns: 0,
                completions: 0,
                input: 0,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                spent: 0,
                spend_overflow,
                usage_overflow,
                result: RunResult::Refused { detail: Box::new([]) },
            },
        });
        assert_eq!(world.seen.fault, Some(Fault::Rules));
        assert!(world.seen.answer.is_none());
        world.cleanup();
        world.settled();
    }
}

#[test]
fn every_final_accounting_field_reaches_parent_without_repricing_or_recounting() {
    for result in [
        RunResult::Accepted { outcome: Box::from(&b"actual outcome"[..]) },
        RunResult::Parked,
        RunResult::Failed { failure: RunFailure::PriceOverflow },
    ] {
        let mut world = World::new(304, limits());
        world.live();
        world.up(recorded(1, 42, false, false));
        commit(&mut world, 1);
        world.up(Up::Answer {
            answer: Answer {
                turns: 1,
                completions: 4,
                input: 0x0102_0304_0506_0708,
                output: 0x1112_1314_1516_1718,
                cache_read: 0x2122_2324_2526_2728,
                cache_write: 0x3132_3334_3536_3738,
                spent: 57,
                spend_overflow: false,
                usage_overflow: false,
                result,
            },
        });
        let actual = world.seen.answer.as_ref().expect("actual final word is forwarded");
        assert_eq!(actual.turns, 1);
        assert_eq!(actual.completions, 4, "child completions are independent of main Turns");
        assert_eq!(actual.input, 0x0102_0304_0506_0708);
        assert_eq!(actual.output, 0x1112_1314_1516_1718);
        assert_eq!(actual.cache_read, 0x2122_2324_2526_2728);
        assert_eq!(actual.cache_write, 0x3132_3334_3536_3738);
        assert_eq!(actual.spent, 57, "raw token totals never become host-unit spend");
        assert!(!actual.spend_overflow && !actual.usage_overflow);
        assert_eq!(world.seen.fault, None);
        world.cleanup();
        world.settled();
    }
}

#[test]
fn each_nonzero_refused_raw_counter_or_completion_count_breaks_the_rules() {
    for field in 0..5 {
        let mut world = World::new(305, limits());
        world.spawn(start());
        world.spawned();
        world.sent();
        world.up(Up::Answer {
            answer: Answer {
                turns: 0,
                completions: u32::from(field == 0),
                input: u64::from(field == 1),
                output: u64::from(field == 2),
                cache_read: u64::from(field == 3),
                cache_write: u64::from(field == 4),
                spent: 0,
                spend_overflow: false,
                usage_overflow: false,
                result: RunResult::Refused { detail: Box::new([]) },
            },
        });
        assert_eq!(world.seen.fault, Some(Fault::Rules), "refused nonzero field {field}");
        assert!(world.seen.answer.is_none());
        world.cleanup();
        world.settled();
    }
}

#[test]
fn a_raw_overflow_keeps_the_atomic_completion_prefix_below_main_turns() {
    for usage_overflow in [false, true] {
        let mut world = World::new(306, limits());
        world.live();
        world.up(recorded(1, 42, false, usage_overflow));
        commit(&mut world, 1);
        world.up(recorded(2, 57, false, usage_overflow));
        commit(&mut world, 2);
        world.up(Up::Answer {
            answer: Answer {
                turns: 2,
                completions: 1,
                input: u64::MAX,
                output: 13,
                cache_read: 17,
                cache_write: 19,
                spent: 57,
                spend_overflow: false,
                usage_overflow,
                result: RunResult::Failed { failure: RunFailure::UsageOverflow },
            },
        });
        if usage_overflow {
            let actual = world.seen.answer.as_ref().expect("the frozen prefix is legal after raw overflow");
            assert_eq!(actual.turns, 2);
            assert_eq!(actual.completions, 1);
            assert_eq!((actual.input, actual.output, actual.cache_read, actual.cache_write), (u64::MAX, 13, 17, 19));
            assert_eq!(actual.spent, 57, "representable units still advance independently");
            assert!(!actual.spend_overflow && actual.usage_overflow);
            assert_eq!(world.seen.fault, None);
        } else {
            assert_eq!(world.seen.fault, Some(Fault::Rules), "known completions cannot be fewer than main Turns");
            assert!(world.seen.answer.is_none());
        }
        world.cleanup();
        world.settled();
    }
}

#[test]
fn child_only_completions_and_maximum_raw_prefixes_need_no_main_turn() {
    let mut world = World::new(307, limits());
    world.live();
    world.up(Up::Answer {
        answer: Answer {
            turns: 0,
            completions: u32::MAX,
            input: u64::MAX,
            output: u64::MAX,
            cache_read: u64::MAX,
            cache_write: u64::MAX,
            spent: 0,
            spend_overflow: false,
            usage_overflow: false,
            result: RunResult::Parked,
        },
    });
    let actual = world.seen.answer.as_ref().expect("maximum numeric fields forward without heap storage");
    assert_eq!((actual.turns, actual.completions), (0, u32::MAX));
    assert_eq!(
        (actual.input, actual.output, actual.cache_read, actual.cache_write),
        (u64::MAX, u64::MAX, u64::MAX, u64::MAX)
    );
    assert_eq!(world.seen.fault, None);
    world.cleanup();
    world.settled();
}
