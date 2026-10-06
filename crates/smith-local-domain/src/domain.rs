//! Local chat transitions and agent composition (domain/host.md, sections 2,
//! 6 and 8–9). The agent's IO requests pass through unchanged. The host saves
//! names before use and holds an answer until every turn has a store terminal.

use alloc::boxed::Box;
use core::mem;
use skein_lib::{Env, List, Queue, ReplyTo, Time, Token};
use smith_domain as agent;

use crate::boundary::{AgentIo, ChatState, Event, ExitStatus, Request};
use crate::chat::{Chat, Phase};
use crate::credentials::Grants;
use crate::person::Line;
use crate::turns::Turns;
use crate::{Config, Invalid, Limits, charter};

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
        Event::Loaded { state, transcript } => loaded(domain, env, state, transcript, out),
        Event::StateSaved => state_saved(domain, env, out),
        Event::TurnSaved { number } => turn_saved(domain, number, out),
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

fn loaded(
    domain: &mut Domain,
    env: &Env<Limits>,
    state: Option<ChatState>,
    transcript: Option<agent::Transcript>,
    out: &mut Queue<Request>,
) {
    assert!(domain.chat.phase == Phase::Loading && domain.chat.load_requested, "Load has one terminal");
    domain.chat.state = state.unwrap_or(ChatState { activation: 0, next_message: 0, read: None });
    domain.transcript = transcript;
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
}

fn send_line(domain: &mut Domain, env: &Env<Limits>) {
    let Some(run) = domain.run else { return };
    let Some(line) = domain.line.take() else { return };
    agent::step(
        &mut domain.agent,
        &agent_env(env),
        agent::Event::Message { run, name: line.name, text: line.text },
        &mut domain.agent_out,
    );
}

fn turn_saved(domain: &mut Domain, number: u32, out: &mut Queue<Request>) {
    let pending = domain.turns.unsaved.pop().expect("TurnSaved answers a pending SaveTurn");
    assert_eq!(number, pending, "turns become durable in order");
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
    domain.stop_failed = true;
    domain.chat.closed = true;
    out.push(Request::Show { text: Box::from(&b"The chat could not be saved"[..]) });
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
                assert!(domain.turns.unsaved.room() > 0, "store backpressure bounds unsaved turns");
                let shown = crate::person::turn_text(&turn, env.limits.show_bytes);
                domain.turns.unsaved.push(number);
                domain.chat.state.read = read;
                out.push(Request::SaveTurn { number, read, turn });
                if !shown.is_empty() {
                    out.push(Request::Show { text: shown });
                }
            }
            agent::Request::Answer { to: _, answer } => {
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
            agent::Request::HostCall { .. } | agent::Request::WithdrawHost { .. } | agent::Request::Deliver { .. } => {
                unreachable!("local chat grants no host tools or delivery");
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
