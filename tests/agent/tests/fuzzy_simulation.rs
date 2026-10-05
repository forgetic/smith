//! Bounded seeded interleavings of the real copied agent tree and scripted
//! host (domain/run.md, section 14; testing-strategy.md, sections 6 and 7).

use skein_lib::{Duration, Rng};
use smith_agent_world::{Job, Settings, World};
use smith_domain::run::{self, Answer, Push};

#[test]
fn many_host_schedules_settle_against_the_boundary_referee() {
    let mut ended = [0; 3];
    for seed in 0..120 {
        let mut rng = Rng::new(seed);
        let calm = Settings::calm(seed);
        let job = match rng.below(5) {
            0 => Job::Coding,
            1 => Job::Review,
            2 => Job::Reporting,
            3 => Job::Delegating,
            _ => Job::Spending,
        };
        let push = match rng.below(4) {
            0 => Push::Moved,
            1 => Push::Failed { failure: run::PushFailure::new(run::PushReason::Unreachable) },
            _ => Push::Done,
        };
        let cancel_at = if rng.chance(300) { Some(Duration::from_millis(rng.below(20_000))) } else { None };
        let budget = run::Budget { turns: u32::try_from(rng.between(4, 64)).expect("a few turns"), ..calm.budget };
        let settings = Settings { job, push, cancel_at, budget, races: 500, ..calm };
        let mut world = World::new(settings);
        world.run(20_000);
        match world.answer() {
            Answer::Accepted { .. } => ended[0] += 1,
            Answer::Failed { .. } => ended[1] += 1,
            Answer::Refused(_) => ended[2] += 1,
        }
        assert_eq!(world.judged().1, 1, "seed {seed}: the host received its answer");
        assert_eq!(world.lost(), 0, "seed {seed}: the shell drained facts within their capacity");
    }
    assert!(ended[0] > 0 && ended[1] > 0, "both successful and failed interleavings occurred: {ended:?}");
}
