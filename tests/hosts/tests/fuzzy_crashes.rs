//! Every scheduling cut of one hosted run, plus seeded scheduler variation,
//! is replayed with the child crashing there (protocol/hosts.md, section 7).

#[cfg(test)]
mod tests {
    use skein_lib::Rng;
    use skein_world::domain::assert_replays;
    use smith_hosts_world::{Observation, Program, World, referee::review};

    fn run(seed: u64, cut: u32) -> (Vec<String>, (bool, Vec<Observation>)) {
        let mut world = World::new(seed, Program::Service);
        world.crash_after(cut);
        world.settle();
        review(world.observations()).assert_passed(seed);
        assert_eq!(world.seen().exits, u32::from(world.seen().started), "seed {seed}: one started child exit");
        let mut trace = world.trace();
        trace.push(format!("seed {seed} cut {cut}"));
        trace.extend(world.observations().iter().map(|event| format!("{event:?}")));
        (trace, (world.crashed(), world.observations().to_vec()))
    }

    #[test]
    fn hosted_agents_crash_at_every_step_and_seeded_schedules_replay() {
        let mut baseline = World::new(7, Program::Service);
        baseline.settle();
        let steps = baseline.iterations();
        assert!(steps < 256, "the baseline hosted run settled");
        for cut in 0..steps {
            assert_replays(7, 8, |seed| run(seed, cut));
        }
        for seed in 0..16_u64 {
            let mut rng = Rng::new(seed);
            let cut = u32::try_from(rng.below(u64::from(steps))).expect("bounded cut");
            assert_replays(seed, seed.checked_add(1).expect("bounded seed"), |seed| run(seed, cut));
        }
    }
}
