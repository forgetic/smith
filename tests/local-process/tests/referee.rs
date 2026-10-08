use skein_world::domain::Verdict;
use smith_local_domain::Fact;
use smith_local_process_world::{
    Files, Placement, World,
    referee::{CheckoutRead, Ending, review},
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
    use skein_fake_llm_domain::api::Query;
    use skein_io::kernel;
    use smith_local_process_world::referee::Seen;
    struct Repository {
        correct: bool,
    }
    impl CheckoutRead for Repository {
        fn head(&self) -> Option<Vec<u8>> {
            Some(b"new-head".to_vec())
        }
        fn message(&self, _: &[u8]) -> Vec<u8> {
            if self.correct {
                b"Updated result\n\nCreated the result file\n\nSmith-Delivery: 1/3/0".to_vec()
            } else {
                b"wrong fields".to_vec()
            }
        }
        fn files(&self, _: &[u8]) -> skein_fake_checkout::git::Tree {
            if self.correct { [(b"result.txt".to_vec(), b"new\n".to_vec())].into() } else { Default::default() }
        }
    }
    let seen = Seen {
        facts: vec![
            Fact::Started { activation: 1 },
            Fact::Turn { number: 1 },
            Fact::Answered { activation: 1 },
            Fact::Shown { activation: 1 },
        ],
        shown: b"Change delivered".to_vec(),
        errors: vec![],
        queries: vec![Query {
            model: b"fake".as_slice().into(),
            system: b"@local-shell".as_slice().into(),
            tools: Box::new([]),
            messages: Box::new([]),
            max_tokens: 1,
        }],
        exit: Some(kernel::Exit::Code(0)),
        pushed: false,
        oauth: None,
    };
    let ending = Ending::Change { before: b"initial-head".to_vec() };
    review(&seen, &Repository { correct: true }, ending.clone()).assert_passed(8);
    assert!(matches!(review(&seen, &Repository { correct: false }, ending.clone()).verdict(), Verdict::Failed(_)));
    let mut pushed = seen;
    pushed.pushed = true;
    assert!(matches!(review(&pushed, &Repository { correct: true }, ending).verdict(), Verdict::Failed(_)));
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
    assert_eq!(observed.trace(), silent.trace(), "facts never change kernel work");
}
