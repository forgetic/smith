//! A typed local host over a durable fake store and the shared scripted
//! agent-provider translation (domain/host.md, sections 8–10).

use std::collections::{BTreeMap, VecDeque};

use skein_fake_llm_domain as provider;
use skein_lib::{Duration, Env, Queue, ReplyTo, Time, Token, Wall};
use smith_agent_world::{self as agent_world, translate};
use smith_domain::{self as agent, run, tools};
use smith_local_domain::{self as local, AgentIo, ChatState, Contract, Event, ExitStatus, Request};

/// Typed fake transcript store, movable across independent invocations.
#[derive(Debug, Default)]
pub struct Store {
    state: Option<ChatState>,
    turns: Vec<agent::Turn>,
}

impl Store {
    fn history(&self) -> Option<agent::Transcript> {
        let first = self.turns.first()?;
        Some(agent::Transcript {
            version: agent::session::record::VERSION,
            endpoint: first.endpoint,
            dialect: first.dialect,
            turns: self.turns.clone().into_boxed_slice(),
            after: Box::new([]),
        })
    }
}

/// One invocation; the store can be taken into a later invocation.
#[derive(Debug)]
pub struct World {
    domain: local::Domain,
    env: Env<local::Limits>,
    out: Queue<Request>,
    provider: provider::Domain,
    provider_env: Env<provider::Config>,
    provider_out: Queue<provider::Request>,
    pending: BTreeMap<Token, (tools::Grants, Box<[agent::llm::Served]>)>,
    events: VecDeque<Event>,
    store: Store,
    shown: Vec<Box<[u8]>>,
    waiting: bool,
    exit: Option<ExitStatus>,
    completions: u32,
}

impl World {
    /// A chat with one account, no workspace and a scripted fake provider.
    #[must_use]
    pub fn new(seed: u64) -> World {
        Self::with_store(seed, Store::default())
    }

    /// A fresh local domain over an existing durable fake store.
    #[must_use]
    pub fn with_store(seed: u64, store: Store) -> World {
        let limits = local::Limits {
            agent: agent_world::LIMITS,
            endpoints: Box::new([run::charter::Endpoint(0)]),
            chat_bytes: 64,
            text_bytes: 4096,
            models: 1,
            line_bytes: 1024,
            show_bytes: 4096,
            lines: 8,
            unsaved: 2,
            facts: 64,
        };
        let config = local::Config {
            chat: b"main".as_slice().into(),
            instructions: b"@chat Assist the person.".as_slice().into(),
            brief: run::charter::Brief { sections: Box::new([]) },
            models: Box::new([run::charter::Llm {
                prices: run::Prices { input: 0, cached: 0, output: 0, unit: 1 },
                dialect: 1,
                account: 0,
                endpoint: run::charter::Endpoint(0),
                model: b"scripted".as_slice().into(),
                max_tokens: 256,
            }]),
            budget: agent_world::BUDGET,
            conventions: None,
            contract: Contract::Report(run::outcome::TextSpec { max: 2048, fields: Box::new([]) }),
            waiting: Duration::from_secs(30),
            resume: true,
            accounts: Box::new([0]),
            workspace: None,
        };
        let provider_config = smith_session_world::Settings::calm(seed).provider;
        let provider = agent_world::scripted_provider(&provider_config, seed ^ 0x25);
        let out = Queue::with_capacity(local::max_out(&limits));
        let domain = local::Domain::new(config, &limits, seed ^ 0x17).expect("world config fits local limits");
        World {
            domain,
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            out,
            provider,
            provider_env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: provider_config },
            provider_out: Queue::with_capacity(provider::MAX_OUT),
            pending: BTreeMap::new(),
            events: VecDeque::new(),
            store,
            shown: Vec::new(),
            waiting: false,
            exit: None,
            completions: 0,
        }
    }

    /// Take the fake store after ending or dropping this invocation.
    #[must_use]
    pub fn into_store(self) -> Store {
        self.store
    }

    /// Type a person line; its name becomes durable before agent delivery.
    pub fn line(&mut self, text: &[u8]) {
        self.events.push_back(Event::Line { text: text.into() });
    }

    /// Text the local host asked to show, in order.
    #[must_use]
    pub fn shown(&self) -> &[Box<[u8]>] {
        &self.shown
    }

    /// Number of durable turns in the fake store.
    #[must_use]
    pub fn saved_turns(&self) -> usize {
        self.store.turns.len()
    }

    /// Whether the child reported that it waits for another person message.
    #[must_use]
    pub fn waiting(&self) -> bool {
        self.waiting
    }

    /// Number of actual fake provider completion calls.
    #[must_use]
    pub fn completions(&self) -> u32 {
        self.completions
    }

    /// Drive at most `iterations` rounds until the first answered question waits.
    pub fn drive(&mut self, iterations: u32) -> bool {
        for _ in 0..iterations {
            if self.domain.is_ready() {
                local::resume(&mut self.domain, &self.env, &mut self.out);
            }
            if let Some(event) = self.events.pop_front() {
                local::step(&mut self.domain, &self.env, event, &mut self.out);
            }
            if self.domain.is_due(self.env.now) {
                local::fire(&mut self.domain, &self.env, &mut self.out);
            }
            while let Some(request) = self.out.pop() {
                self.request(request);
            }
            if self.provider.is_due(self.provider_env.now) {
                provider::fire(&mut self.provider, &self.provider_env, &mut self.provider_out);
            }
            while let Some(reply) = self.provider_out.pop() {
                let provider::Request::Reply { to, result } = reply;
                let owner = to.into_token();
                let (grants, served) = self.pending.remove(&owner).expect("one provider terminal per call");
                let event = match result {
                    Ok(answer) => Event::Agent(AgentIo::Completed {
                        owner,
                        completion: translate::completion(answer, grants, &served),
                    }),
                    Err(error) => Event::Agent(AgentIo::Failed {
                        owner,
                        failure: translate::failure(error),
                        evidence: agent::llm::Evidence::Unknown,
                        detail: Box::new([]),
                    }),
                };
                self.events.push_back(event);
            }
            self.domain.reclaim();
            self.provider.reclaim();
            if self.waiting && self.saved_turns() > 0 {
                return true;
            }
            if !self.domain.is_ready() && self.events.is_empty() && self.out.is_empty() {
                let next = [self.domain.next_deadline(), self.provider.next_deadline()].into_iter().flatten().min();
                let Some(next) = next else { return false };
                self.env.now = next;
                self.provider_env.now = next;
            }
        }
        false
    }

    fn request(&mut self, request: Request) {
        match request {
            Request::Show { text } => {
                if text.as_ref() == b"Waiting for a message" {
                    self.waiting = true;
                }
                self.shown.push(text);
            }
            Request::Load => {
                self.events.push_back(Event::Loaded { state: self.store.state, transcript: self.store.history() });
            }
            Request::SaveState { state } => {
                self.store.state = Some(state);
                self.events.push_back(Event::StateSaved);
            }
            Request::SaveTurn { number, turn } => {
                assert_eq!(usize::try_from(number).expect("turn number fits"), self.store.turns.len() + 1);
                self.store.turns.push(turn);
                self.events.push_back(Event::TurnSaved { number });
            }
            Request::Credential { account } => self.events.push_back(Event::Credential {
                grant: agent::Grant {
                    name: agent::GrantName { account, generation: 1 },
                    valid: Duration::from_secs(7200),
                },
            }),
            Request::Agent(request) => self.agent_request(request),
            Request::Exit { status } => self.exit = Some(status),
        }
    }

    fn agent_request(&mut self, request: agent::Request) {
        match request {
            agent::Request::Complete { owner, prompt, .. } => {
                self.completions += 1;
                let grants = prompt.tools;
                let served = prompt.served.clone();
                assert!(self.pending.insert(owner, (grants, served)).is_none());
                let query = translate::query(prompt);
                provider::step(
                    &mut self.provider,
                    &self.provider_env,
                    provider::Event::Call { reply_to: ReplyTo::new(owner), query },
                    &mut self.provider_out,
                );
            }
            agent::Request::Cancel { owner } => self.events.push_back(Event::Agent(AgentIo::Cancelled { owner })),
            agent::Request::Io { .. }
            | agent::Request::CancelIo { .. }
            | agent::Request::Read { .. }
            | agent::Request::Probe { .. }
            | agent::Request::Check { .. }
            | agent::Request::Abort { .. }
            | agent::Request::Waiting { .. }
            | agent::Request::Turn { .. }
            | agent::Request::Admitted { .. }
            | agent::Request::Answer { .. }
            | agent::Request::Checking { .. }
            | agent::Request::Rejected { .. }
            | agent::Request::Exhausted { .. }
            | agent::Request::HostCall { .. }
            | agent::Request::WithdrawHost { .. }
            | agent::Request::Deliver { .. } => unreachable!("workspace-free chat forwards only provider calls"),
        }
    }
}
