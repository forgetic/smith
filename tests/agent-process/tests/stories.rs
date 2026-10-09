use smith_agent_process_world::{World, charter};
use smith_channel::{RunFailure, RunResult};

#[test]
fn a_run_goes_from_start_to_answer_and_the_process_exits_with_success() {
    let charter = charter();
    let mut world = World::new(7, &charter);
    assert!(world.settle(), "the admitted run answered");
    assert_eq!(world.observed().iter().filter(|frame| frame.kind == 0x0110).count(), 1);
    assert!(world.peer_replied(), "the fake LLM served a plaintext response: {:?}", world.peer_observations());
    assert_eq!(world.peer_queries().len(), 1, "the independent peer decoded one model request");
    let answer = world.answer().expect("host saw final answer");
    assert_eq!(answer.turns(), 1);
    assert!(matches!(answer.result(), RunResult::Failed(_)), "one-turn budget ended the run");
    world.assert_agent_clean();
}

#[test]
fn a_termination_signal_in_the_middle_of_a_turn_answers_cancelled() {
    let charter = charter();
    let mut world = World::new(8, &charter);
    world.signal();
    assert!(world.settle(), "cancellation was answered");
    assert!(world.observed().iter().any(|frame| frame.kind == 0x0106), "run admitted before signal");
    let answer = world.answer().expect("host saw final answer");
    assert!(matches!(answer.result(), RunResult::Failed(failed) if failed.reason() == &RunFailure::Cancelled));
    world.assert_agent_clean();
}

#[test]
fn full_kernel_replay_and_discarded_facts_leave_the_same_answer() {
    for cancel in [false, true] {
        skein_world::domain::assert_replays(7, 8, |seed| {
            let mut world = World::new(seed, &charter());
            if cancel {
                world.signal();
            }
            assert!(world.settle());
            (world.trace(), world.answer())
        });
        let mut kept = World::new(7, &charter());
        let mut discarded = World::new(7, &charter());
        discarded.discard_facts();
        if cancel {
            kept.signal();
            discarded.signal();
        }
        assert!(kept.settle() && discarded.settle());
        assert_eq!(kept.trace(), discarded.trace(), "outside facts change no operation");
        assert_eq!(kept.observed(), discarded.observed());
    }
}

#[test]
fn answered_agent_settles_a_live_provider_before_the_idle_keep_deadline() {
    skein_world::domain::assert_replays(19, 2, |seed| {
        let mut world = World::new(seed, &charter());
        assert!(world.settle(), "the hosted agent exits normally");
        world.assert_agent_clean();
        assert!(world.settled_at() < skein_lib::Time::from_nanos(1_000_000_000));
        assert_eq!(world.peer_queries().len(), 1);
        (world.trace(), world.answer())
    });
}
