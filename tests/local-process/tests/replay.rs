use skein_world::domain::assert_replays;
use smith_local_process_world::{
    Files, Placement, World,
    referee::{Ending, review},
};

#[test]
fn a_seed_replays_the_same_bytes_facts_and_peer_requests() {
    assert_replays(101, 100, |seed| {
        let files = Files::new();
        let mut world = if seed == 100 {
            World::interrupted(seed, &files, Placement::Spawned)
        } else {
            World::new(seed, &files, Placement::Spawned, b"First answer")
        };
        world.settle();
        let seen = world.seen();
        review(&seen, &world, if seed == 100 { Ending::Cancelled } else { Ending::Report(b"First answer".to_vec()) })
            .assert_passed(seed);
        (world.trace(), seen)
    });
}

#[test]
fn an_acknowledgement_may_cross_after_the_agents_last_word() {
    let files = Files::new();
    let mut world = World::new(115, &files, Placement::Spawned, b"First answer");
    world.settle();
    review(&world.seen(), &world, Ending::Report(b"First answer".to_vec())).assert_passed(115);
}
