use skein_io::kernel;
use smith_local_process_world::Placement;
use smith_real_world::{Scratch, World};

#[test]
fn shared_processes_exchange_over_tls_on_the_real_ring() {
    for placement in [Placement::Spawned, Placement::InProcess] {
        let scratch = Scratch::new();
        let mut world = World::new(7, &scratch, placement, b"First answer");
        world.settle();
        assert_eq!(world.seen().exit, Some(kernel::Exit::Code(0)));
        assert!(world.elapsed() < skein_lib::Duration::from_secs(1));
    }
}

#[test]
fn the_shared_browser_follows_the_real_tls_issuer_and_callback() {
    let scratch = Scratch::new();
    let mut world = World::authenticated(8, &scratch, Placement::Spawned);
    world.settle();
    let seen = world.seen();
    assert_eq!(seen.exit, Some(kernel::Exit::Code(0)));
    let oauth = seen.oauth.expect("issuer observed sign-in");
    assert_eq!(oauth.posts, 1);
    assert_eq!(oauth.pages, 1);
    assert!(oauth.browser_replied && oauth.saved_before_query);
    assert!(world.elapsed() < skein_lib::Duration::from_secs(1));
}
