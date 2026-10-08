//! A sample host service supervising the real Smith agent service over Skein's
//! simulated child pipes (protocol/hosts.md, sections 3 and 7). The parent
//! keeps domain and process state; Skein's harness owns scheduling and child
//! lifetime, beside the scenario's fake machine and independent LLM peer. The host routing below is the part another host copies.

use skein_io::{self as io, kernel};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use smith_agent_process_world as agent_fixture;
use smith_host_domain as host;
use smith_host_protocol as protocol;

pub mod referee;

mod world;

pub use world::World;

/// Which child program the scripted machine supplies for the host's spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    /// A second simulated process running the actual agent service.
    Service,
    /// The machine refuses to start the requested agent.
    Refused,
    /// A child writes an error and never finishes its channel opening.
    ErrorTail,
    /// A child starts but never speaks the channel opening.
    Silent,
}

/// Boundary observations retained by the sample host's parent.
#[derive(Debug, Default)]
pub struct Seen {
    pub started: bool,
    pub admitted: bool,
    pub answered: Option<host::Answer>,
    pub gone: Option<host::End>,
    pub detail: Option<Box<[u8]>>,
    pub turns: u32,
    pub calls: u32,
    pub exits: u32,
    pub reaps: u32,
    pub signals: Vec<host::Signal>,
}

/// Boundary observations for an independent host-process referee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    /// The host domain accepted a run start.
    Started,
    /// The channel's last answer reached the parent.
    Answered,
    /// A named host call reached the parent.
    Called(Token),
    /// The parent supplied one terminal for a host call.
    CallAnswered(Token),
    /// The child process exited.
    Exited,
    /// The child and its pipes were reaped.
    Reaped,
    /// Spawn or opening failed and the process resources settled.
    Unspawned,
    /// The domain released the run slot.
    Gone,
}

/// The copied host composition: domain, process adapter, io and their queues.
#[expect(missing_debug_implementations, reason = "world service holds a non-Debug simulated kernel queue")]
#[expect(clippy::struct_excessive_bools, reason = "independent referee requests and startup-root close flags")]
pub struct HostService {
    domain: host::Domain,
    domain_env: Env<host::Limits>,
    io: io::Io,
    io_env: Env<io::Limits>,
    process: Option<protocol::Process>,
    domain_owner: Option<Token>,
    process_limits: protocol::Limits,
    root: kernel::Fd,
    domain_events: Queue<host::Event>,
    domain_requests: Queue<host::Request>,
    process_events: Queue<protocol::ProcessEvent>,
    io_events: Queue<io::Event>,
    io_requests: Queue<io::Request>,
    submissions: Queue<kernel::Submit>,
    completions: Queue<kernel::Complete>,
    seen: Seen,
    observations: Vec<Observation>,
    root_closing: bool,
    root_closed: bool,
    stop_requested: bool,
    crash_requested: bool,
    crashed: bool,
}

fn host_limits() -> host::Limits {
    let mut limits = smith_host_world::limits();
    limits.accounts = 1;
    limits.charter_bytes = 65_536;
    limits.transcript_bytes = 65_536;
    limits.answered_bytes = 65_536;
    limits.turns = 8;
    limits.turn_bytes = 65_536;
    limits.unacknowledged_bytes = 1_000_000_000;
    limits.outcome_bytes = 4096;
    limits.spawn_timeout = Duration::from_secs(2);
    limits.no_progress = Duration::from_secs(10);
    limits.grace = Duration::from_secs(5);
    limits.kill_after = Duration::from_secs(1);
    limits
}

fn io_limits() -> io::Limits {
    io::Limits {
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
    }
}

fn start() -> host::Start {
    host::Start {
        logical_run: Token::new(1),
        activation: 1,
        workspace: None,
        charter: agent_fixture::charter(),
        transcript: None,
        answered: Box::new([]),
        directories: Box::new([]),
        grants: Box::new([host::Grant { account: 0, generation: 1, valid: Duration::from_secs(7200) }]),
    }
}

impl HostService {
    /// Construct one host slot and queue a parent Spawn.
    #[must_use]
    pub fn new(root: kernel::Fd, now: Time, wall: Wall) -> HostService {
        let domain_limits = host_limits();
        let io_limits = io_limits();
        let agent_limits = agent_fixture::limits();
        let mut service = HostService {
            domain: host::Domain::new(&domain_limits),
            domain_env: Env { now, wall, limits: domain_limits },
            io: io::Io::new(&io_limits),
            io_env: Env { now, wall, limits: io_limits },
            process: None,
            domain_owner: None,
            process_limits: protocol::Limits {
                bodies: agent_limits.channel.bodies,
                channel: agent_limits.channel.channel,
                calls: domain_limits.calls,
            },
            root,
            domain_events: Queue::with_capacity(256),
            domain_requests: Queue::with_capacity(256),
            process_events: Queue::with_capacity(256),
            io_events: Queue::with_capacity(256),
            io_requests: Queue::with_capacity(256),
            submissions: Queue::with_capacity(256),
            completions: Queue::with_capacity(256),
            seen: Seen::default(),
            observations: Vec::with_capacity(256),
            root_closing: false,
            root_closed: false,
            stop_requested: false,
            crash_requested: false,
            crashed: false,
        };
        service.domain_events.push(host::Event::Spawn { client: Token::new(1), start: start() });
        service
    }

    /// One up pass and one down pass; the world supplies the injected clocks.
    pub fn iterate(&mut self, now: Time, wall: Wall) {
        if self.stop_requested {
            self.stop_requested = false;
            self.stop();
        }
        if self.crash_requested {
            self.crash_requested = false;
            self.crashed = true;
            self.process.as_ref().expect("started child").signal(kernel::Signal::Kill, &mut self.io_requests);
        }
        self.domain_env.now = now;
        self.domain_env.wall = wall;
        self.io_env.now = now;
        self.io_env.wall = wall;
        for _ in 0..self.completions.capacity() {
            let Some(complete) = self.completions.pop() else { break };
            if complete.op == Token::new(u64::MAX) {
                assert!(complete.result.is_ok(), "parent root closes");
                self.root_closed = true;
            } else {
                io::up(&mut self.io, &self.io_env, complete, &mut self.io_events, &mut self.submissions);
            }
        }
        for _ in 0..self.io_env.limits.sockets {
            if self.io.is_ready() {
                io::resume(&mut self.io, &self.io_env, &mut self.io_events, &mut self.submissions);
            }
            if self.io.is_due(now) {
                io::fire(&mut self.io, &self.io_env, &mut self.io_events, &mut self.submissions);
            }
        }
        for _ in 0..self.io_events.capacity() {
            let Some(event) = self.io_events.pop() else { break };
            self.process.as_mut().expect("spawn request preceded io event").from_io(
                now,
                event,
                &mut self.process_events,
                &mut self.io_requests,
            );
        }
        if let Some(process) = &mut self.process {
            process.fire(now, &mut self.process_events, &mut self.io_requests);
        }
        for _ in 0..self.process_events.capacity() {
            let Some(event) = self.process_events.pop() else { break };
            self.route_process(event);
        }
        for _ in 0..self.domain_env.limits.agents.saturating_mul(3) {
            if self.domain.is_due(now) {
                host::fire(&mut self.domain, &self.domain_env, &mut self.domain_requests);
            }
        }
        for _ in 0..self.domain_events.capacity() {
            let Some(event) = self.domain_events.pop() else { break };
            host::step(&mut self.domain, &self.domain_env, event, &mut self.domain_requests);
        }
        for _ in 0..self.domain_requests.capacity() {
            let Some(request) = self.domain_requests.pop() else { break };
            self.route_domain(request);
        }
        if let Some(process) = &mut self.process {
            for _ in 0..16_u32 {
                let Some(request) = process.next_channel_down() else { break };
                process.channel_down(request, &mut self.io_requests);
            }
        }
        for _ in 0..self.io_requests.capacity() {
            let Some(request) = self.io_requests.pop() else { break };
            io::down(&mut self.io, &self.io_env, request, &mut self.submissions);
        }
        if self.seen.gone.is_some() && !self.root_closing {
            self.root_closing = true;
            self.submissions
                .push(kernel::Submit { op: Token::new(u64::MAX), kind: kernel::Op::Close { fd: self.root } });
        }
        self.io.reclaim();
        self.domain.reclaim();
    }

    fn route_process(&mut self, event: protocol::ProcessEvent) {
        match event {
            protocol::ProcessEvent::Spawned { agent, process } => {
                self.domain_events.push(host::Event::Spawned { owner: agent, process });
            }
            protocol::ProcessEvent::Unspawned { agent, detail } => {
                self.observations.push(Observation::Unspawned);
                self.domain_events.push(host::Event::Unspawned { owner: agent, detail });
            }
            protocol::ProcessEvent::Exited { agent } => {
                self.observations.push(Observation::Exited);
                self.seen.exits = self.seen.exits.checked_add(1).expect("bounded observed exits");
                self.domain_events.push(host::Event::Exited { owner: agent });
            }
            protocol::ProcessEvent::Reaped { agent, detail } => {
                self.observations.push(Observation::Reaped);
                self.seen.reaps = self.seen.reaps.checked_add(1).expect("bounded observed reaps");
                self.domain_events.push(host::Event::Reaped { owner: agent, detail });
            }
            protocol::ProcessEvent::Channel { agent, event } => {
                self.route_channel(agent, event);
            }
        }
    }

    fn route_channel(&mut self, agent: Token, event: protocol::OpenEvent) {
        use protocol::OpenEvent;
        let event = match event {
            // Opened is transport-only. A failed write already settles its
            // send via Unsent; the independent read still owns Hangup.
            OpenEvent::Opened { .. } | OpenEvent::WriteFailed => return,
            OpenEvent::Hangup { .. } => host::Event::Hangup { owner: agent },
            OpenEvent::Sent { .. } => host::Event::Sent { owner: agent },
            OpenEvent::Unsent { .. } => host::Event::Unsent { owner: agent },
            OpenEvent::Answer { answer, .. } => {
                host::Event::Received { owner: agent, message: host::Up::Answer { answer } }
            }
            OpenEvent::Admitted => host::Event::Received { owner: agent, message: host::Up::Admitted },
            OpenEvent::Waiting { read } => host::Event::Received { owner: agent, message: host::Up::Waiting { read } },
            OpenEvent::Long { span } => host::Event::Received { owner: agent, message: host::Up::Long { span } },
            OpenEvent::LongDone => host::Event::Received { owner: agent, message: host::Up::LongDone },
            OpenEvent::Turn { turn } => host::Event::Received { owner: agent, message: host::Up::Turn { turn } },
            OpenEvent::Fact { body } => host::Event::Received { owner: agent, message: host::Up::Fact { body } },
            OpenEvent::Rejected { account, generation } => {
                host::Event::Received { owner: agent, message: host::Up::Rejected { account, generation } }
            }
            OpenEvent::Exhausted { account, retry_after } => {
                host::Event::Received { owner: agent, message: host::Up::Exhausted { account, retry_after } }
            }
            OpenEvent::Call { call, name, deadline, ask } => {
                host::Event::Received { owner: agent, message: host::Up::Call { call, name, deadline, ask } }
            }
            OpenEvent::Withdraw { call } => {
                host::Event::Received { owner: agent, message: host::Up::Withdraw { call } }
            }
        };
        self.domain_events.push(event);
    }

    fn route_domain(&mut self, request: host::Request) {
        match request {
            host::Request::Spawn { owner, deadline, .. } => {
                self.domain_owner = Some(owner);
                let mut process =
                    protocol::Process::new(owner, &self.process_limits, self.domain_env.limits.detail_bytes)
                        .expect("bounded host process");
                process.spawn(
                    protocol::Launch {
                        program: Box::from(&b"smith"[..]),
                        arguments: Box::from([Box::from(&b"agent"[..])]),
                        environment: Box::new([]),
                        root: self.root,
                        directory: Box::from(&b"."[..]),
                    },
                    deadline,
                    &mut self.io_requests,
                );
                self.process = Some(process);
            }
            host::Request::Started { .. } => {
                self.seen.started = true;
                self.observations.push(Observation::Started);
            }
            host::Request::Admitted { .. } => self.seen.admitted = true,
            host::Request::Answered { answer, .. } => {
                self.seen.answered = Some(answer);
                self.observations.push(Observation::Answered);
            }
            host::Request::Gone { end, detail, .. } => {
                self.seen.gone = Some(end);
                self.seen.detail = Some(detail);
                self.observations.push(Observation::Gone);
            }
            host::Request::Turn { turn, .. } => {
                self.seen.turns = self.seen.turns.checked_add(1).expect("bounded observed turns");
                self.domain_events.push(host::Event::Acknowledge { agent: self.agent_owner(), turn: turn.number });
            }
            host::Request::Called { call, .. } => {
                self.seen.calls = self.seen.calls.checked_add(1).expect("bounded observed calls");
                self.observations.push(Observation::Called(call));
                self.domain_events.push(host::Event::Answer {
                    agent: self.agent_owner(),
                    call,
                    reply: host::Reply::Unavailable,
                });
            }
            host::Request::Send { message, .. } => self.send(message),
            host::Request::Signal { signal, .. } => {
                self.seen.signals.push(signal);
                let signal_below = match signal {
                    host::Signal::Terminate => kernel::Signal::Terminate,
                    host::Signal::Kill => kernel::Signal::Kill,
                };
                self.process.as_ref().expect("spawned child").signal(signal_below, &mut self.io_requests);
                self.domain_events.push(host::Event::Signalled { owner: self.agent_owner() });
            }
            host::Request::Read { .. }
            | host::Request::Wait { .. }
            | host::Request::Reap { .. }
            | host::Request::Waiting { .. }
            | host::Request::Withdrawn { .. }
            | host::Request::Rejected { .. }
            | host::Request::Exhausted { .. }
            | host::Request::Told { .. }
            | host::Request::Faulted { .. }
            | host::Request::Bounced { .. } => {}
        }
    }

    fn send(&mut self, message: host::Down) {
        let process = self.process.as_mut().expect("spawned child");
        let token = Token::new(99);
        let answering = match &message {
            host::Down::Answer { call, .. } => Some(*call),
            host::Down::Start { .. }
            | host::Down::Message { .. }
            | host::Down::Acknowledge { .. }
            | host::Down::Grant { .. }
            | host::Down::Cancel => None,
        };
        let sent = match message {
            host::Down::Start { start, window } => process.send_start(
                start,
                window,
                protocol::Values { paths: Box::new([]), credentials: Box::new([Box::from(&b"\0\x03acctoken"[..])]) },
                token,
                &mut self.process_events,
                &mut self.io_requests,
            ),
            host::Down::Message { name, label, text } => {
                process.send_message(name, label, text, token, &mut self.process_events, &mut self.io_requests)
            }
            host::Down::Answer { call, reply } => {
                process.send_reply(call, reply, token, &mut self.process_events, &mut self.io_requests)
            }
            host::Down::Acknowledge { turn } => {
                process.send_acknowledge(turn, token, &mut self.process_events, &mut self.io_requests)
            }
            host::Down::Grant { grant } => process.send_grant(
                grant,
                Box::from(&b"\0\x03acctoken"[..]),
                token,
                &mut self.process_events,
                &mut self.io_requests,
            ),
            host::Down::Cancel => process.send_cancel(token, &mut self.process_events, &mut self.io_requests),
        };
        if sent.is_err() {
            self.domain_events.push(host::Event::Unsent { owner: self.agent_owner() });
        } else if let Some(call) = answering {
            self.observations.push(Observation::CallAnswered(call));
        }
    }

    fn agent_owner(&self) -> Token {
        self.domain_owner.expect("host domain admitted one agent")
    }

    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        [
            self.domain.next_deadline(),
            self.io.next_deadline(),
            self.process.as_ref().and_then(protocol::Process::next_deadline),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    #[must_use]
    pub fn work_pending(&self, now: Time) -> bool {
        self.io.is_ready()
            || self.io.is_due(now)
            || self.domain.is_due(now)
            || !self.io_events.is_empty()
            || !self.domain_events.is_empty()
            || !self.domain_requests.is_empty()
            || !self.process_events.is_empty()
            || !self.io_requests.is_empty()
            || !self.completions.is_empty()
    }

    pub fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }

    /// The sample parent requests a run stop through the host domain.
    pub fn stop(&mut self) {
        self.domain_events.push(host::Event::Stop { agent: self.agent_owner() });
    }

    pub fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    #[must_use]
    pub fn seen(&self) -> &Seen {
        &self.seen
    }

    /// Boundary events in their order at the host-process interface.
    #[must_use]
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }

    #[must_use]
    pub fn io_empty(&self) -> bool {
        self.io.is_empty()
    }
}
