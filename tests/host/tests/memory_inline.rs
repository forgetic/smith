//! Both inline slots retain independent root inventories within their checked bound.
use skein_lib::{Env, Queue, Time, Token, Wall};
use skein_world::domain::heap::{Counting, Meter};
use smith_agent_world::Job;
use smith_host_domain::parent;
use smith_host_world::inline;
use smith_inline_agent::{Domain, Input, Output};

#[global_allocator]
static HEAP: Counting = Counting;

#[test]
fn two_full_inline_slots_and_their_emitted_prompts_fit_the_worst_case() {
    let limits = inline::limits();
    let bound = smith_inline_agent::worst_case(&limits).expect("checked bound");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let meter = Meter::new();
    let mut domain = Domain::new(&limits, inline::configuration(), 319);
    let mut out = Queue::with_capacity(smith_inline_agent::max_out(&limits));
    for client in 1..=2 {
        let start = inline::start(319, Job::HostTools);
        meter.start();
        smith_inline_agent::step(
            &mut domain,
            &env,
            Input::Parent(parent::Event::Spawn { client: Token::new(client), start }),
            &mut out,
        );
        let measured = meter.end();
        let mut emitted = false;
        while let Some(request) = out.pop() {
            if matches!(request, Output::Lower { request: smith_inline_agent::Lower::Complete { .. }, .. }) {
                emitted = true;
            }
        }
        assert!(emitted, "each composed root reaches the actual provider boundary");
        meter.check(measured, bound, &limits);
    }
    assert_eq!(domain.hosted(), 2, "maximum configured slots actually retained");
    assert!(meter.held() > 0);
}
