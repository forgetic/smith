//! Bounded seed coverage of the shared-kit consumer story
//! (testing-strategy.md, sections 6 and 8; domain/README.md, section 4).

mod common;

#[test]
fn scheduled_terminal_orders_replay_across_small_seed_sweep() {
    for seed in 0_u64..64 {
        assert_eq!(common::run(seed), common::run(seed), "seed {seed} replays exactly");
    }
}
