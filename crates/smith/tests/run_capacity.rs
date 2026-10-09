use skein_lib::{Env, Queue, ReplyTo, Time, Token, Wall};
use smith_domain::run;

#[test]
fn standard_profile_admits_256_turn_charter_and_refuses_257_before_effects() {
    let configuration = smith::config::parse(
        br#"{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,"endpoints":[],"environment":[]}"#,
    )
    .expect("production standard profile");
    let endpoints = smith_agent_process_world::configuration().channel_endpoints;
    let encoded = smith_agent_process_world::charter();
    for turns in [64, 128, 256, 257] {
        let mut charter = smith_protocol_channel::decode_charter(&encoded, &smith_charter::CEILINGS, &endpoints)
            .expect("actual charter translation");
        charter.budget.turns = turns;
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: configuration.service.limits.domain.run };
        let mut domain = run::Domain::new(&env.limits);
        let mut output = Queue::with_capacity(run::MAX_OUT);
        run::step(
            &mut domain,
            &env,
            run::Event::Start {
                reply_to: ReplyTo::new(Token::new(1)),
                host_run: Token::new(1),
                activation: 1,
                window: run::Window {
                    turns: configuration.service.limits.channel.turns,
                    bytes: u64::MAX,
                    largest_turn: 1,
                },
                charter,
                workspace: None,
                transcript: None,
            },
            &mut output,
        );
        let mut requests = Vec::new();
        while let Some(request) = output.pop() {
            requests.push(request);
        }
        if turns <= 256 {
            let [run::Request::Admitted { .. }, run::Request::Open { opening, .. }] = requests.as_slice() else {
                panic!("the production run must admit and open the actual charter: {requests:?}");
            };
            assert_eq!(opening.budget.turns, turns);
            assert!(opening.budget.turns <= configuration.service.limits.domain.session.budget.turns);
        } else {
            let [run::Request::Answer { answer, .. }] = requests.as_slice() else {
                panic!("an oversized budget refuses before opening or IO: {requests:?}");
            };
            assert_eq!(*answer, run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Budget)));
            assert_eq!(domain.runs(), 0);
            assert_eq!(domain.conversations(), 0);
        }
    }
}
