//! Entry-point tests for the local and host domain composition.

use alloc::boxed::Box;
use skein_lib::{Duration, List, Time, Wall};
use skein_world::Host;
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
            sub_agents: false,
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
    for byte in b"hello\n" {
        service.feed(*byte);
    }
    iterate(&mut service, Time::ZERO, Wall::EPOCH);
    let Some(local::Request::Credential { account: 7 }) = service.shell_requests().pop() else {
        panic!("credential before start")
    };
    let Some(local::Request::SaveState { state, fresh: false }) = service.shell_requests().pop() else {
        panic!("durable state before start")
    };
    assert_eq!(state.activation, 1);
}

#[test]
fn original_conflict_files_are_read_through_the_services_kernel_routes() {
    use skein_lib::{Map, Queue, Token};
    let mut machine = skein_fake_machine::Machine::new();
    let handle = machine.lay(&[
        skein_fake_machine::Item::file(b"resolved.rs", b"clean\n"),
        skein_fake_machine::Item::file(b"conflict.rs", b"<<<<<<< ours\n"),
    ]);
    let mut sim = skein_sim::Sim::new(8, skein_sim::Config::calm());
    let pid = sim.spawn_process();
    let root = sim.root(pid, skein_sim::Handle::new(handle.raw()));
    let config = config();
    let mut process = crate::process::ProcessAdapter::new(config.limits.process, config.launch).unwrap();
    process.adopt_delivery_roots(Box::new([root]), Box::new([]));
    let mut local_events = Queue::with_capacity(256);
    let mut host_events = Queue::with_capacity(256);
    let mut host_requests = Queue::with_capacity(256);
    let mut terminal_events = Queue::with_capacity(256);
    let mut terminal =
        smith_local_protocol::Terminal::new(smith_local_protocol::TerminalLimits { line_bytes: 256, show_bytes: 1024 })
            .unwrap();
    process.start_markers(
        Token::new(7),
        0,
        Box::new([Box::from(b"resolved.rs".as_slice()), Box::from(b"conflict.rs".as_slice())]),
        Time::from_nanos(1_000_000_000),
        &mut local_events,
    );
    for _ in 0_u32..100 {
        sim.reap(pid, process.completions());
        process.up(sim.now(), sim.wall(), &mut host_events, &mut terminal, &mut terminal_events, &mut local_events);
        process.down(sim.now(), sim.wall(), &mut host_requests, &mut None, &Map::with_capacity(1), &mut host_events);
        sim.submit(pid, process.submissions());
        let mut calls = Queue::with_capacity(256);
        let mut answers = Queue::with_capacity(256);
        sim.calls(&mut calls);
        while let Some(call) = calls.pop() {
            skein_fake_machine::step(&mut machine, call, &mut answers);
        }
        sim.answer(&mut answers);
        if !local_events.is_empty() {
            break;
        }
        if let Some(at) = sim.next_due() {
            sim.advance_to(at);
        }
    }
    let Some(local::Event::Git { owner, result: local::GitResult::Markers { first: Some(path) } }) = local_events.pop()
    else {
        panic!("marker terminal after file settlement")
    };
    assert_eq!(owner, Token::new(7));
    assert_eq!(path.as_ref(), b"conflict.rs");
    assert!(local_events.is_empty());
    assert!(!process.work_pending(sim.now()));
}

fn settle_file_io(
    process: &mut crate::process::ProcessAdapter,
    sim: &mut skein_sim::Sim,
    machine: &mut skein_fake_machine::Machine,
    pid: skein_sim::Pid,
    local_events: &mut skein_lib::Queue<local::Event>,
) {
    let mut host_events = skein_lib::Queue::with_capacity(256);
    let mut host_requests = skein_lib::Queue::with_capacity(256);
    let mut terminal_events = skein_lib::Queue::with_capacity(256);
    let mut terminal =
        smith_local_protocol::Terminal::new(smith_local_protocol::TerminalLimits { line_bytes: 256, show_bytes: 1024 })
            .unwrap();
    for _ in 0_u32..400 {
        sim.reap(pid, process.completions());
        process.up(sim.now(), sim.wall(), &mut host_events, &mut terminal, &mut terminal_events, local_events);
        process.down(
            sim.now(),
            sim.wall(),
            &mut host_requests,
            &mut None,
            &skein_lib::Map::with_capacity(1),
            &mut host_events,
        );
        sim.submit(pid, process.submissions());
        let mut calls = skein_lib::Queue::with_capacity(256);
        let mut answers = skein_lib::Queue::with_capacity(256);
        sim.calls(&mut calls);
        while let Some(call) = calls.pop() {
            skein_fake_machine::step(machine, call, &mut answers);
        }
        sim.answer(&mut answers);
        if !process.work_pending(sim.now()) && sim.ready(pid) == 0 && !sim.deferred(pid) {
            match sim.next_due() {
                Some(at) => sim.advance_to(at),
                None => return,
            }
        }
    }
    panic!("file IO settles within bounded iterations")
}

#[test]
fn plain_status_uses_the_runs_snapshot_and_refreshes_it_before_the_next_run() {
    use skein_lib::{Queue, Token};
    let mut machine = skein_fake_machine::Machine::new();
    let handle = machine
        .lay(&[skein_fake_machine::Item::directory(b"src"), skein_fake_machine::Item::file(b"src/file", b"before")]);
    let mut sim = skein_sim::Sim::new(8, skein_sim::Config::calm());
    let pid = sim.spawn_process();
    let root = sim.root(pid, skein_sim::Handle::new(handle.raw()));
    let config = config();
    let mut process = crate::process::ProcessAdapter::new(config.limits.process, config.launch).unwrap();
    process.adopt_delivery_roots(Box::new([root]), Box::new([]));
    let mut events = Queue::with_capacity(256);
    let mut directories = List::with_capacity(1);
    directories.push(0).unwrap();
    process.capture_plain(directories.clone(), Time::from_nanos(10_000_000_000));
    settle_file_io(&mut process, &mut sim, &mut machine, pid, &mut events);
    assert_eq!(process.take_capture_done(), Some(true));
    process.plain_status(Token::new(7), 0, Time::from_nanos(10_000_000_000), &mut events);
    settle_file_io(&mut process, &mut sim, &mut machine, pid, &mut events);
    let Some(local::Event::PlainStatus { changed: false, .. }) = events.pop() else { panic!("unchanged snapshot") };
    let file = machine.open(handle, b"src/file", skein_fake_machine::How::Read).unwrap();
    machine.write(file, 0, b"edited").unwrap();
    machine.close(file);
    process.plain_status(Token::new(8), 0, Time::from_nanos(10_000_000_000), &mut events);
    settle_file_io(&mut process, &mut sim, &mut machine, pid, &mut events);
    let Some(local::Event::PlainStatus { owner, changed: true }) = events.pop() else { panic!("content changed") };
    assert_eq!(owner, Token::new(8));
    process.capture_plain(directories, Time::from_nanos(10_000_000_000));
    settle_file_io(&mut process, &mut sim, &mut machine, pid, &mut events);
    assert_eq!(process.take_capture_done(), Some(true));
    process.plain_status(Token::new(9), 0, Time::from_nanos(10_000_000_000), &mut events);
    settle_file_io(&mut process, &mut sim, &mut machine, pid, &mut events);
    let Some(local::Event::PlainStatus { changed: false, .. }) = events.pop() else { panic!("fresh run baseline") };
    assert!(events.is_empty());
}

#[test]
#[expect(clippy::wildcard_enum_match_arm, reason = "the story names unexpected shell requests")]
fn a_colocated_chat_routes_a_turn_through_the_shared_plaintext_effects_without_spawning() {
    use skein_lib::Queue;
    let (config, lower) = colocated_config();
    let mut sim_config = skein_sim::Config::calm();
    sim_config.wall = skein_tls_world::pki::VALID;
    let mut sim = skein_sim::Sim::new(8, sim_config);
    let pid = sim.spawn_process();
    let peer_pid = sim.spawn_process();
    let mut peer = smith_agent_process_world::fake::peer();
    let mut service = Service::new_in_process(config, lower, Box::new([]), 8).expect("colocated service");
    let mut machine = skein_fake_machine::Machine::new();
    let mut saved_turn = false;
    let mut shown = false;
    let mut sent = false;
    for _ in 0_u32..2000 {
        sim.reap(peer_pid, peer.completions());
        peer.iterate(sim.now(), sim.wall());
        sim.submit(peer_pid, peer.submissions());
        sim.reap(pid, service.completions());
        iterate(&mut service, sim.now(), sim.wall());
        while let Some(request) = service.shell_requests().pop() {
            match request {
                local::Request::Load => {
                    service.local_event(local::Event::Loaded {
                        state: None,
                        transcript: None,
                        deliveries: Box::new([]),
                    });
                    if !sent {
                        for byte in b"hello\n" {
                            service.feed(*byte);
                        }
                        sent = true;
                    }
                }
                local::Request::Credential { account: 0 } => service.credential(
                    smith_domain::Grant {
                        name: smith_domain::GrantName { account: 0, generation: 1 },
                        valid: Duration::from_secs(7200),
                    },
                    Box::from(b"\0\x03acctoken".as_slice()),
                ),
                local::Request::SaveState { .. } => service.local_event(local::Event::StateSaved),
                local::Request::SaveTurn { number, .. } => {
                    saved_turn = true;
                    service.local_event(local::Event::TurnSaved { number });
                }
                other => panic!("unexpected shell request {other:?}"),
            }
        }
        while let Some(text) = service.output().pop() {
            shown |= !text.is_empty();
        }
        assert!(service.lower_requests().is_empty(), "colocated mode never spawns an agent");
        sim.submit(pid, service.submissions());
        let mut calls = Queue::with_capacity(256);
        let mut answers = Queue::with_capacity(256);
        sim.calls(&mut calls);
        while let Some(call) = calls.pop() {
            skein_fake_machine::step(&mut machine, call, &mut answers);
        }
        sim.answer(&mut answers);
        if saved_turn && !service.work_pending(sim.now()) && sim.ready(pid) == 0 && !sim.deferred(pid) {
            break;
        }
        if sim.ready(pid) == 0
            && sim.ready(peer_pid) == 0
            && !sim.deferred(pid)
            && !sim.deferred(peer_pid)
            && !service.work_pending(sim.now())
            && !peer.work_pending(sim.now())
            && let Some(at) =
                [sim.next_due(), service.next_deadline(), peer.next_deadline()].into_iter().flatten().min()
        {
            sim.advance_to(at);
        }
    }
    assert!(saved_turn, "local policy acknowledged the shared effect's turn");
    assert!(smith_agent_process_world::fake::replied(&peer), "the fake plaintext LLM received the completion");
    assert!(shown, "the terminal observed the run");
    settle_colocated_exit(&mut service, &mut sim, pid, &mut peer, peer_pid);
}

#[expect(clippy::wildcard_enum_match_arm, reason = "the story names unexpected ending requests")]
fn settle_colocated_exit(
    service: &mut Service,
    sim: &mut skein_sim::Sim,
    pid: skein_sim::Pid,
    peer: &mut skein_fake_peers::llm::Peer,
    peer_pid: skein_sim::Pid,
) {
    service.closed();
    let mut exited = false;
    for _ in 0_u32..1000 {
        sim.reap(peer_pid, peer.completions());
        peer.iterate(sim.now(), sim.wall());
        sim.submit(peer_pid, peer.submissions());
        sim.reap(pid, service.completions());
        iterate(service, sim.now(), sim.wall());
        while let Some(request) = service.shell_requests().pop() {
            match request {
                local::Request::Exit { .. } => exited = true,
                other => panic!("unexpected ending {other:?}"),
            }
        }
        sim.submit(pid, service.submissions());
        if exited {
            break;
        }
        if sim.ready(pid) == 0
            && sim.ready(peer_pid) == 0
            && !sim.deferred(pid)
            && !sim.deferred(peer_pid)
            && !service.work_pending(sim.now())
            && !peer.work_pending(sim.now())
            && let Some(at) =
                [sim.next_due(), service.next_deadline(), peer.next_deadline()].into_iter().flatten().min()
        {
            sim.advance_to(at);
        }
    }
    assert!(exited, "local exit waits for lower cleanup");
    sim.assert_quiescent(pid);
    sim.assert_no_open_fds(pid);
    assert!(!service.work_pending(sim.now()));
}

fn colocated_config() -> (Config, smith_agent_service::Config) {
    let lower = smith_agent_process_world::configuration();
    let mut config = config();
    config.limits.local.agent = lower.limits.domain;
    config.limits.local.endpoints = lower.domain.endpoints.clone();
    config.local.accounts = Box::new([0]);
    config.local.models[0].account = 0;
    config.local.models[0].endpoint = Endpoint(0);
    config.local.models[0].prices = run::Prices { input: 0, cached: 0, output: 0, unit: 1 };
    config.local.models[0].max_tokens = 1024;
    config.local.budget.turns = 1;
    config.local.models[0].model = Box::from(b"fake".as_slice());
    (config, lower)
}
