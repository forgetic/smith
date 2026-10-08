//! Entry-point tests for the local and host domain composition.

use alloc::boxed::Box;
use skein_lib::{Duration, List, Time, Wall};
use smith_domain::run::{self, charter::Endpoint, outcome};
use smith_local_domain as local;

use crate::{Config, Launch, Limits, ProcessLimits, Service, iterate};

fn config() -> Config {
    let local_limits = local::Limits {
        agent: smith_agent_world::LIMITS,
        endpoints: Box::new([Endpoint(4)]),
        chat_bytes: 64,
        text_bytes: 1024,
        models: 1,
        line_bytes: 256,
        show_bytes: 1024,
        lines: 8,
        unsaved: 2,
        facts: 16,
    };
    let mut host_limits = smith_host_world::limits();
    host_limits.agents = 1;
    host_limits.accounts = 1;
    let queue = local::max_out(&local_limits).max(smith_host_domain::max_out(&host_limits));
    let agent_limits = smith_agent_process_world::limits();
    let process = ProcessLimits {
        io: skein_io::Limits {
            sockets: 8,
            refusals: 1,
            intake: 4096,
            receive: 4096,
            output: 1_000_000,
            sends: 8,
            accepts: 1,
            backlog: 2,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(1),
        },
        channel: smith_host_protocol::Limits {
            bodies: agent_limits.channel.bodies,
            channel: agent_limits.channel.channel,
            calls: host_limits.calls,
        },
        detail_bytes: host_limits.detail_bytes,
        queue,
    };
    Config {
        local: local::Config {
            chat: Box::from(&b"main"[..]),
            instructions: Box::from(&b"Assist"[..]),
            brief: run::charter::Brief { sections: Box::new([]) },
            models: Box::new([run::charter::Llm {
                prices: run::Prices { input: 1, cached: 1, output: 1, unit: 1 },
                dialect: 1,
                account: 7,
                endpoint: Endpoint(4),
                model: Box::from(&b"small"[..]),
                max_tokens: 128,
            }]),
            budget: run::Budget { turns: 8, spend: 1, time: Duration::from_secs(60) },
            conventions: None,
            contract: local::Contract::Report(outcome::TextSpec { max: 256, fields: Box::new([]) }),
            deliver: None,
            title_field: Box::from(&b"title"[..]),
            waiting: Duration::from_secs(30),
            resume: true,
            accounts: Box::new([7]),
            workspace: None,
            push: None,
        },
        limits: Limits { local: local_limits, host: host_limits, process, queue },
        charter: Box::new([]),
        endpoints: smith_protocol_channel::Endpoints::new(List::with_capacity(0)),
        paths: Box::new([]),
        launch: Launch {
            program: Box::from(&b"smith"[..]),
            arguments: Box::from([Box::from(&b"agent"[..]), Box::from(&b"agent.json"[..])]),
            environment: Box::new([]),
            root: skein_io::kernel::Fd::new(0),
            directory: Box::from(&b"."[..]),
        },
    }
}

#[test]
fn the_local_service_loads_then_routes_durable_start_requests_to_the_shell() {
    let mut service = Service::new(config(), 7).expect("bounded local service");
    iterate(&mut service, Time::ZERO, Wall::EPOCH);
    let Some(local::Request::Load) = service.shell_requests().pop() else { panic!("load first") };
    service.local_event(local::Event::Loaded { state: None, transcript: None, deliveries: Box::new([]) });
    service.local_event(local::Event::Line { text: Box::from(&b"hello"[..]) });
    iterate(&mut service, Time::ZERO, Wall::EPOCH);
    let Some(local::Request::Credential { account: 7 }) = service.shell_requests().pop() else {
        panic!("credential before start")
    };
    let Some(local::Request::SaveState { state, fresh: false }) = service.shell_requests().pop() else {
        panic!("durable state before start")
    };
    assert_eq!(state.activation, 1);
}
