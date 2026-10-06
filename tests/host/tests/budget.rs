//! Host channel fences exact, monotonic spend on turns and final answers.
use smith_host_domain::{Answer, Event, Fault, RunFailure, RunResult, Turn, Up};
use smith_host_world::{Lower, World, limits};

fn commit(world: &mut World, number: u32) {
    world.event(Event::Acknowledge { agent: world.agent(), turn: number });
    while world.lower.contains(Lower::Send) {
        world.sent();
    }
}

fn recorded(number: u32, spent: u64) -> Up {
    Up::Turn { turn: Turn { number, spent, read: None, body: Box::from(&b"actual turn"[..]) } }
}

fn ended(turns: u32, spent: u64) -> Up {
    Up::Answer { answer: Answer { turns, spent, result: RunResult::Failed { failure: RunFailure::PriceOverflow } } }
}

#[test]
fn charged_prefix_reaches_parent_with_typed_failure() {
    let mut world = World::new(301, limits());
    world.live();
    world.up(recorded(1, 42));
    assert_eq!(world.seen.turn_metadata, [(1, 42)]);
    commit(&mut world, 1);
    world.up(ended(1, 42));
    let answer = world.seen.answer.as_ref().expect("final word reaches parent");
    assert_eq!((answer.turns, answer.spent), (1, 42));
    assert_eq!(world.seen.fault, None);
    world.cleanup();
    world.settled();
}

#[test]
fn committed_spend_cannot_regress_in_turn_or_answer() {
    for final_word in [false, true] {
        let mut world = World::new(302, limits());
        world.live();
        world.up(recorded(1, 42));
        commit(&mut world, 1);
        world.up(if final_word { ended(1, 41) } else { recorded(2, 41) });
        assert_eq!(world.seen.fault, Some(Fault::Rules));
        assert!(world.seen.answer.is_none());
        assert_eq!(world.seen.turn_metadata.len(), 1);
        world.cleanup();
        world.settled();
    }
}

#[test]
fn exact_maximum_spend_is_forwarded() {
    let mut world = World::new(303, limits());
    world.live();
    world.up(ended(0, u64::MAX));
    assert_eq!(world.seen.answer.as_ref().expect("answer").spent, u64::MAX);
    assert_eq!(world.seen.fault, None);
    world.cleanup();
    world.settled();
}
