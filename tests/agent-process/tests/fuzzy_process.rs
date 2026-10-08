//! Seeded process schedules across full answers and cancellation after admission.

use smith_agent_process_world::{World, charter};
use smith_channel::{RunFailure, RunResult};

#[test]
fn seeded_runs_answer_once_and_release_their_inherited_descriptors() {
    let charter = charter();
    for seed in 0..64_u64 {
        let mut world = World::new(seed, &charter);
        if seed % 2 == 0 {
            for _ in 0..100 {
                world.step();
                if world.observed().iter().any(|frame| frame.kind == 0x0106) {
                    break;
                }
            }
            assert!(world.observed().iter().any(|frame| frame.kind == 0x0106), "seed {seed} admitted");
            world.signal();
        }
        assert!(world.settle(), "seed {seed} answered");
        assert_eq!(world.observed().iter().filter(|frame| frame.kind == 0x0110).count(), 1, "seed {seed}");
        if seed % 2 == 0 {
            let answer = world.answer().expect("terminal answer");
            assert!(
                matches!(answer.result(), RunResult::Failed(failed) if failed.reason() == &RunFailure::Cancelled),
                "seed {seed} cancelled"
            );
        } else {
            assert!(world.peer_replied(), "seed {seed} received model response");
        }
        world.assert_agent_clean();
    }
}
