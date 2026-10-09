use std::path::PathBuf;

use skein_lib::{Duration, Wall};
use skein_shell::Host;
use smith_local_domain as local;
use smith_local_service as service;
use smith_local_shell::local_host::{Local, Resources, token_limits};
use smith_local_shell::{local_settings, local_tokens};

fn configuration(root: skein_io::kernel::Fd) -> (service::Config, smith_agent_service::Config) {
    let lower = smith_agent_process_world::configuration();
    let settings: local_settings::Settings = serde_json::from_value(serde_json::json!({
        "agent": {}, "chat": "main", "instructions": "Assist",
        "models": [{"endpoint": "", "name": "fake", "window":8192,"output":1024,"reasoning_item":2048,"head":60000,"idle":30000,
            "input_price": 0, "cached_price": 0, "output_price": 0, "price_unit": 1}],
        "budget": {"turns": 1, "spend": 1, "seconds": 60}, "waiting_seconds": 30,
        "contract": {"form": "report", "max": 128}
    }))
    .expect("local settings");
    let endpoints = lower.channel_endpoints.clone();
    let policy = local_settings::policy(&settings, &endpoints, lower.limits.domain).expect("local policy");
    let host = smith_host_world::limits();
    let queue = local::max_out(&policy.limits).max(smith_host_domain::max_out(&host)).max(256);
    let process = service::ProcessLimits {
        io: lower.limits.io,
        channel: smith_host_protocol::Limits {
            bodies: lower.limits.channel.bodies,
            charter: lower.limits.channel.charter,
            transcript: lower.limits.channel.transcript,
            endpoints: lower.limits.channel.endpoints,
            channel: lower.limits.channel.channel,
            calls: host.calls,
        },
        detail_bytes: host.detail_bytes,
        queue,
    };
    (
        service::Config {
            local: policy.config,
            limits: service::Limits { local: policy.limits, host, process, queue },
            endpoints,
            paths: policy.paths,
            launch: service::Launch {
                program: b"smith".as_slice().into(),
                arguments: Box::new([]),
                environment: Box::new([]),
                root,
                directory: b".".as_slice().into(),
            },
        },
        lower,
    )
}

struct Invocation {
    sim: skein_sim::Sim,
    machine: skein_fake_machine::Machine,
    pid: skein_sim::Pid,
    peer_pid: skein_sim::Pid,
    peer: skein_fake_peers::llm::Peer,
    local: Local,
    input: skein_io::kernel::Fd,
    output: skein_io::kernel::Fd,
    seen: Vec<u8>,
    closed: bool,
    directory: PathBuf,
}

impl Invocation {
    fn new(name: &str, seed: u64) -> Self {
        let directory = std::env::temp_dir().join(format!("smith-library-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("test directory");
        let mut config = skein_sim::Config::calm();
        config.wall = skein_tls_world::pki::VALID;
        let mut sim = skein_sim::Sim::new(seed, config);
        let pid = sim.spawn_process();
        let peer_pid = sim.spawn_process();
        let mut machine = skein_fake_machine::Machine::new();
        let root = machine.lay(&[]);
        let root = sim.root(pid, skein_sim::Handle::new(root.raw()));
        let input = sim.open_inherited_read(pid);
        let output = sim.open_inherited_write(pid);
        let signals = sim.open_signal_source(pid);
        let token_directory = directory.join("tokens");
        let tokens = local_tokens::Tokens::new(&token_directory, token_limits()).expect("private tokens");
        let token = skein_oauth::encode_record(
            &skein_oauth::SavedToken {
                key: 0,
                generation: 1,
                access_token: b"token".as_slice().into(),
                refresh_token: Some(b"refresh".as_slice().into()),
                metadata: None,
                expires_at: Wall::from_nanos(
                    config.wall.as_nanos().saturating_add(Duration::from_secs(7200).as_nanos()),
                ),
            },
            &token_limits(),
        )
        .expect("saved token");
        tokens.save(0, &token).expect("token durable");
        let (config, lower) = configuration(root);
        let local = Local::new(
            config,
            Some(lower),
            Resources {
                state_directory: directory.join("chat"),
                token_directory,
                input,
                output,
                signals,
                delivery_roots: Box::new([]),
                effect_roots: Box::new([]),
                delivery_environment: Box::new([]),
                accounts: Box::new([local_settings::Account { number: 0, account_id: "acc".into(), oauth: None }]),
                seed,
                oauth_entropy: [17; 32],
            },
            Box::new(std::io::sink()),
        )
        .expect("shared local shell");
        Self {
            sim,
            machine,
            pid,
            peer_pid,
            peer: smith_agent_process_world::fake::peer(),
            local,
            input,
            output,
            seen: Vec::new(),
            closed: false,
            directory,
        }
    }

    fn step(&mut self) {
        self.sim.reap(self.peer_pid, self.peer.completions());
        self.peer.iterate(self.sim.now(), self.sim.wall());
        self.sim.submit(self.peer_pid, self.peer.submissions());
        self.sim.reap(self.pid, self.local.completions());
        self.local.iterate(self.sim.now(), self.sim.wall());
        self.local.drain();
        self.sim.submit(self.pid, self.local.submissions());
        skein_fake_machine::serve(&mut self.machine, &mut self.sim);
        self.seen.extend(self.sim.peer_drain(self.pid, self.output, 4096));
        if !self.local.work_pending(self.sim.now())
            && !self.peer.work_pending(self.sim.now())
            && self.sim.ready(self.pid) == 0
            && self.sim.ready(self.peer_pid) == 0
            && !self.sim.deferred(self.pid)
            && !self.sim.deferred(self.peer_pid)
            && let Some(at) =
                [self.sim.next_due(), self.local.next_deadline(), self.peer.next_deadline()].into_iter().flatten().min()
        {
            self.sim.advance_to(at);
        }
    }

    fn finish(&mut self) {
        if !self.closed {
            self.sim.peer_close(self.pid, self.input);
            self.closed = true;
        }
        for _ in 0..2000 {
            self.step();
            if self.local.is_empty() {
                break;
            }
        }
        assert!(
            matches!(self.local.result(), Some(Ok(local::ExitStatus::Success))),
            "local invocation finishes successfully"
        );
        self.sim.assert_quiescent(self.pid);
        self.sim.assert_no_open_fds(self.pid);
    }
}

impl Drop for Invocation {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.directory));
    }
}

#[test]
fn the_shared_library_saves_a_plaintext_turn_and_flushes_terminal_output_before_exiting() {
    let mut invocation = Invocation::new("turn", 17);
    assert_eq!(invocation.sim.peer_feed(invocation.pid, invocation.input, b"hello\n"), 6);
    for _ in 0..2000 {
        invocation.step();
        if invocation.directory.join("chat/0000000001.turn").exists() {
            break;
        }
    }
    assert!(
        smith_agent_process_world::fake::replied(&invocation.peer),
        "the shared service reached the plaintext peer"
    );
    let local::Event::Loaded { transcript: Some(transcript), .. } =
        invocation.local.store().load().expect("saved chat")
    else {
        panic!("the shell saved the typed turn");
    };
    assert_eq!(transcript.turns.len(), 1);
    invocation.finish();
    assert!(!invocation.seen.is_empty(), "output crossed the inherited terminal descriptor");
}

#[test]
fn an_empty_shared_invocation_closes_its_terminal_signals_and_launch_directory() {
    let mut invocation = Invocation::new("empty", 18);
    invocation.finish();
}
