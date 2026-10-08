mod support;

use smith_local_process_world::Placement;
use smith_real_world::{Scratch, World};

#[test]
fn the_shipped_local_and_agent_answer_on_a_terminal() {
    let scratch = Scratch::new();
    let scenario = World::new(31, &scratch, Placement::Spawned, b"First answer").scenario;
    let seen = support::run(&scratch, scenario);
    assert_eq!(seen.queries.len(), 2);
}
