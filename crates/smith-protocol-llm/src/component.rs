//! A bounded LLM component over skein's connection pool.
//! It keeps endpoint names, two grant generations per account, and a context
//! for each accepted call. `close` settles the shared pool at owner shutdown.
//! It never knows retry policy, host credentials, or
//! provider wire grammar. `from_domain`, `from_below`, `fire` and `reclaim`
//! route one bounded pass; accepted calls have one typed terminal.
//! Contract: protocol/llm.md, sections 1, 6 to 10.

#![expect(clippy::manual_let_else, clippy::single_match, reason = "explicit bounded routing cases")]

use alloc::boxed::Box;
use core::fmt;

use skein_io::{Event as IoEvent, Request as IoRequest};
use skein_lib::{Duration, Env, List, Map, Queue, Set, Time, Token, bytes};
use skein_llm::{self as shared, Credential, Error};
use skein_llm_connection as connection;
use smith_domain::{Event as DomainEvent, GrantName, llm, run};

use crate::IdentityProfile;
use crate::boundary::{FromDomain, ToDomain};
use crate::{Context, EndpointOptions, Endpoints, GrantError, Grants, Receiving};

/// Startup bounds and phase deadlines for the LLM component.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ComponentLimits {
    /// Translation and receiving bounds.
    pub adapter: crate::Limits,
    /// The connection pool and child-machine bounds.
    pub connection: connection::Limits,
    /// Largest Complete receiving contract admitted by this service.
    pub receiving: Receiving,
    /// Maximum retained charter result and delivery rule bytes per call.
    pub contract_bytes: u64,
    /// Maximum distinct credential accounts.
    pub accounts: u32,
    /// Maximum bearer and provider account-id bytes in one grant.
    pub grant_value_bytes: u32,
    /// Connecting deadline before a socket exists.
    pub connect: Option<Duration>,
    /// TLS handshake deadline.
    pub handshake: Option<Duration>,
    /// Response-head deadline.
    pub head: Option<Duration>,
    /// Inactivity deadline between response events.
    pub idle: Option<Duration>,
}

/// A startup configuration problem.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ComponentError {
    /// The adapter and connection child disagree on the client bounds.
    ClientLimits,
    /// A finite bound or worst case is invalid.
    Limits,
    /// The grant table could not be allocated at its bound.
    Grants(GrantError),
    /// The resolved endpoint list cannot initialize the connection pool.
    Connection(connection::EndpointError),
}

struct Active {
    context: Context,
    outcome: run::outcome::OutcomeSpec,
    deliver: Option<run::outcome::ChangeSpec>,
}

/// Named endpoints, grants and the connection child for active calls.
pub struct Component {
    limits: ComponentLimits,
    options: Map<u32, EndpointOptions>,
    grants: Grants,
    active: Map<Token, Active>,
    next: Set<Token>,
    connection: connection::Component,
    events: Queue<connection::Event>,
}

impl fmt::Debug for Component {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Component")
            .field("active", &self.active.len())
            .field("grants", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// Output room required before one component entrance.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MaxOut {
    /// Domain events from one entrance.
    pub above: u32,
    /// Io requests from one entrance.
    pub below: u32,
}

/// A fire pass may issue one pending Next and advance one child phase.
pub const MAX_OUT: MaxOut = MaxOut { above: 128, below: 128 };

/// Checked retained pool, grant, context, and event-queue ownership.
#[must_use]
pub fn component_worst_case(limits: &ComponentLimits) -> Option<u64> {
    if limits.accounts == 0 || limits.connection.connections == 0 {
        return None;
    }
    let per_call = crate::worst_case(&limits.adapter, &limits.receiving)?;
    let base = connection::worst_case(&limits.connection)?
        .checked_add(Grants::worst_case(limits.accounts, limits.grant_value_bytes)?)?
        .checked_add(Map::<u32, EndpointOptions>::worst_case(limits.connection.endpoints)?)?
        .checked_add(Map::<Token, Active>::worst_case(limits.connection.connections)?)?
        .checked_add(Set::<Token>::worst_case(limits.connection.connections)?)?
        .checked_add(Queue::<connection::Event>::worst_case(MAX_OUT.above)?)?;
    base.checked_add(
        per_call.checked_add(limits.contract_bytes)?.checked_mul(u64::from(limits.connection.connections))?,
    )
}

impl Component {
    /// Validate the configured endpoint and pool bounds before the loop.
    pub fn new(limits: &ComponentLimits, endpoints: Endpoints) -> Result<Component, ComponentError> {
        if limits.adapter.client != limits.connection.llm {
            return Err(ComponentError::ClientLimits);
        }
        component_worst_case(limits).ok_or(ComponentError::Limits)?;
        let grants = match Grants::new(limits.accounts, limits.grant_value_bytes) {
            Ok(grants) => grants,
            Err(error) => return Err(ComponentError::Grants(error)),
        };
        let (destinations, options) = endpoints.into_parts();
        let connection = match connection::Component::new(destinations, &limits.connection) {
            Ok(connection) => connection,
            Err(error) => return Err(ComponentError::Connection(error)),
        };
        Ok(Component {
            limits: *limits,
            options,
            grants,
            active: Map::with_capacity(limits.connection.connections),
            next: Set::with_capacity(limits.connection.connections),
            connection,
            events: Queue::with_capacity(MAX_OUT.above),
        })
    }

    /// Install a validated generation from the channel or a colocated host.
    pub fn grant(&mut self, name: GrantName, credential: Credential, lapses: Time) -> Result<(), GrantError> {
        self.grants.grant(name, credential, lapses)
    }

    /// Stops accepting calls and schedules active and idle bindings to close.
    /// The owner keeps routing io answers until their physical settlement.
    pub fn close(&mut self) {
        for _ in 0..self.next.len() {
            let _call = self.next.pop_first().expect("each retained demand is counted");
        }
        self.connection.close();
    }

    /// Route one root-domain request to the connection child.
    pub fn from_domain(
        &mut self,
        env: &Env<ComponentLimits>,
        request: FromDomain,
        to_domain: &mut Queue<ToDomain>,
        io: &mut Queue<IoRequest>,
    ) {
        match request {
            FromDomain::Complete { owner, grant, prompt, timeout, bounds, outcome, deliver } => {
                self.start(env, owner, grant, prompt, timeout, bounds, outcome, deliver, to_domain, io);
            }
            FromDomain::Cancel { owner } => {
                self.next.remove(&owner);
                let child_env = self.child_env(env);
                self.connection.down(&child_env, connection::Request::Cancel { call: owner }, &mut self.events, io);
                self.route(to_domain);
            }
        }
    }

    /// Route one io answer to the connection child.
    pub fn from_below(
        &mut self,
        env: &Env<ComponentLimits>,
        event: IoEvent,
        to_domain: &mut Queue<ToDomain>,
        io: &mut Queue<IoRequest>,
    ) {
        let child_env = self.child_env(env);
        self.connection.up(&child_env, event, &mut self.events, io);
        self.route(to_domain);
    }

    /// Advance one bounded pass of deadlines and buffered connection work.
    pub fn fire(&mut self, env: &Env<ComponentLimits>, to_domain: &mut Queue<ToDomain>, io: &mut Queue<IoRequest>) {
        let child_env = self.child_env(env);
        match self.next.pop_first() {
            Some(call) => self.connection.down(&child_env, connection::Request::Next { call }, &mut self.events, io),
            None => {}
        }
        self.connection.fire(&child_env, &mut self.events, io);
        self.route(to_domain);
    }

    /// The next child deadline, if any.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        self.connection.next_deadline()
    }

    /// Whether buffered child work remains ready without another io answer.
    #[must_use]
    pub fn has_work(&self) -> bool {
        !self.events.is_empty() || !self.next.is_empty() || self.connection.has_work()
    }

    /// Release settled child slots at the service's reclaim point.
    pub fn reclaim(&mut self) {
        self.connection.reclaim();
    }

    fn child_env(&self, env: &Env<ComponentLimits>) -> Env<connection::Limits> {
        Env { now: env.now, wall: env.wall, limits: self.limits.connection }
    }

    #[expect(clippy::too_many_arguments, reason = "one domain Complete and its two output queues")]
    fn start(
        &mut self,
        env: &Env<ComponentLimits>,
        owner: Token,
        grant: GrantName,
        prompt: llm::Prompt,
        timeout: Duration,
        bounds: Receiving,
        outcome: run::outcome::OutcomeSpec,
        deliver: Option<run::outcome::ChangeSpec>,
        to_domain: &mut Queue<ToDomain>,
        io: &mut Queue<IoRequest>,
    ) {
        if bounds.max_completion_bytes > self.limits.receiving.max_completion_bytes
            || bounds.max_completion_blocks > self.limits.receiving.max_completion_blocks
            || bounds.decoded_call_bytes > self.limits.receiving.decoded_call_bytes
            || bounds.max_failure_bytes > self.limits.receiving.max_failure_bytes
        {
            Self::local_failure(owner, llm::Failure::Limit, bounds.max_failure_bytes, to_domain);
            return;
        }
        if self.active.contains_key(&owner) {
            Self::local_failure(owner, llm::Failure::Invalid, bounds.max_failure_bytes, to_domain);
            return;
        }
        let options = match self.options.get(&prompt.endpoint.0) {
            Some(options) if options.account == grant.account => options,
            Some(_) | None => {
                Self::local_failure(owner, llm::Failure::Invalid, bounds.max_failure_bytes, to_domain);
                return;
            }
        };
        let credential = match self.grants.read(grant, env.now) {
            Ok(credential) => credential,
            Err(GrantError::Missing | GrantError::Lapsed) => {
                Self::local_failure(owner, llm::Failure::Unauthorized, bounds.max_failure_bytes, to_domain);
                return;
            }
            Err(GrantError::Bounds | GrantError::Stale | GrantError::Full) => {
                Self::local_failure(owner, llm::Failure::Invalid, bounds.max_failure_bytes, to_domain);
                return;
            }
        };
        let (mut translated, context) = match crate::prompt::prepare_component(
            owner,
            prompt,
            &outcome,
            deliver.as_ref(),
            bounds,
            &self.limits.adapter,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                Self::refusal(owner, error, bounds.max_failure_bytes, to_domain);
                return;
            }
        };
        translated.reasoning_effort.clone_from(&options.reasoning_effort);
        translated.cache_key.clone_from(&options.cache_key);
        match options.identity {
            IdentityProfile::Plain => {}
            IdentityProfile::ClaudeCode => match shared::anthropic::identity::instructions(&translated.instructions) {
                Ok(instructions) => translated.instructions = instructions,
                Err(error) => {
                    Self::refusal(owner, error, bounds.max_failure_bytes, to_domain);
                    return;
                }
            },
        }
        if let Err(error) = translated.output_ceiling(options.provider, translated.max_output_tokens.unwrap_or(0)) {
            Self::refusal(owner, error, bounds.max_failure_bytes, to_domain);
            return;
        }
        let index = options.index;
        match self.active.insert(owner, Active { context, outcome, deliver }) {
            Ok(None) => {}
            Ok(Some(_)) | Err(_) => {
                Self::local_failure(owner, llm::Failure::Limit, bounds.max_failure_bytes, to_domain);
                return;
            }
        }
        let deadlines = connection::Deadlines {
            connect: self.limits.connect,
            handshake: self.limits.handshake,
            head: self.limits.head,
            idle: self.limits.idle,
            whole: Some(timeout),
        };
        let child_env = self.child_env(env);
        self.connection.down(
            &child_env,
            connection::Request::Start { call: owner, endpoint: index, prompt: translated, credential, deadlines },
            &mut self.events,
            io,
        );
        self.next.insert(owner).expect("one pending demand per admitted call");
        self.route(to_domain);
    }

    fn route(&mut self, to_domain: &mut Queue<ToDomain>) {
        for _ in 0..MAX_OUT.above {
            let event = match self.events.pop() {
                Some(event) => event,
                None => break,
            };
            match event {
                connection::Event::Refused { call, why } => {
                    self.next.remove(&call);
                    let active = self.active.remove(&call);
                    let maximum = match &active {
                        Some(active) => active.context.receiving().max_failure_bytes,
                        None => 0,
                    };
                    let error = match why {
                        connection::Refusal::Closed | connection::Refusal::Endpoint => Error::Invalid,
                        connection::Refusal::Pool => Error::Limit,
                        connection::Refusal::Client(error) => error,
                    };
                    Self::refusal(call, error, maximum, to_domain);
                }
                connection::Event::Delta { call, delta } => {
                    match delta {
                        shared::Delta::Text { text, .. } | shared::Delta::Reasoning { text, .. } => {
                            let count = u32::try_from(text.len()).unwrap_or(u32::MAX);
                            to_domain.push(ToDomain::Text { owner: call, bytes: count });
                        }
                        shared::Delta::ToolArguments { .. } => {}
                    }
                    self.next.insert(call).expect("one pending demand per admitted call");
                }
                connection::Event::Block { call, block: _ } => {
                    self.next.insert(call).expect("one pending demand per admitted call");
                }
                connection::Event::Completed { call, completion } => {
                    self.next.remove(&call);
                    match self.active.remove(&call) {
                        Some(active) => {
                            let resolved = self.resolve(&completion, &active);
                            match crate::completion(active.context, call, completion, resolved) {
                                Ok(event) => Self::domain_event(event, to_domain),
                                Err(error) => Self::refusal(
                                    call,
                                    error,
                                    self.limits.adapter.client.dialect.detail_bytes,
                                    to_domain,
                                ),
                            }
                        }
                        None => {}
                    }
                }
                connection::Event::Failed { call, failure, evidence, detail } => {
                    self.next.remove(&call);
                    match self.active.remove(&call) {
                        Some(active) => match crate::failed(active.context, call, failure, evidence, detail) {
                            Ok(event) => Self::domain_event(event, to_domain),
                            Err(error) => {
                                Self::refusal(call, error, self.limits.adapter.client.dialect.detail_bytes, to_domain);
                            }
                        },
                        None => {}
                    }
                }
                connection::Event::Cancelled { call } => {
                    self.next.remove(&call);
                    match self.active.remove(&call) {
                        Some(active) => match crate::cancelled(active.context, call) {
                            Ok(event) => Self::domain_event(event, to_domain),
                            Err(error) => {
                                Self::refusal(call, error, self.limits.adapter.client.dialect.detail_bytes, to_domain);
                            }
                        },
                        None => {}
                    }
                }
            }
        }
    }

    fn resolve(&self, completion: &shared::Completion, active: &Active) -> Box<[crate::ResolvedCall]> {
        let mut resolved = List::with_capacity(self.limits.adapter.client.dialect.parts);
        for (position, block) in completion.content.iter().enumerate() {
            let shared::Block::ToolCall { name, arguments, .. } = block else {
                continue;
            };
            let mut offered = false;
            for descriptor in active.context.application() {
                if descriptor.name.as_ref() == name.as_ref() {
                    offered = true;
                }
            }
            if !offered {
                continue;
            }
            let call = match name.as_ref() {
                b"finish" => crate::decode_finish(arguments, &active.outcome, &self.limits.adapter),
                b"deliver" => match &active.deliver {
                    Some(change) => crate::decode_deliver(arguments, change, &self.limits.adapter),
                    None => llm::Decoded::Invalid { problem: llm::Problem::UnknownTool },
                },
                _ => crate::decode(
                    name,
                    arguments,
                    active.context.grants(),
                    active.context.served(),
                    &self.limits.adapter,
                ),
            };
            // The completion adapter resolves host declarations itself.
            let mut hosted = false;
            for offered in active.context.served() {
                match offered {
                    llm::Served::Host(tool) if tool.name.as_ref() == name.as_ref() => hosted = true,
                    llm::Served::Host(_)
                    | llm::Served::Wait
                    | llm::Served::Deliver
                    | llm::Served::Finish
                    | llm::Served::SubAgent => {}
                }
            }
            if hosted {
                continue;
            }
            match u32::try_from(position) {
                Ok(position) => {
                    resolved
                        .push(crate::ResolvedCall {
                            position,
                            name: bytes::copy_of(name),
                            input: bytes::copy_of(arguments),
                            call,
                        })
                        .expect("admitted completion block count");
                }
                Err(_) => break,
            }
        }
        resolved.into_boxed()
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only LLM terminal variants are returned by the translation functions"
    )]
    fn domain_event(event: DomainEvent, to_domain: &mut Queue<ToDomain>) {
        match event {
            DomainEvent::Completed { owner, completion } => to_domain.push(ToDomain::Completed { owner, completion }),
            DomainEvent::Failed { owner, failure, evidence, detail } => {
                to_domain.push(ToDomain::Failed { owner, failure, evidence, detail });
            }
            DomainEvent::Cancelled { owner } => to_domain.push(ToDomain::Cancelled { owner }),
            _ => unreachable!("translation returns only LLM terminals"),
        }
    }

    fn refusal(owner: Token, error: Error, maximum: u32, to_domain: &mut Queue<ToDomain>) {
        Self::domain_event(crate::refusal(owner, error, maximum), to_domain);
    }

    fn local_failure(owner: Token, failure: llm::Failure, maximum: u32, to_domain: &mut Queue<ToDomain>) {
        let detail = b"credential or endpoint unavailable";
        let length = detail.len().min(usize::try_from(maximum).expect("u32 fits usize"));
        to_domain.push(ToDomain::Failed {
            owner,
            failure,
            evidence: llm::Evidence::Unsent,
            detail: bytes::copy_of(detail.get(..length).expect("clipped fixed local diagnostic")),
        });
    }
}
