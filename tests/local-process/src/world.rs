//! The simulated kernel's loop owns process binding, machine answers and
//! faults. Every driven process is a Host. It calls the binary's shared local
//! shell unchanged and exposes only terminal, peer and service observations.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use skein_fake_llm_domain::api;
use skein_fake_machine::{How, Machine, Opened};
use skein_io::kernel;
use skein_lib::{Duration, Queue, Token, Wall};

const STARTUP_ERROR_CLOSE: Token = Token::new(1 << 62);
use skein_sim::{Ask, Pid, Reply, Sim};
use skein_world::Host;
use smith::local_host::{Local, Resources, token_limits};
use smith::{local_settings, local_tokens};
use smith_agent_service as agent;
use smith_local_domain as local;
use smith_local_service as service;

use crate::git::Child;
use crate::llm::Peer;
use crate::oauth::{Browser, Peer as Issuer};
use crate::terminal::{Action, Terminal};
use skein_fake_checkout::git::{Remote, Tree};
use skein_fake_checkout::{Checkout, git};
use skein_fake_oauth as fake_oauth;
use smith_local_world::git::History;

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Both placements enter the same shell and local policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Spawned,
    InProcess,
}

/// Credential state at the next shell startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authentication {
    SignIn,
    Refresh,
    Refused,
    Proactive,
}

/// Files surviving separate invocations of the actual shared shell.
pub struct Files {
    path: PathBuf,
}

impl Files {
    #[must_use]
    pub fn new() -> Self {
        let index = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("smith-local-process-{}-{index}", std::process::id()));
        std::fs::create_dir_all(&path).expect("fixture directory");
        Self { path }
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Default for Files {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn lower_configuration() -> agent::Config {
    let mut lower = smith_agent_process_world::configuration();
    lower.limits.llm.head = Some(Duration::from_secs(10));
    lower.limits.machine.file_bytes = 4096;
    lower.limits.machine.entries = 32;
    lower.limits.file_bytes = 8192;
    lower.limits.file_entries = 32;
    lower.limits.file_slots = 8;
    lower
}

fn configuration(root: kernel::Fd, change: bool) -> (service::Config, agent::Config) {
    let lower = lower_configuration();
    let mut source = serde_json::json!({
        "agent": {}, "chat": "main", "instructions": "@local-shell Assist",
        "models": [{"endpoint": "", "name": "fake", "max_tokens": 1024,
            "input_price": 0, "cached_price": 0, "output_price": 0, "price_unit": 1}],
        "budget": {"turns": 8, "spend": 1, "seconds": 60}, "waiting_seconds": 30,
        "contract": {"form": "report", "max": 128}
    });
    if change {
        source["directories"] = serde_json::json!([{ "name": "repo", "path": "repo", "writable": true, "git": true }]);
        source["contract"] = serde_json::json!({ "form": "change", "checks_must_pass": false, "fields": [{"name": "title", "max": 256}, {"name": "body", "max": 1024}] });
    }
    let settings: local_settings::Settings = serde_json::from_value(source).expect("host settings");
    let endpoints = lower.channel_endpoints.clone();
    let policy = local_settings::policy(&settings, &endpoints, lower.limits.domain).expect("host charter policy");
    let charter = local_settings::charter(&policy.config, &endpoints).expect("wire charter");
    let mut host = smith_host_world::limits();
    host.accounts = 1;
    host.charter_bytes = 65_536;
    host.transcript_bytes = 1_048_576;
    host.answered_bytes = 65_536;
    host.message_bytes = 4096;
    host.messages = 8;
    host.call_bytes = 32_768;
    host.turns = 8;
    host.turn_bytes = 65_536;
    host.unacknowledged_bytes = 1_000_000_000;
    host.fact_bytes = 512;
    host.outcome_bytes = 4096;
    let queue = local::max_out(&policy.limits).max(smith_host_domain::max_out(&host)).max(256);
    let process = service::ProcessLimits {
        io: lower.limits.io,
        channel: smith_host_protocol::Limits {
            bodies: lower.limits.channel.bodies,
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
            charter,
            endpoints,
            paths: policy.paths,
            launch: service::Launch {
                program: b"smith-agent".as_slice().into(),
                arguments: Box::new([]),
                environment: Box::new([]),
                root,
                directory: b".".as_slice().into(),
            },
        },
        lower,
    )
}

/// One hosted shell invocation and its independent neighbours.
pub struct World {
    sim: Sim,
    machine: Machine,
    master: Opened,
    terminal_pid: Pid,
    terminal: Terminal,
    local: Option<(Pid, Local)>,
    agents: Vec<(Pid, agent::Service)>,
    peer_pid: Pid,
    peer: Peer,
    placement: Placement,
    state_directory: PathBuf,
    token_directory: PathBuf,
    seed: u64,
    facts: Vec<local::Fact>,
    observe_facts: bool,
    interrupted: bool,
    interrupt_on_query: bool,
    steps: u32,
    checkout: Option<(Checkout, History, u64)>,
    git_children: Vec<(Pid, Child)>,
    git_commands: Vec<Box<[Box<[u8]>]>>,
    git_replies: std::collections::VecDeque<(Box<[u8]>, u8)>,
    accounts: Box<[local_settings::Account]>,
    issuer: Option<(Pid, Issuer)>,
    browser: Option<(Pid, Browser)>,
    pages: u32,
    saved_before_query: bool,
    first_post: Option<Wall>,
}

fn scripts() -> Box<[api::Script]> {
    let mut turns = Vec::new();
    for answer in ["First answer", "Second answer", "Third answer"] {
        turns.push(api::Turn {
            lines: Box::new([api::Line::Call { name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() }]),
            finish: api::Finish::ToolCalls,
            tokens: 1,
        });
        turns.push(api::Turn {
            lines: Box::new([api::Line::Call {
                name: b"finish".as_slice().into(),
                arguments: format!("{{\"form\":\"report\",\"text\":\"{answer}\",\"fields\":{{}}}}").into_bytes().into(),
            }]),
            finish: api::Finish::ToolCalls,
            tokens: 1,
        });
    }
    Box::new([api::Script { cue: b"@local-shell".as_slice().into(), turns: turns.into() }])
}

fn change_scripts() -> Box<[api::Script]> {
    let turns = [
        ("wait", "{}"),
        ("write", "{\"path\":\"result.txt\",\"content\":\"new\\n\"}"),
        (
            "finish",
            "{\"form\":\"change\",\"text\":\"\",\"fields\":{\"title\":\"Updated result\",\"body\":\"Created the result file\"}}",
        ),
    ];
    Box::new([api::Script {
        cue: b"@local-shell".as_slice().into(),
        turns: turns
            .into_iter()
            .map(|(name, arguments)| api::Turn {
                lines: Box::new([api::Line::Call {
                    name: name.as_bytes().into(),
                    arguments: arguments.as_bytes().into(),
                }]),
                finish: api::Finish::ToolCalls,
                tokens: 1,
            })
            .collect(),
    }])
}

impl World {
    #[must_use]
    pub fn new(seed: u64, files: &Files, placement: Placement, answer: &[u8]) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![Action::Send(b"hello\n".as_slice().into()), Action::Wait(answer.into()), Action::Eof],
            false,
            true,
            false,
        )
    }

    #[must_use]
    pub fn interrupted(seed: u64, files: &Files, placement: Placement) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![
                Action::Send(b"hello\n".as_slice().into()),
                Action::Wait(b"Run cancelled".as_slice().into()),
                Action::Eof,
            ],
            true,
            true,
            false,
        )
    }

    #[must_use]
    pub fn without_facts(seed: u64, files: &Files, placement: Placement, answer: &[u8]) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![Action::Send(b"hello\n".as_slice().into()), Action::Wait(answer.into()), Action::Eof],
            false,
            false,
            false,
        )
    }

    pub fn changed(seed: u64, files: &Files, placement: Placement) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![
                Action::Send(b"change it\n".as_slice().into()),
                Action::Wait(b"Change delivered".as_slice().into()),
                Action::Eof,
            ],
            false,
            true,
            true,
        )
    }

    /// Attach a fake issuer and browser to the actual shared token store.
    #[must_use]
    pub fn authenticated(seed: u64, files: &Files, placement: Placement, mode: Authentication) -> Self {
        let commands = vec![
            Action::Send(b"hello\n".as_slice().into()),
            Action::Wait(if mode == Authentication::Refused {
                b"account is unavailable".as_slice().into()
            } else {
                b"First answer".as_slice().into()
            }),
            Action::Eof,
        ];
        let mut world = Self::configured(seed, files, placement, commands, false, true, false);
        let certificate = files.path.join("root.der");
        std::fs::write(&certificate, skein_tls_world::pki::ROOT).expect("fixture certificate");
        world.accounts = Box::new([local_settings::Account {
            number: 0,
            account_id: "acc".into(),
            oauth: Some(local_settings::OAuth {
                authorization_url: "https://skein.test/authorize".into(),
                token_endpoint: "https://skein.test/token".into(),
                client_id: "client".into(),
                redirect_uri: "http://127.0.0.1:2345/callback".into(),
                scope: "read".into(),
                address: "127.0.0.1:444".into(),
                server_name: "skein.test".into(),
                trust_der: Some(certificate.to_str().expect("fixture path").into()),
                json: false,
            }),
        }]);
        let limits = fake_oauth::Limits {
            document: token_limits(),
            uri_bytes: 8192,
            request_bytes: 16_384,
            codes: 4,
            rotations: 4,
            plans: 4,
        };
        let mut issuer = fake_oauth::Issuer::new(
            fake_oauth::Config {
                authorization_url: b"https://skein.test/authorize".as_slice().into(),
                token_endpoint: b"https://skein.test/token".as_slice().into(),
                client_id: b"client".as_slice().into(),
                client_secret: None,
                redirect_uri: b"http://127.0.0.1:2345/callback".as_slice().into(),
                refresh_token: b"refresh-old".as_slice().into(),
            },
            limits,
        )
        .expect("issuer fixture");
        issuer
            .queue(fake_oauth::Plan {
                status: 200,
                body: fake_oauth::Body::Token(skein_oauth::TokenResponse {
                    access_token: b"access-new".as_slice().into(),
                    refresh_token: Some(b"refresh-new".as_slice().into()),
                    expires_in: 7200,
                }),
                delay: Duration::ZERO,
                retry_after: Duration::ZERO,
            })
            .expect("token plan");
        let tokens = local_tokens::Tokens::new(&world.token_directory, token_limits()).expect("private token store");
        if mode == Authentication::SignIn {
            std::fs::remove_file(world.token_directory.join("0.json")).expect("no saved token");
        } else {
            let prior = skein_oauth::SavedToken {
                key: 0,
                generation: 1,
                access_token: if mode == Authentication::Proactive {
                    b"access-new".as_slice().into()
                } else {
                    b"old".as_slice().into()
                },
                refresh_token: if mode == Authentication::Refused {
                    b"wrong".as_slice().into()
                } else {
                    b"refresh-old".as_slice().into()
                },
                metadata: None,
                expires_at: if mode == Authentication::Proactive {
                    Wall::from_nanos(world.sim.wall().as_nanos() + Duration::from_secs(61).as_nanos())
                } else {
                    world.sim.wall()
                },
            };
            tokens
                .save(0, &skein_oauth::encode_record(&prior, &token_limits()).expect("expired token encoding"))
                .expect("expired token fixture");
        }
        world.peer = Peer::new(
            scripts(),
            skein_llm::Credential {
                access_token: b"access-new".as_slice().into(),
                account_id: b"acc".as_slice().into(),
            },
            if mode == Authentication::Proactive { Duration::from_secs(2) } else { Duration::ZERO },
        );
        let pid = world.sim.spawn_process();
        world.issuer = Some((
            pid,
            Issuer::new(
                issuer,
                &limits,
                kernel::Addr::from((std::net::Ipv4Addr::LOCALHOST, 444)),
                b"https://skein.test/token".as_slice().into(),
            ),
        ));
        world
    }

    fn configured(
        seed: u64,
        files: &Files,
        placement: Placement,
        commands: Vec<Action>,
        interrupt_on_query: bool,
        observe_facts: bool,
        change: bool,
    ) -> Self {
        let mut config = skein_sim::Config::calm();
        config.wall = skein_tls_world::pki::VALID;
        if seed >= 100 {
            config.faults.latency = 300;
            config.faults.latency_max = Duration::from_millis(1);
            config.faults.short_recv = 500;
            config.faults.short_send = 500;
            config.faults.short_read = 500;
            config.faults.short_write = 500;
            config.faults.cancel_race = 500;
        }
        let mut sim = Sim::new(seed, config);
        let mut machine = Machine::new();
        let items = if change {
            vec![
                skein_fake_machine::Item::directory(b"repo"),
                skein_fake_machine::Item::file(b"repo/original.txt", b"original\n"),
            ]
        } else {
            vec![]
        };
        let master = machine.lay(&items);
        let checkout = if change {
            let mut history = History::new(Tree::from([(b"original.txt".to_vec(), b"original\n".to_vec())]));
            let mut checkout = Checkout::new();
            git::clone_repository(&mut history, &mut checkout, b"repo", b"work").expect("clone fixture");
            git::check_out(&history, &mut checkout, b"work", 1).expect("checkout fixture");
            Some((checkout, history, 1))
        } else {
            None
        };
        let terminal_pid = sim.spawn_process();
        let peer_pid = sim.spawn_process();
        let opened = machine.open(master, b".", How::Directory).expect("terminal launch root");
        let root = sim.root(terminal_pid, skein_sim::Handle::new(opened.raw()));
        let token_directory = files.path.join("tokens");
        let tokens = local_tokens::Tokens::new(&token_directory, token_limits()).expect("private token store");
        if tokens.load(0).expect("token record").is_none() {
            let bytes = skein_oauth::encode_record(
                &skein_oauth::SavedToken {
                    key: 0,
                    generation: 1,
                    access_token: b"token".as_slice().into(),
                    refresh_token: b"refresh".as_slice().into(),
                    metadata: None,
                    expires_at: Wall::from_nanos(config.wall.as_nanos() + Duration::from_secs(7200).as_nanos()),
                },
                &token_limits(),
            )
            .expect("token fixture");
            tokens.save(0, &bytes).expect("durable token fixture");
        }
        let credential =
            skein_llm::Credential { access_token: b"token".as_slice().into(), account_id: b"acc".as_slice().into() };
        let peer = Peer::new(
            if change { change_scripts() } else { scripts() },
            credential,
            if interrupt_on_query { Duration::from_secs(3600) } else { Duration::ZERO },
        );
        Self {
            sim,
            machine,
            master,
            terminal_pid,
            terminal: Terminal::new(root, commands),
            local: None,
            agents: Vec::new(),
            peer_pid,
            peer,
            placement,
            state_directory: files.path.join("chat"),
            token_directory,
            seed,
            facts: Vec::new(),
            observe_facts,
            interrupted: false,
            interrupt_on_query,
            steps: 0,
            checkout,
            git_children: Vec::new(),
            git_commands: Vec::new(),
            git_replies: std::collections::VecDeque::new(),
            accounts: Box::new([local_settings::Account { number: 0, account_id: "acc".into(), oauth: None }]),
            issuer: None,
            browser: None,
            pages: 0,
            saved_before_query: false,
            first_post: None,
        }
    }

    fn bind_local(&mut self, pidfd: kernel::Fd) {
        assert!(self.local.is_none());
        let (pid, pipes) = self.sim.bind_service(self.terminal_pid, pidfd);
        let input = pipes.iter().find(|(child, _)| *child == 0).expect("stdin").1;
        let output = pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1;
        let error = pipes.iter().find(|(child, _)| *child == 2).expect("startup error pipe").1;
        let mut close = Queue::with_capacity(1);
        close.push(kernel::Submit { op: STARTUP_ERROR_CLOSE, kind: kernel::Op::Close { fd: error } });
        self.sim.submit(pid, &mut close);
        let signals = self.sim.open_signal_source(pid);
        let opened = self.machine.open(self.master, b".", How::Directory).expect("local launch root");
        let root = self.sim.root(pid, skein_sim::Handle::new(opened.raw()));
        let change = self.checkout.is_some();
        let (config, lower) = configuration(root, change);
        let delivery_roots = if change { Box::new([self.open_root(pid, b"repo")]) as Box<[_]> } else { Box::new([]) };
        let effect_roots = if change && self.placement == Placement::InProcess {
            Box::new([self.open_root(pid, b"repo")]) as Box<[_]>
        } else {
            Box::new([])
        };
        let local = Local::new(
            config,
            match self.placement {
                Placement::Spawned => None,
                Placement::InProcess => Some(lower),
            },
            Resources {
                state_directory: self.state_directory.clone(),
                token_directory: self.token_directory.clone(),
                input,
                output,
                signals,
                delivery_roots,
                effect_roots,
                delivery_environment: Box::new([]),
                accounts: self.accounts.clone(),
                seed: self.seed,
                oauth_entropy: [31; 32],
            },
        )
        .expect("actual shared local shell");
        self.local = Some((pid, local));
    }

    fn bind_agent(&mut self, parent: Pid, pidfd: kernel::Fd) {
        let (pid, pipes) = self.sim.bind_service(parent, pidfd);
        let mut service = agent::Service::new(lower_configuration(), self.seed).expect("agent service");
        let input = pipes.iter().find(|(child, _)| *child == 0).expect("stdin").1;
        let output = pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1;
        let error = pipes.iter().find(|(child, _)| *child == 2).expect("startup error pipe").1;
        let mut close = Queue::with_capacity(1);
        close.push(kernel::Submit { op: STARTUP_ERROR_CLOSE, kind: kernel::Op::Close { fd: error } });
        self.sim.submit(pid, &mut close);
        let signals = self.sim.open_signal_source(pid);
        service.adopt_streams(input, output, signals).expect("agent pipes and signals");
        self.agents.push((pid, service));
    }

    fn open_root(&mut self, pid: Pid, path: &[u8]) -> kernel::Fd {
        let opened = self.machine.open(self.master, path, How::Directory).expect("workspace root");
        self.sim.root(pid, skein_sim::Handle::new(opened.raw()))
    }

    fn bind_git(&mut self, parent: Pid, pidfd: kernel::Fd) {
        let (pid, pipes) = self.sim.bind_service(parent, pidfd);
        let (bytes, code) = self.git_replies.pop_front().expect("spawned git reply");
        self.git_children.push((pid, Child::new(&pipes, bytes, code)));
    }

    pub fn step(&mut self) {
        self.steps = self.steps.checked_add(1).expect("bounded schedule");
        assert!(self.steps <= 20_000 && self.sim.trace().len() <= 100_000, "finite world schedule");
        assert!(self.agents.len() <= 8 && self.git_children.len() <= 128, "finite world process cells");
        if let Some((pid, issuer)) = &mut self.issuer {
            self.sim.reap(*pid, issuer.completions());
            issuer.iterate(self.sim.now(), self.sim.wall());
            if issuer.posts() > 0 && self.first_post.is_none() {
                self.first_post = Some(self.sim.wall());
            }
            self.sim.submit(*pid, issuer.submissions());
        }
        if let Some((pid, browser)) = &mut self.browser {
            self.sim.reap(*pid, browser.completions());
            browser.iterate(self.sim.now(), self.sim.wall());
            self.sim.submit(*pid, browser.submissions());
        }
        self.sim.reap(self.peer_pid, self.peer.completions());
        self.peer.iterate(self.sim.now(), self.sim.wall());
        self.sim.submit(self.peer_pid, self.peer.submissions());
        if self.issuer.is_some() && !self.peer.queries().is_empty() && !self.saved_before_query {
            let tokens = local_tokens::Tokens::new(&self.token_directory, token_limits()).expect("private token store");
            assert_eq!(
                tokens
                    .load(0)
                    .expect("token record")
                    .expect("token already saved before grant use")
                    .access_token
                    .as_ref(),
                b"access-new"
            );
            self.saved_before_query = true;
        }
        if self.interrupt_on_query && !self.interrupted && !self.peer.queries().is_empty() {
            self.terminal.interrupt();
            self.interrupted = true;
        }
        for (pid, child) in &mut self.git_children {
            if self.sim.service_running(*pid) {
                self.sim.reap(*pid, child.completions());
                child.iterate(self.sim.now(), self.sim.wall());
                self.sim.submit(*pid, child.submissions());
                if child.is_empty() {
                    self.sim.finish_service(*pid, kernel::Exit::Code(child.code()));
                }
            }
        }
        for (pid, service) in &mut self.agents {
            if self.sim.service_running(*pid) {
                let mut arrived = Queue::with_capacity(256);
                self.sim.reap(*pid, &mut arrived);
                while let Some(complete) = arrived.pop() {
                    if complete.op == STARTUP_ERROR_CLOSE {
                        assert!(complete.result.is_ok());
                    } else {
                        service.completions().push(complete);
                    }
                }
                while let Some(path) = service.root_to_open() {
                    let opened = self.machine.open(self.master, path, How::Directory).expect("agent workspace root");
                    let root = self.sim.root(*pid, skein_sim::Handle::new(opened.raw()));
                    service.root_opened(Ok(root));
                }
                agent::iterate(service, self.sim.now(), self.sim.wall());
                self.sim.submit(*pid, service.submissions());
                if let Some(success) = agent::done(service) {
                    self.sim.finish_service(*pid, kernel::Exit::Code(u8::from(!success)));
                }
            }
        }
        if let Some((pid, local)) = &mut self.local
            && self.sim.service_running(*pid)
        {
            let mut arrived = Queue::with_capacity(256);
            self.sim.reap(*pid, &mut arrived);
            let mut spawned = Vec::new();
            let mut git_spawned = Vec::new();
            while let Some(complete) = arrived.pop() {
                if complete.op == STARTUP_ERROR_CLOSE {
                    assert!(complete.result.is_ok());
                    continue;
                }
                if let Ok(kernel::Done::Spawned { pidfd, .. }) = &complete.result
                    && let kernel::Op::Spawn { spawn } = &complete.kind
                {
                    if spawn.program.as_ref() == b"smith-agent" {
                        spawned.push((*pid, *pidfd));
                    } else if spawn.program.as_ref() == b"/usr/bin/git" {
                        git_spawned.push((*pid, *pidfd));
                    }
                }
                local.completions().push(complete);
            }
            local.iterate(self.sim.now(), self.sim.wall());
            self.sim.submit(*pid, local.submissions());
            while let Some(fact) = local.pop_fact() {
                if self.observe_facts {
                    assert!(self.facts.len() < 256);
                    self.facts.push(fact);
                }
            }
            if let Some(result) = local.result() {
                assert!(result.is_ok(), "shared shell IO error: {result:?}");
                let success = matches!(result, Ok(local::ExitStatus::Success));
                self.sim.finish_service(*pid, kernel::Exit::Code(u8::from(!success)));
                self.peer.stop();
                if let Some((_, issuer)) = &mut self.issuer {
                    issuer.stop();
                }
            }
            for (parent, pidfd) in git_spawned {
                self.bind_git(parent, pidfd);
            }
            for (parent, pidfd) in spawned {
                self.bind_agent(parent, pidfd);
            }
        }
        let mut arrived = Queue::with_capacity(256);
        self.sim.reap(self.terminal_pid, &mut arrived);
        while let Some(complete) = arrived.pop() {
            if let Ok(kernel::Done::Spawned { pidfd, .. }) = &complete.result {
                self.bind_local(*pidfd);
            }
            self.terminal.completions().push(complete);
        }
        self.terminal.iterate(self.sim.now(), self.sim.wall());
        self.sim.submit(self.terminal_pid, self.terminal.submissions());
        if self.browser.is_none()
            && let Some((_, issuer)) = &mut self.issuer
        {
            let shown = self.terminal.shown();
            if let Some(at) = shown.windows(9).position(|part| part == b"Sign in: ") {
                let url = &shown[at + 9..];
                // Visit only after the whole final URL parameter has reached
                // the terminal. A writer may hand it out across many packets.
                if let Some(end) = url.iter().position(|byte| *byte == b'\n') {
                    let redirect = issuer.authorize(url[..end].into(), self.sim.now());
                    self.browser = Some((self.sim.spawn_process(), Browser::new(redirect)));
                    self.pages += 1;
                }
            }
        }
        self.serve_machine();
        if self.idle()
            && let Some(at) = [
                self.sim.next_due(),
                self.terminal.next_deadline(),
                self.peer.next_deadline(),
                self.issuer.as_ref().and_then(|(_, issuer)| issuer.next_deadline()),
                self.browser.as_ref().and_then(|(_, browser)| browser.next_deadline()),
                self.local
                    .as_ref()
                    .filter(|(pid, _)| self.sim.service_running(*pid))
                    .and_then(|(_, local)| local.next_deadline()),
                self.agents
                    .iter()
                    .filter(|(pid, _)| self.sim.service_running(*pid))
                    .filter_map(|(_, service)| agent::next_deadline(service))
                    .min(),
            ]
            .into_iter()
            .flatten()
            .min()
        {
            self.sim.advance_to(at);
        }
    }

    fn idle(&self) -> bool {
        !self.terminal.work_pending(self.sim.now())
            && !self.peer.work_pending(self.sim.now())
            && self.sim.ready(self.terminal_pid) == 0
            && self.sim.ready(self.peer_pid) == 0
            && !self.sim.deferred(self.terminal_pid)
            && !self.sim.deferred(self.peer_pid)
            && self.issuer.as_ref().is_none_or(|(pid, issuer)| {
                !issuer.work_pending(self.sim.now()) && self.sim.ready(*pid) == 0 && !self.sim.deferred(*pid)
            })
            && self.browser.as_ref().is_none_or(|(pid, browser)| {
                !browser.work_pending(self.sim.now()) && self.sim.ready(*pid) == 0 && !self.sim.deferred(*pid)
            })
            && self.local.as_ref().is_none_or(|(pid, local)| {
                !self.sim.service_running(*pid)
                    || (!local.work_pending(self.sim.now()) && self.sim.ready(*pid) == 0 && !self.sim.deferred(*pid))
            })
            && self.git_children.iter().all(|(pid, child)| {
                !self.sim.service_running(*pid)
                    || (!child.work_pending(self.sim.now()) && self.sim.ready(*pid) == 0 && !self.sim.deferred(*pid))
            })
            && self.agents.iter().all(|(pid, service)| {
                !self.sim.service_running(*pid)
                    || (!agent::work_pending(service, self.sim.now())
                        && self.sim.ready(*pid) == 0
                        && !self.sim.deferred(*pid))
            })
    }

    fn current_tree(&mut self) -> Tree {
        let root = self.machine.open(self.master, b"repo", How::Directory).expect("repo");
        let mut pending = vec![(root, Vec::<u8>::new())];
        let mut tree = Tree::new();
        while let Some((directory, prefix)) = pending.pop() {
            for (kind, name) in self.machine.list(directory, 32, 4096).expect("bounded fixture tree") {
                if name.as_ref() == b".git" {
                    continue;
                }
                let path = [prefix.as_slice(), name.as_ref()].concat();
                match kind {
                    skein_fake_machine::Is::Directory => {
                        let opened = self.machine.open(directory, &name, How::Directory).expect("directory");
                        pending.push((opened, [path.as_slice(), b"/"].concat()));
                    }
                    skein_fake_machine::Is::File => {
                        let opened = self.machine.open(directory, &name, How::Read).expect("file");
                        tree.insert(path, self.machine.read(opened, 0, 8192).expect("fixture bytes"));
                        self.machine.close(opened);
                    }
                    _ => panic!("fixture uses regular files only"),
                }
            }
            self.machine.close(directory);
        }
        tree
    }

    fn git_reply(&mut self, args: &[Box<[u8]>]) -> (Box<[u8]>, u8) {
        let tree = self.current_tree();
        let (checkout, history, head) = self.checkout.as_mut().expect("git fixture");
        checkout.replace_tree(b"work", &tree);
        let verb = args.first().expect("git verb").as_ref();
        let bytes = match verb {
            b"rev-parse" if args.len() == 2 => format!("{head:040x}\n").into_bytes(),
            b"rev-parse" => return (Box::new([]), 1),
            b"status" => {
                let previous = history.tree(*head);
                let mut changed = Vec::new();
                for (path, bytes) in &tree {
                    if previous.get(path) != Some(bytes) {
                        changed.extend_from_slice(b" M ");
                        changed.extend_from_slice(path);
                        changed.push(0);
                    }
                }
                for path in previous.keys() {
                    if !tree.contains_key(path) {
                        changed.extend_from_slice(b" D ");
                        changed.extend_from_slice(path);
                        changed.push(0);
                    }
                }
                changed
            }
            b"add" => Vec::new(),
            b"-c" => {
                *head = git::commit(history, checkout, b"work", *head, args.last().expect("commit message"))
                    .expect("local commit parent")
                    .expect("changed tree");
                Vec::new()
            }
            b"log" => {
                let pattern = &args[3];
                let trailer = &pattern[b"--grep=^".len()..pattern.len() - 1];
                let mut at = Some(*head);
                let mut found = Vec::new();
                while let Some(commit) = at {
                    let message = history.commit_message(commit);
                    if message.split(|byte| *byte == b'\n').any(|line| line == trailer) {
                        found = format!("{commit:040x}\0").into_bytes();
                        found.extend_from_slice(message);
                        found.push(b'\n');
                        break;
                    }
                    at = history.parent(commit);
                }
                found
            }
            b"push" => panic!("push was not configured"),
            _ => panic!("unexpected git command {args:?}"),
        };
        (bytes.into(), 0)
    }

    #[must_use]
    pub fn commit_message(&self) -> &[u8] {
        let (_, history, head) = self.checkout.as_ref().expect("checkout");
        history.commit_message(*head)
    }
    #[must_use]
    pub fn commit_files(&self) -> Tree {
        let (_, history, head) = self.checkout.as_ref().expect("checkout");
        history.tree(*head)
    }
    #[must_use]
    pub fn git_commands(&self) -> &[Box<[Box<[u8]>]>] {
        &self.git_commands
    }

    fn serve_machine(&mut self) {
        let mut calls = Queue::with_capacity(256);
        let mut answers = Queue::with_capacity(256);
        self.sim.calls(&mut calls);
        while let Some(call) = calls.pop() {
            match &call.ask {
                Ask::Spawn { program, .. }
                    if [b"smith-local".as_slice(), b"smith-agent"].contains(&program.as_ref()) =>
                {
                    answers.push(skein_sim::Answer {
                        ticket: call.ticket,
                        result: Ok(Reply::Program(skein_sim::Program::Service)),
                    })
                }
                Ask::Spawn { program, args, .. } if program.as_ref() == b"/usr/bin/git" => {
                    let args = args.clone();
                    let reply = self.git_reply(&args);
                    self.git_commands.push(args);
                    self.git_replies.push_back(reply);
                    answers.push(skein_sim::Answer {
                        ticket: call.ticket,
                        result: Ok(Reply::Program(skein_sim::Program::Service)),
                    });
                }
                _ => skein_fake_machine::step(&mut self.machine, call, &mut answers),
            }
        }
        self.sim.answer(&mut answers);
    }

    pub fn settle(&mut self) {
        for _ in 0..20_000 {
            self.step();
            if self.terminal.is_empty()
                && self.peer.is_empty()
                && self.issuer.as_ref().is_none_or(|(_, issuer)| issuer.is_empty())
                && self.browser.as_ref().is_none_or(|(_, browser)| browser.is_empty())
            {
                assert!(self.local.as_ref().is_some_and(|(_, local)| local.is_empty()));
                self.sim.assert_quiescent(self.terminal_pid);
                self.sim.assert_no_open_fds(self.terminal_pid);
                self.sim.assert_quiescent(self.peer_pid);
                self.sim.assert_no_open_fds(self.peer_pid);
                for (pid, _) in &self.agents {
                    self.sim.assert_quiescent(*pid);
                    self.sim.assert_no_open_fds(*pid);
                }
                for (pid, _) in &self.git_children {
                    self.sim.assert_quiescent(*pid);
                    self.sim.assert_no_open_fds(*pid);
                }
                assert!(self.git_replies.is_empty());
                if let Some((pid, _)) = &self.issuer {
                    self.sim.assert_quiescent(*pid);
                    self.sim.assert_no_open_fds(*pid);
                }
                if let Some((pid, _)) = &self.browser {
                    self.sim.assert_quiescent(*pid);
                    self.sim.assert_no_open_fds(*pid);
                }
                let pid = self.local.as_ref().expect("local child").0;
                self.sim.assert_quiescent(pid);
                self.sim.assert_no_open_fds(pid);
                return;
            }
        }
        panic!(
            "local world settles: steps {}, terminal {:?}, shown {:?}, queries {}, facts {:?}",
            self.steps,
            self.terminal.exit(),
            String::from_utf8_lossy(self.terminal.shown()),
            self.peer.queries().len(),
            self.facts
        );
    }

    #[must_use]
    pub fn first_token_post(&self) -> Option<Wall> {
        self.first_post
    }

    #[must_use]
    pub fn oauth_posts(&self) -> u64 {
        self.issuer.as_ref().map_or(0, |(_, issuer)| issuer.posts())
    }
    #[must_use]
    pub fn page_visits(&self) -> u32 {
        self.pages
    }
    #[must_use]
    pub fn browser_replied(&self) -> bool {
        self.browser.as_ref().is_some_and(|(_, browser)| browser.saw_reply())
    }
    #[must_use]
    pub fn saved_before_query(&self) -> bool {
        self.saved_before_query
    }

    #[must_use]
    pub fn seen(&self) -> crate::referee::Seen {
        crate::referee::Seen {
            facts: self.facts.clone(),
            shown: self.terminal.shown().to_vec(),
            errors: self.terminal.errors().to_vec(),
            queries: self.peer.queries().to_vec(),
            exit: self.terminal.exit(),
            pushed: self.git_commands.iter().any(|args| args[0].as_ref() == b"push"),
            oauth: self.issuer.as_ref().map(|(_, issuer)| crate::referee::OAuthSeen {
                posts: issuer.posts(),
                pages: self.pages,
                browser_replied: self.browser_replied(),
                saved_before_query: self.saved_before_query,
            }),
        }
    }

    /// Replay application observations; rustls deliberately draws signing
    /// randomness from the kernel, so TLS record lengths are not a seed trace.
    #[must_use]
    pub fn trace(&self) -> Vec<String> {
        let mut trace = skein_world::domain::Trace::default();
        for fact in &self.facts {
            trace.log(skein_lib::Time::ZERO, format!("fact {fact:?}"));
        }
        for query in self.peer.queries() {
            trace.log(skein_lib::Time::ZERO, format!("query {query:?}"));
        }
        trace.log(skein_lib::Time::ZERO, format!("terminal {:?} {:?}", self.terminal.shown(), self.terminal.exit()));
        trace.lines().to_vec()
    }

    /// Host bounds plus the finite world's trace, fixture and observation cells.
    #[must_use]
    pub fn worst_case(&self) -> u64 {
        let hosts = self.issuer.as_ref().map_or(0, |(_, issuer)| issuer.worst_case())
            + self.browser.as_ref().map_or(0, |(_, browser)| browser.worst_case())
            + self.terminal.worst_case()
            + self.peer.worst_case()
            + self.local.as_ref().map_or(0, |(_, local)| local.worst_case())
            + self
                .agents
                .iter()
                .map(|(_, agent)| {
                    smith_agent_service::worst_case(&lower_configuration().limits).expect("agent bound")
                        + std::mem::size_of_val(agent) as u64
                })
                .sum::<u64>()
            + self.git_children.iter().map(|(_, child)| child.worst_case()).sum::<u64>();
        // The world admits at most 100,000 kernel trace records. Trace
        // summaries own names, not payload buffers; this fixture uses paths
        // and programs under 64 bytes. The remaining finite fixture allowance
        // prices its graph, fake disk, sockets and decoded peer observations.
        hosts + 100_000 * (std::mem::size_of::<skein_sim::Entry>() as u64 + 256) + 32 * 1024 * 1024
    }

    #[must_use]
    pub fn shown(&self) -> &[u8] {
        self.terminal.shown()
    }
    #[must_use]
    pub fn queries(&self) -> &[api::Query] {
        self.peer.queries()
    }
    #[must_use]
    pub fn facts(&self) -> &[local::Fact] {
        &self.facts
    }
    #[must_use]
    pub fn exit(&self) -> Option<kernel::Exit> {
        self.terminal.exit()
    }
    #[must_use]
    pub fn turns(&mut self) -> usize {
        let local::Event::Loaded { transcript, .. } =
            self.local.as_mut().expect("local child").1.store().load().expect("durable history")
        else {
            unreachable!()
        };
        transcript.map_or(0, |history| history.turns.len())
    }
    #[must_use]
    pub fn steps(&self) -> u32 {
        self.steps
    }
}

impl crate::referee::CheckoutRead for World {
    fn head(&self) -> Option<Vec<u8>> {
        self.checkout.as_ref().map(|(_, _, head)| format!("{head:040x}").into_bytes())
    }
    fn message(&self, commit: &[u8]) -> Vec<u8> {
        let commit =
            u64::from_str_radix(std::str::from_utf8(commit).expect("fixture head"), 16).expect("fixture object");
        self.checkout.as_ref().expect("checkout").1.commit_message(commit).to_vec()
    }
    fn files(&self, commit: &[u8]) -> Tree {
        let commit =
            u64::from_str_radix(std::str::from_utf8(commit).expect("fixture head"), 16).expect("fixture object");
        self.checkout.as_ref().expect("checkout").1.tree(commit)
    }
}
