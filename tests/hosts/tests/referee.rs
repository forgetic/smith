//! The host-process referee accepts a settled child and rejects duplicate
//! answers, early release and missing or repeated call terminals.

#[cfg(test)]
mod tests {
    use skein_lib::Token;
    use skein_world::domain::Verdict;
    use smith_hosts_world::{Observation, Program, World, referee::review};

    #[test]
    fn a_hosted_run_releases_its_slot_after_the_child_is_gone() {
        let mut world = World::new(7, Program::Service);
        world.settle();
        review(world.observations()).assert_passed(7);
    }

    #[test]
    fn a_duplicate_answer_and_an_early_slot_release_are_rejected() {
        let duplicate = review(&[
            Observation::Started,
            Observation::Answered,
            Observation::Answered,
            Observation::Exited,
            Observation::Reaped,
            Observation::Gone,
        ]);
        assert!(matches!(duplicate.verdict(), Verdict::Failed(_)), "duplicate answer must fail");
        let early = review(&[Observation::Started, Observation::Gone]);
        assert!(matches!(early.verdict(), Verdict::Failed(_)), "slot cannot release before child teardown");
    }

    #[test]
    fn every_host_call_requires_one_terminal() {
        let call = Token::new(1);
        let missing = review(&[
            Observation::Started,
            Observation::Called(call),
            Observation::Exited,
            Observation::Reaped,
            Observation::Gone,
        ]);
        assert!(matches!(missing.verdict(), Verdict::Failed(_)), "unanswered call must fail");
        let repeated = review(&[
            Observation::Started,
            Observation::Called(call),
            Observation::CallAnswered(call),
            Observation::CallAnswered(call),
        ]);
        assert!(matches!(repeated.verdict(), Verdict::Failed(_)), "duplicate terminal must fail");
        let complete = review(&[
            Observation::Started,
            Observation::Called(call),
            Observation::CallAnswered(call),
            Observation::Exited,
            Observation::Reaped,
            Observation::Gone,
        ]);
        complete.assert_passed(1);
    }
}
