//! A typed local host over a durable fake store and the shared scripted
//! agent-provider translation (domain/host.md, sections 8–10).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    history_override: Option<agent::Transcript>,
}

impl Store {
    fn history(&self) -> Option<agent::Transcript> {
        if let Some(history) = &self.history_override {
            return Some(history.clone());
        }
        let first = self.turns.first()?;
        Some(agent::Transcript {
            version: agent::session::record::VERSION,
            endpoint: first.endpoint,
            dialect: first.dialect,
            turns: self.turns.clone().into_boxed_slice(),
            after: Box::new([]),
        })
    }

    /// Last durable activation number, including a start whose agent never ran.
    #[must_use]
    pub fn activation(&self) -> u64 {
        self.state.map_or(0, |state| state.activation)
    }

    /// Number of durable turns across activations.
    #[must_use]
    pub fn turns(&self) -> usize {
        self.turns.len()
    }

    /// Last person message named by a durable turn.
    #[must_use]
    pub fn read(&self) -> Option<Token> {
        self.state.and_then(|state| state.read)
    }

    /// Inject an unsupported transcript version for refusal stories.
    pub fn refuse_version(&mut self) {
        self.history_override = Some(agent::Transcript {
            version: agent::session::record::VERSION.saturating_add(1),
            endpoint: agent::session::llm::Endpoint(0),
            dialect: 1,
            turns: Box::new([]),
            after: Box::new([]),
        });
    }
}

/// A boundary at which the world drops its domain without settling more work.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cut {
    /// The next activation metadata is durable; its terminal is not delivered.
    AfterSaveState,
    /// A numbered turn request reached the store but was not written.
    BeforeSaveTurn(u32),
    /// A numbered turn is durable and its terminal reached the domain.
    AfterTurnSaved(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Goal {
    Waiting,
    Parked,
    Cut,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
enum FirstFailure {
    None,
    Reject,
    Exhaust,
    Used,
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
    cancelled: BTreeSet<Token>,
    events: VecDeque<Event>,
    store: Store,
    shown: Vec<Box<[u8]>>,
    reached: BTreeSet<Goal>,
    exit: Option<ExitStatus>,
    completions: u32,
    prompt_assistants: Vec<usize>,
    activation_turns: u32,
    cut: Option<Cut>,
    slow_store: bool,
    delayed_turns: VecDeque<u32>,
    first_failure: FirstFailure,
    credential_requests: u32,
}

impl World {
    /// A chat with one account, no workspace and a scripted fake provider.
    #[must_use]
    pub fn new(seed: u64) -> World {
        Self::with_store(seed, Store::default())
    }

    /// A one-turn unsaved window for store-pressure stories.
    #[must_use]
    pub fn tight_unsaved(seed: u64) -> World {
        Self::with_capacity(seed, Store::default(), true, 1)
    }

    /// A fresh local domain over an existing durable fake store.
    #[must_use]
    pub fn with_store(seed: u64, store: Store) -> World {
        Self::with_resume(seed, store, true)
    }

    /// Start fresh despite saved history the agent would refuse.
    #[must_use]
    pub fn with_store_fresh(seed: u64, store: Store) -> World {
        Self::with_resume(seed, store, false)
    }

    fn with_resume(seed: u64, store: Store, resume: bool) -> World {
        Self::with_capacity(seed, store, resume, 2)
    }

    fn with_capacity(seed: u64, store: Store, resume: bool, unsaved: u32) -> World {
        let limits = local::Limits {
            agent: agent_world::LIMITS,
            endpoints: Box::new([run::charter::Endpoint(0)]),
            chat_bytes: 64,
            text_bytes: 4096,
            models: 1,
            line_bytes: 1024,
            show_bytes: 4096,
            lines: 8,
            unsaved,
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
            resume,
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
            cancelled: BTreeSet::new(),
            events: VecDeque::new(),
            store,
            shown: Vec::new(),
            reached: BTreeSet::new(),
            exit: None,
            completions: 0,
            prompt_assistants: Vec::new(),
            activation_turns: 0,
            cut: None,
            slow_store: false,
            delayed_turns: VecDeque::new(),
            first_failure: FirstFailure::None,
            credential_requests: 0,
        }
    }

    /// Take the fake store after ending or dropping this invocation.
    #[must_use]
    pub fn into_store(self) -> Store {
        self.store
    }

    /// Make the first completion reject its initial credential generation.
    pub fn reject_first_credential(&mut self) {
        self.first_failure = FirstFailure::Reject;
    }

    /// Make the first completion report account exhaustion.
    pub fn exhaust_first_account(&mut self) {
        self.first_failure = FirstFailure::Exhaust;
    }

    /// Hold store turn acknowledgements until released by the story.
    pub fn slow_store(&mut self) {
        self.slow_store = true;
    }

    /// Release the oldest delayed turn acknowledgement.
    pub fn release_turn(&mut self) -> bool {
        let Some(number) = self.delayed_turns.pop_front() else { return false };
        self.events.push_back(Event::TurnSaved { number });
        true
    }

    /// Number of delayed turn acknowledgements.
    #[must_use]
    pub fn delayed_turns(&self) -> usize {
        self.delayed_turns.len()
    }

    /// Number of credential requests actually made by the local host.
    #[must_use]
    pub fn credential_requests(&self) -> u32 {
        self.credential_requests
    }

    /// The person interrupts the live run.
    pub fn interrupt(&mut self) {
        self.events.push_back(Event::Interrupt);
    }

    /// Drive until a cancelled answer is shown.
    pub fn drive_to_cancelled(&mut self, iterations: u32) -> bool {
        self.drive_until(iterations, Goal::Cancelled)
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
        self.reached.contains(&Goal::Waiting)
    }

    /// Number of actual fake provider completion calls.
    #[must_use]
    pub fn completions(&self) -> u32 {
        self.completions
    }

    /// Number of assistant messages in each provider request.
    #[must_use]
    pub fn prompt_assistants(&self) -> &[usize] {
        &self.prompt_assistants
    }

    /// Last activation saved by this invocation.
    #[must_use]
    pub fn activation(&self) -> u64 {
        self.store.activation()
    }

    /// Run through the waiting interval until the agent parks.
    pub fn drive_to_park(&mut self, iterations: u32) -> bool {
        self.drive_until(iterations, Goal::Parked)
    }

    /// Drop at a chosen durable-store boundary.
    pub fn drive_to_cut(&mut self, iterations: u32, cut: Cut) -> bool {
        self.cut = Some(cut);
        self.drive_until(iterations, Goal::Cut)
    }

    /// Drive at most `iterations` rounds until the first answered question waits.
    pub fn drive(&mut self, iterations: u32) -> bool {
        self.drive_until(iterations, Goal::Waiting)
    }

    fn drive_until(&mut self, iterations: u32, goal: Goal) -> bool {
        for _ in 0..iterations {
            if self.domain.is_ready() {
                local::resume(&mut self.domain, &self.env, &mut self.out);
            }
            if let Some(event) = self.events.pop_front() {
                let saved = match &event {
                    Event::TurnSaved { number } => Some(*number),
                    Event::Line { .. }
                    | Event::Interrupt
                    | Event::Closed
                    | Event::Loaded { .. }
                    | Event::StateSaved
                    | Event::StoreFailed { .. }
                    | Event::Credential { .. }
                    | Event::NoCredential { .. }
                    | Event::Agent(_) => None,
                };
                local::step(&mut self.domain, &self.env, event, &mut self.out);
                if let (Some(Cut::AfterTurnSaved(target)), Some(number)) = (self.cut, saved)
                    && target == number
                {
                    self.reached.insert(Goal::Cut);
                }
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
                if self.cancelled.remove(&owner) {
                    continue;
                }
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
            let reached = self.reached.contains(&goal) && (goal != Goal::Waiting || self.saved_turns() > 0);
            if reached {
                return true;
            }
            if !self.domain.is_ready() && self.events.is_empty() && self.out.is_empty() {
                if self.slow_store && !self.delayed_turns.is_empty() && self.pending.is_empty() {
                    return false;
                }
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
                    self.reached.insert(Goal::Waiting);
                }
                if text.as_ref() == b"Chat parked" {
                    self.reached.insert(Goal::Parked);
                }
                if text.as_ref() == b"Run cancelled" {
                    self.reached.insert(Goal::Cancelled);
                }
                self.shown.push(text);
            }
            Request::Load => {
                self.events.push_back(Event::Loaded { state: self.store.state, transcript: self.store.history() });
            }
            Request::SaveState { state, fresh } => {
                if self.store.activation() < state.activation {
                    self.activation_turns = 0;
                }
                if fresh {
                    self.store.turns.clear();
                    self.store.history_override = None;
                }
                self.store.state = Some(state);
                self.events.push_back(Event::StateSaved);
                if self.cut == Some(Cut::AfterSaveState) {
                    self.reached.insert(Goal::Cut);
                }
            }
            Request::SaveTurn { number, read, turn } => {
                if self.cut == Some(Cut::BeforeSaveTurn(number)) {
                    self.reached.insert(Goal::Cut);
                    return;
                }
                assert_eq!(number, self.activation_turns.saturating_add(1));
                self.activation_turns = number;
                let state = self.store.state.as_mut().expect("activation state saved before any turn");
                state.read = read;
                self.store.turns.push(turn);
                if self.slow_store {
                    self.delayed_turns.push_back(number);
                } else {
                    self.events.push_back(Event::TurnSaved { number });
                }
            }
            Request::Credential { account } => {
                self.credential_requests = self.credential_requests.saturating_add(1);
                self.events.push_back(Event::Credential {
                    grant: agent::Grant {
                        name: agent::GrantName { account, generation: u64::from(self.credential_requests) },
                        valid: Duration::from_secs(7200),
                    },
                });
            }
            Request::Agent(request) => self.agent_request(request),
            Request::Exit { status } => self.exit = Some(status),
        }
    }

    fn agent_request(&mut self, request: agent::Request) {
        match request {
            agent::Request::Complete { owner, prompt, .. } => {
                self.completions += 1;
                self.prompt_assistants
                    .push(prompt.messages.iter().filter(|message| message.role == agent::llm::Role::Assistant).count());
                let grants = prompt.tools;
                let served = prompt.served.clone();
                if matches!(self.first_failure, FirstFailure::Exhaust) {
                    self.first_failure = FirstFailure::Used;
                    self.events.push_back(Event::Agent(AgentIo::Failed {
                        owner,
                        failure: agent::llm::Failure::Exhausted { retry_after: Duration::from_secs(60) },
                        evidence: agent::llm::Evidence::Response,
                        detail: Box::new([]),
                    }));
                    return;
                }
                if matches!(self.first_failure, FirstFailure::Reject) {
                    self.first_failure = FirstFailure::Used;
                    self.events.push_back(Event::Agent(AgentIo::Failed {
                        owner,
                        failure: agent::llm::Failure::Unauthorized,
                        evidence: agent::llm::Evidence::Response,
                        detail: Box::new([]),
                    }));
                    return;
                }
                assert!(self.pending.insert(owner, (grants, served)).is_none());
                let query = translate::query(prompt);
                provider::step(
                    &mut self.provider,
                    &self.provider_env,
                    provider::Event::Call { reply_to: ReplyTo::new(owner), query },
                    &mut self.provider_out,
                );
            }
            agent::Request::Cancel { owner } => {
                self.cancelled.insert(owner);
                self.events.push_back(Event::Agent(AgentIo::Cancelled { owner }));
            }
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
