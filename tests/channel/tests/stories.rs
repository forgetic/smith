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

#[test]
fn a_start_that_decodes_reaches_the_agent() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    assert!(world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_charter_in_an_unread_version_is_refused_as_invalid_with_no_turns() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from([0, 2]));
    world.settle();
    assert!(world.observations().contains(&Observation::InvalidStart {
        why: smith_channel::InvalidStart::CharterVersion,
        turns: 0,
        spent: 0,
    }));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_malformed_charter_is_refused_with_its_own_reason() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from([0, 1]));
    world.settle();
    assert!(world.observations().contains(&Observation::InvalidStart {
        why: smith_channel::InvalidStart::MalformedCharter,
        turns: 0,
        spent: 0,
    }));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_start_record_that_does_not_decode_breaks_the_channel_rules() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.agent_hears_malformed_start();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(256))));
    assert!(!world.observations().contains(&Observation::AgentStart));
}
