//! Seeded messages across real run callbacks. Contract: domain/run.md, section 13.
use skein_world::domain::Verdict;
use smith_run_world::{World, messages_referee::judge, noisy};
use std::collections::BTreeMap;

#[test]
fn seeded_message_callbacks_replay_with_read_and_unread_terminals() {
    let mut cells = BTreeMap::<&str, u32>::new();
    for seed in 0..96 {
        let mut settings = noisy(seed);
        settings.run.facts = 512;
        let mut world = World::new(settings);
        world.inject_messages(500);
        world.run(1_000_000);
        assert_eq!(world.facts().1, 0);
        assert!(judge(world.message_seen()).iter().all(|verdict| *verdict == Verdict::Passed), "seed {seed}");
        for (cell, count) in world.message_cells() {
            *cells.entry(cell).or_default() += count;
        }
        let mut replay = World::new(settings);
        replay.inject_messages(500);
        replay.run(1_000_000);
        assert_eq!(world.trace(), replay.trace(), "seeded message callbacks replay exactly");
    }
    for cell in ["preparing", "working", "landing", "winding"] {
        assert!(cells.get(cell).copied().unwrap_or(0) > 0, "missing {cell}: {cells:?}");
    }
}
