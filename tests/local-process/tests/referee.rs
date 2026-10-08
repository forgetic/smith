use skein_world::domain::Verdict;
use smith_local_domain::Fact;
use smith_local_process_world::{
    Files, Placement, World,
    referee::{Ending, review},
};

#[test]
fn observations_pass_and_duplicate_or_early_answers_fail() {
    let files = Files::new();
    let mut world = World::new(7, &files, Placement::Spawned, b"First answer");
    world.settle();
    let seen = world.seen();
    review(&seen, &world, Ending::Report(b"First answer".to_vec())).assert_passed(7);
    let mut duplicate = seen.clone();
    duplicate.facts.push(Fact::Answered { activation: 1 });
    assert!(matches!(
        review(&duplicate, &world, Ending::Report(b"First answer".to_vec())).verdict(),
        Verdict::Failed(_)
    ));
    let mut early = seen.clone();
    early.facts.insert(0, Fact::Shown { activation: 1 });
    assert!(matches!(review(&early, &world, Ending::Report(b"First answer".to_vec())).verdict(), Verdict::Failed(_)));
    let mut wrong = seen;
    wrong.shown = b"a different answer".to_vec();
    assert!(matches!(review(&wrong, &world, Ending::Report(b"First answer".to_vec())).verdict(), Verdict::Failed(_)));
}

#[test]
fn commit_fields_and_files_are_checked_through_the_checkout_interface() {
    let files = Files::new();
    let mut world = World::changed(8, &files, Placement::InProcess);
    world.settle();
    let seen = world.seen();
    review(&seen, &world, Ending::Change).assert_passed(8);
    struct Wrong;
    impl smith_local_process_world::referee::CheckoutRead for Wrong {
        fn head(&self) -> Option<u64> {
            Some(2)
        }
        fn message(&self, _: u64) -> Vec<u8> {
            b"wrong fields".to_vec()
        }
        fn files(&self, _: u64) -> skein_fake_checkout::git::Tree {
            Default::default()
        }
    }
    assert!(matches!(review(&seen, &Wrong, Ending::Change).verdict(), Verdict::Failed(_)));
    let mut pushed = seen;
    pushed.pushed = true;
    assert!(matches!(review(&pushed, &world, Ending::Change).verdict(), Verdict::Failed(_)));
}

#[test]
fn discarding_service_facts_changes_no_terminal_or_peer_work() {
    let first_files = Files::new();
    let second_files = Files::new();
    let mut observed = World::new(9, &first_files, Placement::InProcess, b"First answer");
    let mut silent = World::without_facts(9, &second_files, Placement::InProcess, b"First answer");
    observed.settle();
    silent.settle();
    assert!(silent.facts().is_empty());
    assert_eq!(observed.shown(), silent.shown());
    assert_eq!(observed.queries(), silent.queries());
    assert_eq!(observed.exit(), silent.exit());
}
