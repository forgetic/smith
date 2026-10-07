//! Channel opening stories (protocol/channel.md, sections 2 and 10).
use skein_channel::{Closed, StreamMode};
use smith_channel::CEILINGS;
use smith_channel_world::{Observation, World};

#[test]
fn the_halves_open_on_the_highest_version_both_speak() {
    for mode in [StreamMode::One, StreamMode::Two] {
        let mut world = World::new(CEILINGS, CEILINGS, mode);
        world.settle();
        assert!(world.observations().contains(&Observation::HostOpened(1)));
        assert!(world.observations().contains(&Observation::AgentOpened(1)));
    }
}

#[test]
fn no_version_in_common_ends_the_channel_before_a_start() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.agent_hears_foreign_version();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(1))));
    assert!(!world.observations().contains(&Observation::AgentOpened(1)));
}

#[test]
fn a_host_whose_terms_are_too_small_is_refused_at_the_opening() {
    let mut host = CEILINGS;
    host.turn_body = 1;
    let mut world = World::new(host, CEILINGS, StreamMode::Two);
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(2))));
}
