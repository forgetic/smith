//! Local chat transitions and agent composition (domain/host.md, sections 2,
//! 6 and 8–9). The agent's IO requests pass through unchanged. The host saves
//! names before use and holds an answer until every turn has a store terminal.

use alloc::boxed::Box;
use core::mem;
use skein_lib::{Env, List, Queue, ReplyTo, Time, Token};
use smith_domain as agent;

use crate::boundary::{
    AgentIo, ChatState, DeliveryIntent, DeliveryRecord, DeliveryState, Event, ExitStatus, GitOp, GitResult, Request,
};
use crate::chat::{Chat, Phase};
use crate::credentials::Grants;
use crate::delivery::{InPlace, Stage, Step, commit_message};
use crate::person::Line;
use crate::turns::Turns;
use crate::{Config, Fact, Invalid, Limits, charter};

/// Upper bound on local output from one event or ready child step.
#[must_use]
pub const fn max_out(limits: &Limits) -> u32 {
    agent::max_out(&limits.agent).saturating_mul(4).saturating_add(limits.lines).saturating_add(8)
}

/// One local chat and its in-process agent.
#[derive(Debug)]
pub struct Domain {
    config: Config,
    agent: agent::Domain,
    agent_out: Queue<agent::Request>,
    chat: Chat,
    transcript: Option<agent::Transcript>,
    record: Option<DeliveryRecord>,
    delivery: Option<InPlace>,
    saving: Option<DeliveryRecord>,
    delivery_owner: Option<Token>,
    landed: List<agent::run::Receipt>,
    reconciling: Option<Reconcile>,
    last_turn: u32,
    raw_lines: Queue<Box<[u8]>>,
    line: Option<Line>,
    grants: Grants,
    grant_failed: bool,
    run: Option<Token>,
    turns: Turns,
    held: Queue<AgentIo>,
    stop_failed: bool,
    show_bytes: u32,
    wall_deadline: Option<Time>,
    facts: Queue<Fact>,
    facts_lost: u64,
}

#[derive(Debug)]
struct Reconcile {
    record: DeliveryRecord,
    next: u32,
    receipts: List<agent::run::Receipt>,
}

impl Reconcile {
    fn new(record: DeliveryRecord) -> Self {
        Self { record, next: 0, receipts: List::with_capacity(agent::run::MAX_DIRECTORIES) }
    }

    fn intent(&self) -> &DeliveryIntent {
        match &self.record.state {
            DeliveryState::Intent(intent) => intent,
            DeliveryState::Answer(_) => unreachable!("only an intent is reconciled"),
        }
    }

    fn receipt(&mut self, directory: u32, text: Box<[u8]>) {
        self.receipts
            .push(agent::run::Receipt::new(directory, text).expect("bounded receipt"))
            .expect("admitted directory count");
    }
}

impl Domain {
    /// Validate local choices and build the agent with the configured endpoints.
    pub fn new(config: Config, limits: &Limits, seed: u64) -> Result<Domain, Invalid> {
        config.validate(limits)?;
        let agent_config = agent::Config { endpoints: Box::from(limits.endpoints.as_ref()) };
        Ok(Domain {
            config,
            agent: agent::Domain::new(&limits.agent, agent_config, seed),
            agent_out: Queue::with_capacity(agent::max_out(&limits.agent)),
            chat: Chat::new(),
            transcript: None,
            record: None,
            delivery: None,
            saving: None,
            delivery_owner: None,
            landed: List::with_capacity(agent::run::MAX_DIRECTORIES),
            reconciling: None,
            last_turn: 0,
            raw_lines: Queue::with_capacity(limits.lines),
            line: None,
            grants: Grants::new(limits.agent.accounts),
            grant_failed: false,
            run: None,
            turns: Turns::new(limits.unsaved),
            held: Queue::with_capacity(1),
            stop_failed: false,
            show_bytes: limits.show_bytes,
            wall_deadline: None,
            facts: Queue::with_capacity(limits.facts),
            facts_lost: 0,
        })
    }

    /// Whether initial loading or the child has deferred work.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        (self.chat.phase == Phase::Loading && !self.chat.load_requested) || self.agent.is_ready()
    }

    /// Earliest child deadline.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        if self.pressured() { self.wall_deadline } else { self.agent.next_deadline() }
    }

    /// Whether the child has a deadline due at `now`.
    #[must_use]
    pub fn is_due(&self, now: Time) -> bool {
        match self.next_deadline() {
            Some(deadline) => deadline <= now,
            None => false,
        }
    }

    /// Reclaim child slots after delivering one iteration's output.
    pub fn reclaim(&mut self) {
        self.agent.reclaim();
    }

    /// Oldest content-free observation, if the caller wants it.
    pub fn pop_fact(&mut self) -> Option<Fact> {
        self.facts.pop()
    }

    /// Observations lost because the caller did not drain them.
    #[must_use]
    pub fn facts_lost(&self) -> u64 {
        self.facts_lost
    }

    fn pressured(&self) -> bool {
        self.turns.unsaved.room() == 0 && !self.held.is_empty()
    }
}

/// Handle one typed event and its immediate child handoffs.
pub fn step(domain: &mut Domain, env: &Env<Limits>, event: Event, out: &mut Queue<Request>) {
    match event {
        Event::Line { text } => receive_line(domain, env, text, out),
        Event::Interrupt => interrupt(domain, env, out),
        Event::Closed => closed(domain, out),
        Event::Loaded { state, transcript, delivery } => loaded(domain, env, state, transcript, delivery, out),
        Event::StateSaved => state_saved(domain, env, out),
        Event::TurnSaved { number } => turn_saved(domain, number, out),
        Event::DeliverySaved { name } => delivery_saved(domain, env, name, out),
        Event::Git { owner, result } => git_result(domain, env, owner, result, out),
        Event::PlainStatus { owner, changed } => plain_status(domain, env, owner, changed, out),
        Event::StoreFailed { reason: _ } => store_failed(domain, env, out),
        Event::Credential { grant } => credential(domain, env, grant, out),
        Event::NoCredential { account: _, reason: _ } => no_credential(domain, env, out),
        Event::Agent(io) => agent_terminal(domain, env, io),
    }
    route_agent(domain, env, out);
    release_held(domain, env, out);
}

fn agent_terminal(domain: &mut Domain, env: &Env<Limits>, io: AgentIo) {
    let completion = match &io {
        AgentIo::Completed { .. } | AgentIo::Failed { .. } | AgentIo::Cancelled { .. } => true,
        AgentIo::Done { .. }
        | AgentIo::Read { .. }
        | AgentIo::Probed { .. }
        | AgentIo::Checked { .. }
        | AgentIo::Aborted { .. } => false,
    };
    if completion && domain.turns.unsaved.room() == 0 {
        domain.held.push(io);
    } else {
        agent::step(&mut domain.agent, &agent_env(env), io.into_event(), &mut domain.agent_out);
    }
}

fn release_held(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if domain.turns.unsaved.room() == 0 {
        return;
    }
    let Some(io) = domain.held.pop() else { return };
    agent::step(&mut domain.agent, &agent_env(env), io.into_event(), &mut domain.agent_out);
    route_agent(domain, env, out);
}

/// Fire one child deadline and route its output.
pub fn fire(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if domain.pressured() {
        if let Some(deadline) = domain.wall_deadline
            && deadline <= env.now
            && domain.chat.phase == Phase::Running
        {
            interrupt(domain, env, out);
            domain.wall_deadline = None;
        }
    } else {
        agent::fire(&mut domain.agent, &agent_env(env), &mut domain.agent_out);
    }
    route_agent(domain, env, out);
}

/// Ask for the initial transcript or advance one ready agent handoff.
pub fn resume(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if domain.chat.phase == Phase::Loading && !domain.chat.load_requested {
        domain.chat.load_requested = true;
        out.push(Request::Load);
    } else if domain.agent.is_ready() {
        agent::resume(&mut domain.agent, &agent_env(env), &mut domain.agent_out);
        route_agent(domain, env, out);
    }
}

fn agent_env(env: &Env<Limits>) -> Env<agent::Limits> {
    Env { now: env.now, wall: env.wall, limits: env.limits.agent }
}

fn receive_line(domain: &mut Domain, env: &Env<Limits>, text: Box<[u8]>, out: &mut Queue<Request>) {
    if text.len() > usize::try_from(env.limits.line_bytes).expect("u32 fits usize") {
        out.push(Request::Show { text: Box::from(&b"Line is too long"[..]) });
        return;
    }
    if domain.raw_lines.try_push(text).is_err() {
        out.push(Request::Show { text: Box::from(&b"Too many lines waiting"[..]) });
        return;
    }
    dispatch_line(domain, env, out);
}

fn dispatch_line(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if domain.line.is_some() || domain.chat.state_pending || domain.chat.closed {
        return;
    }
    match domain.chat.phase {
        Phase::Idle | Phase::Running => {}
        Phase::Loading | Phase::Starting | Phase::Ending | Phase::Done => return,
    }
    let Some(text) = domain.raw_lines.pop() else { return };
    let Some(next_message) = domain.chat.state.next_message.checked_add(1) else {
        stop_names(domain, out);
        return;
    };
    domain.chat.state.next_message = next_message;
    domain.line = Some(Line { name: Token::new(next_message), text });
    if domain.chat.phase == Phase::Idle {
        let Some(activation) = domain.chat.state.activation.checked_add(1) else {
            stop_names(domain, out);
            return;
        };
        domain.chat.state.activation = activation;
        domain.chat.phase = Phase::Starting;
        domain.chat.start_saved = false;
        domain.grant_failed = false;
        domain.grants.values = List::with_capacity(env.limits.agent.accounts);
        domain.grants.awaiting = u32::try_from(domain.config.accounts.len()).expect("validated account count");
        for account in &domain.config.accounts {
            out.push(Request::Credential { account: *account });
        }
    }
    domain.chat.state_pending = true;
    let fresh = domain.chat.phase == Phase::Starting && !domain.config.resume;
    out.push(Request::SaveState { state: domain.chat.state, fresh });
}

fn stop_names(domain: &mut Domain, out: &mut Queue<Request>) {
    domain.chat.phase = Phase::Done;
    out.push(Request::Show { text: Box::from(&b"Chat names exhausted"[..]) });
    out.push(Request::Exit { status: ExitStatus::Failed });
}

#[expect(clippy::manual_map, reason = "strict domain code uses an exhaustive match instead of a closure")]
fn loaded(
    domain: &mut Domain,
    env: &Env<Limits>,
    state: Option<ChatState>,
    transcript: Option<agent::Transcript>,
    delivery: Option<Box<DeliveryRecord>>,
    out: &mut Queue<Request>,
) {
    assert!(domain.chat.phase == Phase::Loading && domain.chat.load_requested, "Load has one terminal");
    domain.chat.state = state.unwrap_or(ChatState { activation: 0, next_message: 0, read: None });
    domain.transcript = transcript;
    domain.record = match delivery {
        Some(record) => Some(*record),
        None => None,
    };
    let pending = match &domain.record {
        Some(record) => match &record.state {
            DeliveryState::Intent(_) => true,
            DeliveryState::Answer(_) => false,
        },
        None => false,
    };
    if pending {
        domain.reconciling = Some(Reconcile::new(domain.record.as_ref().expect("pending intent").clone()));
        reconcile_next(domain, env, out);
        return;
    }
    domain.chat.phase = Phase::Idle;
    domain.chat.load_requested = false;
    if domain.chat.closed {
        domain.chat.phase = Phase::Done;
        out.push(Request::Exit { status: ExitStatus::Success });
    } else {
        dispatch_line(domain, env, out);
    }
}

fn state_saved(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    assert!(domain.chat.state_pending, "StateSaved answers one SaveState");
    domain.chat.state_pending = false;
    match domain.chat.phase {
        Phase::Starting => {
            domain.chat.start_saved = true;
            maybe_start(domain, env, out);
        }
        Phase::Running => {
            send_line(domain, env);
            dispatch_line(domain, env, out);
        }
        Phase::Loading | Phase::Idle | Phase::Ending | Phase::Done => {
            unreachable!("SaveState is emitted only while starting or running");
        }
    }
}

fn credential(domain: &mut Domain, env: &Env<Limits>, grant: agent::Grant, out: &mut Queue<Request>) {
    match domain.chat.phase {
        Phase::Starting => {
            assert!(domain.config.accounts.contains(&grant.name.account), "grant names a requested account");
            assert!(domain.grants.awaiting > 0, "one terminal per account request");
            domain.grants.awaiting = domain.grants.awaiting.checked_sub(1).expect("pending account");
            domain.grants.values.push(grant).expect("validated account capacity");
            maybe_start(domain, env, out);
        }
        Phase::Running | Phase::Ending => {
            agent::step(&mut domain.agent, &agent_env(env), agent::Event::Grant { grant }, &mut domain.agent_out);
        }
        Phase::Loading | Phase::Idle | Phase::Done => unreachable!("grant answers a live account request"),
    }
}

fn no_credential(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    match domain.chat.phase {
        Phase::Starting => {
            assert!(domain.grants.awaiting > 0, "one terminal per account request");
            domain.grants.awaiting = domain.grants.awaiting.checked_sub(1).expect("pending account");
            domain.grant_failed = true;
            maybe_start(domain, env, out);
        }
        Phase::Running | Phase::Ending => {
            out.push(Request::Show { text: Box::from(&b"A model account could not refresh"[..]) });
        }
        Phase::Loading | Phase::Idle | Phase::Done => unreachable!("missing grant answers a live account request"),
    }
}

fn maybe_start(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if domain.chat.phase != Phase::Starting || !domain.chat.start_saved || domain.grants.awaiting > 0 {
        return;
    }
    if domain.grant_failed {
        domain.chat.phase = Phase::Idle;
        domain.line = None;
        out.push(Request::Show { text: Box::from(&b"A model account is unavailable"[..]) });
        return;
    }
    let grants = mem::replace(&mut domain.grants.values, List::with_capacity(env.limits.agent.accounts)).into_boxed();
    let history = if domain.config.resume { domain.transcript.take() } else { None };
    domain.wall_deadline = Some(env.now.saturating_add(domain.config.budget.time));
    domain.last_turn = 0;
    agent::step(
        &mut domain.agent,
        &agent_env(env),
        agent::Event::Start {
            reply_to: ReplyTo::new(Token::new(domain.chat.state.activation)),
            host_run: Token::new(1),
            activation: domain.chat.state.activation,
            charter: charter(&domain.config),
            workspace: domain.config.workspace.clone(),
            transcript: history,
            grants,
        },
        &mut domain.agent_out,
    );
    domain.chat.phase = Phase::Running;
    observe(domain, Fact::Started { activation: domain.chat.state.activation });
}

fn send_line(domain: &mut Domain, env: &Env<Limits>) {
    let Some(run) = domain.run else { return };
    let Some(line) = domain.line.take() else { return };
    let text = crate::delivery::waking_text(domain.record.as_ref(), line.text, env.limits.agent.run.message_bytes);
    agent::step(
        &mut domain.agent,
        &agent_env(env),
        agent::Event::Message { run, name: line.name, text },
        &mut domain.agent_out,
    );
    observe(domain, Fact::Message { name: line.name });
}

fn turn_saved(domain: &mut Domain, number: u32, out: &mut Queue<Request>) {
    if domain.stop_failed {
        return;
    }
    let pending = domain.turns.unsaved.pop().expect("TurnSaved answers a pending SaveTurn");
    assert_eq!(number, pending, "turns become durable in order");
    if let Some(record) = &mut domain.record
        && number > record.after_turn
    {
        record.told = true;
    }
    if domain.turns.unsaved.is_empty() {
        finish_answer(domain, out);
    }
}

fn interrupt(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    match domain.chat.phase {
        Phase::Idle => {
            domain.chat.phase = Phase::Done;
            out.push(Request::Exit { status: ExitStatus::Success });
        }
        Phase::Running => {
            domain.chat.phase = Phase::Ending;
            domain.wall_deadline = None;
            if let Some(run) = domain.run {
                agent::step(&mut domain.agent, &agent_env(env), agent::Event::Cancel { run }, &mut domain.agent_out);
            }
        }
        Phase::Loading | Phase::Starting | Phase::Ending | Phase::Done => domain.chat.closed = true,
    }
}

fn closed(domain: &mut Domain, out: &mut Queue<Request>) {
    domain.chat.closed = true;
    if domain.chat.phase == Phase::Idle {
        domain.chat.phase = Phase::Done;
        out.push(Request::Exit { status: ExitStatus::Success });
    }
}

fn store_failed(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if domain.stop_failed {
        return;
    }
    domain.stop_failed = true;
    domain.chat.closed = true;
    domain.wall_deadline = None;
    domain.turns.unsaved = Queue::with_capacity(env.limits.unsaved);
    out.push(Request::Show { text: Box::from(&b"The chat could not be saved"[..]) });
    if let Some(record) = domain.saving.take() {
        let owner = domain.delivery_owner.take();
        match &record.state {
            DeliveryState::Answer(agent::run::Delivery::Delivered(_)) => out.push(Request::Show {
                text: Box::from(&b"A delivery committed, but its answer could not be saved"[..]),
            }),
            DeliveryState::Intent(_)
            | DeliveryState::Answer(
                agent::run::Delivery::Nothing
                | agent::run::Delivery::Refused(_)
                | agent::run::Delivery::Failed(_)
                | agent::run::Delivery::Stale,
            ) => {}
        }
        let delivery = match record.state {
            DeliveryState::Intent(_) => {
                agent::run::Delivery::Failed(agent::run::DeliveryFailure::new(0, agent::run::DeliveryReason::Broken))
            }
            DeliveryState::Answer(delivery) => delivery,
        };
        if let Some(owner) = owner {
            agent::step(
                &mut domain.agent,
                &agent_env(env),
                agent::Event::Delivered { owner, delivery },
                &mut domain.agent_out,
            );
        }
    }
    if let Some(run) = domain.run {
        agent::step(&mut domain.agent, &agent_env(env), agent::Event::Cancel { run }, &mut domain.agent_out);
        domain.chat.phase = Phase::Ending;
    } else {
        domain.chat.phase = Phase::Done;
        out.push(Request::Exit { status: ExitStatus::Failed });
    }
}

fn route_agent(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    while let Some(request) = domain.agent_out.pop() {
        match request {
            agent::Request::Admitted { host_run: _, run } => {
                domain.run = Some(run);
                send_line(domain, env);
            }
            agent::Request::Turn { host_run: _, number, read, spent: _, turn } => {
                if domain.stop_failed {
                    continue;
                }
                assert!(domain.turns.unsaved.room() > 0, "store backpressure bounds unsaved turns");
                let shown = crate::person::turn_text(&turn, env.limits.show_bytes);
                domain.turns.unsaved.push(number);
                domain.last_turn = number;
                domain.chat.state.read = read;
                observe(domain, Fact::Turn { number });
                out.push(Request::SaveTurn { number, read, turn });
                if !shown.is_empty() {
                    out.push(Request::Show { text: shown });
                }
            }
            agent::Request::Answer { to: _, answer } => {
                observe(domain, Fact::Answered { activation: domain.chat.state.activation });
                domain.turns.answer = Some(answer);
                if domain.turns.unsaved.is_empty() {
                    finish_answer(domain, out);
                }
            }
            agent::Request::Waiting { host_run: _, read: _ } => {
                out.push(Request::Show { text: Box::from(&b"Waiting for a message"[..]) });
            }
            agent::Request::Checking { host_run: _, deadline: _ } => {
                out.push(Request::Show { text: Box::from(&b"Running checks"[..]) });
            }
            agent::Request::Rejected { grant } => out.push(Request::Credential { account: grant.account }),
            agent::Request::Exhausted { account: _, retry_after: _ } => {
                out.push(Request::Show { text: Box::from(&b"A model account is exhausted"[..]) });
            }
            agent::Request::HostCall { .. } | agent::Request::WithdrawHost { .. } => {
                unreachable!("local chat grants no host tools");
            }
            agent::Request::Deliver { name, owner, change, deadline, .. } => {
                begin_delivery(domain, env, name, owner, change, deadline, out);
            }
            agent::Request::Cancel { owner } => {
                if !held_completion(domain, owner) {
                    out.push(Request::Agent(agent::Request::Cancel { owner }));
                }
            }
            forwarded @ (agent::Request::Complete { .. }
            | agent::Request::Io { .. }
            | agent::Request::CancelIo { .. }
            | agent::Request::Read { .. }
            | agent::Request::Probe { .. }
            | agent::Request::Check { .. }
            | agent::Request::Abort { .. }) => out.push(Request::Agent(forwarded)),
        }
    }
}

fn finish_answer(domain: &mut Domain, out: &mut Queue<Request>) {
    let Some(answer) = domain.turns.answer.take() else { return };
    if domain.stop_failed {
        domain.run = None;
        domain.chat.phase = Phase::Done;
        out.push(Request::Exit { status: ExitStatus::Failed });
        return;
    }
    observe(domain, Fact::Shown { activation: domain.chat.state.activation });
    let text = match answer {
        agent::run::Answer::Accepted { outcome, .. } => match outcome {
            agent::run::outcome::Declared::Report(report) => report.text,
            agent::run::outcome::Declared::Failure(failure) => failure.reason,
            agent::run::outcome::Declared::Verdict(verdict) => verdict.text,
            agent::run::outcome::Declared::Change(_) => Box::from(&b"Change delivered"[..]),
        },
        agent::run::Answer::Parked { .. } => Box::from(&b"Chat parked"[..]),
        agent::run::Answer::Refused(_) => Box::from(&b"The agent refused this run"[..]),
        agent::run::Answer::Failed { failure, .. } => match failure {
            agent::run::Failure::Cancelled => Box::from(&b"Run cancelled"[..]),
            agent::run::Failure::Transcript(_) => Box::from(&b"Saved transcript was refused"[..]),
            agent::run::Failure::Model(_)
            | agent::run::Failure::Budget(_)
            | agent::run::Failure::Policy(_)
            | agent::run::Failure::Stale => Box::from(&b"Run failed"[..]),
        },
    };
    out.push(Request::Show { text: crate::person::bounded_text(&text, domain.show_bytes) });
    domain.run = None;
    domain.wall_deadline = None;
    domain.line = None;
    if domain.chat.closed {
        domain.chat.phase = Phase::Done;
        let status = if domain.stop_failed { ExitStatus::Failed } else { ExitStatus::Success };
        out.push(Request::Exit { status });
    } else {
        domain.chat.phase = Phase::Loading;
        domain.chat.load_requested = true;
        out.push(Request::Load);
    }
}

fn held_completion(domain: &Domain, owner: Token) -> bool {
    for io in &domain.held {
        let pending = match io {
            AgentIo::Completed { owner: held, .. }
            | AgentIo::Failed { owner: held, .. }
            | AgentIo::Cancelled { owner: held } => *held,
            AgentIo::Done { .. }
            | AgentIo::Read { .. }
            | AgentIo::Probed { .. }
            | AgentIo::Checked { .. }
            | AgentIo::Aborted { .. } => unreachable!("only completion terminals are held"),
        };
        if pending == owner {
            return true;
        }
    }
    false
}

fn observe(domain: &mut Domain, fact: Fact) {
    if domain.facts.try_push(fact).is_err() {
        domain.facts_lost = domain.facts_lost.saturating_add(1);
    }
}

fn begin_delivery(
    domain: &mut Domain,
    env: &Env<Limits>,
    name: agent::run::CallName,
    owner: Token,
    change: agent::run::outcome::Change,
    deadline: Time,
    out: &mut Queue<Request>,
) {
    assert!(domain.delivery.is_none() && domain.saving.is_none(), "one delivery in flight");
    domain.landed = List::with_capacity(agent::run::MAX_DIRECTORIES);
    if let Some(delivery) = crate::delivery::cached(domain.record.as_ref(), name) {
        agent::step(
            &mut domain.agent,
            &agent_env(env),
            agent::Event::Delivered { owner, delivery },
            &mut domain.agent_out,
        );
        return;
    }
    let rules = match &domain.config.contract {
        crate::Contract::Change(spec) => &spec.fields,
        crate::Contract::Report(_) => match &domain.config.deliver {
            Some(spec) => &spec.fields,
            None => unreachable!("a Report delivery has a separate grant"),
        },
    };
    let Some(message) = commit_message(&change, &domain.config.title_field, rules) else {
        save_delivery(
            domain,
            name,
            owner,
            agent::run::Delivery::Failed(agent::run::DeliveryFailure::new(0, agent::run::DeliveryReason::TooLarge)),
            out,
        );
        return;
    };
    let Some(message) = crate::delivery::named_message(message, name) else {
        save_delivery(
            domain,
            name,
            owner,
            agent::run::Delivery::Failed(agent::run::DeliveryFailure::new(0, agent::run::DeliveryReason::TooLarge)),
            out,
        );
        return;
    };
    let in_place = InPlace::new(name, owner, deadline, message);
    advance_delivery(domain, env, in_place, out);
}

fn advance_delivery(domain: &mut Domain, env: &Env<Limits>, mut in_place: InPlace, out: &mut Queue<Request>) {
    let workspace = domain.config.workspace.as_ref().expect("admitted delivery has a workspace");
    match in_place.stage {
        Stage::Survey => {
            let start = usize::try_from(in_place.next).expect("bounded directory position");
            for (index, directory) in workspace.directories.iter().enumerate().skip(start) {
                if !directory.writable {
                    continue;
                }
                let position = u32::try_from(index).expect("admitted directory count fits u32");
                if env.now >= in_place.deadline {
                    delivery_failure(
                        domain,
                        in_place.name,
                        in_place.owner,
                        position,
                        agent::run::DeliveryReason::TimedOut,
                        out,
                    );
                    return;
                }
                in_place.next = position;
                let request = if directory.git {
                    in_place.step = Step::Status;
                    Request::Git {
                        owner: in_place.owner,
                        directory: position,
                        op: GitOp::Status,
                        deadline: in_place.deadline,
                    }
                } else {
                    in_place.step = Step::Plain;
                    Request::PlainStatus { owner: in_place.owner, directory: position, deadline: in_place.deadline }
                };
                domain.delivery = Some(in_place);
                out.push(request);
                return;
            }
            let mut entries = List::with_capacity(agent::run::MAX_DIRECTORIES);
            for entry in &in_place.directories {
                entries.push(entry.clone()).expect("admitted directory count");
            }
            let record = DeliveryRecord {
                name: in_place.name,
                state: DeliveryState::Intent(DeliveryIntent { directories: entries.into_boxed() }),
                landed: Box::new([]),
                after_turn: domain.last_turn,
                told: false,
            };
            domain.saving = Some(record.clone());
            domain.delivery_owner = Some(in_place.owner);
            domain.delivery = Some(in_place);
            out.push(Request::SaveDelivery { record: Box::new(record) });
        }
        Stage::Execute => {
            for index in 0..in_place.directories.len() {
                let entry = in_place.directories.get(index).expect("surveyed directory").clone();
                if entry.directory < in_place.next || !entry.changed {
                    continue;
                }
                if env.now >= in_place.deadline {
                    delivery_failure(
                        domain,
                        in_place.name,
                        in_place.owner,
                        entry.directory,
                        agent::run::DeliveryReason::TimedOut,
                        out,
                    );
                    return;
                }
                in_place.next = entry.directory;
                if entry.head.is_some() {
                    commit_request(domain, in_place, out);
                    return;
                }
                in_place.receipt(entry.directory, Box::from(&b"files kept"[..]));
                domain
                    .landed
                    .push(
                        agent::run::Receipt::new(entry.directory, Box::from(&b"files kept"[..]))
                            .expect("bounded receipt"),
                    )
                    .expect("admitted directories");
                in_place.next = entry.directory.checked_add(1).expect("admitted directory position");
            }
            let name = in_place.name;
            let owner = in_place.owner;
            save_delivery(domain, name, owner, in_place.result(), out);
        }
    }
}

fn plain_status(domain: &mut Domain, env: &Env<Limits>, owner: Token, changed: bool, out: &mut Queue<Request>) {
    let mut in_place = domain.delivery.take().expect("plain status answers one delivery operation");
    assert!(in_place.owner == owner && in_place.step == Step::Plain, "plain status matches the active request");
    if changed {
        in_place.directory(in_place.next, true, None);
    } else {
        in_place.directory(in_place.next, false, None);
    }
    in_place.next = in_place.next.checked_add(1).expect("admitted directory position");
    advance_delivery(domain, env, in_place, out);
}

#[expect(
    clippy::too_many_lines,
    reason = "one exhaustive git terminal transition owns status, markers, commit and push"
)]
fn git_result(domain: &mut Domain, env: &Env<Limits>, owner: Token, result: GitResult, out: &mut Queue<Request>) {
    if domain.reconciling.is_some() {
        reconcile_git(domain, env, owner, result, out);
        return;
    }
    let mut in_place = domain.delivery.take().expect("git terminal answers one delivery operation");
    assert_eq!(in_place.owner, owner, "git terminal matches active delivery");
    let directory = in_place.next;
    match in_place.step {
        Step::Status => match result {
            GitResult::Status { changed: false, merging: None, head } => {
                if head.is_empty() || head.len() > agent::run::Receipt::CAPACITY {
                    delivery_failure(
                        domain,
                        in_place.name,
                        owner,
                        directory,
                        agent::run::DeliveryReason::TooLarge,
                        out,
                    );
                    return;
                }
                in_place.directory(directory, false, Some(head));
                in_place.next = directory.checked_add(1).expect("admitted directory position");
                advance_delivery(domain, env, in_place, out);
            }
            GitResult::Status { changed: _, merging: Some(paths), head } => {
                let within_count =
                    paths.len() <= usize::try_from(env.limits.agent.run.conflicts).expect("u32 fits usize");
                let mut within_bytes = true;
                for path in &paths {
                    if path.len() > usize::try_from(env.limits.agent.run.conflict_path_bytes).expect("u32 fits usize") {
                        within_bytes = false;
                    }
                }
                if !within_count || !within_bytes {
                    delivery_failure(
                        domain,
                        in_place.name,
                        owner,
                        directory,
                        agent::run::DeliveryReason::TooLarge,
                        out,
                    );
                    return;
                }
                if head.is_empty() || head.len() > agent::run::Receipt::CAPACITY {
                    delivery_failure(
                        domain,
                        in_place.name,
                        owner,
                        directory,
                        agent::run::DeliveryReason::TooLarge,
                        out,
                    );
                    return;
                }
                in_place.marker_head = Some(head);
                in_place.step = Step::Markers;
                out.push(Request::Git { owner, directory, op: GitOp::Markers { paths }, deadline: in_place.deadline });
                domain.delivery = Some(in_place);
            }
            GitResult::Status { changed: true, merging: None, head } => {
                if head.is_empty() || head.len() > agent::run::Receipt::CAPACITY {
                    delivery_failure(
                        domain,
                        in_place.name,
                        owner,
                        directory,
                        agent::run::DeliveryReason::TooLarge,
                        out,
                    );
                    return;
                }
                in_place.directory(directory, true, Some(head));
                in_place.next = directory.checked_add(1).expect("admitted directory position");
                advance_delivery(domain, env, in_place, out);
            }
            GitResult::Failed { reason, diagnostic } => {
                delivery_failed(domain, in_place.name, owner, directory, reason, &diagnostic, out);
            }
            GitResult::Markers { .. }
            | GitResult::Committed { .. }
            | GitResult::Pushed
            | GitResult::Stale
            | GitResult::Inspected { .. } => {
                unreachable!("status awaits a status terminal")
            }
        },
        Step::Markers => match result {
            GitResult::Markers { first: None } => {
                let head = in_place.marker_head.take();
                in_place.directory(directory, true, head);
                in_place.next = directory.checked_add(1).expect("admitted directory position");
                advance_delivery(domain, env, in_place, out);
            }
            GitResult::Markers { first: Some(path) } => {
                let Some(marker) = agent::run::Marker::new(directory, path) else {
                    delivery_failure(
                        domain,
                        in_place.name,
                        owner,
                        directory,
                        agent::run::DeliveryReason::TooLarge,
                        out,
                    );
                    return;
                };
                let refusal =
                    agent::run::DeliveryRefusal::new(Some(marker), Box::from(&b"conflict markers remain"[..]))
                        .expect("bounded host explanation");
                save_delivery(domain, in_place.name, owner, agent::run::Delivery::Refused(refusal), out);
            }
            GitResult::Failed { reason, diagnostic } => {
                delivery_failed(domain, in_place.name, owner, directory, reason, &diagnostic, out);
            }
            GitResult::Status { .. }
            | GitResult::Committed { .. }
            | GitResult::Pushed
            | GitResult::Stale
            | GitResult::Inspected { .. } => {
                unreachable!("markers await a marker terminal")
            }
        },
        Step::Commit => match result {
            GitResult::Committed { receipt } => {
                if receipt.is_empty() || receipt.len() > agent::run::Receipt::CAPACITY {
                    delivery_failure(
                        domain,
                        in_place.name,
                        owner,
                        directory,
                        agent::run::DeliveryReason::TooLarge,
                        out,
                    );
                    return;
                }
                in_place.receipt(directory, receipt.clone());
                domain
                    .landed
                    .push(agent::run::Receipt::new(directory, receipt).expect("bounded receipt"))
                    .expect("admitted directories");
                let target = match &domain.config.push {
                    Some(targets) => match targets.get(usize::try_from(directory).expect("bounded directory position"))
                    {
                        Some(target) => target.as_ref(),
                        None => None,
                    },
                    None => None,
                };
                if let Some(target) = target {
                    in_place.step = Step::Push;
                    out.push(Request::Git {
                        owner,
                        directory,
                        op: GitOp::Push { remote: target.remote.clone(), branch: target.branch.clone() },
                        deadline: in_place.deadline,
                    });
                    domain.delivery = Some(in_place);
                } else {
                    in_place.next = directory.checked_add(1).expect("admitted directory position");
                    advance_delivery(domain, env, in_place, out);
                }
            }
            GitResult::Failed { reason, diagnostic } => {
                delivery_failed(domain, in_place.name, owner, directory, reason, &diagnostic, out);
            }
            GitResult::Status { .. }
            | GitResult::Markers { .. }
            | GitResult::Pushed
            | GitResult::Stale
            | GitResult::Inspected { .. } => {
                unreachable!("commit awaits a commit terminal")
            }
        },
        Step::Push => match result {
            GitResult::Pushed => {
                in_place.next = directory.checked_add(1).expect("admitted directory position");
                advance_delivery(domain, env, in_place, out);
            }
            GitResult::Stale => save_delivery(domain, in_place.name, owner, agent::run::Delivery::Stale, out),
            GitResult::Failed { reason, diagnostic } => {
                delivery_failed(domain, in_place.name, owner, directory, reason, &diagnostic, out);
            }
            GitResult::Status { .. }
            | GitResult::Markers { .. }
            | GitResult::Committed { .. }
            | GitResult::Inspected { .. } => {
                unreachable!("push awaits a push terminal")
            }
        },
        Step::Plain => unreachable!("git terminal cannot answer a plain status"),
    }
}

fn commit_request(domain: &mut Domain, mut in_place: InPlace, out: &mut Queue<Request>) {
    in_place.step = Step::Commit;
    out.push(Request::Git {
        owner: in_place.owner,
        directory: in_place.next,
        op: GitOp::Commit { message: in_place.message.clone() },
        deadline: in_place.deadline,
    });
    domain.delivery = Some(in_place);
}

fn delivery_failure(
    domain: &mut Domain,
    name: agent::run::CallName,
    owner: Token,
    directory: u32,
    reason: agent::run::DeliveryReason,
    out: &mut Queue<Request>,
) {
    delivery_failed(domain, name, owner, directory, reason, &agent::run::Diagnostic::empty(), out);
}

fn delivery_failed(
    domain: &mut Domain,
    name: agent::run::CallName,
    owner: Token,
    directory: u32,
    reason: agent::run::DeliveryReason,
    diagnostic: &agent::run::Diagnostic,
    out: &mut Queue<Request>,
) {
    save_delivery(
        domain,
        name,
        owner,
        agent::run::Delivery::Failed(agent::run::DeliveryFailure { directory, reason, diagnostic: *diagnostic }),
        out,
    );
}

fn save_delivery(
    domain: &mut Domain,
    name: agent::run::CallName,
    owner: Token,
    delivery: agent::run::Delivery,
    out: &mut Queue<Request>,
) {
    let mut landed = List::with_capacity(agent::run::MAX_DIRECTORIES);
    for receipt in &domain.landed {
        landed.push(receipt.clone()).expect("admitted directories");
    }
    let record = DeliveryRecord {
        name,
        state: DeliveryState::Answer(delivery),
        landed: landed.into_boxed(),
        after_turn: domain.last_turn,
        told: false,
    };
    assert!(domain.saving.is_none(), "one delivery store operation in flight");
    domain.saving = Some(record.clone());
    domain.delivery_owner = Some(owner);
    domain.delivery = None;
    out.push(Request::SaveDelivery { record: Box::new(record) });
}

fn delivery_saved(domain: &mut Domain, env: &Env<Limits>, name: agent::run::CallName, out: &mut Queue<Request>) {
    let record = domain.saving.take().expect("DeliverySaved answers one SaveDelivery");
    assert_eq!(record.name, name, "delivery store terminal names the decision");
    domain.record = Some(record.clone());
    match record.state {
        DeliveryState::Intent(_) => {
            let mut in_place = domain.delivery.take().expect("saved intent retains delivery");
            in_place.stage = Stage::Execute;
            in_place.next = 0;
            advance_delivery(domain, env, in_place, out);
        }
        DeliveryState::Answer(delivery) => {
            if let Some(owner) = domain.delivery_owner.take() {
                observe(domain, Fact::DeliveryReturned { name });
                agent::step(
                    &mut domain.agent,
                    &agent_env(env),
                    agent::Event::Delivered { owner, delivery },
                    &mut domain.agent_out,
                );
            } else {
                domain.chat.phase = Phase::Idle;
                domain.chat.load_requested = false;
                if domain.chat.closed {
                    domain.chat.phase = Phase::Done;
                    out.push(Request::Exit { status: ExitStatus::Success });
                } else {
                    dispatch_line(domain, env, out);
                }
            }
        }
    }
}

fn reconcile_next(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    let mut reconcile = domain.reconciling.take().expect("loaded intent is being reconciled");
    let start = usize::try_from(reconcile.next).expect("bounded directory position");
    for index in start..reconcile.intent().directories.len() {
        let entry = reconcile.intent().directories.get(index).expect("position in saved intent").clone();
        reconcile.next = u32::try_from(index).expect("admitted directory count");
        if !entry.changed {
            continue;
        }
        match entry.head {
            Some(_) => {
                let name = reconcile.record.name;
                domain.reconciling = Some(reconcile);
                out.push(Request::Git {
                    owner: Token::new(0),
                    directory: entry.directory,
                    op: GitOp::Inspect { name },
                    deadline: env.now.saturating_add(domain.config.budget.time),
                });
                return;
            }
            None => reconcile.receipt(entry.directory, Box::from(&b"files kept"[..])),
        }
    }
    reconcile_answer(domain, reconcile, None, out);
}

fn reconcile_git(domain: &mut Domain, env: &Env<Limits>, owner: Token, result: GitResult, out: &mut Queue<Request>) {
    assert_eq!(owner, Token::new(0), "recovery inspection has its own owner");
    let mut reconcile = domain.reconciling.take().expect("inspection answers a loaded intent");
    let entry = reconcile
        .intent()
        .directories
        .get(usize::try_from(reconcile.next).expect("bounded position"))
        .expect("position in saved intent")
        .clone();
    let before = entry.head.expect("inspection applies to a git directory");
    match result {
        GitResult::Inspected { head, named } => {
            if head != before && named && !head.is_empty() && head.len() <= agent::run::Receipt::CAPACITY - 7 {
                let mut text =
                    List::with_capacity(u32::try_from(agent::run::Receipt::CAPACITY).expect("fixed receipt bound"));
                for byte in b"commit ".iter().chain(head.iter()) {
                    text.push(*byte).expect("bounded receipt");
                }
                reconcile.receipt(entry.directory, text.into_boxed());
                reconcile.next = reconcile.next.checked_add(1).expect("admitted position");
                domain.reconciling = Some(reconcile);
                reconcile_next(domain, env, out);
            } else {
                reconcile_answer(domain, reconcile, Some(entry.directory), out);
            }
        }
        GitResult::Failed { .. } => reconcile_answer(domain, reconcile, Some(entry.directory), out),
        GitResult::Status { .. }
        | GitResult::Markers { .. }
        | GitResult::Committed { .. }
        | GitResult::Pushed
        | GitResult::Stale => {
            unreachable!("recovery awaits one inspection terminal")
        }
    }
}

fn reconcile_answer(domain: &mut Domain, reconcile: Reconcile, interrupted: Option<u32>, out: &mut Queue<Request>) {
    let mut landed = List::with_capacity(agent::run::MAX_DIRECTORIES);
    for receipt in &reconcile.receipts {
        landed.push(receipt.clone()).expect("admitted directories");
    }
    let answer = match interrupted {
        None => {
            let receipts = reconcile.receipts.into_boxed();
            if receipts.is_empty() {
                agent::run::Delivery::Nothing
            } else {
                agent::run::Delivery::Delivered(
                    agent::run::Delivered::new(receipts).expect("ordered unique directories"),
                )
            }
        }
        Some(directory) => {
            let mut text = List::with_capacity(512);
            crate::delivery::append_bounded(&mut text, b"delivery interrupted; committed: ");
            if reconcile.receipts.is_empty() {
                crate::delivery::append_bounded(&mut text, b"none");
            } else {
                for receipt in &reconcile.receipts {
                    crate::delivery::append_bounded(&mut text, b"directory ");
                    crate::delivery::decimal(&mut text, u64::from(receipt.directory()));
                    crate::delivery::append_bounded(&mut text, b": ");
                    crate::delivery::append_bounded(&mut text, receipt.text());
                    crate::delivery::append_bounded(&mut text, b"; ");
                }
            }
            agent::run::Delivery::Failed(agent::run::DeliveryFailure {
                directory,
                reason: agent::run::DeliveryReason::Broken,
                diagnostic: agent::run::Diagnostic::new(&text.into_boxed(), 0),
            })
        }
    };
    let record = DeliveryRecord {
        name: reconcile.record.name,
        state: DeliveryState::Answer(answer),
        landed: landed.into_boxed(),
        after_turn: reconcile.record.after_turn,
        told: false,
    };
    domain.saving = Some(record.clone());
    out.push(Request::SaveDelivery { record: Box::new(record) });
}
