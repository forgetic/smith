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
        answer: Answer { turns, spent, spend_overflow, usage_overflow, result: RunResult::Failed { failure } },
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
