//! Seeded short IO, completion delays and cancellation races in the real
//! shared shell, replayed using Skein's trace and referee machinery.
use smith_local_process_world::{
    Authentication, Files, Placement, World,
    referee::{CheckoutRead, Ending, review},
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
                let authentication = match seed % 8 {
                    2 => Some(Authentication::SignIn),
                    3 => Some(Authentication::Refresh),
                    6 => Some(Authentication::Refused),
                    7 => Some(Authentication::Proactive),
                    _ => None,
                };
                let mut world = if let Some(mode) = authentication {
                    World::authenticated(seed, &files, placement, mode)
                } else if changed {
                    World::changed(seed, &files, placement)
                } else if cancelled {
                    World::interrupted(seed, &files, placement)
                } else {
                    World::new(seed, &files, placement, b"First answer")
                };
                let before = world.head();
                world.settle();
                let seen = world.seen();
                review(
                    &seen,
                    &world,
                    if authentication == Some(Authentication::Refused) {
                        Ending::Unavailable
                    } else if changed {
                        Ending::Change { before: before.expect("initial head") }
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
