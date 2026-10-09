use smith_agent_process_world::{World, charter};
use smith_channel::{RunFailure, RunResult};

#[test]
fn an_answered_agent_exits_0_with_no_signal_from_its_host() {
    let charter = charter();
    let mut world = World::new(7, &charter);
    assert!(world.settle(), "the admitted run answered");
    assert_eq!(world.observed().iter().filter(|frame| frame.kind == 0x0110).count(), 1);
    assert!(world.peer_replied(), "the fake LLM served a plaintext response: {:?}", world.peer_observations());
    assert_eq!(world.peer_queries().len(), 1, "the independent peer decoded one model request");
    let answer = world.answer().expect("host saw final answer");
    assert_eq!(answer.turns(), 1);
    assert!(matches!(answer.result(), RunResult::Failed(_)), "one-turn budget ended the run");
    assert_eq!(world.exit(), Some(skein_io::kernel::Exit::Code(0)));
    assert!(!world.signalled(), "answered process exits without a host signal");
    assert!(!String::from_utf8(world.errors()).expect("diagnostic UTF-8").contains("could not answer"));
    world.assert_agent_clean();
}

#[test]
fn a_termination_signal_in_the_middle_of_a_turn_answers_cancelled() {
    let charter = charter();
    let mut world = World::new(8, &charter);
    world.signal();
    assert!(world.settle(), "cancellation was answered");
    assert!(world.observed().iter().any(|frame| frame.kind == 0x0106), "run admitted before signal");
    let answer = world.answer().expect("host saw final answer");
    assert!(matches!(answer.result(), RunResult::Failed(failed) if failed.reason() == &RunFailure::Cancelled));
    world.assert_agent_clean();
}

#[test]
fn full_kernel_replay_leaves_the_same_answer() {
    for cancel in [false, true] {
        skein_world::domain::assert_replays(7, 8, |seed| {
            let mut world = World::new(seed, &charter());
            if cancel {
                world.signal();
            }
            assert!(world.settle());
            (world.trace(), world.answer())
        });
    }
}

#[test]
fn an_agent_whose_host_hangs_up_before_its_answer_says_so_and_exits_1() {
    let mut world = World::new(9, &charter());
    world.hang_up();
    assert!(world.settle(), "the unstarted channel settles without a signal");
    assert_eq!(world.exit(), Some(skein_io::kernel::Exit::Code(1)));
    assert!(world.answer().is_none());
    assert!(!world.signalled());
    let errors = String::from_utf8(world.errors()).expect("diagnostic UTF-8");
    assert!(errors.starts_with("smith: agent started; worst case "));
    assert_eq!(errors.lines().last(), Some("smith: the run could not answer: Some(ChannelEnded)"));
    assert_eq!(errors.lines().count(), 2, "startup and failure each appear once");
    world.assert_agent_clean();
}

#[test]
fn a_refused_configuration_writes_one_line_and_exits_1_without_a_channel() {
    use smith_agent_shell::{Agent, Resources};
    let errors = smith_agent_process_world::AgentErrors::default();
    let startup = Agent::read(
        std::path::Path::new(""),
        Resources {
            input: skein_io::kernel::Fd::new(100),
            output: skein_io::kernel::Fd::new(101),
            signals: skein_io::kernel::Fd::new(102),
            error: None,
            seed: 1,
            roots: Some(Box::new([])),
        },
        Box::new(errors.clone()),
    );
    let exit = match startup {
        Ok(agent) => {
            assert!(skein_world::Host::is_empty(&agent), "a refused startup never adopts these nonexistent streams");
            skein_world::Host::exit(&agent)
        }
        Err(_) => Some(skein_io::kernel::Exit::Code(1)),
    };
    assert_eq!(exit, Some(skein_io::kernel::Exit::Code(1)));
    let text = String::from_utf8(errors.bytes()).expect("diagnostic UTF-8");
    assert!(text.starts_with("smith: configuration metadata:"));
    assert_eq!(text.lines().count(), 1);
}
