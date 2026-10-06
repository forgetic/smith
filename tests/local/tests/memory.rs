//! The full local line queue and child allocation remain inside `worst_case`.

use skein_lib::{Duration, Env, Queue, Time, Wall};
use skein_world::domain::heap::{Counting, Meter};
use smith_domain::run::{self, charter::Endpoint, outcome};
use smith_local_domain::{self as local, Contract, Event, Request};

#[global_allocator]
static HEAP: Counting = Counting;

#[test]
fn a_local_domain_with_every_line_slot_full_stays_within_its_worst_case() {
    let limits = local::Limits {
        agent: smith_agent_world::LIMITS,
        endpoints: Box::new([Endpoint(0)]),
        chat_bytes: 64,
        text_bytes: 4096,
        models: 1,
        line_bytes: 1024,
        show_bytes: 4096,
        lines: 8,
        unsaved: 2,
        facts: 64,
    };
    let config = local::Config {
        chat: b"main".as_slice().into(),
        instructions: b"Assist the person".as_slice().into(),
        brief: run::charter::Brief { sections: Box::new([]) },
        models: Box::new([run::charter::Llm {
            prices: run::Prices { input: 0, cached: 0, output: 0, unit: 1 },
            dialect: 1,
            account: 0,
            endpoint: Endpoint(0),
            model: b"scripted".as_slice().into(),
            max_tokens: 256,
        }]),
        budget: smith_agent_world::BUDGET,
        conventions: None,
        contract: Contract::Report(outcome::TextSpec { max: 2048, fields: Box::new([]) }),
        waiting: Duration::from_secs(30),
        resume: true,
        accounts: Box::new([0]),
        workspace: None,
    };
    let bound = local::worst_case(&limits).expect("valid memory bound");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let meter = Meter::new();
    let mut out = Queue::<Request>::with_capacity(local::max_out(&env.limits));
    meter.start();
    let mut domain = local::Domain::new(config, &env.limits, 83).expect("valid local domain");
    meter.check(meter.end(), bound, &env.limits);
    for _ in 0..env.limits.lines {
        meter.start();
        local::step(
            &mut domain,
            &env,
            Event::Line { text: vec![b'x'; env.limits.line_bytes as usize].into_boxed_slice() },
            &mut out,
        );
        meter.check(meter.end(), bound, &env.limits);
    }
    assert!(meter.held() >= u64::from(env.limits.lines) * u64::from(env.limits.line_bytes));
    assert!(out.is_empty(), "loading retains all lines until the store answers");
}
