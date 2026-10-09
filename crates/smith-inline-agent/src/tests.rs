//! Entry-point and boundary ownership cases; clocks and values are supplied.
use alloc::boxed::Box;
use skein_lib::{Duration, Env, Id, Queue, Time, Token, Wall};
use smith_domain::{self as smith, run};
use smith_host_domain::{self as host, parent};

use crate::{Below, Domain, Input, Limits, Lower, Output, fire, reclaim, resume, step, translate};

fn bounds() -> Limits {
    let smith = smith_agent_world::LIMITS;
    Limits {
        slots: 2,
        smith,
        window: smith::Window {
            turns: 8,
            bytes: smith::max_turn_bytes(&smith).expect("root bound").checked_mul(8).expect("small window"),
        },
        wall_time: Duration::from_secs(3600),
        facts: 32,
    }
}

fn configuration() -> smith::Config {
    smith::Config {
        endpoints: Box::from([run::charter::Endpoint(0)]),
        models: Box::from([smith::ConfiguredModel {
            endpoint: run::charter::Endpoint(0),
            model: b"fake-1".as_slice().into(),
            window: 8192,
            output: 4096,
        }]),
    }
}

fn start() -> host::Start {
    let settings =
        smith_agent_world::Settings { job: smith_agent_world::Job::HostTools, ..smith_agent_world::Settings::calm(1) };
    let mut charter = smith_agent_world::scripted_charter(&settings);
    charter.conventions = None;
    charter.models = Box::default();
    charter.grants.agents = false;
    charter.grants.tools = run::charter::Tools { inspect: false, modify: false, shell: false };
    host::Start {
        messages: Box::default(),
        logical_run: Token::new(7),
        activation: 1,
        workspace: None,
        charter: host::Charter::new(charter, settings.limits.run.run_bytes).expect("root policy"),
        transcript: None,
        answered: Box::default(),
        directories: Box::default(),
        grants: Box::from([host::Grant { account: 0, generation: 1, valid: Duration::from_secs(7200) }]),
    }
}

fn env() -> Env<Limits> {
    Env { now: Time::ZERO, wall: Wall::EPOCH, limits: bounds() }
}

#[test]
fn workspace_starts_are_refused_before_a_slot_or_provider_request() {
    let env = env();
    let mut agent = Domain::new(&env.limits, configuration(), 1);
    let mut out = Queue::with_capacity(crate::max_out(&env.limits));
    let mut source = start();
    source.workspace = Some(Token::new(8));
    step(&mut agent, &env, Input::Parent(parent::Event::Spawn { client: Token::new(1), start: source }), &mut out);
    assert_eq!(
        out.pop(),
        Some(Output::Parent(parent::Request::Gone {
            client: Token::new(1),
            end: host::End::Invalid(host::Invalid::Directories),
            detail: Box::default()
        }))
    );
    assert!(out.is_empty());
    assert_eq!(agent.hosted(), 0);
}

#[test]
fn a_told_turn_keeps_the_roots_exact_concrete_value() {
    let env = env();
    let mut agent = Domain::new(&env.limits, configuration(), 2);
    let mut out = Queue::with_capacity(crate::max_out(&env.limits));
    step(&mut agent, &env, Input::Parent(parent::Event::Spawn { client: Token::new(1), start: start() }), &mut out);
    let Some(Output::Parent(parent::Request::Started { agent: handle, .. })) = out.pop() else {
        panic!("Started first")
    };
    for _ in 0..crate::max_out(&env.limits) {
        out.pop();
    }
    let turn = smith::session::record::Turn {
        version: smith::session::record::VERSION,
        endpoint: smith::session::llm::Endpoint(0),
        dialect: 1,
        sequence: 1,
        usage: smith::session::llm::Usage::ZERO,
        spent: 0,
        messages: Box::default(),
    };
    agent.lower.push(smith::Request::Turn {
        host_run: Token::new(7),
        number: 1,
        position: 1,
        read: Some(Token::new(42)),
        spent: run::Spend::ZERO,
        turn: turn.clone(),
    });
    translate::route(&mut agent, &env, Id::from_token(handle), &mut out);
    assert_eq!(
        out.pop(),
        Some(Output::Parent(parent::Request::Turn {
            client: Token::new(1),
            turn: host::Turn {
                number: 1,
                spent: 0,
                read: Some(Token::new(42)),
                body: host::TurnValue::new(turn, u64::MAX).expect("concrete value")
            }
        }))
    );
    assert!(out.is_empty());
}

#[test]
fn cancel_fire_resume_terminal_and_reclaim_settle_exactly_once() {
    let mut env = env();
    env.limits.wall_time = Duration::from_secs(1);
    let mut agent = Domain::new(&env.limits, configuration(), 3);
    let mut out = Queue::with_capacity(crate::max_out(&env.limits));
    step(&mut agent, &env, Input::Parent(parent::Event::Spawn { client: Token::new(1), start: start() }), &mut out);
    let Some(Output::Parent(parent::Request::Started { agent: handle, .. })) = out.pop() else {
        panic!("Started first")
    };
    let mut completion_owner = None;
    for _ in 0..crate::max_out(&env.limits) {
        if let Some(Output::Lower { request: Lower::Complete { owner, .. }, .. }) = out.pop() {
            completion_owner = Some(owner);
        }
    }
    let owner = completion_owner.expect("actual provider request");
    resume(&mut agent, &env, &mut out);
    assert!(out.is_empty());
    env.now = Time::ZERO.saturating_add(Duration::from_secs(1));
    assert!(agent.is_due(env.now));
    fire(&mut agent, &env, &mut out);
    assert_eq!(out.pop(), Some(Output::Lower { agent: handle, request: Lower::Cancel { owner } }));
    assert!(out.is_empty());
    step(&mut agent, &env, Input::Parent(parent::Event::Stop { agent: handle }), &mut out);
    assert!(out.is_empty());
    assert_eq!(agent.hosted(), 1);
    step(&mut agent, &env, Input::Below { agent: handle, terminal: Below::Cancelled { owner } }, &mut out);
    let Some(Output::Parent(parent::Request::Answered { answer, .. })) = out.pop() else { panic!("cancel answer") };
    assert_eq!(answer.result, host::RunResult::Failed { failure: host::RunFailure::Cancelled });
    assert_eq!(
        out.pop(),
        Some(Output::Parent(parent::Request::Gone {
            client: Token::new(1),
            end: host::End::Stopped,
            detail: Box::default()
        }))
    );
    step(&mut agent, &env, Input::Below { agent: handle, terminal: Below::Cancelled { owner } }, &mut out);
    assert!(out.is_empty());
    assert!(agent.pop_fact(Token::new(1)).is_some(), "native observations survive retirement");
    while agent.pop_fact(Token::new(1)).is_some() {}
    while agent.pop_content(Token::new(1)).is_some() {}
    reclaim(&mut agent);
    assert_eq!(agent.hosted(), 0);
    step(&mut agent, &env, Input::Below { agent: handle, terminal: Below::Cancelled { owner } }, &mut out);
    assert!(out.is_empty());
}

#[test]
fn checked_limits_require_one_concrete_turn_and_positive_wall_time() {
    let mut limits = bounds();
    assert!(crate::worst_case(&limits).is_some());
    limits.window.bytes = smith::max_turn_bytes(&limits.smith).expect("turn bound") - 1;
    assert_eq!(crate::worst_case(&limits), None);
    limits = bounds();
    limits.wall_time = Duration::ZERO;
    assert_eq!(crate::worst_case(&limits), None);
}
