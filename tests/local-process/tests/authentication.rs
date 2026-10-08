use skein_io::kernel;
use smith_local_process_world::{
    Authentication, Files, Placement, World,
    referee::{Ending, review},
};

#[test]
fn a_sign_in_uses_the_page_and_redirect_before_lending_the_saved_token() {
    for placement in [Placement::Spawned, Placement::InProcess] {
        let files = Files::new();
        let mut world = World::authenticated(20, &files, placement, Authentication::SignIn);
        world.settle();
        assert_eq!(world.exit(), Some(kernel::Exit::Code(0)));
        assert_eq!(world.oauth_posts(), 1);
        assert_eq!(world.page_visits(), 1);
        assert!(world.browser_replied());
        assert!(world.saved_before_query());
        review(&world.seen(), &world, Ending::Report(b"First answer".to_vec())).assert_passed(20);
    }
}

#[test]
fn an_expired_grant_refreshes_through_the_shared_private_token_store() {
    for placement in [Placement::Spawned, Placement::InProcess] {
        let files = Files::new();
        let mut world = World::authenticated(21, &files, placement, Authentication::Refresh);
        world.settle();
        assert_eq!(world.exit(), Some(kernel::Exit::Code(0)));
        assert_eq!(world.oauth_posts(), 1);
        assert_eq!(world.page_visits(), 0);
        assert!(world.saved_before_query());
        review(&world.seen(), &world, Ending::Report(b"First answer".to_vec())).assert_passed(20);
    }
}

#[test]
fn a_refused_refresh_never_lends_a_token_or_starts_the_agent() {
    for placement in [Placement::Spawned, Placement::InProcess] {
        let files = Files::new();
        let mut world = World::authenticated(22, &files, placement, Authentication::Refused);
        world.settle();
        assert_eq!(world.exit(), Some(kernel::Exit::Code(0)));
        assert_eq!(world.oauth_posts(), 1);
        assert_eq!(world.page_visits(), 0);
        assert!(world.queries().is_empty());
        assert!(world.facts().is_empty());
        assert!(!world.saved_before_query());
        review(&world.seen(), &world, Ending::Unavailable).assert_passed(22);
    }
}

#[test]
fn a_lent_grant_refreshes_while_the_model_is_running_before_it_lapses() {
    let files = Files::new();
    let mut world = World::authenticated(23, &files, Placement::InProcess, Authentication::Proactive);
    world.settle();
    assert_eq!(world.exit(), Some(kernel::Exit::Code(0)));
    assert_eq!(world.oauth_posts(), 1);
    assert_eq!(world.page_visits(), 0);
    let post = world.first_token_post().expect("scheduled refresh reached the peer");
    assert!(post.as_nanos() >= skein_tls_world::pki::VALID.as_nanos() + skein_lib::Duration::from_secs(1).as_nanos());
    assert!(post.as_nanos() < skein_tls_world::pki::VALID.as_nanos() + skein_lib::Duration::from_secs(61).as_nanos());
    let token = smith::local_tokens::Tokens::new(&files.path().join("tokens"), smith::local_host::token_limits())
        .expect("private store")
        .load(0)
        .expect("record")
        .expect("saved rotation");
    assert_eq!(token.generation, 2);
    assert_eq!(token.refresh_token.as_ref(), b"refresh-new");
    review(&world.seen(), &world, Ending::Report(b"First answer".to_vec())).assert_passed(23);
}
