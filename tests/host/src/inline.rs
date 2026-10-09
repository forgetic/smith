//! Inline capability over the native fake provider, with the shared parent
//! referee (domain/host.md, sections 9 and 10; testing-strategy.md, section 6).
//! Stage, Schedule, Ledger and Trace own flow, time, races and observations.

use skein_fake_llm_domain as provider;
use skein_lib::{Duration, Env, Queue, ReplyTo, Time, Token};
use skein_world::domain::{Ledger, Schedule, Stage, Trace};
use smith_domain::{self as smith, run};
use smith_host_domain::{self as host, parent};
use smith_inline_agent::{self as inline, Completion, Request};
use std::collections::BTreeMap;

use crate::Seen;

#[derive(Debug)]
struct Binding {
    agent: Token,
    owner: Token,
    tools: smith::tools::Grants,
    served: Box<[smith::llm::Served]>,
    cancelled: bool,
}

/// Checked two-slot capacities, sharing the agent world's shipped root bounds.
#[must_use]
pub fn limits() -> inline::Limits {
    let smith = smith_agent_world::LIMITS;
    inline::Limits {
        slots: 2,
        smith,
        window: smith::Window { turns: 8, bytes: smith::max_turn_bytes(&smith).expect("root turn bound") * 8 },
        wall_time: Duration::from_secs(3600),
        facts: 32,
    }
}

/// Explicit value declarations for the existing native fake scripts.
#[must_use]
pub fn configuration() -> smith::Config {
    smith::Config {
        endpoints: Box::from([run::charter::Endpoint(0)]),
        models: [b"fake-1".as_slice(), b"fake-2".as_slice(), b"fake-3".as_slice()]
            .into_iter()
            .map(|model| smith::ConfiguredModel {
                endpoint: run::charter::Endpoint(0),
                model: model.into(),
                window: 8192,
                output: 4096,
            })
            .collect(),
    }
}

/// Workspace-free script, moved through the same typed parent Start.
#[must_use]
pub fn start(seed: u64, job: smith_agent_world::Job) -> host::Start {
    let mut settings = smith_agent_world::Settings::calm(seed);
    settings.job = job;
    let mut charter = smith_agent_world::scripted_charter(&settings);
    charter.conventions = None;
    charter.grants.tools = run::charter::Tools { inspect: false, modify: false, shell: false };
    charter.grants.agents = false;
    charter.models = Box::default();
    host::Start {
        messages: Box::default(),
        logical_run: Token::new(7),
        activation: 1,
        workspace: None,
        charter: host::Charter::new(charter, settings.limits.run.run_bytes).expect("bounded script charter"),
        transcript: None,
        answered: Box::default(),
        directories: Box::default(),
        grants: Box::from([host::Grant { account: 0, generation: 1, valid: Duration::from_secs(7200) }]),
    }
}

/// Real inline agent with native fake LLM and boundary-only rights accounting.
#[derive(Debug)]
pub struct World {
    pub agent: inline::Agent,
    pub stage: Stage<inline::Limits, parent::Event, Request>,
    pub schedule: Schedule<parent::Event>,
    pub seen: BTreeMap<Token, Seen>,
    pub history: BTreeMap<Token, Vec<smith::session::record::Turn>>,
    pub facts: Vec<(Token, smith::Fact)>,
    pub content: Vec<(Token, smith::Content)>,
    pub observations: Vec<inline::Fact>,
    pub trace: Trace,
    pub lower: Ledger<Token, (Token, Token)>,
    provider: provider::Domain,
    provider_env: Env<provider::Config>,
    provider_out: Queue<provider::Request>,
    bindings: BTreeMap<Token, Binding>,
    minted: u64,
    pub auto_ack: bool,
    pub auto_answer: bool,
    pending: Vec<parent::Event>,
}

impl World {
    /// Construct from bounded capacities and deterministic provider timing.
    #[must_use]
    pub fn new(seed: u64, bounds: inline::Limits) -> Self {
        let configuration = smith_agent_world::Settings::calm(seed).provider;
        let mut trace = Trace::default();
        trace.log(Time::ZERO, format!("seed {seed}"));
        Self {
            agent: inline::Agent::new(&bounds, configuration_value(), seed),
            stage: Stage::new(bounds, inline::max_out(&bounds), inline::max_out(&bounds) + 2),
            schedule: Schedule::new(),
            seen: BTreeMap::new(),
            history: BTreeMap::new(),
            facts: Vec::new(),
            content: Vec::new(),
            observations: Vec::new(),
            trace,
            lower: Ledger::new("inline native provider terminal"),
            provider: smith_agent_world::scripted_provider(&configuration, seed),
            provider_env: Env { now: Time::ZERO, wall: skein_lib::Wall::EPOCH, limits: configuration },
            provider_out: Queue::with_capacity(provider::MAX_OUT),
            bindings: BTreeMap::new(),
            minted: 0,
            auto_ack: true,
            auto_answer: true,
            pending: Vec::new(),
        }
    }

    /// Return the generation-safe parent handle learned from Started.
    #[must_use]
    pub fn handle(&self, client: Token) -> Token {
        self.seen[&client].agent.expect("actual Started")
    }

    /// Deliver one parent command, recording actual parent terminals first.
    pub fn event(&mut self, event: parent::Event) {
        match &event {
            parent::Event::Answer { agent, call, .. } => {
                let seen =
                    self.seen.values_mut().find(|seen| seen.agent == Some(*agent)).expect("parent's actual slot");
                assert!(seen.calls.remove(call), "one actual parent terminal");
            }
            parent::Event::Acknowledge { agent, turn } => {
                let seen =
                    self.seen.values_mut().find(|seen| seen.agent == Some(*agent)).expect("parent's actual slot");
                seen.turns.remove(turn);
            }
            parent::Event::Spawn { .. }
            | parent::Event::Message { .. }
            | parent::Event::Grant { .. }
            | parent::Event::Stop { .. } => {}
        }
        self.trace.log(self.stage.env.now, format!("up {event:?}"));
        self.stage.push(event);
        while let Some(event) = self.stage.next_event() {
            inline::step(&mut self.agent, &self.stage.env, event, &mut self.stage.out);
            self.outputs();
        }
        self.pass();
    }

    /// Advance to one actual clock point; queued inputs precede root alarms.
    pub fn at(&mut self, now: Time) {
        assert!(now >= self.stage.env.now);
        self.stage.tick(now);
        self.provider_env.now = now;
        while let Some(event) = self.schedule.next(now) {
            self.event(event);
        }
        while self.provider.is_due(now) {
            provider::fire(&mut self.provider, &self.provider_env, &mut self.provider_out);
            self.provider_replies();
        }
        while self.agent.is_due(now) {
            assert!(self.stage.has_room());
            inline::fire(&mut self.agent, &self.stage.env, &mut self.stage.out);
            self.outputs();
            self.pass();
        }
        self.pass();
    }

    /// Drive real alarms until every requested slot has its containment terminal.
    pub fn run(&mut self) {
        for _ in 0..4096 {
            if self.seen.values().all(|seen| seen.gone.is_some()) && !self.seen.is_empty() {
                return;
            }
            let next = self
                .provider
                .next_deadline()
                .into_iter()
                .chain(self.agent.next_deadline())
                .chain(self.schedule.next_time())
                .min()
                .expect("live owner has an alarm");
            self.at(next);
        }
        panic!("bounded inline story did not settle\n{}", self.trace.lines().join("\n"));
    }

    /// Verify every lower right and emitted parent call/turn is settled.
    pub fn settled(&self) {
        self.lower.assert_settled();
        assert!(self.bindings.is_empty());
        assert_eq!(self.agent.hosted(), 0);
        for seen in self.seen.values() {
            assert!(seen.gone.is_some());
            assert!(seen.calls.is_empty());
            assert!(seen.turns.is_empty());
        }
        assert_eq!(self.agent.next_deadline(), None);
        assert!(self.schedule.is_empty());
    }

    fn outputs(&mut self) {
        while let Some(request) = self.stage.out.pop() {
            self.trace.log(self.stage.env.now, format!("down {request:?}"));
            match request {
                Request::Parent(request) => {
                    let client = parent_client(&request);
                    let seen = self.seen.entry(client).or_default();
                    if let parent::Request::Turn { turn, .. } = &request {
                        self.history.entry(client).or_default().push(turn.body.value().clone());
                        if self.auto_ack {
                            self.pending.push(parent::Event::Acknowledge {
                                agent: seen.agent.expect("Started before Turn"),
                                turn: turn.number,
                            });
                        }
                    }
                    if let parent::Request::Called { call, .. } = &request {
                        if self.auto_answer {
                            self.pending.push(parent::Event::Answer {
                                agent: seen.agent.expect("Started before Called"),
                                call: *call,
                                reply: host::Reply::Host { error: false, body: b"host completed".as_slice().into() },
                            });
                        }
                    }
                    let lower_settled = !self.bindings.values().any(|binding| Some(binding.agent) == seen.agent);
                    seen.observe_parent(request, lower_settled, lower_settled);
                }
                Request::Lower { agent, request } => match request {
                    smith::Request::Complete { owner, prompt, .. } => {
                        self.minted += 1;
                        let callback = Token::new(self.minted);
                        let tools = prompt.tools;
                        let served = prompt.served.clone();
                        assert!(
                            self.bindings
                                .insert(callback, Binding { agent, owner, tools, served, cancelled: false })
                                .is_none()
                        );
                        self.lower.open(callback, (agent, owner));
                        provider::step(
                            &mut self.provider,
                            &self.provider_env,
                            provider::Event::Call {
                                reply_to: ReplyTo::new(callback),
                                query: smith_agent_world::translate::query(prompt),
                            },
                            &mut self.provider_out,
                        );
                        self.provider_replies();
                    }
                    smith::Request::Cancel { owner } => {
                        let binding = self
                            .bindings
                            .values_mut()
                            .find(|binding| binding.agent == agent && binding.owner == owner)
                            .expect("cancel retains actual provider call");
                        assert!(!binding.cancelled, "cancel emitted once");
                        binding.cancelled = true;
                    }
                    other => panic!("workspace-free lower request {other:?}"),
                },
            }
        }
    }

    fn provider_replies(&mut self) {
        while let Some(provider::Request::Reply { to, result }) = self.provider_out.pop() {
            let callback = to.into_token();
            let binding = self.bindings.remove(&callback).expect("one fake terminal");
            self.lower.end(callback);
            let completion = if binding.cancelled {
                Completion::Cancelled { owner: binding.owner }
            } else {
                match result {
                    Ok(answer) => Completion::Completed {
                        owner: binding.owner,
                        completion: smith_agent_world::translate::completion(answer, binding.tools, &binding.served),
                    },
                    Err(error) => Completion::Failed {
                        owner: binding.owner,
                        failure: smith_agent_world::translate::failure(error),
                        evidence: smith::llm::Evidence::Response,
                        detail: Box::default(),
                    },
                }
            };
            inline::terminal(&mut self.agent, &self.stage.env, binding.agent, completion, &mut self.stage.out);
            self.outputs();
        }
        self.provider.reclaim();
    }

    fn pass(&mut self) {
        for _ in 0..4096 {
            self.outputs();
            let pending = std::mem::take(&mut self.pending);
            for event in pending {
                self.event(event);
            }
            if !self.agent.is_ready() {
                break;
            }
            assert!(self.stage.has_room());
            inline::resume(&mut self.agent, &self.stage.env, &mut self.stage.out);
        }
        for client in self.seen.keys() {
            while let Some(fact) = self.agent.pop_fact(*client) {
                self.facts.push((*client, fact));
            }
            while let Some(content) = self.agent.pop_content(*client) {
                self.content.push((*client, content));
            }
        }
        while let Some(fact) = self.agent.pop_observation() {
            self.observations.push(fact);
        }
        inline::reclaim(&mut self.agent);
    }
}

fn configuration_value() -> smith::Config {
    configuration()
}

fn parent_client(request: &parent::Request) -> Token {
    match request {
        parent::Request::Started { client, .. }
        | parent::Request::Admitted { client }
        | parent::Request::Called { client, .. }
        | parent::Request::Withdrawn { client, .. }
        | parent::Request::Turn { client, .. }
        | parent::Request::Waiting { client, .. }
        | parent::Request::Long { client, .. }
        | parent::Request::LongDone { client }
        | parent::Request::Rejected { client, .. }
        | parent::Request::Exhausted { client, .. }
        | parent::Request::Told { client, .. }
        | parent::Request::Answered { client, .. }
        | parent::Request::Faulted { client, .. }
        | parent::Request::MessageRefused { client, .. }
        | parent::Request::Gone { client, .. } => *client,
    }
}
