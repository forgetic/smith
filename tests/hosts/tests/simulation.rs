#[cfg(test)]
mod tests {
    use smith_host_domain::End;
    use smith_hosts_world::{Program, World};

    #[test]
    fn an_agent_that_starts_runs_and_exits_after_its_last_word() {
        let mut world = World::new(7, Program::Service);
        world.settle();
        let seen = world.seen();
        assert!(seen.started);
        assert!(seen.admitted);
        assert!(seen.answered.is_some(), "the actual agent service supplied its last word");
        assert_eq!(seen.gone, Some(End::Stopped));
        assert_eq!(seen.exits, 1);
        assert_eq!(seen.reaps, 1);
        assert!(world.peer_replied(), "the fake LLM served the child");
    }

    #[test]
    fn an_agent_that_fails_to_start_is_reported_with_its_errors_tail() {
        let mut world = World::new(7, Program::ErrorTail);
        world.settle();
        let seen = world.seen();
        assert_eq!(seen.gone, Some(End::Unspawned));
        assert!(!seen.started);
        assert!(seen.answered.is_none());
        assert_eq!(seen.detail.as_deref(), Some(&b"agent configuration failed"[..]));
    }

    #[test]
    fn a_refused_spawn_reports_an_unstarted_agent() {
        let mut world = World::new(7, Program::Refused);
        world.settle();
        assert_eq!(world.seen().gone, Some(End::Unspawned));
    }

    #[test]
    fn an_agent_that_misses_the_openings_deadline_is_stopped_and_reported() {
        let mut world = World::new(7, Program::Silent);
        world.settle();
        let seen = world.seen();
        assert_eq!(seen.gone, Some(End::Unspawned));
        assert!(!seen.started);
        assert!(seen.answered.is_none());
    }

    #[test]
    fn an_agent_that_ignores_the_cancel_is_killed_after_the_grace() {
        let mut world = World::new(11, Program::Service);
        world.ignore_cancel();
        world.settle();
        let seen = world.seen();
        assert_eq!(seen.gone, Some(End::Stopped));
        assert_eq!(seen.signals, [smith_host_domain::Signal::Terminate, smith_host_domain::Signal::Kill]);
        assert_eq!(seen.exits, 1);
        assert_eq!(seen.reaps, 1);
    }

    #[test]
    fn a_child_killed_during_start_settles_the_write_and_the_independent_read_once() {
        let mut world = World::new(7, Program::Service);
        world.crash_after(0);
        world.settle();
        assert!(world.crashed());
        assert!(world.seen().gone.is_some());
        assert_eq!(world.seen().exits, 1);
        assert_eq!(world.seen().reaps, 1);
    }
}
