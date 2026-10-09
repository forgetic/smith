//! Bounded startup and staged routing for one agent process.
//! The retained state is one root domain, three protocol components, Skein IO,
//! one file driver, and queues for their requests and terminals. It never
//! chooses a domain outcome or interprets a bearer token.
//! Contract: protocol/agent.md, sections 4–6; protocol/README.md, section 6.

#![expect(clippy::manual_let_else, reason = "production steps use explicit bounded matches")]

use alloc::boxed::Box;

use skein_channel::{Lower as ChannelLower, LowerEvent as ChannelLowerEvent, StreamMode};
use skein_io::{self as io, file, file_layer, kernel};
use skein_lib::{Duration, Env, Id, List, Map, Queue, ReplyTo, Slab, Time, Token, Wall, stream};
use smith_domain::{self as domain, run, tools};
use smith_protocol_channel as channel;
use smith_protocol_llm as llm;
use smith_protocol_machine as machine;

#[derive(Debug)]
struct PendingMessage {
    name: Token,
    label: Box<[u8]>,
    text: Box<[u8]>,
}

const TRACE_PROMPT_BYTES: u32 = 65_536;
const COMPONENT_OWNER_BIT: u64 = 1 << 63;

/// Fixed limits for one agent process and every stage it owns.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub domain: domain::Limits,
    pub channel: channel::Limits,
    pub llm: llm::ComponentLimits,
    pub machine: machine::Limits,
    pub io: io::Limits,
    pub file_slots: u32,
    pub file_read: u32,
    pub file_entries: u32,
    pub file_bytes: u32,
    pub file_timeout: Duration,
    /// Cells in each queue between layers.
    pub queue: u32,
    /// Concurrent IO owner and kernel submission routes.
    pub routes: u32,
    /// Startup refuses a complete bound above this many heap bytes.
    pub memory: u64,
}

/// All startup inputs except the inherited descriptors, which the shell
/// adopts after constructing the service (protocol/agent.md, section 4).
#[derive(Debug)]
pub struct Config {
    pub limits: Limits,
    pub domain: domain::Config,
    pub channel_endpoints: channel::Endpoints,
    pub llm_endpoints: llm::Endpoints,
    pub environment: Box<[tools::Var]>,
    pub stream_mode: StreamMode,
    /// Capture bounded typed prompts for an everything trace.
    pub capture_prompts: bool,
}

/// Why startup cannot build a bounded agent service.
#[derive(Debug)]
pub enum ConfigError {
    /// IO cannot run under the requested capacities.
    Io,
    /// A stage cannot reserve its maximum output.
    Queue,
    /// One checked retained-memory calculation overflowed or is unusable.
    Memory,
    /// The complete bound exceeds the configured memory ceiling.
    MemoryCeiling,
    /// The host channel cannot open under these limits or endpoints.
    Channel(channel::Error),
    /// The LLM pool cannot retain these endpoints or limits.
    Llm(llm::ComponentError),
}

/// The first failure that prevented an answer from crossing the channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The channel ended before the answer.
    ChannelEnded,
    /// The channel's output side failed.
    WriteFailed,
    /// A host grant could not be installed below the domain.
    Credential,
    /// A required host record could not be encoded or queued.
    Send,
}

/// A lower owner belongs to exactly one protocol component.
#[derive(Clone, Copy, Debug)]
enum Owner {
    Llm(Token),
    Machine(Token),
}

/// One submission's original token and the layer that issued it.
#[derive(Clone, Copy, Debug)]
enum Operation {
    Io(Token),
    File(Token),
}

/// The running agent and every bounded queue between its layers.
#[derive(Debug)]
#[expect(clippy::struct_excessive_bools, reason = "answer, channel, cleanup and root-close progress are independent")]
pub struct Service {
    limits: Limits,
    domain: Option<domain::Domain>,
    channel: Option<channel::Component>,
    llm: llm::Component,
    machine: machine::Component,
    io: io::Io,
    files: file_layer::FileIo,
    domain_env: Env<domain::Limits>,
    llm_env: Env<llm::ComponentLimits>,
    machine_env: Env<machine::Limits>,
    io_env: Env<io::Limits>,
    channel_events: Queue<channel::OpenEvent>,
    trace_facts: Queue<domain::Fact>,
    trace_prompt: Option<(Token, Box<[u8]>)>,
    capture_prompts: bool,
    pending_messages: Queue<PendingMessage>,
    channel_below: Queue<ChannelLower>,
    llm_events: Queue<llm::ToDomain>,
    llm_below: Queue<io::Request>,
    machine_events: Queue<machine::ToDomain>,
    machine_below: Queue<machine::Below>,
    domain_events: Queue<domain::Event>,
    domain_requests: Queue<domain::Request>,
    io_events: Queue<io::Event>,
    io_requests: Queue<io::Request>,
    file_events: Queue<file::Event>,
    io_submissions: Queue<kernel::Submit>,
    file_submissions: Queue<kernel::Submit>,
    submissions: Queue<kernel::Submit>,
    completions: Queue<kernel::Complete>,
    owners: Slab<Owner>,
    operations: Map<Token, Operation>,
    next_operation: u64,
    next_send: u64,
    input: Option<Token>,
    output: Option<Token>,
    signals: Option<Token>,
    admitted: Option<Token>,
    run_started: Time,
    cancel_pending: bool,
    answer_sent: bool,
    channel_ended: bool,
    failed: bool,
    failure: Option<Failure>,
    lost_trace_facts: u64,
    lost_trace_prompts: u64,
    outcome: Option<run::outcome::OutcomeSpec>,
    delivery: Option<run::outcome::ChangeSpec>,
    pending_start: Option<Box<channel::DecodedStart>>,
    directories: List<run::Directory>,
    root_index: u32,
    pipe_routes: Map<Token, Token>,
    roots: List<Token>,
    root_close_next: u32,
    root_close_pending: bool,
    cleanup_started: bool,
}

/// Agent LLM and machine effects owned by a host that keeps the domain itself.
/// Typed requests have typed terminals, with no channel, frames or agent process
/// (domain/host.md, section 9; protocol/hosts.md, section 5.6).
#[derive(Debug)]
pub struct Effects {
    service: Service,
    installed: Map<u32, domain::GrantName>,
}

impl Effects {
    /// Build the shared lower stages, retaining neither a root domain nor a channel.
    pub fn new(config: Config, seed: u64) -> Result<Self, ConfigError> {
        let accounts = config.limits.llm.accounts;
        if effects_worst_case(&config.limits, &config.llm_endpoints).ok_or(ConfigError::Memory)? > config.limits.memory
        {
            return Err(ConfigError::MemoryCeiling);
        }
        let mut service = Service::new(config, seed)?;
        service.domain = None;
        service.channel = None;
        service.capture_prompts = false;
        Ok(Self { service, installed: Map::with_capacity(accounts) })
    }

    /// Configure the result shapes used by completion tool schemas.
    pub fn contract(&mut self, outcome: run::outcome::OutcomeSpec) {
        self.service.delivery.clone_from(&outcome.change);
        self.service.outcome = Some(outcome);
    }

    /// Adopt opened workspace roots and replace the host's opaque names with file tokens.
    pub fn workspace(
        &mut self,
        mut workspace: run::Workspace,
        roots: Box<[kernel::Fd]>,
    ) -> Result<run::Workspace, ConfigError> {
        if workspace.directories.len() != roots.len()
            || roots.len() > usize::try_from(self.service.limits.machine.roots).expect("root bound fits")
        {
            return Err(ConfigError::Io);
        }
        for (directory, fd) in workspace.directories.iter_mut().zip(&roots) {
            let root = self.service.files.adopt_root(*fd).ok_or(ConfigError::Io)?;
            self.service.roots.push(root).expect("admitted root count");
            directory.root = root;
        }
        self.service.root_close_next = self.service.roots.len();
        self.service.machine.workspace(&workspace);
        Ok(workspace)
    }

    /// Install a grant's bounded secret envelope before supplying the grant to the domain.
    pub fn grant(&mut self, grant: domain::Grant, value: &[u8], now: Time) -> Result<(), Failure> {
        if self.installed.get(&grant.name.account) == Some(&grant.name) {
            return Ok(());
        }
        let credential = credential(value).ok_or(Failure::Credential)?;
        match self.service.llm.grant(grant.name, credential, now.saturating_add(grant.valid)) {
            Ok(()) => {
                self.installed.insert(grant.name.account, grant.name).expect("admitted account count");
                Ok(())
            }
            Err(_) => Err(Failure::Credential),
        }
    }

    /// Remaining cells for typed IO requests; the owner reserves them before draining.
    #[must_use]
    pub fn request_room(&self) -> u32 {
        self.service.domain_requests.room()
    }

    /// Queue one effect emitted by the host's in-process agent domain.
    pub fn request(&mut self, request: domain::Request) {
        match request {
            request @ (domain::Request::Complete { .. }
            | domain::Request::Cancel { .. }
            | domain::Request::Io { .. }
            | domain::Request::CancelIo { .. }
            | domain::Request::Read { .. }
            | domain::Request::Probe { .. }
            | domain::Request::Check { .. }
            | domain::Request::Abort { .. }) => self.service.domain_requests.push(request),
            domain::Request::MessageRefused { .. }
            | domain::Request::Waiting { .. }
            | domain::Request::Turn { .. }
            | domain::Request::HostCall { .. }
            | domain::Request::WithdrawHost { .. }
            | domain::Request::Admitted { .. }
            | domain::Request::Answer { .. }
            | domain::Request::Checking { .. }
            | domain::Request::ChecksEnded { .. }
            | domain::Request::Deliver { .. }
            | domain::Request::Rejected { .. }
            | domain::Request::Exhausted { .. } => unreachable!("local domain owns host requests"),
        }
    }

    /// Take one typed LLM or machine terminal for the host's domain.
    pub fn event(&mut self) -> Option<domain::Event> {
        self.service.domain_events.pop()
    }

    /// Advance the same lower stages used by a framed agent at injected clocks.
    pub fn iterate(&mut self, now: Time, wall: Wall) {
        self.up(now, wall);
        self.down();
    }

    /// Reap IO and translate its settled terminals before the host's domain pass.
    pub fn up(&mut self, now: Time, wall: Wall) {
        let service = &mut self.service;
        clocks(service, now, wall);
        io_up(service);
        lower_events(service);
        component_up(service);
    }

    /// Submit domain effects and reclaim lower entities after the host's down pass.
    pub fn down(&mut self) {
        let service = &mut self.service;
        domain_down(service);
        component_down(service);
        cleanup(service);
        io_down(service);
        service.io.reclaim();
        service.llm.reclaim();
        service.owners.reclaim();
    }

    /// Completions supplied by the shell's kernel or a world's simulator.
    pub fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        self.service.completions()
    }

    /// Kernel work issued by the shared lower stages.
    pub fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        self.service.submissions()
    }

    /// The next deadline belonging to an LLM, machine or IO entity.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        next_deadline(&self.service)
    }

    /// Whether another lower pass can progress without waiting.
    #[must_use]
    pub fn work_pending(&self, now: Time) -> bool {
        work_pending(&self.service, now)
    }

    /// Close reusable lower entities after the local domain has ended its run.
    pub fn close(&mut self) {
        self.service.channel_ended = true;
        self.service.answer_sent = true;
    }

    /// Every lower child and adopted root has completed its close.
    #[must_use]
    pub fn closed(&self) -> bool {
        done(&self.service).is_some()
    }
}

/// Construction peak and retained bound for the host-owned effects adapter
/// with its configured endpoint request heads.
#[must_use]
pub fn effects_worst_case(limits: &Limits, endpoints: &llm::Endpoints) -> Option<u64> {
    worst_case(limits, endpoints)?
        .checked_add(Map::<u32, domain::GrantName>::worst_case(limits.llm.accounts)?)?
        .checked_add(u64::try_from(size_of::<Effects>()).ok()?)
}

/// Checked maximum of every owned layer and inter-layer queue. The file
/// driver's transient buffers are counted by its own bound; configured
/// endpoints supply the request-head and credential bounds.
#[must_use]
pub fn worst_case(limits: &Limits, endpoints: &llm::Endpoints) -> Option<u64> {
    let queue = limits.queue;
    let stages = Queue::<channel::OpenEvent>::worst_case(queue)?
        .checked_add(Queue::<domain::Fact>::worst_case(queue)?)?
        .checked_add(u64::from(queue).checked_mul(limits.domain.session.completion_bytes)?)?
        .checked_add(List::<u8>::worst_case(TRACE_PROMPT_BYTES)?.checked_mul(2)?)?
        .checked_add(Queue::<ChannelLower>::worst_case(queue)?)?
        .checked_add(Queue::<PendingMessage>::worst_case(queue)?)?
        .checked_add(Queue::<llm::ToDomain>::worst_case(queue)?)?
        .checked_add(Queue::<io::Request>::worst_case(queue)?.checked_mul(2)?)?
        .checked_add(Queue::<machine::ToDomain>::worst_case(queue)?)?
        .checked_add(Queue::<machine::Below>::worst_case(queue)?)?
        .checked_add(Queue::<domain::Event>::worst_case(queue)?)?
        .checked_add(Queue::<domain::Request>::worst_case(queue)?)?
        .checked_add(Queue::<io::Event>::worst_case(queue)?)?
        .checked_add(Queue::<file::Event>::worst_case(queue)?)?
        .checked_add(Queue::<kernel::Submit>::worst_case(queue)?.checked_mul(3)?)?
        .checked_add(Queue::<kernel::Complete>::worst_case(queue)?)?
        .checked_add(Slab::<Owner>::worst_case(limits.routes)?)?
        .checked_add(Map::<Token, Operation>::worst_case(limits.routes)?)?;
    let pipe_routes = Map::<Token, Token>::worst_case(limits.machine.processes.checked_mul(2)?)?;
    let roots = List::<run::Directory>::worst_case(limits.machine.roots)?
        .checked_add(List::<Token>::worst_case(limits.machine.roots)?)?;
    domain::worst_case(&limits.domain)?
        .checked_add(channel::worst_case(&limits.channel)?)?
        .checked_add(llm::component_worst_case(&limits.llm, endpoints)?)?
        .checked_add(machine::worst_case(&limits.machine)?)?
        .checked_add(io::worst_case(&limits.io)?)?
        .checked_add(file_layer::FileIo::worst_case(
            limits.file_slots,
            limits.file_read,
            limits.file_entries,
            limits.file_bytes,
        )?)?
        .checked_add(stages)?
        .checked_add(roots)?
        .checked_add(pipe_routes)
}

impl Service {
    /// Prepare every child and queue before a channel byte is read.
    pub fn new(config: Config, seed: u64) -> Result<Service, ConfigError> {
        let limits = config.limits;
        if !limits.io.is_usable() || limits.io.sockets < 3 || limits.file_slots < limits.machine.roots {
            return Err(ConfigError::Io);
        }
        let room = domain::max_out(&limits.domain)
            .max(channel::max_out(&limits.channel).below)
            .max(machine::max_out(&limits.machine).below)
            .max(llm::MAX_OUT.above)
            .max(llm::MAX_OUT.below)
            .max(io::MAX_OUT_UP.events)
            .max(io::MAX_OUT_UP.submissions);
        let lower_routes = match io::operations(&limits.io) {
            Some(operations) => operations.checked_add(2).ok_or(ConfigError::Queue)?,
            None => return Err(ConfigError::Queue),
        };
        if limits.queue < room || limits.routes < lower_routes {
            return Err(ConfigError::Queue);
        }
        let memory = worst_case(&limits, &config.llm_endpoints).ok_or(ConfigError::Memory)?;
        if memory > limits.memory {
            return Err(ConfigError::MemoryCeiling);
        }
        let channel = match channel::Component::new(&limits.channel, config.stream_mode, config.channel_endpoints) {
            Ok(channel) => channel,
            Err(error) => return Err(ConfigError::Channel(error)),
        };
        let llm = match llm::Component::new(&limits.llm, config.llm_endpoints) {
            Ok(llm) => llm,
            Err(error) => return Err(ConfigError::Llm(error)),
        };
        let mut machine = machine::Component::new(&limits.machine);
        machine.configure_environment(config.environment);
        let start = Time::ZERO;
        let wall = Wall::EPOCH;
        Ok(Service {
            limits,
            domain: Some(domain::Domain::new(&limits.domain, config.domain, seed)),
            channel: Some(channel),
            llm,
            machine,
            io: io::Io::new(&limits.io),
            files: file_layer::FileIo::with_whole_limit(
                limits.file_slots,
                limits.file_read,
                limits.file_entries,
                limits.file_bytes,
                limits.file_timeout,
            ),
            domain_env: Env { now: start, wall, limits: limits.domain },
            llm_env: Env { now: start, wall, limits: limits.llm },
            machine_env: Env { now: start, wall, limits: limits.machine },
            io_env: Env { now: start, wall, limits: limits.io },
            channel_events: Queue::with_capacity(limits.queue),
            trace_facts: Queue::with_capacity(limits.queue),
            trace_prompt: None,
            capture_prompts: config.capture_prompts,
            pending_messages: Queue::with_capacity(limits.queue),
            channel_below: Queue::with_capacity(limits.queue),
            llm_events: Queue::with_capacity(limits.queue),
            llm_below: Queue::with_capacity(limits.queue),
            machine_events: Queue::with_capacity(limits.queue),
            machine_below: Queue::with_capacity(limits.queue),
            domain_events: Queue::with_capacity(limits.queue),
            domain_requests: Queue::with_capacity(limits.queue),
            io_events: Queue::with_capacity(limits.queue),
            io_requests: Queue::with_capacity(limits.queue),
            file_events: Queue::with_capacity(limits.queue),
            io_submissions: Queue::with_capacity(limits.queue),
            file_submissions: Queue::with_capacity(limits.queue),
            submissions: Queue::with_capacity(limits.queue),
            completions: Queue::with_capacity(limits.queue),
            owners: Slab::with_capacity(limits.routes),
            operations: Map::with_capacity(limits.routes),
            next_operation: 1,
            next_send: 1,
            input: None,
            output: None,
            signals: None,
            admitted: None,
            run_started: Time::ZERO,
            cancel_pending: false,
            answer_sent: false,
            channel_ended: false,
            failed: false,
            failure: None,
            lost_trace_facts: 0,
            lost_trace_prompts: 0,
            outcome: None,
            delivery: None,
            pending_start: None,
            directories: List::with_capacity(limits.machine.roots),
            root_index: 0,
            pipe_routes: Map::with_capacity(limits.machine.processes.checked_mul(2).ok_or(ConfigError::Memory)?),
            roots: List::with_capacity(limits.machine.roots),
            root_close_next: 0,
            root_close_pending: false,
            cleanup_started: false,
        })
    }

    /// Kernel completions arriving since the last iteration.
    pub const fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }

    /// Submissions to hand to the kernel after an iteration.
    pub const fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    /// One content-free fact offered to a local trace, independent of the channel.
    pub fn pop_trace_fact(&mut self) -> Option<domain::Fact> {
        self.trace_facts.pop()
    }

    /// One owned content observation for local capture, never sent to the host.
    pub fn pop_trace_content(&mut self) -> Option<domain::Content> {
        self.domain.as_mut().expect("framed agent owns its domain").pop_content()
    }

    /// One bounded typed prompt, captured before its ownership passes to LLM.
    pub fn pop_trace_prompt(&mut self) -> Option<(Token, Box<[u8]>)> {
        self.trace_prompt.take()
    }

    /// Number of local trace facts lost to its bounded service queue.
    #[must_use]
    pub const fn lost_trace_facts(&self) -> u64 {
        self.lost_trace_facts
    }

    /// Prompt records that could not be retained by the local trace slot.
    #[must_use]
    pub const fn lost_trace_prompts(&self) -> u64 {
        self.lost_trace_prompts
    }

    /// Number of projected host facts dropped at the channel's output reserve.
    #[must_use]
    pub fn lost_channel_facts(&self) -> u64 {
        self.channel.as_ref().expect("framed agent owns its channel").lost_facts()
    }

    /// Give Skein IO the inherited channel and termination descriptors. The
    /// shell opens these before iterating; io closes them after the run.
    pub fn adopt_streams(
        &mut self,
        input: kernel::Fd,
        output: kernel::Fd,
        signals: kernel::Fd,
    ) -> Result<(), kernel::Fd> {
        let input = self.io.adopt_read_pipe(input)?;
        self.input = Some(input);
        let output = self.io.adopt_write_pipe(output)?;
        self.output = Some(output);
        let signals = self.io.adopt_signals(signals)?;
        self.signals = Some(signals);
        Ok(())
    }

    /// The next host-supplied mount path that the shell must open as a root.
    #[must_use]
    pub fn root_to_open(&self) -> Option<&[u8]> {
        let start = self.pending_start.as_ref()?;
        let mounts = start.mounts.as_ref()?;
        let index = usize::try_from(self.root_index).ok()?;
        match mounts.get(index) {
            Some(mount) => Some(&mount.path),
            None => None,
        }
    }

    /// Attach one opened mount in Start order, or refuse the Start if the
    /// shell could not open it. Root paths never enter the domain.
    pub fn root_opened(&mut self, root: Result<kernel::Fd, ()>) {
        match root {
            Ok(fd) => match self.files.adopt_root(fd) {
                Some(token) => {
                    self.roots.push(token).expect("root count was checked before opening");
                    self.root_close_next = self.roots.len();
                    let start = self.pending_start.as_ref().expect("a root belongs to a pending Start");
                    let mounts = start.mounts.as_ref().expect("a requested root belongs to a workspace");
                    let index = usize::try_from(self.root_index).expect("root index fits usize");
                    let mount = mounts.get(index).expect("one mount requested at a time");
                    self.directories
                        .push(run::Directory {
                            name: mount.name.clone(),
                            root: token,
                            writable: mount.writable,
                            git: mount.git,
                            conflicts: mount.conflicts.clone(),
                        })
                        .expect("root count was checked before opening");
                    self.root_index = self.root_index.checked_add(1).expect("bounded mount count");
                    if self.root_to_open().is_none() {
                        self.start_ready();
                    }
                }
                None => self.refuse_start(),
            },
            Err(()) => self.refuse_start(),
        }
    }

    fn next_send(&mut self) -> Token {
        let token = Token::new(self.next_send);
        self.next_send = self.next_send.checked_add(1).expect("one run does not exhaust send names");
        token
    }

    fn mark_failed(&mut self, reason: Failure) {
        self.failed = true;
        if self.failure.is_none() {
            self.failure = Some(reason);
        }
    }

    fn refuse_start(&mut self) {
        self.pending_start = None;
        let token = self.next_send();
        let answer = run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Workspace));
        let read = None;
        if self
            .channel
            .as_mut()
            .expect("framed agent owns its channel")
            .send_answer(answer, read, token, &mut self.channel_events, &mut self.channel_below)
            .is_err()
        {
            self.mark_failed(Failure::Send);
        } else {
            self.answer_sent = true;
        }
    }

    fn start_ready(&mut self) {
        let start = self.pending_start.take().expect("all roots opened for pending Start");
        let workspace = match start.mounts {
            Some(_) => Some(run::Workspace {
                directories: core::mem::replace(&mut self.directories, List::with_capacity(self.limits.machine.roots))
                    .into_boxed(),
            }),
            None => None,
        };
        if let Some(ref workspace) = workspace {
            self.machine.workspace(workspace);
        }
        self.outcome = Some(start.charter.outcome.clone());
        self.delivery.clone_from(&start.charter.outcome.change);
        let mut grants = List::with_capacity(u32::try_from(start.grants.len()).expect("bounded grant count"));
        for grant in &start.grants {
            let value = match self.channel.as_ref().expect("framed agent owns its channel").grant_value(grant.name) {
                Some(value) => value,
                None => {
                    self.refuse_start();
                    return;
                }
            };
            let credential = match credential(value) {
                Some(credential) => credential,
                None => {
                    self.refuse_start();
                    return;
                }
            };
            let lapses = self.domain_env.now.saturating_add(grant.valid);
            if self.llm.grant(grant.name, credential, lapses).is_err() {
                self.refuse_start();
                return;
            }
            grants
                .push(domain::Grant { name: grant.name, valid: grant.valid })
                .expect("grant count was checked at Start");
        }
        self.run_started = self.domain_env.now;
        self.domain_events.push(domain::Event::Start {
            messages: start.messages,
            reply_to: ReplyTo::new(Token::new(1)),
            host_run: Token::new(1),
            activation: start.activation,
            window: start.window,
            charter: start.charter,
            workspace,
            transcript: start.transcript,
            answered: start.answered,
            grants: grants.into_boxed(),
        });
    }
}

/// A grant value is a two-byte big-endian account-ID length, then that ID
/// and the bearer token. The host service uses the same bounded envelope.
fn credential(value: &[u8]) -> Option<skein_llm::Credential> {
    let header = value.get(..2)?;
    let length = usize::from(u16::from_be_bytes([*header.first()?, *header.get(1)?]));
    let end = 2_usize.checked_add(length)?;
    let account_id = value.get(2..end)?;
    let bearer = value.get(end..)?;
    if bearer.is_empty() {
        return None;
    }
    Some(skein_llm::Credential { access_token: Box::from(bearer), account_id: Box::from(account_id) })
}

/// A bounded local projection of the typed prompt. The trace writer decides
/// whether to retain it. If it exceeds the trace cap, the record is dropped
/// whole rather than holding the provider request back.
fn capture_prompt(prompt: &domain::llm::Prompt) -> Option<Box<[u8]>> {
    let mut bytes = List::with_capacity(TRACE_PROMPT_BYTES);
    if !append(&mut bytes, b"model:")
        || !append(&mut bytes, &prompt.model)
        || !append(&mut bytes, b"\nsystem:")
        || !append(&mut bytes, &prompt.system)
    {
        return None;
    }
    for message in &prompt.messages {
        let role = match message.role {
            domain::llm::Role::User => b"\nuser:".as_slice(),
            domain::llm::Role::Assistant => b"\nassistant:".as_slice(),
        };
        if !append(&mut bytes, role) {
            return None;
        }
        for block in &message.content {
            let kept = match block {
                domain::llm::Block::Opaque { bytes: opaque } => {
                    append(&mut bytes, b"\nopaque:") && append(&mut bytes, opaque)
                }
                domain::llm::Block::Refusal { text, replay } => {
                    append(&mut bytes, b"\nrefusal:")
                        && append(&mut bytes, text)
                        && append_replay(&mut bytes, replay.as_ref())
                }
                domain::llm::Block::Text { text, replay } => {
                    append(&mut bytes, b"\ntext:")
                        && append(&mut bytes, text)
                        && append_replay(&mut bytes, replay.as_ref())
                }
                domain::llm::Block::ToolCall { id, name, input, replay } => {
                    append(&mut bytes, b"\ncall:")
                        && append(&mut bytes, id)
                        && append(&mut bytes, b":")
                        && append(&mut bytes, name)
                        && append(&mut bytes, b":")
                        && append(&mut bytes, input)
                        && append_replay(&mut bytes, replay.as_ref())
                }
                domain::llm::Block::ToolResult { id, result } => {
                    append(&mut bytes, b"\nresult:") && append(&mut bytes, id) && append_result(&mut bytes, result)
                }
            };
            if !kept {
                return None;
            }
        }
    }
    Some(bytes.into_boxed())
}

fn append_replay(bytes: &mut List<u8>, replay: Option<&domain::llm::Replay>) -> bool {
    match replay {
        Some(replay) => append(bytes, b"\nreplay:") && append(bytes, &replay.bytes),
        None => true,
    }
}

fn append_result(bytes: &mut List<u8>, result: &domain::llm::Returned) -> bool {
    match result {
        domain::llm::Returned::Text { text, error: _, replay } => {
            append(bytes, b":text:") && append(bytes, text) && append_replay(bytes, replay.as_ref())
        }
        domain::llm::Returned::Withdrawn => append(bytes, b":withdrawn"),
        domain::llm::Returned::Owned { outcome: _ } => append(bytes, b":owned"),
        domain::llm::Returned::Served { returned: _, error: _ } => append(bytes, b":served"),
        domain::llm::Returned::Invalid { problem: _ } => append(bytes, b":invalid"),
        domain::llm::Returned::NotRun => append(bytes, b":not-run"),
    }
}

fn append(bytes: &mut List<u8>, part: &[u8]) -> bool {
    let Ok(count) = u32::try_from(part.len()) else {
        return false;
    };
    if count > bytes.room() {
        return false;
    }
    for byte in part {
        bytes.push(*byte).expect("checked room before append");
    }
    true
}

/// Advance io, the components, and the domain once at injected clocks.
pub fn iterate(service: &mut Service, now: Time, wall: Wall) {
    clocks(service, now, wall);
    io_up(service);
    lower_events(service);
    component_up(service);
    domain_up(service);
    domain_down(service);
    component_down(service);
    cleanup(service);
    io_down(service);
    service.io.reclaim();
    service.llm.reclaim();
    service.domain.as_mut().expect("framed agent owns its domain").reclaim();
    service.owners.reclaim();
}

fn clocks(service: &mut Service, now: Time, wall: Wall) {
    service.domain_env.now = now;
    service.domain_env.wall = wall;
    service.llm_env.now = now;
    service.llm_env.wall = wall;
    service.machine_env.now = now;
    service.machine_env.wall = wall;
    service.io_env.now = now;
    service.io_env.wall = wall;
}

fn io_up(service: &mut Service) {
    for _ in 0..service.limits.io.sockets {
        if !service.io.is_ready()
            || service.io_events.room() < io::MAX_OUT_RESUME.events
            || service.io_submissions.room() < io::MAX_OUT_RESUME.submissions
        {
            break;
        }
        io::resume(&mut service.io, &service.io_env, &mut service.io_events, &mut service.io_submissions);
    }
    for _ in 0..service.completions.capacity() {
        let complete = match service.completions.pop() {
            Some(complete) => complete,
            None => break,
        };
        let route = service.operations.remove(&complete.op).expect("one route per submitted operation");
        match route {
            Operation::Io(original) => {
                let complete = kernel::Complete { op: original, kind: complete.kind, result: complete.result };
                io::up(&mut service.io, &service.io_env, complete, &mut service.io_events, &mut service.io_submissions);
            }
            Operation::File(original) => {
                let complete = kernel::Complete { op: original, kind: complete.kind, result: complete.result };
                file_layer::up(&mut service.files, complete, &mut service.file_events, &mut service.file_submissions);
            }
        }
    }
    for _ in 0..service.limits.io.sockets {
        if !service.io.is_due(service.io_env.now)
            || service.io_events.room() < io::MAX_OUT_FIRE.events
            || service.io_submissions.room() < io::MAX_OUT_FIRE.submissions
        {
            break;
        }
        io::fire(&mut service.io, &service.io_env, &mut service.io_events, &mut service.io_submissions);
    }
    if service.files.is_due(service.io_env.now) && service.file_submissions.room() >= 2 {
        file_layer::expire(&mut service.files, service.io_env.now, &mut service.file_submissions);
    }
}

fn lower_events(service: &mut Service) {
    for _ in 0..service.io_events.capacity() {
        if service.domain_events.room() == 0
            || service.channel_events.room() < channel::max_out(&service.limits.channel).to_domain
            || service.channel_below.room() < channel::max_out(&service.limits.channel).below
            || service.llm_events.room() < llm::MAX_OUT.above
            || service.llm_below.room() < llm::MAX_OUT.below
            || service.machine_events.room() < machine::max_out(&service.limits.machine).to_domain
            || service.machine_below.room() < machine::max_out(&service.limits.machine).below
        {
            break;
        }
        let event = match service.io_events.pop() {
            Some(event) => event,
            None => break,
        };
        route_io_event(service, event);
    }
    for _ in 0..service.file_events.capacity() {
        if service.machine_events.room() < machine::max_out(&service.limits.machine).to_domain
            || service.machine_below.room() < machine::max_out(&service.limits.machine).below
        {
            break;
        }
        let event = match service.file_events.pop() {
            Some(event) => event,
            None => break,
        };
        if event.owner() == Token::new(u64::MAX) && service.root_close_pending {
            service.root_close_pending = false;
            continue;
        }
        service.machine.from_below(
            &service.machine_env,
            machine::BelowEvent::File(event),
            &mut service.machine_events,
            &mut service.machine_below,
        );
    }
}

fn cleanup(service: &mut Service) {
    if !service.channel_ended {
        return;
    }
    if !service.cleanup_started && service.io_requests.room() >= 3 {
        service.cleanup_started = true;
        if let Some(entity) = service.input {
            service.io_requests.push(io::Request::Close { entity });
        }
        if let Some(entity) = service.output {
            service.io_requests.push(io::Request::Close { entity });
        }
        if let Some(entity) = service.signals {
            service.io_requests.push(io::Request::Close { entity });
        }
    }
    if service.owners.is_empty()
        && service.pipe_routes.is_empty()
        && service.files.takes()
        && !service.root_close_pending
        && service.file_events.room() > 0
        && service.file_submissions.room() >= 2
    {
        if service.root_close_next > 0 {
            service.root_close_next = service.root_close_next.checked_sub(1).expect("positive root count");
            let root = *service.roots.get(service.root_close_next).expect("closed roots in reverse order");
            service.root_close_pending = true;
            file_layer::down(
                &mut service.files,
                service.io_env.now,
                file::Request::Close { owner: Token::new(u64::MAX), file: root },
                &mut service.file_events,
                &mut service.file_submissions,
            );
        } else {
            service.roots.clear();
        }
    }
}

fn route_io_event(service: &mut Service, event: io::Event) {
    match event {
        io::Event::Shutdown { signal: _ } => {
            if let Some(run) = service.admitted {
                service.domain_events.push(domain::Event::Cancel { run });
            } else {
                service.cancel_pending = true;
            }
        }
        io::Event::Stream { owner, up } if Some(owner) == service.input => {
            service.channel.as_mut().expect("framed agent owns its channel").from_below(
                ChannelLowerEvent::Read(up),
                &mut service.channel_events,
                &mut service.channel_below,
            );
        }
        io::Event::Output { owner, up } if Some(owner) == service.output => {
            service.channel.as_mut().expect("framed agent owns its channel").from_below(
                ChannelLowerEvent::Write(up),
                &mut service.channel_events,
                &mut service.channel_below,
            );
        }
        io::Event::Stream { owner, up } if Some(owner) == service.output => match up {
            stream::Up::Failed(fault) => service.channel.as_mut().expect("framed agent owns its channel").from_below(
                ChannelLowerEvent::WriteFailed(fault),
                &mut service.channel_events,
                &mut service.channel_below,
            ),
            stream::Up::Bytes(_) | stream::Up::Room | stream::Up::End => {}
        },
        io::Event::Closed { owner } if Some(owner) == service.input => {
            service.channel.as_mut().expect("framed agent owns its channel").from_below(
                ChannelLowerEvent::Read(stream::Up::End),
                &mut service.channel_events,
                &mut service.channel_below,
            );
        }
        io::Event::Closed { owner } if Some(owner) == service.output || Some(owner) == service.signals => {}
        event @ (io::Event::Listening { .. }
        | io::Event::Accepted { .. }
        | io::Event::Connecting { .. }
        | io::Event::Connected { .. }
        | io::Event::Stream { .. }
        | io::Event::Output { .. }
        | io::Event::Spawned { .. }
        | io::Event::Exited { .. }
        | io::Event::Failed { .. }
        | io::Event::Closed { .. }) => route_owned_io(service, event),
    }
}

fn route_owned_io(service: &mut Service, event: io::Event) {
    let owner = io_event_owner(&event).expect("only signal events lack owner");
    if service.pipe_routes.contains_key(&owner) {
        let closed = match &event {
            io::Event::Closed { .. } => true,
            io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Stream { .. }
            | io::Event::Output { .. }
            | io::Event::Spawned { .. }
            | io::Event::Exited { .. }
            | io::Event::Failed { .. }
            | io::Event::Shutdown { .. } => false,
        };
        service.machine.from_below(
            &service.machine_env,
            machine::BelowEvent::Process(event),
            &mut service.machine_events,
            &mut service.machine_below,
        );
        if closed {
            service.pipe_routes.remove(&owner);
        }
        return;
    }
    let id = Id::<Owner>::from_token(Token::new(owner.raw() & !COMPONENT_OWNER_BIT));
    let route = *service.owners.get(id).expect("IO event has a live component route");
    let closed = match &event {
        io::Event::Closed { .. } => true,
        io::Event::Listening { .. }
        | io::Event::Accepted { .. }
        | io::Event::Connecting { .. }
        | io::Event::Connected { .. }
        | io::Event::Stream { .. }
        | io::Event::Output { .. }
        | io::Event::Spawned { .. }
        | io::Event::Exited { .. }
        | io::Event::Failed { .. }
        | io::Event::Shutdown { .. } => false,
    };
    match route {
        Owner::Llm(original) => {
            let event = remap_io_owner(event, original);
            service.llm.from_below(&service.llm_env, event, &mut service.llm_events, &mut service.llm_below);
        }
        Owner::Machine(original) => {
            if let io::Event::Spawned { pipes, .. } = &event {
                for pipe in pipes {
                    service.pipe_routes.insert(*pipe, original).expect("bounded process pipes");
                }
            }
            let event = remap_io_owner(event, original);
            service.machine.from_below(
                &service.machine_env,
                machine::BelowEvent::Process(event),
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
    }
    if closed {
        service.owners.retire(id);
    }
}

fn io_event_owner(event: &io::Event) -> Option<Token> {
    match event {
        io::Event::Listening { owner, .. }
        | io::Event::Accepted { owner, .. }
        | io::Event::Connecting { owner, .. }
        | io::Event::Connected { owner }
        | io::Event::Stream { owner, .. }
        | io::Event::Output { owner, .. }
        | io::Event::Spawned { owner, .. }
        | io::Event::Exited { owner, .. }
        | io::Event::Failed { owner, .. }
        | io::Event::Closed { owner } => Some(*owner),
        io::Event::Shutdown { .. } => None,
    }
}

fn component_owner(route: Token) -> Token {
    assert!(route.raw() & COMPONENT_OWNER_BIT == 0, "one process cannot exhaust the component owner namespace");
    Token::new(route.raw() | COMPONENT_OWNER_BIT)
}

fn remap_io_owner(event: io::Event, owner: Token) -> io::Event {
    match event {
        io::Event::Listening { listener, addr, .. } => io::Event::Listening { owner, listener, addr },
        io::Event::Accepted { socket, peer, .. } => io::Event::Accepted { owner, socket, peer },
        io::Event::Connecting { socket, .. } => io::Event::Connecting { owner, socket },
        io::Event::Connected { .. } => io::Event::Connected { owner },
        io::Event::Stream { up, .. } => io::Event::Stream { owner, up },
        io::Event::Output { up, .. } => io::Event::Output { owner, up },
        io::Event::Spawned { child, pipes, .. } => io::Event::Spawned { owner, child, pipes },
        io::Event::Exited { exit, .. } => io::Event::Exited { owner, exit },
        io::Event::Failed { error, .. } => io::Event::Failed { owner, error },
        io::Event::Closed { .. } => io::Event::Closed { owner },
        io::Event::Shutdown { .. } => unreachable!("signals have no component owner"),
    }
}

fn component_up(service: &mut Service) {
    let channel_max = channel::max_out(&service.limits.channel);
    if service.channel_events.room() >= channel_max.to_domain
        && service.channel_below.room() >= channel_max.below
        && let Some(channel) = &mut service.channel
    {
        channel.fire(&mut service.channel_events, &mut service.channel_below);
    }
    let llm_due = match service.llm.next_deadline() {
        Some(at) => at <= service.llm_env.now,
        None => false,
    };
    if (service.llm.has_work() || llm_due)
        && service.llm_events.room() >= llm::MAX_OUT.above
        && service.llm_below.room() >= llm::MAX_OUT.below
    {
        service.llm.fire(&service.llm_env, &mut service.llm_events, &mut service.llm_below);
    }
    let machine_max = machine::max_out(&service.limits.machine);
    let machine_due = match service.machine.next_deadline() {
        Some(at) => at <= service.machine_env.now,
        None => false,
    };
    if machine_due
        && service.machine_events.room() >= machine_max.to_domain
        && service.machine_below.room() >= machine_max.below
    {
        service.machine.fire(&service.machine_env, &mut service.machine_events, &mut service.machine_below);
    }
    for _ in 0..service.channel_events.capacity() {
        if service.domain_events.room() == 0 || (service.admitted.is_none() && service.pending_messages.room() == 0) {
            break;
        }
        let event = match service.channel_events.pop() {
            Some(event) => event,
            None => break,
        };
        channel_event(service, event);
    }
    for _ in 0..service.llm_events.capacity() {
        if service.domain_events.room() == 0
            || service.channel_events.room() < channel::max_out(&service.limits.channel).to_domain
            || service.channel_below.room() < channel::max_out(&service.limits.channel).below
        {
            break;
        }
        let event = match service.llm_events.pop() {
            Some(event) => event,
            None => break,
        };
        match event {
            llm::ToDomain::Completed { owner, completion } => {
                service.domain_events.push(domain::Event::Completed { owner, completion });
            }
            llm::ToDomain::Failed { owner, failure, evidence, detail } => {
                service.domain_events.push(domain::Event::Failed { owner, failure, evidence, detail });
            }
            llm::ToDomain::Cancelled { owner } => service.domain_events.push(domain::Event::Cancelled { owner }),
            llm::ToDomain::Text { owner: _, bytes } => {
                if service.channel.is_none() {
                    continue;
                }
                let token = service.next_send();
                if service
                    .channel
                    .as_mut()
                    .expect("framed agent owns its channel")
                    .send_text_arrived(
                        service.domain_env.now.saturating_since(Time::ZERO),
                        u64::from(bytes),
                        token,
                        &mut service.channel_events,
                        &mut service.channel_below,
                    )
                    .is_err()
                {
                    service.mark_failed(Failure::Send);
                }
            }
        }
    }
    for _ in 0..service.machine_events.capacity() {
        if service.domain_events.room() == 0 {
            break;
        }
        let event = match service.machine_events.pop() {
            Some(event) => event,
            None => break,
        };
        match event {
            machine::ToDomain::Done { owner, done } => service.domain_events.push(domain::Event::Done { owner, done }),
            machine::ToDomain::Read { owner, read } => service.domain_events.push(domain::Event::Read { owner, read }),
            machine::ToDomain::Probed { owner, executable } => {
                service.domain_events.push(domain::Event::Probed { owner, executable });
            }
            machine::ToDomain::Checked { owner, ran } => {
                service.domain_events.push(domain::Event::Checked { owner, ran });
            }
            machine::ToDomain::Aborted { owner } => service.domain_events.push(domain::Event::Aborted { owner }),
        }
    }
}

fn channel_event(service: &mut Service, event: channel::OpenEvent) {
    match event {
        channel::OpenEvent::Opened { version: _ } => {}
        channel::OpenEvent::Start { start } => {
            let count = match &start.mounts {
                Some(mounts) => u32::try_from(mounts.len()).unwrap_or(u32::MAX),
                None => 0,
            };
            if count > service.limits.machine.roots {
                service.refuse_start();
                return;
            }
            let empty = match &start.mounts {
                Some(mounts) => mounts.is_empty(),
                None => true,
            };
            service.pending_start = Some(start);
            if empty {
                service.start_ready();
            }
        }
        channel::OpenEvent::Message { name, label, text } => {
            if let Some(run) = service.admitted {
                service.domain_events.push(domain::Event::Message { run, name, label, text });
            } else {
                service.pending_messages.push(PendingMessage { name, label, text });
            }
        }
        channel::OpenEvent::HostReturned { relay, reply } => {
            service.domain_events.push(domain::Event::HostReturned { relay, reply });
        }
        channel::OpenEvent::Delivered { owner, delivery } => {
            service.domain_events.push(domain::Event::Delivered { owner, delivery: *delivery });
        }
        channel::OpenEvent::DeliveryUnsent { owner } => {
            service.domain_events.push(domain::Event::Delivered { owner, delivery: run::Delivery::Stale });
        }
        channel::OpenEvent::Acknowledged { turn } => {
            if let Some(run) = service.admitted {
                service.domain_events.push(domain::Event::Acknowledge { run, turn });
            }
        }
        channel::OpenEvent::Grant { grant } => {
            match service.channel.as_ref().expect("framed agent owns its channel").grant_value(grant.name) {
                Some(value) => match credential(value) {
                    Some(value) => {
                        let lapses = service.domain_env.now.saturating_add(grant.valid);
                        if service.llm.grant(grant.name, value, lapses).is_ok() {
                            service.domain_events.push(domain::Event::Grant { grant });
                        } else {
                            service.mark_failed(Failure::Credential);
                        }
                    }
                    None => service.mark_failed(Failure::Credential),
                },
                None => service.mark_failed(Failure::Credential),
            }
        }
        channel::OpenEvent::Cancel => {
            if let Some(run) = service.admitted {
                service.domain_events.push(domain::Event::Cancel { run });
            } else {
                service.cancel_pending = true;
            }
        }
        channel::OpenEvent::WriteFailed => service.mark_failed(Failure::WriteFailed),
        channel::OpenEvent::Ended { why: _ } => {
            service.channel_ended = true;
            if !service.answer_sent {
                service.mark_failed(Failure::ChannelEnded);
                if let Some(run) = service.admitted {
                    service.domain_events.push(domain::Event::Cancel { run });
                }
            }
        }
    }
}

fn domain_up(service: &mut Service) {
    let maximum = domain::max_out(&service.limits.domain);
    for _ in 0..service.limits.domain.run.conversations {
        if !service.domain.as_ref().expect("framed agent owns its domain").is_ready()
            || service.domain_requests.room() < maximum
        {
            break;
        }
        domain::resume(
            service.domain.as_mut().expect("framed agent owns its domain"),
            &service.domain_env,
            &mut service.domain_requests,
        );
    }
    for _ in 0..service.domain_events.capacity() {
        if service.domain_requests.room() < maximum {
            break;
        }
        let event = match service.domain_events.pop() {
            Some(event) => event,
            None => break,
        };
        domain::step(
            service.domain.as_mut().expect("framed agent owns its domain"),
            &service.domain_env,
            event,
            &mut service.domain_requests,
        );
    }
    if service.domain.as_ref().expect("framed agent owns its domain").is_due(service.domain_env.now)
        && service.domain_requests.room() >= maximum
    {
        domain::fire(
            service.domain.as_mut().expect("framed agent owns its domain"),
            &service.domain_env,
            &mut service.domain_requests,
        );
    }
}

fn domain_down(service: &mut Service) {
    let channel_max = channel::max_out(&service.limits.channel);
    let machine_max = machine::max_out(&service.limits.machine);
    if let Some(run) = service.admitted {
        for _ in 0..service.pending_messages.capacity() {
            if service.domain_events.room() == 0 {
                break;
            }
            let PendingMessage { name, label, text } = match service.pending_messages.pop() {
                Some(message) => message,
                None => break,
            };
            service.domain_events.push(domain::Event::Message { run, name, label, text });
        }
    }
    for _ in 0..service.domain_requests.capacity() {
        if service.channel_events.room() < channel_max.to_domain
            || service.channel_below.room() < channel_max.below
            || service.llm_events.room() < llm::MAX_OUT.above
            || service.llm_below.room() < llm::MAX_OUT.below
            || service.machine_events.room() < machine_max.to_domain
            || service.machine_below.room() < machine_max.below
        {
            break;
        }
        let request = match service.domain_requests.pop() {
            Some(request) => request,
            None => break,
        };
        domain_request(service, request);
    }
    match service.admitted {
        Some(run) if service.cancel_pending && service.domain_events.room() > 0 => {
            service.domain_events.push(domain::Event::Cancel { run });
            service.cancel_pending = false;
        }
        Some(_) | None => {}
    }
    if service.admitted.is_some() && !service.answer_sent && !service.channel_ended {
        for _ in 0..service.limits.queue {
            if service.channel_events.room() < channel_max.to_domain || service.channel_below.room() < channel_max.below
            {
                break;
            }
            let fact = match service.domain.as_mut().expect("framed agent owns its domain").pop_fact() {
                Some(fact) => fact,
                None => break,
            };
            if service.trace_facts.room() > 0 {
                service.trace_facts.push(fact.clone());
            } else {
                service.lost_trace_facts = service.lost_trace_facts.saturating_add(1);
            }
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_fact(&fact, service.run_started, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
    }
}

#[expect(clippy::too_many_lines, reason = "one exhaustive root-domain request translation")]
fn domain_request(service: &mut Service, request: domain::Request) {
    match request {
        domain::Request::Waiting { host_run: _, read } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_waiting(read, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Turn { host_run: _, number, position: _, read, spent, turn } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_turn(number, read, spent, &turn, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::HostCall { host_run: _, relay, name, tool, effect, input, deadline } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_host_call(
                    service.domain_env.now,
                    name,
                    relay,
                    tool,
                    effect,
                    input,
                    deadline,
                    token,
                    &mut service.channel_events,
                    &mut service.channel_below,
                )
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::WithdrawHost { relay } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_withdraw(relay, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::MessageRefused { host_run: _, name, reason } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_message_refused(name, reason, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Admitted { host_run: _, run } => {
            service.admitted = Some(run);
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_admitted(token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Answer { to, answer, read } => {
            assert!(to.into_token() == Token::new(1), "one Start reply right per process");
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_answer(answer, read, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            } else {
                service.answer_sent = true;
            }
        }
        domain::Request::Checking { host_run: _, deadline } => {
            let token = service.next_send();
            let span = deadline.saturating_since(service.domain_env.now);
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_long(span, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::ChecksEnded { host_run: _ } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_long_done(token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Deliver { name, deadline, host_run: _, owner, change } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_delivery(
                    service.domain_env.now,
                    name,
                    owner,
                    change,
                    deadline,
                    token,
                    &mut service.channel_events,
                    &mut service.channel_below,
                )
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Complete {
            owner,
            grant,
            prompt,
            timeout,
            max_completion_bytes,
            max_completion_blocks,
            max_failure_bytes,
            decoded_call_bytes,
        } => {
            if service.capture_prompts {
                if service.trace_prompt.is_some() {
                    service.lost_trace_prompts = service.lost_trace_prompts.saturating_add(1);
                } else {
                    service.trace_prompt = match capture_prompt(&prompt) {
                        Some(captured) => Some((owner, captured)),
                        None => {
                            service.lost_trace_prompts = service.lost_trace_prompts.saturating_add(1);
                            None
                        }
                    };
                }
            }
            let outcome = service.outcome.clone().expect("an admitted Start retains its result contract");
            let request = llm::FromDomain::Complete {
                owner,
                grant,
                prompt,
                timeout,
                bounds: llm::Receiving {
                    max_completion_bytes,
                    max_completion_blocks,
                    max_failure_bytes,
                    decoded_call_bytes,
                },
                outcome,
                deliver: service.delivery.clone(),
            };
            service.llm.from_domain(&service.llm_env, request, &mut service.llm_events, &mut service.llm_below);
        }
        domain::Request::Rejected { grant } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_rejected(grant, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Exhausted { account, retry_after } => {
            let token = service.next_send();
            if service
                .channel
                .as_mut()
                .expect("framed agent owns its channel")
                .send_exhausted(account, retry_after, token, &mut service.channel_events, &mut service.channel_below)
                .is_err()
            {
                service.mark_failed(Failure::Send);
            }
        }
        domain::Request::Cancel { owner } => {
            service.llm.from_domain(
                &service.llm_env,
                llm::FromDomain::Cancel { owner },
                &mut service.llm_events,
                &mut service.llm_below,
            );
        }
        domain::Request::Io { owner, op, deadline } => {
            service.machine.from_domain(
                &service.machine_env,
                machine::FromDomain::Op { owner, op, deadline },
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
        domain::Request::CancelIo { owner } => {
            service.machine.from_domain(
                &service.machine_env,
                machine::FromDomain::Cancel { owner },
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
        domain::Request::Read { owner, at, max, deadline } => {
            service.machine.from_domain(
                &service.machine_env,
                machine::FromDomain::Read { owner, at, max, deadline },
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
        domain::Request::Probe { owner, at, deadline } => {
            service.machine.from_domain(
                &service.machine_env,
                machine::FromDomain::Probe { owner, at, deadline },
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
        domain::Request::Check { owner, program, deadline, tail } => {
            service.machine.from_domain(
                &service.machine_env,
                machine::FromDomain::Check { owner, program, deadline, tail },
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
        domain::Request::Abort { owner } => {
            service.machine.from_domain(
                &service.machine_env,
                machine::FromDomain::Abort { owner },
                &mut service.machine_events,
                &mut service.machine_below,
            );
        }
    }
}

#[expect(clippy::too_many_lines, reason = "bounded lower routing stays in one down-pass stage")]
fn component_down(service: &mut Service) {
    for _ in 0..service.channel_below.capacity() {
        if service.io_requests.room() == 0 {
            break;
        }
        let request = match service.channel_below.pop() {
            Some(request) => request,
            None => break,
        };
        match request {
            ChannelLower::Read(down) => {
                let input = service.input.expect("the shell adopts the read pipe before iterating");
                service.io_requests.push(io::Request::Stream { stream: input, down });
            }
            ChannelLower::Write(down) => {
                let output = service.output.expect("the shell adopts the write pipe before iterating");
                service.io_requests.push(io::Request::Output { stream: output, down });
            }
            ChannelLower::FinishWrite => {
                let output = service.output.expect("the shell adopts the write pipe before iterating");
                service.io_requests.push(io::Request::Stream { stream: output, down: stream::Down::Finish });
            }
        }
    }
    for _ in 0..service.llm_below.capacity() {
        if service.io_requests.room() == 0 {
            break;
        }
        let request = match service.llm_below.pop() {
            Some(request) => request,
            None => break,
        };
        match request {
            io::Request::Connect { owner, addr } => {
                let route = service.owners.insert(Owner::Llm(owner)).expect("LLM connection route capacity");
                service.io_requests.push(io::Request::Connect { owner: component_owner(route.token()), addr });
            }
            other @ (io::Request::Listen { .. }
            | io::Request::Bind { .. }
            | io::Request::Reject { .. }
            | io::Request::Stream { .. }
            | io::Request::Output { .. }
            | io::Request::Spawn { .. }
            | io::Request::Signal { .. }
            | io::Request::Close { .. }
            | io::Request::Abort { .. }) => service.io_requests.push(other),
        }
    }
    for _ in 0..service.machine_below.capacity() {
        if service.io_requests.room() == 0 || service.file_submissions.room() < 2 || service.file_events.room() == 0 {
            break;
        }
        let waits_for_file = match service.machine_below.iter().next() {
            Some(machine::Below::File { .. } | machine::Below::OpenRead { .. }) => !service.files.takes(),
            Some(machine::Below::CancelFile { .. } | machine::Below::Spawn { .. } | machine::Below::Process(_))
            | None => false,
        };
        if waits_for_file {
            break;
        }
        let request = match service.machine_below.pop() {
            Some(request) => request,
            None => break,
        };
        match request {
            machine::Below::File { request, deadline } => {
                service.files.seed_randomness(service.next_operation);
                file_layer::down_until(
                    &mut service.files,
                    deadline,
                    request,
                    &mut service.file_events,
                    &mut service.file_submissions,
                );
            }
            machine::Below::CancelFile { owner } => {
                file_layer::cancel(&mut service.files, owner, &mut service.file_submissions);
            }
            machine::Below::OpenRead { owner, root, path, deadline } => {
                let descriptor = service.files.descriptor(root).expect("machine admitted an installed root");
                file_layer::down_until(
                    &mut service.files,
                    deadline,
                    file::Request::OpenRead { owner, root: descriptor, name: path },
                    &mut service.file_events,
                    &mut service.file_submissions,
                );
            }
            machine::Below::Spawn { owner, root, spawn } => {
                let descriptor = service.files.descriptor(root).expect("machine admitted an installed root");
                let spawn = kernel::Spawn {
                    program: spawn.program,
                    args: spawn.args,
                    env: spawn.env,
                    root: descriptor,
                    dir: spawn.dir,
                    pipes: spawn.pipes,
                };
                let route = service.owners.insert(Owner::Machine(owner)).expect("process route capacity");
                service.io_requests.push(io::Request::Spawn { owner: component_owner(route.token()), spawn });
            }
            machine::Below::Process(request) => service.io_requests.push(request),
        }
    }
}

fn io_down(service: &mut Service) {
    for _ in 0..service.io_requests.capacity() {
        if !service.io.takes() || service.io_submissions.room() < io::MAX_OUT_DOWN.submissions {
            break;
        }
        let request = match service.io_requests.pop() {
            Some(request) => request,
            None => break,
        };
        io::down(&mut service.io, &service.io_env, request, &mut service.io_submissions);
    }
    for _ in 0..service.io_submissions.capacity() {
        if service.submissions.room() == 0 || service.operations.len() >= service.operations.capacity() {
            break;
        }
        let submit = match service.io_submissions.pop() {
            Some(submit) => submit,
            None => break,
        };
        route_submission(service, submit, false);
    }
    for _ in 0..service.file_submissions.capacity() {
        if service.submissions.room() == 0 || service.operations.len() >= service.operations.capacity() {
            break;
        }
        let submit = match service.file_submissions.pop() {
            Some(submit) => submit,
            None => break,
        };
        route_submission(service, submit, true);
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "the kernel operation is an external vocabulary and only Cancel names another operation"
)]
fn route_submission(service: &mut Service, submit: kernel::Submit, file: bool) {
    let global = Token::new(service.next_operation);
    service.next_operation = service.next_operation.checked_add(1).expect("one run does not exhaust operation names");
    let kind = match submit.kind {
        kernel::Op::Cancel { target } => {
            let mut found = None;
            for (global, route) in &service.operations {
                match route {
                    Operation::File(original) if file && *original == target => found = Some(*global),
                    Operation::Io(original) if !file && *original == target => found = Some(*global),
                    Operation::File(_) | Operation::Io(_) => {}
                }
            }
            kernel::Op::Cancel { target: found.unwrap_or(Token::new(u64::MAX)) }
        }
        other => other,
    };
    let route = if file { Operation::File(submit.op) } else { Operation::Io(submit.op) };
    service.operations.insert(global, route).expect("reserved operation route");
    service.submissions.push(kernel::Submit { op: global, kind });
}

/// Earliest active deadline over all child machines.
#[must_use]
pub fn next_deadline(service: &Service) -> Option<Time> {
    let mut earliest = match &service.domain {
        Some(domain) => domain.next_deadline(),
        None => None,
    };
    for deadline in [
        service.llm.next_deadline(),
        service.machine.next_deadline(),
        service.io.next_deadline(),
        service.files.next_deadline(),
    ] {
        earliest = match deadline {
            Some(next) => match earliest {
                Some(old) => Some(old.min(next)),
                None => Some(next),
            },
            None => earliest,
        };
    }
    earliest
}

/// Whether another iteration can progress without waiting on the kernel.
#[must_use]
pub fn work_pending(service: &Service, now: Time) -> bool {
    let due = match next_deadline(service) {
        Some(deadline) => deadline <= now,
        None => false,
    };
    due || service.io.is_ready()
        || service.llm.has_work()
        || match &service.domain {
            Some(domain) => domain.is_ready(),
            None => false,
        }
        || !service.completions.is_empty()
        || !service.io_events.is_empty()
        || !service.file_events.is_empty()
        || !service.channel_events.is_empty()
        || (!service.pending_messages.is_empty() && service.admitted.is_some())
        || !service.llm_events.is_empty()
        || !service.machine_events.is_empty()
        || !service.domain_events.is_empty()
        || !service.domain_requests.is_empty()
        || !service.channel_below.is_empty()
        || !service.llm_below.is_empty()
        || !service.machine_below.is_empty()
        || !service.io_requests.is_empty()
        || !service.io_submissions.is_empty()
        || !service.file_submissions.is_empty()
        || (service.channel_ended
            && !service.roots.is_empty()
            && service.owners.is_empty()
            && service.pipe_routes.is_empty()
            && service.files.takes()
            && !service.root_close_pending)
}

/// The channel has ended, every lower child has settled, and the final
/// answer crossed the channel.
#[must_use]
pub fn done(service: &Service) -> Option<bool> {
    if service.channel_ended
        && service.channel_events.is_empty()
        && service.channel_below.is_empty()
        && service.llm_events.is_empty()
        && service.llm_below.is_empty()
        && service.machine_events.is_empty()
        && service.machine_below.is_empty()
        && service.domain_events.is_empty()
        && service.domain_requests.is_empty()
        && service.io_events.is_empty()
        && service.io_requests.is_empty()
        && service.file_events.is_empty()
        && service.io_submissions.is_empty()
        && service.file_submissions.is_empty()
        && service.completions.is_empty()
        && service.io.is_empty()
        && service.files.takes()
        && service.files.open_files() == 0
        && service.roots.is_empty()
        && service.owners.is_empty()
        && service.pipe_routes.is_empty()
        && service.operations.is_empty()
        && service.submissions.is_empty()
    {
        Some(service.answer_sent && !service.failed)
    } else {
        None
    }
}

/// The first concrete failure, retained for the shell's final stderr line.
#[must_use]
pub const fn failure(service: &Service) -> Option<Failure> {
    service.failure
}

#[cfg(test)]
#[expect(clippy::wildcard_enum_match_arm, reason = "test assertions describe unexpected events")]
mod tests {
    use super::*;
    use skein_lib::Duration;

    fn limits() -> Limits {
        let client = skein_llm_world::limits();
        let decoded_call_bytes = 4096;
        let channel_bodies = smith_channel::CEILINGS;
        let schema = smith_channel::schema(&channel_bodies).expect("bounded channel schema");
        let version = schema.version(2).expect("v2");
        let mut largest = 0;
        for kind in &version.kinds {
            largest = largest.max(kind.largest);
        }
        let llm_io = io::Limits {
            sockets: 3,
            refusals: 1,
            intake: 19_000,
            receive: 1024,
            output: 19_000,
            sends: 2,
            accepts: 1,
            backlog: 2,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(10),
        };
        let io = io::Limits { sockets: 16, ..llm_io };
        Limits {
            domain: smith_agent_world::LIMITS,
            channel: channel::Limits {
                bodies: channel_bodies,
                charter: smith_charter::CEILINGS,
                transcript: smith_transcript::CEILINGS,
                channel: skein_channel::Limits {
                    chunk: 4096,
                    credential: 0,
                    skip: 4096,
                    output_bytes: largest.checked_add(8).expect("largest frame"),
                    output_frames: 4,
                    kinds: 18,
                },
                endpoints: 1,
                calls: 8,
                turns: 8,
                fact_reserve_frames: 1,
                fact_reserve_bytes: 128,
                grants: 8,
            },
            llm: llm::ComponentLimits {
                adapter: llm::Limits {
                    client,
                    tool_bytes: 32_768,
                    rendered_result: client.dialect.string_bytes,
                    shell_default: skein_lib::Duration::from_secs(120),
                    shell_maximum: skein_lib::Duration::from_secs(1200),
                },
                connection: skein_llm_connection::Limits {
                    endpoints: 1,
                    connections: 2,
                    calls: 2,
                    per_endpoint: 2,
                    idle_keep: Duration::from_secs(1),
                    io: llm_io,
                    tls: skein_tls::client::Limits { read: 4096, send: 4096, records: skein_tls::client::MAX_RECORD },
                },
                receiving: llm::Receiving {
                    max_completion_bytes: llm::completion_worst_case(&client, decoded_call_bytes)
                        .expect("receiving bound"),
                    max_completion_blocks: client.dialect.parts,
                    decoded_call_bytes,
                    max_failure_bytes: client.dialect.detail_bytes,
                },
                contract_bytes: 4096,
                accounts: 1,
                grant_value_bytes: 128,
                connect: Some(Duration::from_secs(1)),
                handshake: Some(Duration::from_secs(1)),
                head: Some(Duration::from_secs(2)),
                idle: Some(Duration::from_secs(3)),
            },
            machine: machine::Limits {
                operations: 2,
                roots: 1,
                path_bytes: 64,
                file_bytes: 16,
                entries: 2,
                entry_bytes: 512,
                processes: 2,
                output_bytes: 64,
                search_hits: 8,
                search_bytes: 256,
                search_line_bytes: 512,
                env_bytes: 512,
                stop_grace: Duration::from_millis(10),
            },
            io,
            file_slots: 4,
            file_read: 64,
            file_entries: 2,
            file_bytes: 512,
            file_timeout: Duration::from_secs(1),
            queue: domain::max_out(&smith_agent_world::LIMITS).max(256),
            routes: 66,
            memory: u64::MAX,
        }
    }

    fn configuration() -> Config {
        let limits = limits();
        let channel_endpoints = channel::Endpoints::new(List::with_capacity(1));
        let llm_endpoints = llm::Endpoints::new(Box::new([]), 1, 1).expect("empty endpoint table");
        Config {
            limits,
            domain: domain::Config { endpoints: Box::new([]) },
            channel_endpoints,
            llm_endpoints,
            environment: Box::new([]),
            stream_mode: StreamMode::Two,
            capture_prompts: false,
        }
    }

    fn service() -> Service {
        Service::new(configuration(), 1).expect("bounded service")
    }

    #[test]
    fn a_colocated_host_gets_typed_machine_terminals_without_an_agent_channel() {
        let mut effects = Effects::new(configuration(), 1).expect("bounded effects");
        effects.request(domain::Request::Probe {
            owner: Token::new(17),
            at: run::Place { root: Token::new(19), path: Box::from(b"checks".as_slice()) },
            deadline: Time::from_nanos(50),
        });
        effects.iterate(Time::ZERO, Wall::EPOCH);
        effects.iterate(Time::ZERO, Wall::EPOCH);
        let Some(domain::Event::Probed { owner, executable: false }) = effects.event() else {
            panic!("typed machine terminal")
        };
        assert_eq!(owner, Token::new(17));
        assert!(effects.event().is_none());
        assert!(effects.submissions().is_empty());
        let grant =
            domain::Grant { name: domain::GrantName { account: 0, generation: 1 }, valid: Duration::from_secs(60) };
        effects.grant(grant, b"\0\x03acctoken", Time::ZERO).expect("first grant");
        effects
            .grant(grant, b"\0\x03acctoken", Time::from_nanos(1))
            .expect("same saved generation on another activation");
        effects.close();
        effects.iterate(Time::ZERO, Wall::EPOCH);
        assert!(effects.closed());
    }

    #[test]
    fn startup_counts_file_driver_and_rejects_missing_route_room() {
        let mut limits = limits();
        let llm_endpoints = llm::Endpoints::new(Box::new([]), 1, 1).expect("empty endpoint table");
        assert!(
            worst_case(&limits, &llm_endpoints).expect("checked memory")
                > file_layer::FileIo::worst_case(4, 64, 2, 512).expect("file bound")
        );
        limits.memory = worst_case(&limits, &llm_endpoints).expect("bound") - 1;
        let channel_endpoints = channel::Endpoints::new(List::with_capacity(1));
        let result = Service::new(
            Config {
                limits,
                domain: domain::Config { endpoints: Box::new([]) },
                channel_endpoints,
                llm_endpoints,
                environment: Box::new([]),
                stream_mode: StreamMode::Two,
                capture_prompts: false,
            },
            1,
        );
        let error = result.expect_err("startup refusal");
        assert_eq!(core::mem::discriminant(&error), core::mem::discriminant(&ConfigError::MemoryCeiling));
    }

    #[test]
    fn grant_envelope_splits_account_and_bearer_without_leaking_the_header() {
        let value = credential(&[0, 3, b'a', b'c', b'c', b't', b'o', b'k']).expect("grant");
        assert_eq!(&*value.account_id, b"acc");
        assert_eq!(&*value.access_token, b"tok");
        assert!(credential(&[0, 2, b'a']).is_none());
        assert!(credential(&[0, 0]).is_none());
    }

    #[test]
    fn io_routes_preserve_component_owner_and_retire_on_close() {
        let mut service = service();
        let route = service.owners.insert(Owner::Llm(Token::new(42))).expect("route");
        route_owned_io(&mut service, io::Event::Closed { owner: component_owner(route.token()) });
        assert_eq!(service.owners.len(), 1);
        service.owners.reclaim();
        assert!(service.owners.is_empty());
    }

    #[test]
    fn machine_terminal_and_channel_message_keep_their_domain_names() {
        let mut service = service();
        let owner = Token::new(42);
        service.machine_events.push(machine::ToDomain::Probed { owner, executable: true });
        component_up(&mut service);
        match service.domain_events.pop().expect("terminal") {
            domain::Event::Probed { owner: observed, executable } => {
                assert_eq!(observed, owner);
                assert!(executable);
            }
            other => panic!("unexpected terminal: {other:?}"),
        }
        channel_event(
            &mut service,
            channel::OpenEvent::Message { name: owner, label: Box::from(&b"host"[..]), text: Box::from(&b"hello"[..]) },
        );
        assert_eq!(service.pending_messages.len(), 1);
        assert!(service.domain_events.is_empty());
        service.admitted = Some(Token::new(9));
        domain_down(&mut service);
        match service.domain_events.pop().expect("message") {
            domain::Event::Message { run, name, label, text } => {
                assert_eq!(label.as_ref(), b"host");
                assert_eq!(run, Token::new(9));
                assert_eq!(name, owner);
                assert_eq!(&*text, b"hello");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn llm_and_host_terminals_reach_the_domain_with_their_owners() {
        let mut service = service();
        let owner = Token::new(73);
        service.llm_events.push(llm::ToDomain::Cancelled { owner });
        component_up(&mut service);
        match service.domain_events.pop().expect("LLM terminal") {
            domain::Event::Cancelled { owner: observed } => assert_eq!(observed, owner),
            other => panic!("unexpected LLM terminal: {other:?}"),
        }
        let relay = run::RelayName { owner: Token::new(5), attempt: 1 };
        channel_event(
            &mut service,
            channel::OpenEvent::HostReturned { relay, reply: run::HostReply::Unanswered(run::Unanswered::Lost) },
        );
        match service.domain_events.pop().expect("host terminal") {
            domain::Event::HostReturned { relay: observed, reply } => {
                assert_eq!(observed, relay);
                assert_eq!(reply, run::HostReply::Unanswered(run::Unanswered::Lost));
            }
            other => panic!("unexpected host terminal: {other:?}"),
        }
    }

    #[test]
    fn kernel_cancel_targets_the_right_layer_operation() {
        let mut service = service();
        route_submission(
            &mut service,
            kernel::Submit { op: Token::new(11), kind: kernel::Op::Close { fd: kernel::Fd::new(3) } },
            false,
        );
        route_submission(
            &mut service,
            kernel::Submit { op: Token::new(11), kind: kernel::Op::Close { fd: kernel::Fd::new(4) } },
            true,
        );
        route_submission(
            &mut service,
            kernel::Submit { op: Token::new(12), kind: kernel::Op::Cancel { target: Token::new(11) } },
            true,
        );
        let io = service.submissions.pop().expect("io close");
        let file = service.submissions.pop().expect("file close");
        let cancel = service.submissions.pop().expect("file cancel");
        assert_ne!(io.op, file.op);
        assert_eq!(cancel.kind, kernel::Op::Cancel { target: file.op });
    }

    #[test]
    fn termination_signal_cancels_the_admitted_run() {
        let mut service = service();
        service.admitted = Some(Token::new(91));
        route_io_event(&mut service, io::Event::Shutdown { signal: kernel::ServiceSignal::Terminate });
        match service.domain_events.pop().expect("cancel event") {
            domain::Event::Cancel { run } => assert_eq!(run, Token::new(91)),
            other => panic!("unexpected signal result: {other:?}"),
        }
    }

    #[test]
    fn signal_during_opening_waits_for_the_admitted_run_handle() {
        let mut service = service();
        route_io_event(&mut service, io::Event::Shutdown { signal: kernel::ServiceSignal::Interrupt });
        assert!(service.cancel_pending);
        service.admitted = Some(Token::new(19));
        domain_down(&mut service);
        match service.domain_events.pop().expect("deferred cancel") {
            domain::Event::Cancel { run } => assert_eq!(run, Token::new(19)),
            other => panic!("unexpected deferred result: {other:?}"),
        }
    }

    #[test]
    fn channel_loss_before_answer_keeps_a_concrete_failure_reason() {
        let mut service = service();
        channel_event(&mut service, channel::OpenEvent::Ended { why: skein_channel::Closed::Truncated });
        assert_eq!(failure(&service), Some(Failure::ChannelEnded));
        assert!(service.channel_ended);
        assert!(!service.answer_sent);
    }

    #[test]
    fn success_requires_a_sent_answer_and_every_lower_route_settled() {
        let mut service = service();
        service.channel_ended = true;
        service.answer_sent = true;
        assert_eq!(done(&service), Some(true));
        service.operations.insert(Token::new(77), Operation::Io(Token::new(4))).expect("one route");
        assert_eq!(done(&service), None);
        service.operations.remove(&Token::new(77));
        assert_eq!(done(&service), Some(true));
    }

    #[test]
    fn typed_prompt_capture_preserves_text_and_tool_input_without_a_credential() {
        let prompt = domain::llm::Prompt {
            endpoint: domain::llm::Endpoint(1),
            model: Box::from(&b"model"[..]),
            system: Box::from(&b"system instructions"[..]),
            tools: tools::Grants { inspect: false, modify: false, shell: false },
            served: Box::new([]),
            messages: Box::new([domain::llm::Message {
                role: domain::llm::Role::User,
                content: Box::new([
                    domain::llm::Block::Text { text: Box::from(&b"request"[..]), replay: None },
                    domain::llm::Block::ToolCall {
                        id: Box::from(&b"id"[..]),
                        name: Box::from(&b"inspect"[..]),
                        input: Box::from(&b"input"[..]),
                        replay: None,
                    },
                ]),
            }]),
            max_tokens: 10,
        };
        let captured = capture_prompt(&prompt).expect("bounded prompt");
        assert_eq!(&*captured, b"model:model\nsystem:system instructions\nuser:\ntext:request\ncall:id:inspect:input");
    }
}
