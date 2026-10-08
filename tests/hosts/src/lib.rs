//! A sample host service supervising the real Smith agent service over Skein's
//! simulated child pipes (protocol/hosts.md, sections 3 and 7). The parent
//! keeps domain and process state; the world owns the simulator, fake machine
//! and fake LLM peer. The host routing below is the part another host copies.

use skein_io::{self as io, kernel};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_sim::{Config as SimConfig, Pid, Sim};
use smith_agent_process_world as agent_fixture;
use smith_agent_service as agent;
use smith_host_domain as host;
use smith_host_protocol as protocol;

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

/// The copied host composition: domain, process adapter, io and their queues.
#[expect(missing_debug_implementations, reason = "world service holds a non-Debug simulated kernel queue")]
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
        };
        service.domain_events.push(host::Event::Spawn { client: Token::new(1), start: start() });
        service
    }

    /// One up pass and one down pass; the world supplies the injected clocks.
    pub fn iterate(&mut self, now: Time, wall: Wall) {
        self.domain_env.now = now;
        self.domain_env.wall = wall;
        self.io_env.now = now;
        self.io_env.wall = wall;
        for _ in 0..self.completions.capacity() {
            let Some(complete) = self.completions.pop() else { break };
            io::up(&mut self.io, &self.io_env, complete, &mut self.io_events, &mut self.submissions);
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
        self.io.reclaim();
        self.domain.reclaim();
    }

    fn route_process(&mut self, event: protocol::ProcessEvent) {
        match event {
            protocol::ProcessEvent::Spawned { agent, process } => {
                self.domain_events.push(host::Event::Spawned { owner: agent, process });
            }
            protocol::ProcessEvent::Unspawned { agent, detail } => {
                self.domain_events.push(host::Event::Unspawned { owner: agent, detail });
            }
            protocol::ProcessEvent::Exited { agent } => {
                self.seen.exits = self.seen.exits.checked_add(1).expect("bounded observed exits");
                self.domain_events.push(host::Event::Exited { owner: agent });
            }
            protocol::ProcessEvent::Reaped { agent, detail } => {
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
            OpenEvent::Opened { .. } => return,
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
            OpenEvent::WriteFailed => host::Event::Malformed { owner: agent },
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
            host::Request::Started { .. } => self.seen.started = true,
            host::Request::Admitted { .. } => self.seen.admitted = true,
            host::Request::Answered { answer, .. } => self.seen.answered = Some(answer),
            host::Request::Gone { end, detail, .. } => {
                self.seen.gone = Some(end);
                self.seen.detail = Some(detail);
            }
            host::Request::Turn { turn, .. } => {
                self.seen.turns = self.seen.turns.checked_add(1).expect("bounded observed turns");
                self.domain_events.push(host::Event::Acknowledge { agent: self.agent_owner(), turn: turn.number });
            }
            host::Request::Called { call, .. } => {
                self.seen.calls = self.seen.calls.checked_add(1).expect("bounded observed calls");
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

    #[must_use]
    pub fn io_empty(&self) -> bool {
        self.io.is_empty()
    }
}

/// The simulated parent, agent child, fake LLM process, and machine.
#[expect(missing_debug_implementations, reason = "the scripted TLS peer has no Debug implementation")]
pub struct World {
    sim: Sim,
    parent: Pid,
    agent: Option<(Pid, agent::Service)>,
    peer_pid: Pid,
    peer: agent_fixture::peer::Peer,
    host: HostService,
    machine: skein_fake_machine::Machine,
    program: Program,
    stopped_child: bool,
}

impl World {
    #[must_use]
    pub fn new(seed: u64, program: Program) -> World {
        let mut config = SimConfig::calm();
        config.wall = skein_tls_world::pki::VALID;
        let mut sim = Sim::new(seed, config);
        let parent = sim.spawn_process();
        let peer_pid = sim.spawn_process();
        let mut machine = skein_fake_machine::Machine::new();
        let root = machine.lay(&[]);
        let root_fd = sim.root(parent, skein_sim::Handle::new(root.raw()));
        let host = HostService::new(root_fd, sim.now(), sim.wall());
        World {
            sim,
            parent,
            agent: None,
            peer_pid,
            peer: agent_fixture::peer::Peer::new(),
            host,
            machine,
            program,
            stopped_child: false,
        }
    }

    pub fn step(&mut self) {
        self.sim.reap(self.peer_pid, self.peer.completions());
        self.peer.step(self.sim.now(), self.sim.wall());
        self.sim.submit(self.peer_pid, self.peer.submissions());
        if let Some((pid, service)) = &mut self.agent
            && self.sim.service_running(*pid)
            && !self.stopped_child
        {
            self.sim.reap(*pid, service.completions());
            agent::iterate(service, self.sim.now(), self.sim.wall());
            self.sim.submit(*pid, service.submissions());
            if let Some(success) = agent::done(service) {
                self.sim.finish_service(*pid, kernel::Exit::Code(u8::from(!success)));
            }
        }
        let mut arrived = Queue::with_capacity(256);
        self.sim.reap(self.parent, &mut arrived);
        for _ in 0..arrived.capacity() {
            let Some(complete) = arrived.pop() else { break };
            if let Ok(kernel::Done::Spawned { pidfd, .. }) = &complete.result
                && (self.program == Program::Service || self.program == Program::ErrorTail)
            {
                let (pid, pipes) = self.sim.bind_service(self.parent, *pidfd);
                match self.program {
                    Program::Service => {
                        let mut service = agent_fixture::service(7);
                        let input = pipes.iter().find(|(child, _)| *child == 0).expect("stdin").1;
                        let output = pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1;
                        let signal = self.sim.open_signal_source(pid);
                        service.adopt_streams(input, output, signal).expect("bound child descriptors");
                        self.agent = Some((pid, service));
                    }
                    Program::ErrorTail => {
                        let error = pipes.iter().find(|(child, _)| *child == 2).expect("stderr").1;
                        let mut writes = Queue::with_capacity(1);
                        writes.push(kernel::Submit {
                            op: Token::new(1),
                            kind: kernel::Op::PipeWrite {
                                fd: error,
                                bytes: Box::from(&b"agent configuration failed"[..]),
                                from: 0,
                            },
                        });
                        self.sim.submit(pid, &mut writes);
                    }
                    Program::Refused | Program::Silent => unreachable!("only service programs bind"),
                }
            }
            self.host.completions().push(complete);
        }
        self.host.iterate(self.sim.now(), self.sim.wall());
        self.sim.submit(self.parent, self.host.submissions());
        self.serve_machine();
        if !self.host.work_pending(self.sim.now())
            && self.sim.ready(self.parent) == 0
            && self.sim.ready(self.peer_pid) == 0
            && !self.peer.work_pending()
            && self.agent.as_ref().is_none_or(|(pid, service)| {
                self.stopped_child || (self.sim.ready(*pid) == 0 && !agent::work_pending(service, self.sim.now()))
            })
        {
            let at = [
                self.sim.next_due(),
                self.host.next_deadline(),
                self.peer.next_deadline(),
                self.agent
                    .as_ref()
                    .filter(|_| !self.stopped_child)
                    .and_then(|(_, service)| agent::next_deadline(service)),
            ]
            .into_iter()
            .flatten()
            .min();
            if let Some(at) = at {
                self.sim.advance_to(at);
            }
        }
    }

    /// Stop driving the hosted agent after admission to model one that ignores Cancel.
    pub fn ignore_cancel(&mut self) {
        assert!(self.host.seen().admitted, "the agent opened before becoming unresponsive");
        self.stopped_child = true;
        self.host.stop();
    }

    fn serve_machine(&mut self) {
        let mut calls = Queue::with_capacity(256);
        let mut answers = Queue::with_capacity(256);
        self.sim.calls(&mut calls);
        for _ in 0..calls.capacity() {
            let Some(call) = calls.pop() else { break };
            match &call.ask {
                skein_sim::Ask::Spawn { program, .. } if program.as_ref() == b"smith" => {
                    let result = match self.program {
                        Program::Service | Program::ErrorTail => {
                            Ok(skein_sim::Reply::Program(skein_sim::Program::Service))
                        }
                        Program::Refused => Err(kernel::Error::NotFound),
                        Program::Silent => Ok(skein_sim::Reply::Program(skein_sim::Program::Never)),
                    };
                    answers.push(skein_sim::Answer { ticket: call.ticket, result });
                }
                skein_sim::Ask::Spawn { .. }
                | skein_sim::Ask::Open { .. }
                | skein_sim::Ask::Read { .. }
                | skein_sim::Ask::Write { .. }
                | skein_sim::Ask::Sync { .. }
                | skein_sim::Ask::Stat { .. }
                | skein_sim::Ask::Rename { .. }
                | skein_sim::Ask::Remove { .. }
                | skein_sim::Ask::MakeDirectory { .. }
                | skein_sim::Ask::List { .. }
                | skein_sim::Ask::Close { .. } => skein_fake_machine::step(&mut self.machine, call, &mut answers),
            }
        }
        self.sim.answer(&mut answers);
    }

    #[must_use]
    pub fn seen(&self) -> &Seen {
        self.host.seen()
    }

    #[must_use]
    pub fn peer_replied(&self) -> bool {
        self.peer.replied()
    }

    #[must_use]
    pub fn done(&self) -> bool {
        self.host.seen().gone.is_some()
    }

    pub fn settle(&mut self) {
        for _ in 0..10_000_u32 {
            self.step();
            if self.done() {
                return;
            }
        }
        assert!(
            self.done(),
            "host did not settle: {:?}; at {:?}\n{}",
            self.host.seen(),
            self.sim.now(),
            self.sim.render_trace()
        );
    }

    #[must_use]
    pub fn trace(&self) -> String {
        self.sim.render_trace()
    }
}
