//! Both agent kinds share the parent referee (domain/host.md, section 10).
use skein_lib::Token;
use smith_agent_world::Job;
use smith_domain::{run, session::record};
use smith_host_domain::{self as host, End, Input, RunFailure, RunResult, parent};
use smith_host_world::{
    World as Spawned,
    inline::{self, World},
};

fn spawn(world: &mut World, client: u64, job: Job) {
    world.event(parent::Event::Spawn { client: Token::new(client), start: inline::start(client, job) });
}

#[test]
fn an_inline_run_answers_and_its_slot_is_released_after_its_last_completion() {
    let mut world = World::new(311, inline::limits());
    world.auto_ack = false;
    spawn(&mut world, 1, Job::HostTools);
    for _ in 0..100 {
        if world.seen[&Token::new(1)].answer.is_some() {
            break;
        }
        let next = world.next_deadline().expect("actual provider or root alarm");
        world.at(next);
    }
    let seen = &world.seen[&Token::new(1)];
    assert!(
        matches!(seen.answer.as_ref().map(|answer| &answer.result), Some(RunResult::Accepted { .. })),
        "{:?}",
        seen.answer
    );
    assert!(seen.gone.is_none(), "answered root retains exact told-turn ACK rights");
    assert_eq!(world.agent.hosted(), 1);
    world.lower.assert_settled();
    let agent = world.handle(Token::new(1));
    let turns: Vec<_> = seen.turns.keys().copied().collect();
    for turn in turns {
        world.event(parent::Event::Acknowledge { agent, turn });
    }
    world.settled();
}

#[test]
fn an_inline_run_waits_parks_and_resumes_from_its_transcript() {
    let mut world = World::new(312, inline::limits());
    spawn(&mut world, 1, Job::Waiting);
    world.run();
    world.settled();
    assert!(
        matches!(world.seen[&Token::new(1)].answer.as_ref().expect("answer").result, RunResult::Parked),
        "{:?}",
        world.seen[&Token::new(1)].answer
    );
    let turns = world.history[&Token::new(1)].clone();
    assert!(!turns.is_empty());
    let endpoint = turns[0].endpoint;
    let dialect = turns[0].dialect;
    let history = record::Transcript { version: record::VERSION, endpoint, dialect, turns: turns.into() };
    let mut next = inline::start(313, Job::Waiting);
    let mut charter = next.charter.into_value();
    charter.resume = true;
    next.charter =
        host::Charter::new(charter, world.stage.env.limits.smith.run.run_bytes).expect("bounded resumed policy");
    next.activation = 2;
    next.logical_run = Token::new(1);
    next.transcript = Some(host::Transcript::new(history, u64::MAX).expect("saved concrete history"));
    next.messages = Box::from([host::Message {
        name: Token::new(92),
        label: b"person".as_slice().into(),
        text: b"Continue".as_slice().into(),
    }]);
    world.event(parent::Event::Spawn { client: Token::new(2), start: next });
    world.run();
    world.settled();
    let restored = &world.history[&Token::new(2)];
    assert!(restored[0].sequence > world.history[&Token::new(1)].last().expect("saved turn").sequence);
    assert_eq!(world.seen[&Token::new(2)].answer.as_ref().expect("resumed answer").read, Some(Token::new(92)));
}

#[test]
fn an_inline_run_cancelled_mid_turn_answers_cancelled() {
    let mut world = World::new(314, inline::limits());
    spawn(&mut world, 1, Job::HostTools);
    assert!(!world.lower.is_empty());
    let agent = world.handle(Token::new(1));
    world.event(parent::Event::Stop { agent });
    assert!(world.seen[&Token::new(1)].gone.is_none(), "cancel still owes actual provider terminal");
    world.run();
    world.settled();
    assert!(matches!(
        world.seen[&Token::new(1)].answer.as_ref().expect("answer").result,
        RunResult::Failed { failure: RunFailure::Cancelled }
    ));
}

#[test]
fn a_message_past_the_runs_bound_is_refused_typed_by_either_kind() {
    let mut inline_bounds = inline::limits();
    inline_bounds.smith.run.message_bytes = 64;
    let mut world = World::new(315, inline_bounds);
    spawn(&mut world, 1, Job::HostTools);
    let agent = world.handle(Token::new(1));
    world.event(parent::Event::Message {
        agent,
        name: Token::new(90),
        label: Box::default(),
        text: vec![b'x'; 63].into(),
    });
    assert_eq!(world.seen[&Token::new(1)].bounces, [host::MessageRefusal::TooLarge]);
    world.run();
    world.settled();
    let mut spawned = Spawned::new(315, smith_host_world::limits());
    spawned.live();
    let agent = spawned.agent();
    spawned.event(Input::Parent(parent::Event::Message {
        agent,
        name: Token::new(90),
        label: Box::default(),
        text: vec![b'x'; 63].into(),
    }));
    assert_eq!(spawned.seen.bounces, [host::MessageRefusal::TooLarge]);
}

#[test]
fn an_inline_runs_facts_are_drained_every_pass() {
    let mut world = World::new(316, inline::limits());
    spawn(&mut world, 1, Job::HostTools);
    assert!(!world.facts.is_empty(), "admission observations available before provider finishes");
    assert!(world.agent.pop_fact(Token::new(1)).is_none(), "drained at this actual iteration");
    world.run();
    world.settled();
    assert!(!world.content.is_empty());
    assert!(
        world
            .observations
            .iter()
            .any(|fact| matches!(fact.kind, smith_inline_agent::FactKind::Gone { client: _, end: End::Stopped }))
    );
}

#[test]
fn two_inline_slots_keep_provider_owners_independent() {
    let mut world = World::new(317, inline::limits());
    spawn(&mut world, 1, Job::HostTools);
    spawn(&mut world, 2, Job::HostTools);
    assert_ne!(world.handle(Token::new(1)), world.handle(Token::new(2)));
    world.run();
    world.settled();
    for client in [Token::new(1), Token::new(2)] {
        assert!(matches!(world.seen[&client].answer.as_ref().expect("answer").result, RunResult::Accepted { .. }));
    }
}

#[test]
fn inline_scripts_use_actual_root_types_without_a_codec() {
    let start = inline::start(318, Job::HostTools);
    assert!(start.workspace.is_none());
    assert!(!start.charter.value().grants.host_tools.is_empty());
    assert_eq!(start.charter.value().grants.tools, run::charter::Tools { inspect: false, modify: false, shell: false });
}
