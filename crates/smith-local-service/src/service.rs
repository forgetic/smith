//! Bounded local-to-host domain routing. Local policy owns durability and
//! delivery; the host kit owns child lifecycle and channel rights. The saved
//! conversation position continues across activation-local channel turn numbers
//! (protocol/channel.md, section 5.1; protocol/hosts.md, section 5.3).

use alloc::boxed::Box;
use core::mem::size_of;
use skein_io::kernel;
use skein_lib::{Env, List, Map, Queue, Time, Token, Wall};
use smith_agent_service as agent_service;
use smith_domain as agent;
use smith_host_domain as host;
use smith_local_domain as local;
use smith_local_protocol as protocol;
use smith_protocol_channel as channel;

use crate::process::{Launch, ProcessAdapter, ProcessLimits};

/// Immutable bounds for one local chat and its host process slot.
#[derive(Clone, Debug)]
pub struct Limits {
    pub local: local::Limits,
    pub host: host::Limits,
    pub process: ProcessLimits,
    pub queue: u32,
}

/// Startup choices and the configured wire charter for spawned agents.
#[derive(Debug)]
pub struct Config {
    pub local: local::Config,
    pub limits: Limits,
    pub charter: Box<[u8]>,
    pub endpoints: channel::Endpoints,
    pub paths: Box<[Box<[u8]>]>,
    pub launch: Launch,
}

/// Startup cannot allocate or represent the requested local composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Local policy refused the operator's configuration.
    Local(local::Invalid),
    /// The caller's queues cannot reserve a complete transition.
    Queue,
    /// The requested retained bound or directory table cannot be represented.
    Memory,
    /// The lower agent or process configuration cannot be composed.
    Process,
}

/// Checked memory bound for both domains and their routing queues.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    local::worst_case(&limits.local)?
        .checked_add(protocol::terminal_worst_case(&protocol::TerminalLimits {
            line_bytes: limits.local.line_bytes,
            show_bytes: limits.local.show_bytes,
        })?)?
        .checked_add(host::worst_case(&limits.host)?)?
        .checked_add(ProcessAdapter::worst_case(&limits.process)?)?
        .checked_add(Queue::<local::Event>::worst_case(limits.queue)?)?
        .checked_add(Queue::<local::Request>::worst_case(limits.queue)?.checked_mul(2)?)?
        .checked_add(Queue::<host::Event>::worst_case(limits.queue)?)?
        .checked_add(Queue::<host::Request>::worst_case(limits.queue)?.checked_mul(2)?)?
        .checked_add(Queue::<protocol::TerminalEvent>::worst_case(limits.queue)?)?
        .checked_add(Queue::<Box<[u8]>>::worst_case(limits.queue)?)?
        .checked_add(Map::<u32, Box<[u8]>>::worst_case(limits.local.agent.accounts)?)?
        .checked_add(u64::from(limits.local.agent.accounts).checked_mul(limits.host.answer_bytes)?)?
        .checked_add(
            u64::from(limits.local.agent.run.directories).checked_mul(u64::try_from(size_of::<kernel::Fd>()).ok()?)?,
        )?
        .checked_add(Queue::<kernel::Complete>::worst_case(limits.queue)?)?
        .checked_add(Queue::<kernel::Submit>::worst_case(limits.queue)?)?
        .checked_add(List::<u32>::worst_case(64)?)?
        .checked_add(u64::try_from(size_of::<Service>()).ok()?)
}

/// Bound for local policy, its terminal/delivery IO and the colocated agent effects.
#[must_use]
pub fn in_process_worst_case(limits: &Limits, effects: &agent_service::Limits) -> Option<u64> {
    worst_case(limits)?.checked_add(agent_service::effects_worst_case(effects)?)
}

#[derive(Debug)]
enum SnapshotState {
    Idle,
    Capturing { saved: bool },
    Failed,
}

/// One local chat, its host kit and bounded requests to process and shell.
#[derive(Debug)]
pub struct Service {
    limits: Limits,
    local: local::Domain,
    host: Option<host::Domain>,
    effects: Option<agent_service::Effects>,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    now: Time,
    plain_directories: List<u32>,
    capture_activation: Option<u64>,
    snapshot_state: SnapshotState,
    pending_exit: Option<local::ExitStatus>,
    process: ProcessAdapter,
    terminal: protocol::Terminal,
    terminal_events: Queue<protocol::TerminalEvent>,
    output: Queue<Box<[u8]>>,
    local_events: Queue<local::Event>,
    local_requests: Queue<local::Request>,
    host_events: Queue<host::Event>,
    host_requests: Queue<host::Request>,
    shell: Queue<local::Request>,
    lower: Queue<host::Request>,
    values: Map<u32, Box<[u8]>>,
    paths: Box<[Box<[u8]>]>,
    start_values: Option<StartValues>,
    pending_start: Option<local::ExternalStart>,
    charter: Box<[u8]>,
    endpoints: channel::Endpoints,
    host_agent: Option<Token>,
    sequence: u32,
    failed: bool,
}

/// Ordered lower values paired with the host kit's opaque Start names.
#[derive(Debug)]
pub struct StartValues {
    pub paths: Box<[Box<[u8]>]>,
    pub credentials: Box<[Box<[u8]>]>,
}

impl Service {
    /// Construct one spawned-agent local chat without touching files or processes.
    pub fn new(config: Config, seed: u64) -> Result<Self, Error> {
        Self::with_effects(config, seed, None)
    }

    /// Build a local chat whose agent contract crosses as typed entities in this process.
    pub fn new_in_process(
        mut config: Config,
        lower: agent_service::Config,
        roots: Box<[kernel::Fd]>,
        seed: u64,
    ) -> Result<Self, Error> {
        if lower.limits.domain != config.limits.local.agent {
            return Err(Error::Process);
        }
        for endpoint in &config.limits.local.endpoints {
            if !lower.domain.endpoints.contains(endpoint) {
                return Err(Error::Process);
            }
        }
        in_process_worst_case(&config.limits, &lower.limits).ok_or(Error::Memory)?;
        let Ok(mut effects) = agent_service::Effects::new(lower, seed) else { return Err(Error::Process) };
        effects.contract(local::charter(&config.local).outcome);
        config.local.workspace = match config.local.workspace.take() {
            Some(workspace) => match effects.workspace(workspace, roots) {
                Ok(workspace) => Some(workspace),
                Err(_) => return Err(Error::Process),
            },
            None if roots.is_empty() => None,
            None => return Err(Error::Memory),
        };
        Self::with_effects(config, seed, Some(effects))
    }

    fn with_effects(config: Config, seed: u64, effects: Option<agent_service::Effects>) -> Result<Self, Error> {
        let required = local::max_out(&config.limits.local).max(host::max_out(&config.limits.host)).max(64);
        if config.limits.queue < required {
            return Err(Error::Queue);
        }
        if worst_case(&config.limits).is_none() {
            return Err(Error::Memory);
        }
        let mut plain_directories = List::with_capacity(64);
        if let Some(workspace) = &config.local.workspace {
            for (position, directory) in workspace.directories.iter().enumerate() {
                if directory.writable
                    && !directory.git
                    && plain_directories.push(u32::try_from(position).expect("admitted directory")).is_err()
                {
                    return Err(Error::Memory);
                }
            }
        }
        let local_result = if effects.is_some() {
            local::Domain::new(config.local, &config.limits.local, seed)
        } else {
            local::Domain::new_external(config.local, &config.limits.local, seed)
        };
        let local = match local_result {
            Ok(local) => local,
            Err(error) => return Err(Error::Local(error)),
        };
        let host = if effects.is_some() { None } else { Some(host::Domain::new(&config.limits.host)) };
        let process = ProcessAdapter::new(config.limits.process, config.launch).ok_or(Error::Process)?;
        let terminal = protocol::Terminal::new(protocol::TerminalLimits {
            line_bytes: config.limits.local.line_bytes,
            show_bytes: config.limits.local.show_bytes,
        })
        .ok_or(Error::Memory)?;
        let queue = config.limits.queue;
        let accounts = config.limits.local.agent.accounts;
        Ok(Self {
            limits: config.limits,
            local,
            host,
            effects,
            completions: Queue::with_capacity(queue),
            submissions: Queue::with_capacity(queue),
            now: Time::ZERO,
            plain_directories,
            capture_activation: None,
            snapshot_state: SnapshotState::Idle,
            pending_exit: None,
            process,
            terminal,
            terminal_events: Queue::with_capacity(queue),
            output: Queue::with_capacity(queue),
            local_events: Queue::with_capacity(queue),
            local_requests: Queue::with_capacity(queue),
            host_events: Queue::with_capacity(queue),
            host_requests: Queue::with_capacity(queue),
            shell: Queue::with_capacity(queue),
            lower: Queue::with_capacity(queue),
            values: Map::with_capacity(accounts),
            paths: config.paths,
            start_values: None,
            pending_start: None,
            charter: config.charter,
            endpoints: config.endpoints,
            host_agent: None,
            sequence: 0,
            failed: false,
        })
    }

    /// Queue a terminal, store, git or OAuth outcome from the shell.
    pub fn local_event(&mut self, event: local::Event) {
        self.local_events.push(event);
    }

    /// Feed one standard-input byte to the bounded line translator.
    pub fn feed(&mut self, byte: u8) {
        self.terminal.feed(byte, &mut self.terminal_events);
    }

    /// Deliver a terminal interrupt to the run or stop its child on a repeat.
    pub fn interrupt(&mut self) {
        self.terminal.interrupt(&mut self.terminal_events);
    }

    /// Report standard-input closure to local policy.
    pub fn closed(&mut self) {
        self.terminal.closed(&mut self.terminal_events);
    }

    /// Text ready for the terminal writer.
    pub fn output(&mut self) -> &mut Queue<Box<[u8]>> {
        &mut self.output
    }

    /// Lend a credential value only beside the domain's grant name.
    pub fn credential(&mut self, grant: agent::Grant, envelope: Box<[u8]>) {
        if let Some(effects) = &mut self.effects
            && effects.grant(grant, &envelope, self.now).is_err()
        {
            self.local_events.push(local::Event::NoCredential {
                account: grant.name.account,
                reason: local::CredentialFailure::Missing,
            });
            return;
        }
        self.values.insert(grant.name.account, envelope).expect("configured account capacity");
        self.local_events.push(local::Event::Credential { grant });
    }

    /// Queue one host-channel or process terminal from the lower adapter.
    pub fn host_event(&mut self, event: host::Event) {
        self.host_events.push(event);
    }

    /// Shell work, including terminal, file, credential and git requests.
    pub fn shell_requests(&mut self) -> &mut Queue<local::Request> {
        &mut self.shell
    }

    /// Requests for the lower process adapter; it returns host events.
    pub fn lower_requests(&mut self) -> &mut Queue<host::Request> {
        &mut self.lower
    }

    /// Parallel workspace paths and grant envelopes for the first Start send.
    pub fn take_start_values(&mut self) -> Option<StartValues> {
        self.start_values.take()
    }

    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        let mut next: Option<Time> = None;
        for deadline in [
            self.local.next_deadline(),
            match &self.host {
                Some(host) => host.next_deadline(),
                None => None,
            },
            self.process.next_deadline(),
            match &self.effects {
                Some(effects) => effects.next_deadline(),
                None => None,
            },
        ]
        .into_iter()
        .flatten()
        {
            next = Some(match next {
                Some(current) => current.min(deadline),
                None => deadline,
            });
        }
        next
    }

    #[must_use]
    pub fn work_pending(&self, now: Time) -> bool {
        !self.completions.is_empty()
            || !self.submissions.is_empty()
            || self.local.is_ready()
            || self.local.is_due(now)
            || match &self.host {
                Some(host) => host.is_due(now),
                None => false,
            }
            || match &self.effects {
                Some(effects) => effects.work_pending(now),
                None => false,
            }
            || !self.local_events.is_empty()
            || !self.local_requests.is_empty()
            || !self.host_events.is_empty()
            || !self.host_requests.is_empty()
            || !self.terminal_events.is_empty()
            || self.process.work_pending(now)
    }

    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed || self.process.failed()
    }

    /// Kernel completions for the child process and its three pipes.
    pub fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }

    /// Adopt terminal input and blocked termination signals into the IO pass.
    pub fn adopt_terminal(&mut self, input: kernel::Fd, signals: kernel::Fd) -> Result<(), kernel::Fd> {
        self.process.adopt_terminal(input, signals)
    }

    /// Adopt separate workspace descriptors for delivery IO and its explicit child environment.
    /// Colocated effects own their roots independently; these descriptors must not alias them.
    pub fn adopt_delivery_roots(
        &mut self,
        roots: Box<[kernel::Fd]>,
        environment: Box<[Box<[u8]>]>,
    ) -> Result<(), Error> {
        if roots.len() > 64
            || roots.len() > usize::try_from(self.limits.local.agent.run.directories).expect("directory bound fits")
            || environment.len() > 64
        {
            return Err(Error::Memory);
        }
        let mut bytes = 0_usize;
        for entry in &environment {
            bytes = bytes.checked_add(entry.len()).ok_or(Error::Memory)?;
            if bytes > 4096 || !entry.contains(&b'=') || entry.contains(&0) {
                return Err(Error::Memory);
            }
        }
        self.process.adopt_delivery_roots(roots, environment);
        Ok(())
    }

    /// Kernel work submitted by the supervised child process.
    pub fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    /// Content-free local observations for the shell or an independent referee.
    pub fn pop_fact(&mut self) -> Option<local::Fact> {
        self.local.pop_fact()
    }

    fn route_completions(&mut self) {
        for _ in 0..self.completions.capacity() {
            let Some(complete) = self.completions.pop() else { break };
            if complete.op.raw() & EFFECT_OPERATION_BIT == 0 {
                self.process.completions().push(complete);
            } else {
                self.effects.as_mut().expect("effect namespace belongs to colocated mode").completions().push(
                    kernel::Complete {
                        op: Token::new(complete.op.raw() & !EFFECT_OPERATION_BIT),
                        kind: complete.kind,
                        result: complete.result,
                    },
                );
            }
        }
    }

    fn flush_submissions(&mut self) {
        for _ in 0..self.limits.queue {
            if self.submissions.room() == 0 {
                break;
            }
            let Some(submit) = self.process.submissions().pop() else { break };
            self.submissions.push(namespace_submit(submit, false));
        }
        if let Some(effects) = &mut self.effects {
            for _ in 0..self.limits.queue {
                if self.submissions.room() == 0 {
                    break;
                }
                let Some(submit) = effects.submissions().pop() else { break };
                self.submissions.push(namespace_submit(submit, true));
            }
        }
    }

    fn state_saved(&mut self) -> bool {
        match &mut self.snapshot_state {
            SnapshotState::Capturing { saved } => {
                *saved = true;
                false
            }
            SnapshotState::Idle => true,
            SnapshotState::Failed => false,
        }
    }

    fn finish_exit(&mut self) {
        let lower_closed = match &self.effects {
            Some(effects) => effects.closed(),
            None => true,
        };
        if self.shell.room() > 0
            && self.process.closed()
            && lower_closed
            && let Some(status) = self.pending_exit.take()
        {
            self.shell.push(local::Request::Exit { status });
        }
    }

    fn route_local(&mut self, now: Time, request: local::Request) {
        match request {
            local::Request::External(external) => self.route_external(now, *external),
            local::Request::Git { owner, directory, op, deadline } => match op {
                local::GitOp::Markers { paths } => {
                    self.process.start_markers(owner, directory, paths, deadline, &mut self.local_events);
                }
                op @ (local::GitOp::Head
                | local::GitOp::Status
                | local::GitOp::Inspect { .. }
                | local::GitOp::Commit { .. }
                | local::GitOp::Push { .. }) => {
                    self.process.start_git(owner, directory, op, deadline, &mut self.local_events);
                }
            },
            local::Request::Agent(request) => {
                self.effects.as_mut().expect("typed IO belongs to colocated mode").request(request);
            }
            local::Request::Show { text } => self.terminal.show(text, &mut self.terminal_events),
            local::Request::Load => {
                self.terminal.answer_finished();
                self.shell.push(local::Request::Load);
            }
            local::Request::PlainStatus { owner, directory, deadline } => {
                self.process.plain_status(owner, directory, deadline, &mut self.local_events);
            }
            local::Request::SaveState { state, fresh } => {
                if self.effects.is_some() && self.capture_activation != Some(state.activation) {
                    self.capture_activation = Some(state.activation);
                    self.snapshot_state = SnapshotState::Capturing { saved: false };
                    self.process
                        .capture_plain(self.plain_directories.clone(), now.saturating_add(self.limits.host.wall_time));
                }
                self.shell.push(local::Request::SaveState { state, fresh });
            }
            local::Request::Exit { status } => {
                self.pending_exit = Some(status);
                self.process.close();
                if let Some(effects) = &mut self.effects {
                    effects.close();
                }
            }
            other @ (local::Request::SaveTurn { .. }
            | local::Request::SaveDelivery { .. }
            | local::Request::Credential { .. }) => self.shell.push(other),
        }
    }

    fn drain_terminal(&mut self) {
        for _ in 0..self.terminal_events.capacity() {
            if self.local_events.room() == 0 || self.output.room() == 0 || !self.process.output_room() {
                break;
            }
            let Some(event) = self.terminal_events.pop() else { break };
            match event {
                protocol::TerminalEvent::Domain(event) => self.local_events.push(event),
                protocol::TerminalEvent::StopProcess => {
                    if self.effects.is_some() {
                        self.local_events.push(local::Event::Interrupt);
                    } else {
                        self.process.force_stop();
                    }
                }
                protocol::TerminalEvent::Write(text) => self.output.push(text),
            }
        }
    }

    fn route_external(&mut self, now: Time, request: local::ExternalRequest) {
        match request {
            local::ExternalRequest::Start(start) => {
                self.sequence = match &start.transcript {
                    Some(transcript) => match transcript.turns.last() {
                        Some(turn) => turn.sequence,
                        None => 0,
                    },
                    None => 0,
                };
                let mut directories = List::with_capacity(64);
                if let Some(workspace) = &start.workspace {
                    for (position, directory) in workspace.directories.iter().enumerate() {
                        if directory.writable && !directory.git {
                            directories
                                .push(u32::try_from(position).expect("admitted directory"))
                                .expect("bounded plain directories");
                        }
                    }
                }
                self.process.capture_plain(directories, now.saturating_add(start.charter.budget.time));
                self.pending_start = Some(start);
            }
            local::ExternalRequest::Message { run, name, text } => {
                self.host_events.push(host::Event::Message {
                    agent: run,
                    name,
                    label: Box::from(&b"person"[..]),
                    text,
                });
            }
            local::ExternalRequest::Acknowledge { run, turn } => {
                self.host_events.push(host::Event::Acknowledge { agent: run, turn });
            }
            local::ExternalRequest::Grant { run, grant } => {
                self.host_events.push(host::Event::Grant {
                    agent: run,
                    grant: host::Grant {
                        account: grant.name.account,
                        generation: grant.name.generation,
                        valid: grant.valid,
                    },
                });
            }
            local::ExternalRequest::Delivery { owner, delivery } => {
                let agent = self.host_agent.expect("delivery belongs to started host agent");
                match protocol::delivery_to_host(delivery) {
                    Ok(delivery) => self.host_events.push(host::Event::Answer {
                        agent,
                        call: owner,
                        reply: host::Reply::Delivery(delivery),
                    }),
                    Err(_) => {
                        self.failed = true;
                        self.host_events.push(host::Event::Answer {
                            agent,
                            call: owner,
                            reply: host::Reply::Unavailable,
                        });
                    }
                }
            }
            local::ExternalRequest::Cancel { run } => self.host_events.push(host::Event::Stop { agent: run }),
        }
    }

    fn start_captured(&mut self) {
        match self.process.take_capture_done() {
            Some(true) => {
                if self.effects.is_some() {
                    let state = core::mem::replace(&mut self.snapshot_state, SnapshotState::Idle);
                    match state {
                        SnapshotState::Capturing { saved: true } => self.local_events.push(local::Event::StateSaved),
                        SnapshotState::Capturing { saved: false } => {}
                        SnapshotState::Idle | SnapshotState::Failed => {
                            unreachable!("capture belongs to activation save")
                        }
                    }
                    return;
                }
                let start = self.pending_start.take().expect("capture belongs to pending start");
                let mut credentials = List::with_capacity(u32::try_from(start.grants.len()).expect("bounded grants"));
                for grant in &start.grants {
                    let value = self.values.get(&grant.name.account).expect("grant value supplied before Start");
                    credentials.push(value.clone()).expect("bounded grant count");
                }
                let prepared = protocol::prepare_start(
                    start,
                    self.charter.clone(),
                    &self.endpoints,
                    self.paths.clone(),
                    credentials.into_boxed(),
                );
                match prepared {
                    Ok(prepared) => {
                        self.start_values =
                            Some(StartValues { paths: prepared.paths, credentials: prepared.credentials });
                        self.host_events.push(host::Event::Spawn { client: Token::new(1), start: prepared.start });
                    }
                    Err(_) => {
                        self.failed = true;
                        self.local_events.push(local::Event::External(local::ExternalEvent::Failed));
                        self.local_events.push(local::Event::External(local::ExternalEvent::Gone));
                    }
                }
            }
            Some(false) => {
                if self.effects.is_some() {
                    self.snapshot_state = SnapshotState::Failed;
                    self.local_events.push(local::Event::StoreFailed { reason: local::StoreFailure::Read });
                    return;
                }
                self.pending_start = None;
                self.failed = true;
                self.local_events.push(local::Event::External(local::ExternalEvent::Failed));
                self.local_events.push(local::Event::External(local::ExternalEvent::Gone));
            }
            None => {}
        }
    }

    fn route_host(&mut self, request: host::Request) {
        match request {
            host::Request::Started { agent, .. } => self.host_agent = Some(agent),
            host::Request::Admitted { .. } => {
                let run = self.host_agent.expect("Started precedes Admitted");
                self.local_events.push(local::Event::External(local::ExternalEvent::Admitted { run }));
            }
            host::Request::Turn { turn, .. } => {
                let decoded = match self.sequence.checked_add(1) {
                    Some(sequence) => {
                        channel::decode_turn(&turn.body, sequence, &smith_transcript::CEILINGS, &self.endpoints).ok()
                    }
                    None => None,
                };
                match decoded {
                    Some(decoded) => {
                        self.sequence = decoded.sequence;
                        self.local_events.push(local::Event::External(local::ExternalEvent::Turn {
                            number: turn.number,
                            read: turn.read,
                            turn: decoded,
                        }));
                    }
                    None => {
                        self.failed = true;
                        self.local_events.push(local::Event::External(local::ExternalEvent::Failed));
                    }
                }
            }
            host::Request::Answered { answer, .. } => match protocol::answer_to_local(answer) {
                Ok(answer) => self.local_events.push(local::Event::External(local::ExternalEvent::Answer { answer })),
                Err(_) => {
                    self.failed = true;
                    self.local_events.push(local::Event::External(local::ExternalEvent::Failed));
                }
            },
            host::Request::Called { call, name, deadline, ask, .. } => self.called(call, name, deadline, ask),
            host::Request::Waiting { .. } => {
                self.local_events.push(local::Event::External(local::ExternalEvent::Waiting));
            }
            host::Request::Rejected { account, .. } => {
                self.local_events.push(local::Event::External(local::ExternalEvent::Rejected { account }));
            }
            host::Request::Exhausted { .. } => {
                self.local_events.push(local::Event::External(local::ExternalEvent::Exhausted));
            }
            host::Request::Faulted { .. } | host::Request::Bounced { .. } => {
                self.failed = true;
                self.local_events.push(local::Event::External(local::ExternalEvent::Failed));
            }
            host::Request::Gone { .. } => {
                self.local_events.push(local::Event::External(local::ExternalEvent::Gone));
                self.host_agent = None;
            }
            host::Request::Withdrawn { .. } | host::Request::Told { .. } => {}
            host::Request::Spawn { .. }
            | host::Request::Send { .. }
            | host::Request::Read { .. }
            | host::Request::Signal { .. }
            | host::Request::Wait { .. }
            | host::Request::Reap { .. } => self.lower.push(request),
        }
    }

    fn called(&mut self, call: Token, name: host::CallName, deadline: Time, ask: host::Ask) {
        let agent = self.host_agent.expect("call belongs to started host agent");
        match ask {
            host::Ask::Deliver { fields } => match protocol::decode_change(&fields) {
                Ok(change) => self.local_events.push(local::Event::External(local::ExternalEvent::Deliver {
                    name: agent::run::CallName {
                        activation: name.activation,
                        completion: name.completion,
                        position: name.position,
                    },
                    owner: call,
                    change,
                    deadline,
                })),
                Err(_) => {
                    self.failed = true;
                    self.host_events.push(host::Event::Answer {
                        agent,
                        call,
                        reply: host::Reply::Delivery(host::Delivery::Failed(host::DeliveryFailure {
                            directory: 0,
                            reason: host::DeliveryReason::Broken,
                            diagnostic: host::Diagnostic::empty(),
                        })),
                    });
                }
            },
            host::Ask::Host { .. } => {
                self.host_events.push(host::Event::Answer { agent, call, reply: host::Reply::Unavailable });
            }
        }
    }
}

/// One bounded up pass followed by one bounded down pass.
pub fn iterate(service: &mut Service, now: Time, wall: Wall) {
    service.now = now;
    service.route_completions();
    if let Some(effects) = &mut service.effects {
        effects.up(now, wall);
        for _ in 0..service.limits.queue {
            if service.local_events.room() == 0 {
                break;
            }
            let Some(event) = effects.event() else { break };
            service.local_events.push(local::Event::Agent(to_agent_io(event)));
        }
    }
    service.process.up(
        now,
        wall,
        &mut service.host_events,
        &mut service.terminal,
        &mut service.terminal_events,
        &mut service.local_events,
    );
    service.drain_terminal();
    service.start_captured();
    local_pass(service, now, wall);
    host_pass(service, now, wall);
    service.process.down(
        now,
        wall,
        &mut service.lower,
        &mut service.start_values,
        &service.values,
        &mut service.host_events,
    );
    if let Some(effects) = &mut service.effects {
        effects.down();
    }
    service.flush_submissions();
    service.finish_exit();
    service.local.reclaim();
    if let Some(host) = &mut service.host {
        host.reclaim();
    }
}

const EFFECT_OPERATION_BIT: u64 = 1 << 63;

fn local_pass(service: &mut Service, now: Time, wall: Wall) {
    let local_env = Env { now, wall, limits: service.limits.local.clone() };
    for _ in 0..service.local_events.capacity() {
        if service.local_requests.room() < local::max_out(&service.limits.local) {
            break;
        }
        let Some(event) = service.local_events.pop() else { break };
        match event {
            local::Event::StateSaved => {
                if !service.state_saved() {
                    continue;
                }
            }
            other @ (local::Event::Line { .. }
            | local::Event::Interrupt
            | local::Event::Closed
            | local::Event::Loaded { .. }
            | local::Event::TurnSaved { .. }
            | local::Event::DeliverySaved { .. }
            | local::Event::Git { .. }
            | local::Event::PlainStatus { .. }
            | local::Event::StoreFailed { .. }
            | local::Event::Credential { .. }
            | local::Event::NoCredential { .. }
            | local::Event::Agent(_)
            | local::Event::External(_)) => {
                local::step(&mut service.local, &local_env, other, &mut service.local_requests);
                continue;
            }
        }
        local::step(&mut service.local, &local_env, local::Event::StateSaved, &mut service.local_requests);
    }
    if service.local.is_due(now) && service.local_requests.room() >= local::max_out(&service.limits.local) {
        local::fire(&mut service.local, &local_env, &mut service.local_requests);
    }
    if service.local.is_ready() && service.local_requests.room() >= local::max_out(&service.limits.local) {
        local::resume(&mut service.local, &local_env, &mut service.local_requests);
    }
    for _ in 0..service.local_requests.capacity() {
        if service.shell.room() == 0
            || service.host_events.room() == 0
            || service.local_events.room() == 0
            || service.terminal_events.room() < protocol::terminal_max_out()
            || !service.process.output_room()
            || match &service.effects {
                Some(effects) => effects.request_room() == 0,
                None => false,
            }
        {
            break;
        }
        let Some(request) = service.local_requests.pop() else { break };
        service.route_local(now, request);
    }
    service.drain_terminal();
    service.start_captured();
}

fn host_pass(service: &mut Service, now: Time, wall: Wall) {
    let host_env = Env { now, wall, limits: service.limits.host };
    for _ in 0..service.host_events.capacity() {
        if service.host_requests.room() < host::max_out(&service.limits.host) {
            break;
        }
        let Some(event) = service.host_events.pop() else { break };
        host::step(
            service.host.as_mut().expect("spawned events own a host domain"),
            &host_env,
            event,
            &mut service.host_requests,
        );
    }
    if match &service.host {
        Some(host) => host.is_due(now),
        None => false,
    } && service.host_requests.room() >= host::max_out(&service.limits.host)
    {
        host::fire(service.host.as_mut().expect("spawned mode owns watchdog"), &host_env, &mut service.host_requests);
    }
    for _ in 0..service.host_requests.capacity() {
        if service.local_events.room() == 0 || service.lower.room() == 0 || service.host_events.room() == 0 {
            break;
        }
        let Some(request) = service.host_requests.pop() else { break };
        service.route_host(request);
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "kernel operations are external; only Cancel names another operation"
)]
fn namespace_submit(submit: kernel::Submit, effects: bool) -> kernel::Submit {
    assert!(submit.op.raw() & EFFECT_OPERATION_BIT == 0, "operation namespace never exhausts its half");
    let bit = if effects { EFFECT_OPERATION_BIT } else { 0 };
    let kind = match submit.kind {
        kernel::Op::Cancel { target } if target.raw() != u64::MAX => {
            assert!(target.raw() & EFFECT_OPERATION_BIT == 0, "lower cancel uses its own namespace");
            kernel::Op::Cancel { target: Token::new(target.raw() | bit) }
        }
        other => other,
    };
    kernel::Submit { op: Token::new(submit.op.raw() | bit), kind }
}

fn to_agent_io(event: agent::Event) -> local::AgentIo {
    match event {
        agent::Event::Completed { owner, completion } => local::AgentIo::Completed { owner, completion },
        agent::Event::Failed { owner, failure, evidence, detail } => {
            local::AgentIo::Failed { owner, failure, evidence, detail }
        }
        agent::Event::Cancelled { owner } => local::AgentIo::Cancelled { owner },
        agent::Event::Done { owner, done } => local::AgentIo::Done { owner, done },
        agent::Event::Read { owner, read } => local::AgentIo::Read { owner, read },
        agent::Event::Probed { owner, executable } => local::AgentIo::Probed { owner, executable },
        agent::Event::Checked { owner, ran } => local::AgentIo::Checked { owner, ran },
        agent::Event::Aborted { owner } => local::AgentIo::Aborted { owner },
        agent::Event::Start { .. }
        | agent::Event::Message { .. }
        | agent::Event::Cancel { .. }
        | agent::Event::Acknowledge { .. }
        | agent::Event::Grant { .. }
        | agent::Event::HostReturned { .. }
        | agent::Event::Delivered { .. } => unreachable!("effects emit only typed IO terminals"),
    }
}
