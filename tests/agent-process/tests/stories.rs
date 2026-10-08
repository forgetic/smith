use smith_agent_process_world::World;

#[test]
fn a_run_goes_from_start_to_answer_and_the_process_exits_with_success() {
    let mut world = World::new(7, &[0, 2]);
    assert!(world.settle(), "a bounded refusal is still an answer");
    assert!(world.has_answer(), "host saw one final answer");
}
