use smith_agent_process_world::{World, charter};
use smith_channel::{RunFailure, RunResult};

#[test]
fn a_run_goes_from_start_to_answer_and_the_process_exits_with_success() {
    let charter = charter();
    let mut world = World::new(7, &charter);
    assert!(world.settle(), "the admitted run answered");
    assert_eq!(world.observed().iter().filter(|frame| frame.kind == 0x0110).count(), 1);
    assert!(world.peer_replied(), "the fake LLM served a TLS response");
    assert!(world.peer_request().starts_with(b"POST "), "the agent sent an HTTP request");
    let answer = world.answer().expect("host saw final answer");
    assert_eq!(answer.turns(), 1);
    assert!(matches!(answer.result(), RunResult::Failed(_)), "one-turn budget ended the run");
    world.assert_agent_clean();
}

#[test]
fn a_termination_signal_in_the_middle_of_a_turn_answers_cancelled() {
    let charter = charter();
    let mut world = World::new(8, &charter);
    for _ in 0..100 {
        world.step();
        if world.observed().iter().any(|frame| frame.kind == 0x0106) {
            break;
        }
    }
    assert!(world.observed().iter().any(|frame| frame.kind == 0x0106), "run admitted before signal");
    world.signal();
    assert!(world.settle(), "cancellation was answered");
    let answer = world.answer().expect("host saw final answer");
    assert!(matches!(answer.result(), RunResult::Failed(failed) if failed.reason() == &RunFailure::Cancelled));
    world.assert_agent_clean();
}
