//! Every shared-harness process iteration and final drop is metered.
use skein_world::domain::heap::Counting;
use smith_local_process_world::{Authentication, Files, Placement, World};
#[global_allocator]
static HEAP: Counting = Counting;
#[test]
fn the_composed_world_including_its_shared_shell_fits_its_bound() {
    for placement in [Placement::Spawned, Placement::InProcess] {
        let files = Files::new();
        let mut world = match placement {
            Placement::Spawned => World::authenticated(10, &files, placement, Authentication::SignIn),
            Placement::InProcess => World::changed(10, &files, placement),
        };
        world.check_memory();
        world.settle();
        for (peak, bound) in world.heaps() {
            assert!(*peak > 0 && peak <= bound, "each independently owned process fits its bound");
        }
        drop(world);
    }
}

#[test]
fn the_shared_processes_select_tls_without_changing_their_referee() {
    let files = Files::new();
    let trust = files.path().join("test-root.der");
    std::fs::write(&trust, skein_tls_world::pki::ROOT).expect("fixed test trust");
    let mut world = World::authenticated(24, &files, Placement::Spawned, Authentication::SignIn);
    world.scenario.launch.tls = true;
    world.scenario.launch.trust_der = Some(trust);
    world.check_memory();
    world.settle();
    assert_eq!(world.oauth_posts(), 1);
    assert!(world.browser_replied());
    smith_local_process_world::referee::review(
        &world.seen(),
        &world,
        smith_local_process_world::referee::Ending::Report(b"First answer".to_vec()),
    )
    .assert_passed(24);
}
