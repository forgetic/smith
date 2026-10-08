//! Bounded local-to-host domain routing. Local policy owns durability and
//! delivery; the host kit owns child lifecycle and channel rights.

use alloc::boxed::Box;
use core::mem::size_of;
use skein_lib::{Env, Map, Queue, Time, Token, Wall};
use smith_domain as agent;
use smith_host_domain as host;
use smith_local_domain as local;
use smith_local_protocol as protocol;
use smith_protocol_channel as channel;

/// Immutable bounds for one local chat and its host process slot.
#[derive(Clone, Debug)]
pub struct Limits {
    pub local: local::Limits,
    pub host: host::Limits,
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
}

/// Startup cannot allocate or represent the requested local composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Local(local::Invalid),
    Queue,
    Memory,
}

/// Checked memory bound for both domains and their routing queues.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    local::worst_case(&limits.local)?
        .checked_add(host::worst_case(&limits.host)?)?
        .checked_add(Queue::<local::Event>::worst_case(limits.queue)?)?
        .checked_add(Queue::<local::Request>::worst_case(limits.queue)?.checked_mul(2)?)?
        .checked_add(Queue::<host::Event>::worst_case(limits.queue)?)?
        .checked_add(Queue::<host::Request>::worst_case(limits.queue)?.checked_mul(2)?)?
        .checked_add(Map::<u32, Box<[u8]>>::worst_case(limits.local.agent.accounts)?)?
        .checked_add(u64::from(limits.local.agent.accounts).checked_mul(limits.host.answer_bytes)?)?
        .checked_add(u64::try_from(size_of::<Service>()).ok()?)
}

/// One local chat, its host kit and bounded requests to process and shell.
#[derive(Debug)]
pub struct Service {
    limits: Limits,
    local: local::Domain,
    host: host::Domain,
    local_events: Queue<local::Event>,
    local_requests: Queue<local::Request>,
    host_events: Queue<host::Event>,
    host_requests: Queue<host::Request>,
    shell: Queue<local::Request>,
    lower: Queue<host::Request>,
    values: Map<u32, Box<[u8]>>,
    paths: Box<[Box<[u8]>]>,
    start_values: Option<StartValues>,
    charter: Box<[u8]>,
    endpoints: channel::Endpoints,
    host_agent: Option<Token>,
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
        let required = local::max_out(&config.limits.local).max(host::max_out(&config.limits.host));
        if config.limits.queue < required {
            return Err(Error::Queue);
        }
        if worst_case(&config.limits).is_none() {
            return Err(Error::Memory);
        }
        let local = match local::Domain::new_external(config.local, &config.limits.local, seed) {
            Ok(local) => local,
            Err(error) => return Err(Error::Local(error)),
        };
        let host = host::Domain::new(&config.limits.host);
        let queue = config.limits.queue;
        let accounts = config.limits.local.agent.accounts;
        Ok(Self {
            limits: config.limits,
            local,
            host,
            local_events: Queue::with_capacity(queue),
            local_requests: Queue::with_capacity(queue),
            host_events: Queue::with_capacity(queue),
            host_requests: Queue::with_capacity(queue),
            shell: Queue::with_capacity(queue),
            lower: Queue::with_capacity(queue),
            values: Map::with_capacity(accounts),
            paths: config.paths,
            start_values: None,
            charter: config.charter,
            endpoints: config.endpoints,
            host_agent: None,
            failed: false,
        })
    }

    /// Queue a terminal, store, git or OAuth outcome from the shell.
    pub fn local_event(&mut self, event: local::Event) {
        self.local_events.push(event);
    }

    /// Lend a credential value only beside the domain's grant name.
    pub fn credential(&mut self, grant: agent::Grant, envelope: Box<[u8]>) {
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
        match (self.local.next_deadline(), self.host.next_deadline()) {
            (Some(local), Some(host)) => Some(local.min(host)),
            (Some(local), None) => Some(local),
            (None, Some(host)) => Some(host),
            (None, None) => None,
        }
    }

    #[must_use]
    pub fn work_pending(&self, now: Time) -> bool {
        self.local.is_ready()
            || self.local.is_due(now)
            || self.host.is_due(now)
            || !self.local_events.is_empty()
            || !self.local_requests.is_empty()
            || !self.host_events.is_empty()
            || !self.host_requests.is_empty()
    }

    #[must_use]
    pub const fn failed(&self) -> bool {
        self.failed
    }

    fn route_local(&mut self, request: local::Request) {
        match request {
            local::Request::External(external) => self.route_external(*external),
            local::Request::Agent(_) => unreachable!("spawned local service has no in-process agent IO"),
            other @ (local::Request::Show { .. }
            | local::Request::Load
            | local::Request::SaveState { .. }
            | local::Request::SaveTurn { .. }
            | local::Request::SaveDelivery { .. }
            | local::Request::Git { .. }
            | local::Request::PlainStatus { .. }
            | local::Request::Credential { .. }
            | local::Request::Exit { .. }) => self.shell.push(other),
        }
    }

    fn route_external(&mut self, request: local::ExternalRequest) {
        match request {
            local::ExternalRequest::Start(start) => {
                let mut credentials =
                    skein_lib::List::with_capacity(u32::try_from(start.grants.len()).expect("bounded grants"));
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

    fn route_host(&mut self, request: host::Request) {
        match request {
            host::Request::Started { agent, .. } => self.host_agent = Some(agent),
            host::Request::Admitted { .. } => {
                let run = self.host_agent.expect("Started precedes Admitted");
                self.local_events.push(local::Event::External(local::ExternalEvent::Admitted { run }));
            }
            host::Request::Turn { turn, .. } => {
                match channel::decode_turn(&turn.body, turn.number, &smith_transcript::CEILINGS, &self.endpoints) {
                    Ok(decoded) => self.local_events.push(local::Event::External(local::ExternalEvent::Turn {
                        number: turn.number,
                        read: turn.read,
                        turn: decoded,
                    })),
                    Err(_) => {
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
                    self.host_events.push(host::Event::Answer { agent, call, reply: host::Reply::Unavailable });
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
    let local_env = Env { now, wall, limits: service.limits.local.clone() };
    let host_env = Env { now, wall, limits: service.limits.host };
    for _ in 0..service.local_events.capacity() {
        let Some(event) = service.local_events.pop() else { break };
        local::step(&mut service.local, &local_env, event, &mut service.local_requests);
    }
    if service.local.is_due(now) {
        local::fire(&mut service.local, &local_env, &mut service.local_requests);
    }
    if service.local.is_ready() {
        local::resume(&mut service.local, &local_env, &mut service.local_requests);
    }
    for _ in 0..service.local_requests.capacity() {
        let Some(request) = service.local_requests.pop() else { break };
        service.route_local(request);
    }
    for _ in 0..service.host_events.capacity() {
        let Some(event) = service.host_events.pop() else { break };
        host::step(&mut service.host, &host_env, event, &mut service.host_requests);
    }
    if service.host.is_due(now) {
        host::fire(&mut service.host, &host_env, &mut service.host_requests);
    }
    for _ in 0..service.host_requests.capacity() {
        let Some(request) = service.host_requests.pop() else { break };
        service.route_host(request);
    }
    service.local.reclaim();
    service.host.reclaim();
}
