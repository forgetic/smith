//! Seeded short IO, completion delays and cancellation races in the real
//! shared shell, replayed using Skein's trace and referee machinery.
use smith_local_process_world::{
    Files, Placement, World,
    referee::{Ending, review},
};

#[test]
fn seeded_hosted_and_colocated_runs_settle_and_replay() {
    for seed in 100..116 {
        for placement in [Placement::Spawned, Placement::InProcess] {
            let mut previous = None;
            for _ in 0..2 {
                let files = Files::new();
                let cancelled = seed % 4 == 0;
                let changed = seed % 8 == 1;
                let mut world = if changed {
                    World::changed(seed, &files, placement)
                } else if cancelled {
                    World::interrupted(seed, &files, placement)
                } else {
                    World::new(seed, &files, placement, b"First answer")
                };
                world.settle();
                let seen = world.seen();
                review(
                    &seen,
                    &world,
                    if changed {
                        Ending::Change
                    } else if cancelled {
                        Ending::Cancelled
                    } else {
                        Ending::Report(b"First answer".to_vec())
                    },
                )
                .assert_passed(seed);
                let actual = (world.trace(), seen);
                if let Some(previous) = previous {
                    assert_eq!(actual, previous, "seed {seed}, {placement:?}");
                }
                previous = Some(actual);
            }
        }
    }
}
