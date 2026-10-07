//! Seeded host and agent input cuts at actual wire bytes.
//! Contract: protocol/channel.md, sections 5 and 10; testing-strategy.md,
//! sections 6 and 8. Skein owns the replay helper and channel parser.

use skein_channel::StreamMode;
use skein_lib::Rng;
use skein_world::domain::assert_replays;
use smith_channel::CEILINGS;
use smith_channel_world::{Observation, World};

const CHARTER: &[u8] = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin");

fn run(seed: u64) -> (Vec<String>, (usize, usize, usize)) {
    let mut rng = Rng::new(seed);
    let mode = if rng.chance(500) { StreamMode::One } else { StreamMode::Two };
    let mut world = World::new(CEILINGS, CEILINGS, mode);
    world.settle();
    let cut = usize::try_from(rng.below(16) + 1).expect("bounded cut");
    if seed.is_multiple_of(2) {
        world.cut_agent_input_after(cut);
        world.send_start(Box::from(CHARTER));
        world.settle();
    } else {
        world.send_start(Box::from(CHARTER));
        world.settle();
        world.agent_admits();
        world.settle();
        world.cut_host_input_after(cut);
        world.agent_parks();
        world.settle();
    }
    let observations = world.observations();
    let agent_starts = observations.iter().filter(|event| matches!(event, Observation::AgentStart)).count();
    let host_answers = observations.iter().filter(|event| matches!(event, Observation::HostAnswer { .. })).count();
    let endings = observations
        .iter()
        .filter(|event| matches!(event, Observation::HostEnded(_) | Observation::AgentEnded(_)))
        .count();
    assert!(agent_starts <= 1, "seed {seed}: duplicate Start");
    assert!(host_answers <= 1, "seed {seed}: duplicate Answer");
    assert!(endings >= 1, "seed {seed}: cut input did not end");
    let (host_bytes, agent_bytes) = world.received_bytes();
    (observations.iter().map(|event| format!("{event:?}")).collect(), (host_bytes, agent_bytes, endings))
}

#[test]
fn cuts_on_both_stream_modes_replay_and_close_without_duplicate_run_records() {
    for seed in 0..64 {
        assert_replays(seed, seed + 1, run);
    }
}
