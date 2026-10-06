//! V2 host world: scripted parent, agent channel and contained tree, driven by
//! skein's Stage, Schedule, Ledger and Trace (testing-strategy.md, sections 2.2,
//! 6 and 7; domain/host.md, section 10). Observations, never domain cells, prove
//! exact terminals, parent-owned turns and no Gone before tree/EOF/rights.
use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Ledger, Schedule, Stage, Trace};
use smith_host_domain::{self as host, Answer, Down, End, Event, Fault, Grant, Limits, Request, Start, Up};
use std::collections::{BTreeMap, BTreeSet};

/// Distinct lower rights retained even after the process exits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lower {
    /// Process admission terminal.
    Spawn,
    /// Serialized channel write terminal.
    Send,
    /// One demand-driven channel read terminal.
    Read,
    /// Process-exited proof.
    Wait,
    /// Empty-tree proof; distinct from process exit.
    Reap,
    /// First polite tree signal terminal.
    Terminate,
    /// Final tree signal terminal.
    Kill,
}

/// Boundary-only state of one physical agent; no access to domain internals.
#[derive(Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent actual tree, channel and parent observations are outside the domain state machine"
)]
pub struct Seen {
    /// Parent handle, available only after Started.
    pub agent: Option<Token>,
    /// Actual lower owner, available to the tree before Started.
    pub owner: Option<Token>,
    /// Agent admission record, independently from Started.
    pub admitted: bool,
    /// One accepted last word.
    pub answer: Option<Answer>,
    /// One failure while the run was untold.
    pub fault: Option<Fault>,
    /// Physical containment terminal.
    pub gone: Option<End>,
    /// Actual bounded operator suffix retained from the Gone terminal.
    pub gone_detail: Option<Box<[u8]>>,
    /// Actual process-exited observation.
    pub exited: bool,
    /// Actual empty-tree observation.
    pub empty: bool,
    /// Actual EOF read terminal.
    pub eof: bool,
    /// Calls whose operation terminal still belongs to the parent.
    pub calls: BTreeSet<Token>,
    /// Turn payload bytes owned by the parent until its exact ACK.
    pub turns: BTreeMap<u32, Box<[u8]>>,
    /// Actual parent Turn metadata: sequence, scalar prefix and independent
    /// currency/raw overflow attestations. ACK never erases these observations.
    /// Contract: domain/host.md, section 6; testing-strategy.md, sections 6 and 7.
    pub turn_metadata: Vec<(u32, u64)>,
    /// Parent notifications after withdrawal, which never consume a call right.
    pub withdrawals: BTreeSet<Token>,
    /// Downlink words handed to the agent by the scripted transport.
    pub down: Vec<Down>,
    /// Message names bounced by the kit.
    pub bounces: Vec<host::Bounce>,
    /// Notices the parent actually heard.
    pub rejected: Vec<(u32, u64)>,
    /// Generic facts forwarded without interpretation.
    pub told: u32,
    /// One-way process signals, in actual order.
    pub signals: Vec<host::Signal>,
}

/// Real host domain and generic harness machinery, with a single scripted tree.
/// Every input goes through the same room-first step/fire path; every lower
/// terminal consumes its independent ledger before the domain sees it.
#[derive(Debug)]
pub struct World {
    /// Deterministic seed is recorded in the trace.
    pub seed: u64,
    /// Real domain under test.
    pub domain: host::Domain,
    /// Bounded stage, injected clock and handed outputs.
    pub stage: Stage<Limits, Event, Request>,
    /// Shared deterministic scheduler for raced terminals.
    pub schedule: Schedule<Event>,
    /// Independent exactly-once lower terminal accounting.
    pub lower: Ledger<Lower, Token>,
    /// Observable parent, agent and tree state.
    pub seen: Seen,
    /// Shared replay trace of boundary observations.
    pub trace: Trace,
}

/// Small valid V2 capacities; every sealed actual delivery fits the reply cap.
#[must_use]
pub fn limits() -> Limits {
    Limits {
        agents: 1,
        directories: 2,
        conflicts: 2,
        path_bytes: 64,
        name_bytes: 64,
        accounts: 2,
        charter_bytes: 256,
        transcript_bytes: 256,
        answered_bytes: 256,
        message_bytes: 64,
        messages: 3,
        calls: 2,
        call_bytes: 128,
        answer_bytes: host::Delivered::worst_case(),
        turns: 2,
        turn_bytes: 64,
        unacknowledged_bytes: 128,
        fact_bytes: 64,
        outcome_bytes: 128,
        detail_bytes: 32,
        spawn_timeout: Duration::from_secs(2),
        no_progress: Duration::from_secs(10),
        long_span: Duration::from_secs(120),
        wall_time: Duration::from_secs(900),
        grace: Duration::from_secs(5),
        kill_after: Duration::from_secs(2),
        facts: 16,
    }
}

/// Opaque V2 source cells, optional workspace and credential names only.
#[must_use]
pub fn start() -> Start {
    Start {
        logical_run: Token::new(7),
        activation: 1,
        workspace: None,
        charter: Box::from(&b"charter"[..]),
        transcript: Some(Box::from(&b"transcript"[..])),
        answered: Box::from(&b"post-transcript answers"[..]),
        directories: Box::new([]),
        grants: Box::new([Grant { account: 1, generation: 1, valid: Duration::from_secs(60) }]),
    }
}

impl World {
    /// Construct only the domain and bounded stage; no generic allocator here.
    #[must_use]
    pub fn new(seed: u64, bounds: Limits) -> Self {
        let mut trace = Trace::default();
        trace.log(Time::ZERO, format!("seed {seed}"));
        Self {
            seed,
            domain: host::Domain::new(&bounds),
            stage: Stage::new(bounds, host::max_out(&bounds), host::max_out(&bounds) + 2),
            schedule: Schedule::new(),
            lower: Ledger::new("host lower operation"),
            seen: Seen::default(),
            trace,
        }
    }

    /// Deliver one typed input, with actual lower accounting and room-first flow.
    pub fn event(&mut self, event: Event) {
        self.observe_terminal(&event);
        self.trace.log(self.stage.env.now, format!("up {event:?}"));
        self.stage.push(event);
        while let Some(event) = self.stage.next_event() {
            host::step(&mut self.domain, &self.stage.env, event, &mut self.stage.out);
            self.outputs();
        }
        self.domain.reclaim();
    }

    /// Advance the injected clock; schedule deliveries precede same-time timers.
    pub fn at(&mut self, seconds: u64) {
        let now = Time::ZERO.saturating_add(Duration::from_secs(seconds));
        assert!(now >= self.stage.env.now);
        self.stage.tick(now);
        while let Some(event) = self.schedule.next(now) {
            self.event(event);
        }
        while self.domain.is_due(now) {
            assert!(self.stage.has_room());
            host::fire(&mut self.domain, &self.stage.env, &mut self.stage.out);
            self.outputs();
        }
        self.domain.reclaim();
    }

    /// Parent spawns the source run; tree owner exists before parent handle.
    pub fn spawn(&mut self, start: Start) {
        self.event(Event::Spawn { client: Token::new(1), start });
    }

    /// Actual process Started, distinct from agent admission.
    pub fn spawned(&mut self) {
        let owner = self.owner();
        self.event(Event::Spawned { owner, process: Token::new(9) });
    }

    /// Actual completion of the currently issued channel send.
    pub fn sent(&mut self) {
        self.event(Event::Sent { owner: self.owner() });
    }

    /// Actual channel message on one previously demanded read.
    pub fn up(&mut self, message: Up) {
        self.event(Event::Received { owner: self.owner(), message });
    }

    /// Start, finish its Send terminal and admit through the actual read path.
    pub fn live(&mut self) {
        self.spawn(start());
        self.spawned();
        self.sent();
        self.up(Up::Admitted);
    }

    /// Opaque parent handle only after Started.
    #[must_use]
    pub fn agent(&self) -> Token {
        self.seen.agent.expect("parent learned Started")
    }

    /// Lower owner learned from actual Spawn request.
    #[must_use]
    pub fn owner(&self) -> Token {
        self.seen.owner.expect("tree learned Spawn")
    }

    /// Settle physical exit, empty tree and EOF in that order. Pending sends,
    /// signals, calls and turns deliberately remain owed to their real owners.
    pub fn cleanup(&mut self) {
        if self.lower.contains(Lower::Send) {
            self.sent();
        }
        let owner = self.owner();
        if self.lower.contains(Lower::Wait) {
            self.event(Event::Exited { owner });
        }
        if self.lower.contains(Lower::Reap) {
            self.event(Event::Reaped { owner, detail: Box::from(&b"bounded operator tail"[..]) });
        }
        if self.lower.contains(Lower::Read) {
            self.event(Event::Hangup { owner });
        }
        for signal in [Lower::Terminate, Lower::Kill] {
            if self.lower.contains(signal) {
                self.event(Event::Signalled { owner });
            }
        }
    }

    /// Final independent quiescence checks (testing-strategy.md, section 6).
    pub fn settled(&self) {
        self.lower.assert_settled();
        assert!(self.seen.calls.is_empty(), "every real parent call right returned");
        assert!(self.seen.turns.is_empty(), "every parent-owned turn committed");
        assert!(self.seen.gone.is_some());
        assert_eq!(self.domain.agents(), 0);
        assert_eq!(self.domain.next_deadline(), None);
        assert!(self.schedule.is_empty());
    }

    fn observe_terminal(&mut self, event: &Event) {
        let key = match event {
            Event::Spawned { .. } | Event::Unspawned { .. } => Some(Lower::Spawn),
            Event::Sent { .. } | Event::Unsent { .. } => Some(Lower::Send),
            Event::Received { .. } | Event::Malformed { .. } | Event::Hangup { .. } => Some(Lower::Read),
            Event::Exited { .. } => {
                self.seen.exited = true;
                Some(Lower::Wait)
            }
            Event::Reaped { .. } => {
                assert!(self.seen.exited);
                self.seen.empty = true;
                Some(Lower::Reap)
            }
            Event::Signalled { .. } => {
                if self.lower.contains(Lower::Terminate) {
                    Some(Lower::Terminate)
                } else {
                    Some(Lower::Kill)
                }
            }
            Event::Answer { call, .. } => {
                assert!(self.seen.calls.remove(call), "actual parent operation right");
                None
            }
            Event::Acknowledge { turn, .. } => {
                self.seen.turns.remove(turn);
                None
            }
            Event::Spawn { .. } | Event::Message { .. } | Event::Grant { .. } | Event::Stop { .. } => None,
        };
        if let Some(key) = key {
            self.lower.end(key);
        }
        match event {
            Event::Hangup { .. } | Event::Malformed { .. } => self.seen.eof = true,
            Event::Spawn { .. }
            | Event::Spawned { .. }
            | Event::Unspawned { .. }
            | Event::Message { .. }
            | Event::Answer { .. }
            | Event::Acknowledge { .. }
            | Event::Grant { .. }
            | Event::Stop { .. }
            | Event::Sent { .. }
            | Event::Unsent { .. }
            | Event::Received { .. }
            | Event::Signalled { .. }
            | Event::Exited { .. }
            | Event::Reaped { .. } => {}
        }
    }

    fn outputs(&mut self) {
        while let Some(request) = self.stage.out.pop() {
            self.trace.log(self.stage.env.now, format!("down {request:?}"));
            match request {
                Request::Spawn { owner, .. } => {
                    self.seen.owner = Some(owner);
                    self.lower.open(Lower::Spawn, owner);
                }
                Request::Started { agent, .. } => {
                    assert!(self.seen.agent.replace(agent).is_none());
                }
                Request::Admitted { .. } => {
                    assert!(!self.seen.admitted);
                    self.seen.admitted = true;
                }
                Request::Called { call, .. } => {
                    assert!(self.seen.calls.insert(call));
                }
                Request::Withdrawn { call, .. } => {
                    assert!(self.seen.calls.contains(&call));
                    assert!(self.seen.withdrawals.insert(call));
                }
                Request::Turn { turn, .. } => {
                    self.seen.turn_metadata.push((turn.number, turn.spent));
                    assert!(self.seen.turns.insert(turn.number, turn.body).is_none());
                }
                Request::Answered { answer, .. } => {
                    assert!(self.seen.fault.is_none());
                    assert!(self.seen.answer.replace(answer).is_none());
                }
                Request::Faulted { fault, .. } => {
                    assert!(self.seen.answer.is_none());
                    assert!(self.seen.fault.replace(fault).is_none());
                }
                Request::Gone { end, detail, .. } => {
                    if end == End::Stopped {
                        assert!(
                            self.seen.exited && self.seen.empty && self.seen.eof,
                            "Gone needs all three real lower proofs"
                        );
                        self.lower.assert_settled();
                        assert!(self.seen.calls.is_empty());
                        assert!(self.seen.turns.is_empty());
                    }
                    assert!(self.seen.gone.replace(end).is_none());
                    assert!(self.seen.gone_detail.replace(detail).is_none());
                }
                Request::Send { owner, message, .. } => {
                    self.lower.open(Lower::Send, owner);
                    self.seen.down.push(message);
                }
                Request::Read { owner, .. } => self.lower.open(Lower::Read, owner),
                Request::Wait { owner, .. } => self.lower.open(Lower::Wait, owner),
                Request::Reap { owner, .. } => self.lower.open(Lower::Reap, owner),
                Request::Signal { owner, signal, .. } => {
                    self.seen.signals.push(signal);
                    let key = match signal {
                        host::Signal::Terminate => Lower::Terminate,
                        host::Signal::Kill => Lower::Kill,
                    };
                    self.lower.open(key, owner);
                }
                Request::Bounced { bounce, .. } => self.seen.bounces.push(bounce),
                Request::Rejected { account, generation, .. } => self.seen.rejected.push((account, generation)),
                Request::Told { .. } => self.seen.told += 1,
                Request::Waiting { .. } | Request::Exhausted { .. } => {}
            }
        }
    }
}
