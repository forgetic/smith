//! The scripted shell for the copied domain tree (domain/host.md, sections 2
//! and 11; domain/run.md, section 14). It keeps schedules, pending boundary
//! requests, checkout bytes and observations; it never uses private state to
//! choose replies. Only final quiescence checks read public domain counters.

use std::collections::BTreeMap;

use skein_fake_checkout::Checkout;
use skein_fake_llm_domain as provider;
use skein_lib::{Duration, ReplyTo, Rng, Time, Token};
use skein_world::domain::{Key, Ledger, Referee, Schedule, Span, Stage, Trace};
use smith_domain::{self as agent, Event, Fact, Grant, GrantName, Limits, Request, llm, run, tools};
use smith_tools_world::translate as io;

use crate::{
    BUDGET, LIMITS, fixture,
    referee::{Meeting, Seen},
    script::{self, Job},
    translate,
};

/// Scripted host choice; materialized as a sealed actual terminal at submission.
/// It is scenario data, not a second protocol or delivery implementation.
/// Contract: domain/run.md, sections 8.2 and 13; testing-strategy.md, section 2.2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "scenario choices retain the sealed fixed diagnostic inline and stay Copy"
)]
pub enum HostReply {
    /// Real changed-directory receipt. Contract: domain/run.md, section 8.2.
    Delivered,
    /// Maximum sealed non-UTF8 receipt, supplied as an actual successful terminal.
    /// Contract: domain/run.md, sections 8.2 and 13; testing-strategy.md, section 7.
    OpaqueDelivered,
    /// No changed directory. Contract: domain/run.md, section 8.2.
    Nothing,
    /// Named conflict marker. Contract: domain/run.md, sections 8.1 and 8.2.
    Refused,
    /// Fixed generic failed operation. Contract: domain/run.md, section 8.2.
    Failed(
        /// Sealed fixed-size reason and diagnostic emitted at the actual host
        /// deadline. Contract: domain/run.md, section 8.2.
        run::DeliveryFailure,
    ),
    /// Moved host context. Contract: domain/run.md, section 8.2.
    Stale,
}

/// One sealed scripted-host receipt, used only by boundary fixtures.
/// Its opaque text has no domain interpretation; mount zero is writable.
/// Contract: domain/run.md, sections 8.2 and 13; testing-strategy.md, section 2.2.
#[must_use]
pub fn delivered() -> run::Delivery {
    run::Delivery::Delivered(
        run::Delivered::new(Box::new([
            run::Receipt::new(0, b"scripted receipt".as_slice().into()).expect("bounded receipt")
        ]))
        .expect("one unique receipt"),
    )
}

impl HostReply {
    fn terminal(self) -> run::Delivery {
        match self {
            Self::Delivered => delivered(),
            Self::OpaqueDelivered => run::Delivery::Delivered(
                run::Delivered::new(Box::new([run::Receipt::new(0, vec![0xff; run::Receipt::CAPACITY].into())
                    .expect("maximum sealed opaque receipt")]))
                .expect("one actual changed directory"),
            ),
            Self::Nothing => run::Delivery::Nothing,
            Self::Refused => run::Delivery::Refused(
                run::DeliveryRefusal::new(
                    Some(run::Marker::new(0, b"src/answer.rs".as_slice().into()).expect("relative marker")),
                    b"conflict markers remain".as_slice().into(),
                )
                .expect("bounded named refusal"),
            ),
            Self::Failed(failure) => run::Delivery::Failed(failure),
            Self::Stale => run::Delivery::Stale,
        }
    }
}

/// Immutable deterministic scenario inputs for the real composed agent on a
/// scripted typed host; terminal choices are sealed only at actual submission.
/// Contract: domain/run.md, sections 8 and 13; testing-strategy.md, section 7.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Host selects supplied history explicitly; false starts fresh even if present.
    /// Contract: domain/run.md, sections 3 and 13.
    pub resume: bool,
    /// Positive idle threshold; independent run wall time never pauses.
    /// Contract: domain/run.md, sections 6 and 10.
    pub waiting: Duration,

    /// Generic-host recovery schedule. Contract: domain/run.md, sections 5.2 and 13.
    pub host: HostSchedule,
    /// Replay seed for the host, agent and provider, independently derived.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub seed: u64,
    /// Script selected by the charter's brief.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub job: Job,
    /// The agent's immutable admission and ownership limits.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub limits: Limits,
    /// The run's token and wall-time allowance, shared across conversations.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub budget: run::Budget,
    /// Whether the scripted host grants checkout modification.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub writable: bool,
    /// The fake provider's latency, failures and ownership limits.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub provider: provider::Config,
    /// Latency of host and IO terminals; zero is permitted.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub network: Span,
    /// Time spent by the fixture's checks, capped by their request deadline.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub check: Span,
    /// The host's reply to each push; refusal feedback remains typed and bounded.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub push: HostReply,
    /// Explicit host cancellation time; `None` sends no cancel.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub cancel_at: Option<Duration>,
    /// A cancel loses to its existing terminal with this chance per mille.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub races: u32,
    /// Whether the shell drains facts; disabling this exercises lossy telemetry.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub drain_facts: bool,
}

impl Settings {
    /// A coding charter, ample bounds and a successful typed host, from `seed`.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn calm(seed: u64) -> Settings {
        let provider = smith_session_world::Settings::calm(seed).provider;
        Settings {
            resume: false,
            waiting: Duration::from_secs(30),
            seed,
            job: Job::Coding,
            limits: LIMITS,
            budget: BUDGET,
            writable: true,
            provider,
            network: Span::millis(1, 20),
            check: Span::millis(100, 500),
            push: HostReply::Delivered,
            host: HostSchedule::Answer,
            cancel_at: None,
            races: 0,
            drain_facts: true,
        }
    }
}

/// Finite typed host choices; responses settle actual attempts before recovery.
/// Contract: domain/run.md, sections 5.2 and 13.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostSchedule {
    /// Immediate first recorded answer. Contract: domain/run.md, section 5.2.
    Answer,
    /// Busy, lost committed answer, then replay first recorded answer. Contract: domain/run.md, section 5.2.
    Replay,
    /// Lost then Busy until receiving cap. Contract: domain/run.md, section 5.2.
    Unknown,
    /// Wait beyond declared deadline, settle withdrawn, then recover. Contract: domain/run.md, section 5.2.
    Withdraw,
    /// Wait for withdrawal, but replay actual recorded answer. Contract: domain/run.md, section 5.2.
    LateAnswer,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Family {
    Completion,
    Io,
    Read,
    Probe,
    Check,
    Delivery,
}

#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "scheduled copied terminal records retain their fixed diagnostic tails without another allocation"
)]
enum Delivery {
    Message { name: Token, text: Box<[u8]> },
    Terminal { family: Family, owner: Token, event: Event },
    Io { owner: Token, op: tools::Op, deadline: Time },
    Command { owner: Token, process: skein_fake_checkout::Process, head: u32, tail: u32, timed_out: bool },
    Check { owner: Token, passed: bool, output: Vec<u8>, timed_out: bool, tail: u32 },
    Host { relay: run::RelayName, reply: run::HostReply },
    Cancel,
}

#[derive(Debug)]
struct Flight {
    key: Option<Key>,
    cancelled: bool,
}

/// The real agent on a typed scripted host. [`World::run`] drives every
/// boundary to settlement, then checks its ledgers, referee and facts.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
#[derive(Debug)]
pub struct World {
    settings: Settings,
    now: Time,
    rng: Rng,
    agent: agent::Domain,
    stage: Stage<Limits, Event, Request>,
    provider: provider::Domain,
    provider_stage: Stage<provider::Config, provider::Event, provider::Request>,
    provider_calls: Ledger<Token, (tools::Grants, Box<[llm::Served]>)>,
    schedule: Schedule<Delivery>,
    flights: Ledger<(Family, Token), Flight>,
    disk: Checkout,
    root: u64,
    admitted: Option<Token>,
    answer: Option<run::Answer>,
    answered: Option<Time>,
    checked: Vec<bool>,
    pushes: Vec<run::Delivery>,
    delivery_names: Vec<(run::CallName, Time)>,
    host_history: crate::host_referee::History,
    host_pending: BTreeMap<(u64, u32), Key>,
    host_decision: Option<run::HostAnswer>,
    host_decisions: u32,
    host_terminals: Vec<(run::RelayName, Time, run::HostReply)>,
    snapshots: BTreeMap<Token, Vec<u8>>,
    landed: Vec<u8>,
    prompts: Vec<provider::api::Query>,
    turns: Vec<agent::Turn>,
    messages_seen: Vec<(Time, crate::messages_referee::Seen)>,
    turn_metadata: Vec<(u32, Option<Token>, run::Spend)>,
    waiting: Vec<(Time, Option<Token>)>,
    bounces: Vec<(Token, run::MessageRefusal)>,
    facts: Vec<Fact>,
    trace: Trace,
    referee: Referee<Meeting>,
    stimuli: Vec<()>,
    terminals: u32,
}

impl World {
    /// Creates one host request and a fresh real agent tree. Fixed queue slack
    /// exercises output pressure; finite trace and delivery caps fail fast.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn new(settings: Settings) -> World {
        Self::with_history(settings, None)
    }

    /// Actual typed V2 entrance, including the committed post-transcript tail.
    /// History is host-owned; root/session admission decides before provider work.
    /// Contract: domain/run.md, sections 3 and 13; domain/session.md, section 3.
    #[must_use]
    pub fn with_history(settings: Settings, transcript: Option<agent::Transcript>) -> World {
        let mut disk = Checkout::new();
        let root = fixture::seed(&mut disk);
        let max_out = agent::max_out(&settings.limits);
        let mut stage = Stage::new(settings.limits, max_out, max_out + 3);
        let charter = charter(&settings, root);
        let observed_contract = charter.outcome.clone();
        stage.push(Event::Start {
            reply_to: ReplyTo::new(Token::new(1)),
            worker: Token::new(1),
            charter,
            grants: Box::new([Grant {
                name: GrantName { account: 0, generation: 1 },
                valid: Duration::from_secs(7200),
            }]),
            transcript,
        });
        let mut schedule = Schedule::new();
        if let Some(after) = settings.cancel_at {
            schedule.send(Time::ZERO.saturating_add(after), Delivery::Cancel);
        }
        let mut referee = Referee::new(Meeting::default());
        let change = matches!(
            settings.job,
            Job::Coding | Job::Delegating | Job::Wandering | Job::MidReport | Job::MidChange | Job::MarkerReport
        );
        let mut stimuli = Vec::new();
        referee.observe(
            Time::ZERO,
            Seen::Started {
                contract: observed_contract,
                outcome_bytes: settings.limits.run.outcome_bytes,
                checks: change,
                delivery: matches!(settings.job, Job::MidReport | Job::MidChange | Job::MarkerReport),
                within: settings.budget.time.saturating_add(Duration::from_secs(120)),
            },
            &mut stimuli,
        );
        World {
            settings,
            now: Time::ZERO,
            rng: Rng::new(settings.seed ^ 0x0b),
            agent: agent::Domain::new(&settings.limits, settings.seed ^ 0x17),
            stage,
            provider: provider::Domain::configured(
                &settings.provider,
                settings.seed ^ 0x25,
                script::all(),
                smith_session_world::provider::menu(),
            )
            .expect("scripts and application menu obey provider admission"),
            provider_stage: Stage::new(settings.provider, provider::MAX_OUT, provider::MAX_OUT + 3),
            provider_calls: Ledger::new("fake provider call"),
            schedule,
            flights: Ledger::new("agent request"),
            disk,
            root,
            admitted: None,
            answer: None,
            answered: None,
            checked: Vec::new(),
            pushes: Vec::new(),
            delivery_names: Vec::new(),
            host_history: crate::host_referee::History::default(),
            host_pending: BTreeMap::new(),
            host_decision: None,
            host_decisions: 0,
            host_terminals: Vec::new(),
            snapshots: BTreeMap::new(),
            landed: Vec::new(),
            prompts: Vec::new(),
            turns: Vec::new(),
            messages_seen: Vec::new(),
            turn_metadata: Vec::new(),
            waiting: Vec::new(),
            bounces: Vec::new(),
            facts: Vec::new(),
            trace: Trace::default(),
            referee,
            stimuli,
            terminals: 0,
        }
    }

    /// Script an actual parent input; delivery requires prior Admitted, as the host does.
    /// Contract: domain/run.md, section 6; testing-strategy.md, section 7.
    pub fn message_at(&mut self, at: Time, name: Token, text: Box<[u8]>) {
        self.schedule.send(at, Delivery::Message { name, text });
    }

    /// Runs at most `iterations` deterministic shell rounds. Success means the
    /// one start answered, every boundary settled, and every expectation passed.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    pub fn run(&mut self, iterations: u32) {
        assert!(
            self.drive(iterations),
            "seed {}: world did not settle within {iterations} rounds\n{}",
            self.settings.seed,
            self.trace.lines().join("\n")
        );
    }

    /// Advance up to this many real loop rounds, allowing another real domain
    /// to translate the emitted boundary records before the next round. Returns
    /// true only after the original root/provider/IO ledgers actually settle.
    /// Contract: domain/host.md, section 9; testing-strategy.md, section 2.3.
    pub fn drive(&mut self, iterations: u32) -> bool {
        for _ in 0..iterations {
            self.stage.tick(self.now);
            self.provider_stage.tick(self.now);
            while let Some(delivery) = self.schedule.next(self.now) {
                self.deliver(delivery);
            }
            while self.stage.has_room() && self.agent.is_ready() {
                agent::resume(&mut self.agent, &self.stage.env, &mut self.stage.out);
                self.gather();
            }
            while let Some(event) = self.stage.next_event() {
                self.trace.log(self.now, format!("agent <- {event:?}"));
                agent::step(&mut self.agent, &self.stage.env, event, &mut self.stage.out);
                self.gather();
            }
            while self.stage.has_room() && self.agent.is_due(self.now) {
                agent::fire(&mut self.agent, &self.stage.env, &mut self.stage.out);
                self.gather();
            }
            while let Some(request) = self.stage.out.pop() {
                self.trace.log(self.now, format!("agent -> {request:?}"));
                self.request(request);
            }
            while let Some(event) = self.provider_stage.next_event() {
                provider::step(&mut self.provider, &self.provider_stage.env, event, &mut self.provider_stage.out);
            }
            while self.provider_stage.has_room() && self.provider.is_due(self.now) {
                provider::fire(&mut self.provider, &self.provider_stage.env, &mut self.provider_stage.out);
            }
            while let Some(provider::Request::Reply { to, result }) = self.provider_stage.out.pop() {
                let owner = to.into_token();
                let (grants, served) = self.provider_calls.end(owner);
                if self.flights.get((Family::Completion, owner)).is_some_and(|flight| !flight.cancelled) {
                    let event = match result {
                        Ok(answer) => {
                            Event::Completed { owner, completion: translate::completion(answer, grants, &served) }
                        }
                        Err(error) => Event::Failed {
                            owner,
                            failure: translate::failure(error),
                            evidence: smith_domain::llm::Evidence::Unknown,
                            detail: Default::default(),
                        },
                    };
                    self.send(Family::Completion, owner, event);
                }
            }
            self.agent.reclaim();
            self.provider.reclaim();
            if self.referee.is_due(self.now) {
                self.referee.fire(self.now, &mut self.stimuli);
            }
            self.referee.assert_holding(self.settings.seed);
            assert!(
                self.trace.lines().len() <= 20_000 && self.schedule.len() <= 256,
                "seed {}: fixture trace and deliveries stay bounded",
                self.settings.seed
            );
            if self.answer.is_some()
                && self.schedule.is_empty()
                && !self.stage.has_events()
                && !self.provider_stage.has_events()
                && self.provider.calls() == 0
                && !self.agent.is_ready()
            {
                self.settled();
                return true;
            }
            let immediate = self.stage.has_events() || self.provider_stage.has_events() || self.agent.is_ready();
            if !immediate {
                self.now = [
                    self.schedule.next_time(),
                    self.agent.next_deadline(),
                    self.provider.next_deadline(),
                    self.referee.next_deadline(),
                ]
                .into_iter()
                .flatten()
                .min()
                .expect("an unsettled world has a next boundary");
            }
        }
        false
    }

    fn observe(&mut self, seen: Seen) {
        self.referee.observe(self.now, seen, &mut self.stimuli);
    }

    fn gather(&mut self) {
        if self.settings.drain_facts {
            while let Some(fact) = self.agent.pop_fact() {
                self.facts.push(fact);
            }
            while self.agent.pop_content().is_some() {}
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive fixture routing table keeps each copied request and its terminal ownership together"
    )]
    fn request(&mut self, request: Request) {
        match request {
            Request::Turn { worker, number, read, spent, turn } => {
                assert_eq!(worker, Token::new(1));
                assert_eq!(usize::try_from(number).expect("bounded output number"), self.turns.len() + 1);
                self.observe(Seen::Turn { number });
                self.messages_seen
                    .push((self.now, crate::messages_referee::Seen::Turn { number, read, turn: turn.clone() }));
                self.turn_metadata.push((number, read, spent));
                self.turns.push(turn);
            }
            Request::Waiting { worker, read } => {
                assert_eq!(worker, Token::new(1));
                self.messages_seen.push((self.now, crate::messages_referee::Seen::Waiting { read }));
                self.waiting.push((self.now, read));
            }
            Request::MessageBounced { run, name, reason } => {
                assert_eq!(Some(run), self.admitted);
                self.messages_seen.push((self.now, crate::messages_referee::Seen::Bounced { name }));
                self.bounces.push((name, reason));
            }
            Request::Admitted { worker, run } => {
                assert_eq!(worker, Token::new(1), "the host's admitted identity is echoed");
                assert!(self.admitted.replace(run).is_none(), "a start is admitted at most once");
                self.messages_seen.push((self.now, crate::messages_referee::Seen::Admitted));
            }
            Request::Answer { to, answer } => {
                if matches!(&answer, run::Answer::Failed { .. }) {
                    self.host_history.shutdown(self.now);
                }
                assert_eq!(to, ReplyTo::new(Token::new(1)), "the host receives its own answer");
                self.observe(Seen::Answered {
                    answer: copy_answer(&answer),
                    pending: self.flights.keys().count() + self.host_pending.len(),
                });
                let (turns, parked) = match &answer {
                    run::Answer::Parked { turns, .. } => (*turns, true),
                    run::Answer::Accepted { turns, .. }
                    | run::Answer::Delivered { turns, .. }
                    | run::Answer::Failed { turns, .. } => (*turns, false),
                    run::Answer::Refused(_) => (0, false),
                };
                assert!(self.answer.replace(answer).is_none(), "one answer per host start");
                self.messages_seen.push((self.now, crate::messages_referee::Seen::Answer { turns, parked }));
                self.answered = Some(self.now);
            }
            Request::Checking { worker, .. } => {
                assert_eq!(worker, Token::new(1), "checking notice echoes the host identity");
            }
            Request::Complete { owner, prompt, .. } => {
                self.host_history.prompt(&prompt).expect("exact feedback for observed provider host call");
                self.observe(Seen::Completing { owner });
                self.flights.open((Family::Completion, owner), Flight { key: None, cancelled: false });
                self.provider_calls.open(owner, (prompt.tools, prompt.served.clone()));
                let query = translate::query(prompt);
                self.messages_seen.push((self.now, crate::messages_referee::Seen::Prompt { query: query.clone() }));
                self.prompts.push(query.clone());
                self.provider_stage.push(provider::Event::Call { reply_to: ReplyTo::new(owner), query });
            }
            Request::Cancel { owner } => self.cancel(Family::Completion, owner, Event::Cancelled { owner }),
            Request::Io { owner, op, deadline } => {
                self.flights.open((Family::Io, owner), Flight { key: None, cancelled: false });
                let delivery = match op {
                    tools::Op::Spawn { cwd, command, env, roots, head, tail } => {
                        match io::spawn(&self.disk, &cwd, &command, &env, &roots, (head, tail)) {
                            Ok(started) => {
                                let duration = Duration::from_nanos(
                                    u64::try_from(started.process.program.duration.as_nanos())
                                        .expect("fixture command duration fits"),
                                );
                                let complete = self.now.saturating_add(duration);
                                let key = self.schedule.send(
                                    complete.min(deadline),
                                    Delivery::Command {
                                        owner,
                                        process: started.process,
                                        head,
                                        tail,
                                        timed_out: complete > deadline,
                                    },
                                );
                                self.flights.get_mut((Family::Io, owner)).expect("the command is pending").key =
                                    Some(key);
                                return;
                            }
                            Err(done) => {
                                Delivery::Terminal { family: Family::Io, owner, event: Event::Done { owner, done } }
                            }
                        }
                    }
                    op @ (tools::Op::Load { .. }
                    | tools::Op::Scan { .. }
                    | tools::Op::Store { .. }
                    | tools::Op::Search { .. }) => Delivery::Io { owner, op, deadline },
                };
                self.schedule_flight(Family::Io, owner, delivery);
            }
            Request::CancelIo { owner } => {
                self.cancel(Family::Io, owner, Event::Done { owner, done: tools::Done::Cancelled });
            }
            Request::Read { owner, at, max, deadline } => {
                let complete = self.now.saturating_add(self.settings.network.draw(&mut self.rng));
                let read = if complete > deadline {
                    run::Read::Failed
                } else {
                    match self.disk.load(at.root.raw(), &at.path, u64::from(max)) {
                        Ok((text, _)) => run::Read::Text { text: text.into(), whole: true },
                        Err(_) => run::Read::Missing,
                    }
                };
                let key = self.schedule.send(
                    complete.min(deadline),
                    Delivery::Terminal { family: Family::Read, owner, event: Event::Read { owner, read } },
                );
                self.flights.open((Family::Read, owner), Flight { key: Some(key), cancelled: false });
            }
            Request::Probe { owner, at, deadline } => {
                let complete = self.now.saturating_add(self.settings.network.draw(&mut self.rng));
                let executable = complete <= deadline
                    && self
                        .disk
                        .load(at.root.raw(), &at.path, 4096)
                        .is_ok_and(|(content, _)| content.starts_with(b"#!"));
                let key = self.schedule.send(
                    complete.min(deadline),
                    Delivery::Terminal { family: Family::Probe, owner, event: Event::Probed { owner, executable } },
                );
                self.flights.open((Family::Probe, owner), Flight { key: Some(key), cancelled: false });
            }
            Request::Check { owner, program, deadline, tail } => {
                self.observe(Seen::Checking { owner, tree: self.code() });
                assert_eq!(&*program.path, fixture::CHECKS, "the copied run checks its baseline convention");
                let (passed, output) = fixture::check(&self.disk, program.root.raw());
                self.flights.open((Family::Check, owner), Flight { key: None, cancelled: false });
                let duration = self.settings.check.draw(&mut self.rng);
                let complete = self.now.saturating_add(duration);
                let key = self.schedule.send(
                    complete.min(deadline),
                    Delivery::Check { owner, passed, output, timed_out: complete > deadline, tail },
                );
                self.flights.get_mut((Family::Check, owner)).expect("the check is pending").key = Some(key);
            }
            Request::Abort { owner } => self.cancel(Family::Check, owner, Event::Aborted { owner }),
            Request::Deliver { worker, owner, change, name, deadline } => {
                // Title/body are this fixture host's final-Change policy only.
                // The Report-only mid-run fixture instead requires opaque ticket.
                let required: &[&[u8]] = if change.fields.iter().any(|field| field.name.as_ref() == b"ticket") {
                    &[b"ticket"]
                } else {
                    &[b"title", b"body"]
                };
                for &name in required {
                    let value = change
                        .fields
                        .iter()
                        .find(|field| &*field.name == name)
                        .expect("the scripted host requires this field");
                    assert!(!value.value.is_empty(), "the host receives the metadata its own contract required");
                }
                assert_eq!(worker, Token::new(1), "the push names the scripted host's request");
                self.delivery_names.push((name, self.now));
                let tree = self.code();
                let finishing = !change.fields.iter().any(|field| field.name.as_ref() == b"ticket");
                self.observe(Seen::Pushing { owner, name, tree: tree.clone(), finishing });
                self.snapshots.insert(owner, tree);
                self.flights.open((Family::Delivery, owner), Flight { key: None, cancelled: false });
                let after = self.settings.network.draw(&mut self.rng);
                let complete = self.now.saturating_add(after);
                let push = if complete > deadline {
                    run::Delivery::Failed(run::DeliveryFailure::new(run::DeliveryReason::TimedOut))
                } else if self.settings.job == Job::MarkerReport
                    && self
                        .disk
                        .load(self.root, b"conflict.txt", u64::MAX)
                        .is_ok_and(|(bytes, _)| bytes.windows(7).any(|part| part == b"<<<<<<<"))
                {
                    run::Delivery::Refused(
                        run::DeliveryRefusal::new(
                            Some(
                                run::Marker::new(0, b"conflict.txt".as_slice().into()).expect("relative fixture path"),
                            ),
                            b"conflict markers remain".as_slice().into(),
                        )
                        .expect("bounded feedback"),
                    )
                } else {
                    self.settings.push.terminal()
                };
                let key = self.schedule.send(
                    complete.min(deadline),
                    Delivery::Terminal { family: Family::Delivery, owner, event: Event::Delivered { owner, push } },
                );
                self.flights.get_mut((Family::Delivery, owner)).expect("submitted delivery").key = Some(key);
            }
            Request::HostCall { worker, relay, name, tool, effect, input, deadline } => {
                self.host_history
                    .submit(crate::host_referee::Submission {
                        worker,
                        relay,
                        name,
                        tool,
                        effect,
                        input,
                        at: self.now,
                        deadline,
                    })
                    .expect("valid recovery history");
                let answer = run::HostAnswer::new(b"opaque host answer: first decision".as_slice().into(), false)
                    .expect("bounded host text");
                let reply = match self.settings.host {
                    HostSchedule::Answer | HostSchedule::LateAnswer => run::HostReply::Answered(answer),
                    HostSchedule::Replay => match relay.attempt {
                        1 => run::HostReply::Busy,
                        2 => run::HostReply::Unanswered(run::Unanswered::Lost),
                        _ => run::HostReply::Answered(answer),
                    },
                    HostSchedule::Unknown => {
                        if relay.attempt == 1 {
                            run::HostReply::Unanswered(run::Unanswered::Lost)
                        } else {
                            run::HostReply::Busy
                        }
                    }
                    HostSchedule::Withdraw => {
                        if relay.attempt == 1 {
                            run::HostReply::Unanswered(run::Unanswered::Withdrawn)
                        } else {
                            run::HostReply::Answered(answer)
                        }
                    }
                };
                let reply = match reply {
                    run::HostReply::Answered(answer) => {
                        let recorded = self.record_host(answer);
                        run::HostReply::Answered(recorded)
                    }
                    run::HostReply::Unanswered(run::Unanswered::Lost) => {
                        self.record_host(
                            run::HostAnswer::new(b"opaque host answer: first decision".as_slice().into(), false)
                                .expect("bounded first record"),
                        );
                        run::HostReply::Unanswered(run::Unanswered::Lost)
                    }
                    run::HostReply::Busy => run::HostReply::Busy,
                    run::HostReply::Unanswered(run::Unanswered::Withdrawn) => {
                        run::HostReply::Unanswered(run::Unanswered::Withdrawn)
                    }
                };
                let at = if matches!(self.settings.host, HostSchedule::LateAnswer | HostSchedule::Withdraw)
                    && relay.attempt == 1
                {
                    deadline.saturating_add(Duration::from_millis(20))
                } else {
                    self.now.saturating_add(self.settings.network.draw(&mut self.rng))
                };
                let key = self.schedule.send(at, Delivery::Host { relay, reply });
                assert!(self.host_pending.insert((relay.owner.raw(), relay.attempt), key).is_none());
            }
            Request::WithdrawHost { relay } => {
                self.host_history.withdraw(relay).expect("withdraw retains actual terminal");
                assert!(self.host_pending.contains_key(&(relay.owner.raw(), relay.attempt)));
            }
            Request::Rejected { .. } | Request::Exhausted { .. } => {}
        }
    }

    fn record_host(&mut self, answer: run::HostAnswer) -> run::HostAnswer {
        if let Some(recorded) = &self.host_decision {
            return recorded.clone();
        }
        self.host_decisions = self.host_decisions.checked_add(1).expect("one immutable host decision");
        self.host_decision = Some(answer.clone());
        answer
    }

    fn send(&mut self, family: Family, owner: Token, event: Event) {
        self.schedule_flight(family, owner, Delivery::Terminal { family, owner, event });
    }

    fn schedule_flight(&mut self, family: Family, owner: Token, delivery: Delivery) {
        let at = self.now.saturating_add(self.settings.network.draw(&mut self.rng));
        let key = self.schedule.send(at, delivery);
        self.flights.get_mut((family, owner)).expect("the terminal is for a pending request").key = Some(key);
    }

    fn cancel(&mut self, family: Family, owner: Token, event: Event) {
        let Some(flight) = self.flights.get((family, owner)) else { return };
        if flight.cancelled {
            return;
        }
        if flight.key.is_some() && self.rng.chance(self.settings.races) {
            return;
        }
        if let Some(key) = flight.key {
            self.schedule.withdraw(key).expect("a pending terminal can be withdrawn");
        }
        self.flights.get_mut((family, owner)).expect("the cancellation owns the pending request").cancelled = true;
        self.send(family, owner, event);
    }

    fn deliver(&mut self, delivery: Delivery) {
        match delivery {
            Delivery::Message { name, text } => {
                self.messages_seen.push((self.now, crate::messages_referee::Seen::Input { name, text: text.clone() }));
                let run = self.admitted.expect("parent sends only after actual admission");
                self.stage.push(Event::Message { run, name, text });
            }
            Delivery::Host { relay, reply } => {
                self.host_terminals.push((relay, self.now, reply.clone()));
                self.host_history.terminal(self.now, relay, &reply).expect("one actual host terminal");
                self.host_pending.remove(&(relay.owner.raw(), relay.attempt)).expect("host relay pending");
                self.stage.push(Event::HostReturned { relay, reply });
                self.terminals += 1;
            }
            Delivery::Cancel => {
                self.observe(Seen::Stopped { failure: run::Failure::Cancelled });
                if let Some(run) = self.admitted {
                    self.host_history.shutdown(self.now);
                    self.stage.push(Event::Cancel { run });
                }
            }
            Delivery::Terminal { family, owner, event } => self.terminal(family, owner, event),
            Delivery::Io { owner, op, deadline } => {
                let done = if self.now > deadline { tools::Done::TimedOut } else { io::perform(&mut self.disk, op) };
                self.terminal(Family::Io, owner, Event::Done { owner, done });
            }
            Delivery::Command { owner, process, head, tail, timed_out } => {
                if !timed_out {
                    self.disk.finish(&process);
                }
                let exit = if timed_out { None } else { Some(process.program.exit) };
                let done = io::exited(exit, &process.program.output, head, tail);
                self.terminal(Family::Io, owner, Event::Done { owner, done });
            }
            Delivery::Check { owner, passed, output, timed_out, tail } => {
                let exit = if timed_out { run::Exit::TimedOut } else { run::Exit::Code { code: u8::from(!passed) } };
                let keep = usize::try_from(tail).expect("fixture output bound fits").min(output.len());
                let cut = output.len() - keep;
                let ran = run::Ran {
                    exit,
                    output: output[cut..].into(),
                    cut: u64::try_from(cut).expect("fixture output fits"),
                };
                self.terminal(Family::Check, owner, Event::Checked { owner, ran });
            }
        }
    }

    fn terminal(&mut self, family: Family, owner: Token, event: Event) {
        self.flights.end((family, owner));
        self.terminals += 1;
        match &event {
            Event::Completed { completion, .. } => {
                for part in &completion.content {
                    match part {
                        llm::Said::ToolCall { id, name, .. } if name.as_ref() == b"host_action" => {
                            self.host_history.called(id.clone()).expect("observed single provider host operation");
                        }
                        llm::Said::Text { .. }
                        | llm::Said::Refusal { .. }
                        | llm::Said::Opaque { .. }
                        | llm::Said::ToolCall { .. } => {}
                    }
                }
                self.observe(Seen::Completed {
                    owner,
                    spent: run::Spend {
                        turns: 1,
                        input: completion.usage.input_tokens,
                        output: completion.usage.output_tokens,
                        cache_read: completion.usage.cache_read_tokens,
                        cache_write: completion.usage.cache_write_tokens,
                    },
                });
            }
            Event::Failed { .. } | Event::Cancelled { .. } => self.observe(Seen::CompletionEnded { owner }),
            Event::Checked { ran, .. } => {
                self.checked.push(ran.exit == (run::Exit::Code { code: 0 }));
                self.observe(Seen::Checked { owner, exit: ran.exit });
            }
            Event::Delivered { push, .. } => {
                self.pushes.push(push.clone());
                let tree = self.snapshots.remove(&owner).expect("a push snapshots its checkout");
                let landed = if matches!(push, run::Delivery::Delivered(_)) { tree } else { Vec::new() };
                if !landed.is_empty() {
                    self.landed.clone_from(&landed);
                }
                self.observe(Seen::Delivered { owner, push: push.clone(), tree: landed });
            }
            Event::Aborted { .. } => self.observe(Seen::Checked { owner, exit: run::Exit::Signalled }),
            Event::HostReturned { .. }
            | Event::Message { .. }
            | Event::Start { .. }
            | Event::Grant { .. }
            | Event::Cancel { .. }
            | Event::Done { .. }
            | Event::Read { .. }
            | Event::Probed { .. } => {}
        }
        match &event {
            Event::Completed { completion, .. } => self.messages_seen.push((
                self.now,
                crate::messages_referee::Seen::Completed {
                    parts: crate::messages_referee::completion_parts(&completion.content),
                },
            )),
            Event::Failed { .. } | Event::Cancelled { .. } => {
                self.messages_seen.push((self.now, crate::messages_referee::Seen::CompletionEnded))
            }
            Event::Start { .. }
            | Event::Message { .. }
            | Event::Grant { .. }
            | Event::Cancel { .. }
            | Event::HostReturned { .. }
            | Event::Delivered { .. }
            | Event::Read { .. }
            | Event::Probed { .. }
            | Event::Checked { .. }
            | Event::Aborted { .. }
            | Event::Done { .. } => {}
        }
        self.stage.push(event);
    }

    fn settled(&self) {
        self.host_history
            .finish(matches!(self.answer(), run::Answer::Accepted { .. }))
            .expect("normal continuation owes exact host feedback; shutdown settles actual relays");
        assert!(self.host_pending.is_empty());
        self.flights.assert_settled();
        self.provider_calls.assert_settled();
        self.referee.assert_passed(self.settings.seed);
        assert_eq!(
            (self.agent.run().runs(), self.agent.run().conversations(), self.agent.run().calls()),
            (0, 0, 0),
            "the run tree settled"
        );
        assert_eq!(
            (
                self.agent.session().sessions(),
                self.agent.session().runs(),
                self.agent.session().kits(),
                self.agent.session().jobs()
            ),
            (0, 0, 0, 0),
            "sessions and tools settled"
        );
        assert_eq!(
            (self.agent.peers(), self.agent.flights(), self.agent.tickets()),
            (0, 0, 0),
            "root routing holds nothing"
        );
        if self.settings.drain_facts && self.agent.facts_lost() == 0 {
            self.check_facts();
        }
    }

    fn check_facts(&self) {
        use agent::{run::facts as rf, session::Fact as sf};
        let mut used = run::Spend::ZERO;
        let (mut opened, mut ended, mut answered) = (0_u32, 0_u32, 0_u32);
        for fact in &self.facts {
            match fact {
                Fact::Session { fact: sf::Used { usage, .. } } => {
                    used = used.saturating_add(run::Spend {
                        turns: 1,
                        input: usage.input_tokens,
                        output: usage.output_tokens,
                        cache_read: usage.cache_read_tokens,
                        cache_write: usage.cache_write_tokens,
                    });
                }
                Fact::Run { fact: rf::Fact::Opened { .. } } => opened += 1,
                Fact::Run { fact: rf::Fact::Ended { .. } } => ended += 1,
                Fact::Run { fact: rf::Fact::Answered { .. } } => answered += 1,
                Fact::Run { .. } | Fact::Session { .. } => {}
            }
        }
        let spent = match self.answer() {
            run::Answer::Refused(_) => run::Spend::ZERO,
            run::Answer::Parked { spent, .. }
            | run::Answer::Delivered { spent, .. }
            | run::Answer::Accepted { spent, .. }
            | run::Answer::Failed { spent, .. } => *spent,
        };
        assert_eq!(used, spent, "facts match independently accepted provider usage and the host answer");
        assert_eq!(opened, ended, "every conversation fact has one terminal");
        assert_eq!(answered, u32::from(self.admitted.is_some()), "every admitted run answers once in its facts");
    }

    /// Exact time-ordered outside observations for the independent chat oracle.
    /// Contract: domain/run.md, sections 6 and 13; testing-strategy.md, section 7.
    #[must_use]
    pub fn messages_seen(&self) -> &[(Time, crate::messages_referee::Seen)] {
        &self.messages_seen
    }

    /// Actual main concrete turns, in emitted order. Contract: domain/run.md, section 13.
    #[must_use]
    pub fn turns(&self) -> &[agent::Turn] {
        &self.turns
    }

    /// Actual activation numbers/read fences/token usage. Contract: domain/run.md, sections 6 and 13.
    #[must_use]
    pub fn turn_metadata(&self) -> &[(u32, Option<Token>, run::Spend)] {
        &self.turn_metadata
    }

    /// Exact entrance decisions for scripted parent messages. Contract: domain/run.md, section 6.
    #[must_use]
    pub fn bounces(&self) -> &[(Token, run::MessageRefusal)] {
        &self.bounces
    }

    /// Actual settled Waiting notices. Contract: domain/run.md, section 6.
    #[must_use]
    pub fn waiting(&self) -> &[(Time, Option<Token>)] {
        &self.waiting
    }

    /// Actual final root output after every lower terminal and main Turn.
    /// Contract: domain/run.md, section 13; testing-strategy.md, section 7.
    #[must_use]
    pub fn answer(&self) -> &run::Answer {
        self.answer.as_ref().expect("the world has settled")
    }

    /// Check outcomes actually delivered to the agent, in request order.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn checked(&self) -> &[bool] {
        &self.checked
    }

    /// Typed push replies actually delivered by the scripted host.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn pushes(&self) -> &[run::Delivery] {
        &self.pushes
    }

    /// Bytes of the fixture code in the shared checkout, independent of the agent.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn code(&self) -> Vec<u8> {
        self.disk.load(self.root, fixture::CODE, u64::MAX).expect("fixture code exists").0
    }

    /// Bytes retained by a successful host push; empty when nothing landed.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn landed(&self) -> &[u8] {
        &self.landed
    }

    /// Content-free facts drained by the shell, never used to choose stimuli.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn facts(&self) -> &[Fact] {
        &self.facts
    }

    /// How many facts or content observations the copied agent dropped.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn lost(&self) -> u64 {
        self.agent.facts_lost()
    }

    /// Provider queries observed at its boundary, including returned tool IDs.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn prompts(&self) -> &[provider::api::Query] {
        &self.prompts
    }

    /// Host-owned recorded decisions, distinct from relay attempts.
    /// Contract: domain/run.md, sections 5.2 and 13.
    #[must_use]
    pub const fn host_decisions(&self) -> u32 {
        self.host_decisions
    }

    /// Actual host relay terminals, retained only by this outside world.
    /// Contract: domain/run.md, sections 5.2 and 13.
    #[must_use]
    pub fn host_terminals(&self) -> &[(run::RelayName, Time, run::HostReply)] {
        &self.host_terminals
    }

    /// Actual observed cancellation/failed-run boundary time.
    /// Contract: domain/run.md, sections 5.2 and 10.
    #[must_use]
    pub const fn host_shutdown_at(&self) -> Option<Time> {
        self.host_history.shutdown_at()
    }

    /// Immutable opaque relay observations for replay and recovery checks.
    /// Contract: domain/run.md, sections 5.2 and 14.
    #[must_use]
    pub fn host_submissions(&self) -> &[crate::host_referee::Submission] {
        self.host_history.submissions()
    }

    /// Stable host names and submission times observed from actual delivery.
    /// Contract: domain/run.md, sections 8.2 and 13; testing-strategy.md, section 7.
    #[must_use]
    pub fn delivery_names(&self) -> &[(run::CallName, Time)] {
        &self.delivery_names
    }

    /// Ordered boundary trace for same-seed replay comparisons.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn trace(&self) -> &[String] {
        self.trace.lines()
    }

    /// Injected monotonic time when the host received the answer.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn answered_at(&self) -> Time {
        self.answered.expect("the world has settled")
    }

    /// Actual safety checks and met liveness obligations from the shared referee.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    #[must_use]
    pub fn judged(&self) -> (u64, u64) {
        self.referee.judged()
    }
}

fn charter(settings: &Settings, root: u64) -> run::Charter {
    use run::charter::{Checkout as Roots, Endpoint, Grants, Llm, Repository, Tools};
    use run::outcome::{ChangeSpec, FieldRule, ItemRule, ItemSpec, OutcomeSpec, TextSpec, VerdictRule};
    let change = matches!(settings.job, Job::Coding | Job::Delegating | Job::Wandering | Job::MidChange);
    let review = settings.job == Job::Review;
    let rule = VerdictRule {
        name: b"request-changes".as_slice().into(),
        text_max: 1024,
        fields: Box::new([]),
        items: ItemSpec {
            min: 1,
            max: 4,
            kinds: Box::new([ItemRule {
                kind: b"blocking".as_slice().into(),
                fields: Box::new([
                    FieldRule { name: b"path".as_slice().into(), max: 1024 },
                    FieldRule { name: b"body".as_slice().into(), max: 1024 },
                ]),
            }]),
        },
    };
    let llm =
        Llm { account: 0, endpoint: Endpoint(0), model: b"fake-1".as_slice().into(), max_tokens: 4096, dialect: 1 };
    run::Charter {
        brief: script::cue(settings.job).unwrap_or(b"Look into the code.").into(),
        checkout: Roots {
            repositories: Box::new([Repository {
                name: b"work".as_slice().into(),
                root: Token::new(root),
                writable: settings.writable,
            }]),
        },
        grants: Grants {
            deliver: if matches!(settings.job, Job::MidReport | Job::MidChange | Job::MarkerReport) {
                Some(ChangeSpec { fields: Box::new([FieldRule { name: b"ticket".as_slice().into(), max: 128 }]) })
            } else {
                None
            },
            tools: Tools { inspect: true, modify: settings.writable, shell: settings.writable },

            agents: true,
            host_tools: if settings.job == Job::HostTools {
                Box::new([run::HostTool {
                    name: b"host_action".as_slice().into(),
                    description: b"Opaque host write".as_slice().into(),
                    schema: br#"{"type":"object"}"#.as_slice().into(),
                    effect: run::HostEffect::Write,
                    timeout: Duration::from_millis(300),
                }])
            } else {
                Box::new([])
            },
        },
        outcome: OutcomeSpec {
            change: if change {
                Some(ChangeSpec {
                    fields: Box::new([
                        smith_domain::run::outcome::FieldRule { name: b"title".as_slice().into(), max: 1024 },
                        smith_domain::run::outcome::FieldRule { name: b"body".as_slice().into(), max: 1024 },
                    ]),
                })
            } else {
                None
            },
            verdicts: if review { Box::new([rule]) } else { Box::new([]) },
            report: if !change && !review && settings.job != Job::Failing {
                Some(TextSpec {
                    min: 0,
                    max: 1024,
                    fields: Box::new([FieldRule { name: b"source".as_slice().into(), max: 128 }]),
                })
            } else {
                None
            },
            failure: if settings.job == Job::Failing {
                Some(TextSpec {
                    min: 1,
                    max: 1024,
                    fields: Box::new([FieldRule { name: b"cause".as_slice().into(), max: 128 }]),
                })
            } else {
                None
            },
        },
        budget: settings.budget,
        models: Box::new([Llm { model: b"fake-2".as_slice().into(), ..llm.clone() }]),
        llm,
        resume: settings.resume,
        waiting: settings.waiting,
    }
}

fn copy_answer(answer: &run::Answer) -> run::Answer {
    use run::outcome::Declared;
    match answer {
        run::Answer::Refused(refusal) => run::Answer::Refused(*refusal),
        run::Answer::Parked { spent, turns } => run::Answer::Parked { spent: *spent, turns: *turns },
        run::Answer::Delivered { name, receipts, stopped, spent, turns } => run::Answer::Delivered {
            name: *name,
            receipts: receipts.clone(),
            stopped: *stopped,
            spent: *spent,
            turns: *turns,
        },
        run::Answer::Failed { failure, spent, turns } => {
            run::Answer::Failed { failure: *failure, spent: *spent, turns: *turns }
        }
        run::Answer::Accepted { outcome, spent, turns } => {
            let outcome = match outcome {
                Declared::Change(change) => Declared::Change(change.clone()),
                Declared::Verdict(verdict) => Declared::Verdict(verdict.clone()),
                Declared::Report(report) => Declared::Report(report.clone()),
                Declared::Failure(failure) => Declared::Failure(failure.clone()),
            };
            run::Answer::Accepted { outcome, spent: *spent, turns: *turns }
        }
    }
}
