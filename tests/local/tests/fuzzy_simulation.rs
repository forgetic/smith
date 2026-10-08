//! Bounded variations of person timing, store outcomes and provider scripts.

use smith_local_domain::ExitStatus;
use smith_local_world::{Cut, StoreFault, World};

#[test]
fn random_local_chats_settle_against_the_boundary_referee() {
    for seed in 0_u64..64 {
        let mode = seed % 10;
        let mut world = if mode == 4 { World::tight_unsaved(seed + 100) } else { World::new(seed + 100) };
        if seed % 3 == 0 && mode != 5 {
            world.drive(5);
        }
        if mode == 1 {
            world.reject_first_credential();
        }
        if mode == 2 {
            world.exhaust_first_account();
        }
        if mode == 3 || mode == 4 {
            world.slow_store();
        }
        if mode == 5 {
            world.fail_store(StoreFault::Load);
        }
        if mode == 6 {
            world.fail_store(StoreFault::State);
        }
        if mode == 7 {
            world.fail_store(StoreFault::Turn);
        }
        world.line(b"Hello from the fuzz person");
        match mode {
            3 | 4 => {
                world.drive(300);
                if mode == 4 {
                    world.interrupt();
                }
                for _ in 0..8 {
                    if world.waiting() || world.exit().is_some() || world.drive_to_cancelled(10) {
                        break;
                    }
                    assert!(world.release_turn(), "seed {seed}: stalled store has an acknowledgement");
                    world.drive(300);
                }
                assert!(
                    world.waiting()
                        || world.exit().is_some()
                        || world.shown().iter().any(|text| text.as_ref().trim_ascii_end() == b"Run cancelled")
                );
            }
            5..=7 => {
                world.drive(300);
                assert_eq!(world.exit(), Some(ExitStatus::Failed), "seed {seed}: store failure stops");
            }
            2 => {
                world.drive(300);
                assert!(
                    world.shown().iter().any(|text| text.as_ref().trim_ascii_end() == b"A model account is exhausted")
                );
            }
            8 | 9 => {
                let cut = if mode == 8 { Cut::BeforeSaveTurn(2) } else { Cut::AfterTurnSaved(2) };
                assert!(world.drive_to_cut(300, cut), "seed {seed}: crash cut reached");
                let mut resumed = World::with_store(seed + 200, world.into_store());
                resumed.line(b"Continue after crash");
                assert!(resumed.drive(300), "seed {seed}: next invocation resumes");
                assert!(resumed.judged().0 > 0);
                continue;
            }
            _ => {
                assert!(world.drive(300), "seed {seed}: the chat waits");
            }
        }
        if mode != 5 {
            assert!(world.judged().0 > 0, "seed {seed}: the referee observed boundaries");
        }
    }
}
