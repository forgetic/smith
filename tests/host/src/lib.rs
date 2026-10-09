//! V2 host world: scripted parent, agent channel and contained tree, driven by
//! skein's Stage, Schedule, Ledger and Trace (testing-strategy.md, sections 2.2,
//! 6 and 7; domain/host.md, section 10). Observations, never domain cells, prove
//! exact terminals, parent-owned turns and no Gone before tree/EOF/rights.
pub mod inline;

use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Ledger, Schedule, Stage, Trace};
use smith_host_domain::{self as host, Answer, Down, End, Fault, Grant, Input, Limits, Output, Start, Up};
use smith_host_domain::{parent, process};
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
    pub turns: BTreeMap<u32, host::TurnValue>,
    /// Last read fence observed in an actual parent Turn or Waiting notice.
    pub read: Option<Token>,
    /// Actual parent Turn metadata: sequence, scalar prefix and independent
    /// currency/raw overflow attestations. ACK never erases these observations.
    /// Contract: domain/host.md, section 6; testing-strategy.md, sections 6 and 7.
    pub turn_metadata: Vec<(u32, u64)>,
    /// Parent notifications after withdrawal, which never consume a call right.
    pub withdrawals: BTreeSet<Token>,
    /// Downlink words handed to the agent by the scripted transport.
    pub down: Vec<Down>,
    /// Message names bounced by the kit.
    pub bounces: Vec<host::MessageRefusal>,
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
    pub stage: Stage<Limits, Input, Output>,
    /// Shared deterministic scheduler for raced terminals.
    pub schedule: Schedule<Input>,
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
        transcript_bytes: 1024,
        answered_bytes: 256,
        message_bytes: 64,
        messages: 3,
        calls: 2,
        call_bytes: 128,
        answer_bytes: host::Delivered::worst_case(),
        turns: 2,
        turn_bytes: 512,
        unacknowledged_bytes: 1024,
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
        messages: Box::default(),
        logical_run: Token::new(7),
        activation: 1,
        workspace: None,
        charter: charter_value(7),
        transcript: Some(transcript_value(512)),
        answered: Box::new([smith_host_domain::AnsweredCall {
            name: smith_host_domain::CallName { activation: 1, completion: 1, position: 0 },
            tool: Box::from(&b"tool"[..]),
            reply: smith_host_domain::SavedReply::Host {
                error: false,
                body: Box::from(&b"post-transcript answers"[..]),
            },
        }]),
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
    pub fn event(&mut self, event: Input) {
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
        self.event(Input::Parent(parent::Event::Spawn { client: Token::new(1), start }));
    }

    /// Actual process Started, distinct from agent admission.
    pub fn spawned(&mut self) {
        let owner = self.owner();
        self.event(Input::Process(process::Event::Spawned { owner, process: Token::new(9) }));
    }

    /// Actual completion of the currently issued channel send.
    pub fn sent(&mut self) {
        self.event(Input::Process(process::Event::Sent { owner: self.owner() }));
    }

    /// Actual channel message on one previously demanded read.
    pub fn up(&mut self, message: Up) {
        self.event(Input::Process(process::Event::Received { owner: self.owner(), message }));
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
    /// signals, calls and turns deliberately remain owed to their owners.
    pub fn cleanup(&mut self) {
        if self.lower.contains(Lower::Send) {
            self.sent();
        }
        let owner = self.owner();
        if self.lower.contains(Lower::Wait) {
            self.event(Input::Process(process::Event::Exited { owner }));
        }
        if self.lower.contains(Lower::Reap) {
            self.event(Input::Process(process::Event::Reaped {
                owner,
                detail: Box::from(&b"bounded operator tail"[..]),
            }));
        }
        if self.lower.contains(Lower::Read) {
            self.event(Input::Process(process::Event::Hangup { owner }));
        }
        for signal in [Lower::Terminate, Lower::Kill] {
            if self.lower.contains(signal) {
                self.event(Input::Process(process::Event::Signalled { owner }));
            }
        }
    }

    /// Final independent quiescence checks (testing-strategy.md, section 6).
    pub fn settled(&self) {
        self.lower.assert_settled();
        assert!(self.seen.calls.is_empty(), "every parent call right returned");
        assert!(self.seen.turns.is_empty(), "every parent-owned turn committed");
        assert!(self.seen.gone.is_some());
        assert_eq!(self.domain.agents(), 0);
        assert_eq!(self.domain.next_deadline(), None);
        assert!(self.schedule.is_empty());
    }

    fn observe_terminal(&mut self, event: &Input) {
        let key = match event {
            Input::Process(process::Event::Spawned { .. } | process::Event::Unspawned { .. }) => Some(Lower::Spawn),
            Input::Process(process::Event::Sent { .. } | process::Event::Unsent { .. }) => Some(Lower::Send),
            Input::Process(
                process::Event::Received { .. } | process::Event::Malformed { .. } | process::Event::Hangup { .. },
            ) => Some(Lower::Read),
            Input::Process(process::Event::Exited { .. }) => {
                self.seen.exited = true;
                Some(Lower::Wait)
            }
            Input::Process(process::Event::Reaped { .. }) => {
                assert!(self.seen.exited);
                self.seen.empty = true;
                Some(Lower::Reap)
            }
            Input::Process(process::Event::Signalled { .. }) => {
                if self.lower.contains(Lower::Terminate) {
                    Some(Lower::Terminate)
                } else {
                    Some(Lower::Kill)
                }
            }
            Input::Parent(parent::Event::Answer { call, .. }) => {
                assert!(self.seen.calls.remove(call), "actual parent operation right");
                None
            }
            Input::Parent(parent::Event::Acknowledge { turn, .. }) => {
                self.seen.turns.remove(turn);
                None
            }
            Input::Parent(
                parent::Event::Spawn { .. }
                | parent::Event::Message { .. }
                | parent::Event::Grant { .. }
                | parent::Event::Stop { .. },
            ) => None,
        };
        if let Some(key) = key {
            self.lower.end(key);
        }
        match event {
            Input::Process(process::Event::Hangup { .. } | process::Event::Malformed { .. }) => self.seen.eof = true,
            Input::Parent(
                parent::Event::Spawn { .. }
                | parent::Event::Message { .. }
                | parent::Event::Answer { .. }
                | parent::Event::Acknowledge { .. }
                | parent::Event::Grant { .. }
                | parent::Event::Stop { .. },
            )
            | Input::Process(
                process::Event::Spawned { .. }
                | process::Event::Unspawned { .. }
                | process::Event::Sent { .. }
                | process::Event::Unsent { .. }
                | process::Event::Received { .. }
                | process::Event::Signalled { .. }
                | process::Event::Exited { .. }
                | process::Event::Reaped { .. },
            ) => {}
        }
    }

    fn outputs(&mut self) {
        while let Some(request) = self.stage.out.pop() {
            self.trace.log(self.stage.env.now, format!("down {request:?}"));
            match request {
                Output::Process(process::Request::Spawn { owner, .. }) => {
                    self.seen.owner = Some(owner);
                    self.lower.open(Lower::Spawn, owner);
                }
                Output::Parent(request) => self.seen.observe_parent(
                    request,
                    self.seen.exited && self.seen.empty && self.seen.eof,
                    self.lower.is_empty(),
                ),
                Output::Process(process::Request::Send { owner, message, .. }) => {
                    self.lower.open(Lower::Send, owner);
                    self.seen.down.push(message);
                }
                Output::Process(process::Request::Read { owner, .. }) => self.lower.open(Lower::Read, owner),
                Output::Process(process::Request::Wait { owner, .. }) => self.lower.open(Lower::Wait, owner),
                Output::Process(process::Request::Reap { owner, .. }) => self.lower.open(Lower::Reap, owner),
                Output::Process(process::Request::Signal { owner, signal, .. }) => {
                    self.seen.signals.push(signal);
                    let key = match signal {
                        host::Signal::Terminate => Lower::Terminate,
                        host::Signal::Kill => Lower::Kill,
                    };
                    self.lower.open(key, owner);
                }
            }
        }
    }
}

/// Typed policy with exactly the requested instruction ownership.
#[must_use]
pub fn charter_value(bytes: u64) -> host::Charter {
    use smith_domain::run;
    let value = run::Charter {
        resume: false,
        waiting: Duration::ZERO,
        instructions: vec![b'x'; usize::try_from(bytes).expect("fixture size")].into_boxed_slice(),
        brief: run::charter::Brief { sections: Box::new([]) },
        conventions: None,
        grants: run::charter::Grants {
            wait: false,
            deliver: None,
            tools: run::charter::Tools { inspect: false, modify: false, shell: false },
            agents: false,
            host_tools: Box::new([]),
        },
        outcome: run::outcome::OutcomeSpec { change: None, verdicts: Box::new([]), report: None, failure: None },
        budget: run::Budget { turns: 0, spend: 0, time: Duration::ZERO },
        llm: run::charter::Llm {
            prices: run::Prices { input: 0, cached: 0, output: 0, unit: 0 },
            dialect: 0,
            account: 0,
            endpoint: run::charter::Endpoint(0),
            model: Box::new([]),
            window: 0,
            output: 0,
        },
        models: Box::new([]),
    };
    host::Charter::new(value, bytes).expect("bounded charter fixture")
}

/// Concrete turn padded to the requested ownership when one message fits.
#[must_use]
pub fn turn_value(bytes: u64) -> host::TurnValue {
    use smith_domain::session::{llm, record};
    let overhead = u64::try_from(core::mem::size_of::<llm::Message>() + core::mem::size_of::<llm::Block>())
        .expect("fixture footprint");
    let messages = if bytes >= overhead {
        Box::from([llm::Message {
            role: llm::Role::Assistant,
            content: Box::from([llm::Block::Text {
                text: vec![b't'; usize::try_from(bytes - overhead).expect("fixture size")].into_boxed_slice(),
                replay: None,
            }]),
        }])
    } else {
        Box::<[llm::Message]>::default()
    };
    host::TurnValue::new(
        record::Turn {
            version: record::VERSION,
            endpoint: llm::Endpoint(0),
            dialect: 0,
            sequence: 1,
            usage: llm::Usage::ZERO,
            spent: 0,
            messages,
        },
        bytes,
    )
    .expect("bounded turn fixture")
}

/// One saved concrete turn with exactly the requested retained ownership.
#[must_use]
pub fn transcript_value(bytes: u64) -> host::Transcript {
    use smith_domain::session::{llm, record};
    let turn_bytes = u64::try_from(core::mem::size_of::<record::Turn>()).expect("turn footprint");
    let turns = if bytes >= turn_bytes {
        Box::from([turn_value(bytes - turn_bytes).into_value()])
    } else {
        Box::<[record::Turn]>::default()
    };
    host::Transcript::new(
        record::Transcript { version: record::VERSION, endpoint: llm::Endpoint(0), dialect: 0, turns },
        bytes,
    )
    .expect("bounded history fixture")
}

/// Generic delivery fields padded to the requested checked ownership.
#[must_use]
pub fn fields_value(bytes: u64) -> host::Fields {
    use smith_domain::run::outcome::{Change, Field};
    let overhead = u64::try_from(core::mem::size_of::<Field>()).expect("field footprint");
    let fields = if bytes >= overhead {
        Box::from([Field {
            name: Box::new([]),
            value: vec![b'f'; usize::try_from(bytes - overhead).expect("fixture size")].into_boxed_slice(),
        }])
    } else {
        Box::<[Field]>::default()
    };
    host::Fields::new(Change { fields }, bytes).expect("bounded fields fixture")
}

/// An accepted report owning exactly the requested text bytes.
#[must_use]
pub fn declared_value(bytes: u64) -> host::Declared {
    use smith_domain::run::outcome::{Declared, Report};
    host::Declared::new(
        Declared::Report(Report {
            text: vec![b'r'; usize::try_from(bytes).expect("fixture size")].into_boxed_slice(),
            fields: Box::new([]),
        }),
        bytes,
    )
    .expect("bounded outcome fixture")
}

impl Seen {
    /// Common parent referee; the kind supplies actual lower containment proof.
    pub fn observe_parent(&mut self, request: parent::Request, contained: bool, lower_settled: bool) {
        match request {
            parent::Request::Started { agent, .. } => {
                assert!(self.agent.replace(agent).is_none());
            }
            parent::Request::Admitted { .. } => {
                assert!(!self.admitted);
                self.admitted = true;
            }
            parent::Request::Called { call, .. } => {
                assert!(self.calls.insert(call));
            }
            parent::Request::Withdrawn { call, .. } => {
                assert!(self.calls.contains(&call));
                assert!(self.withdrawals.insert(call));
            }
            parent::Request::Turn { turn, .. } => {
                self.read = turn.read;
                self.turn_metadata.push((turn.number, turn.spent));
                assert!(self.turns.insert(turn.number, turn.body).is_none());
            }
            parent::Request::Answered { answer, .. } => {
                assert!(self.fault.is_none());
                assert!(self.answer.replace(answer).is_none(), "one actual run answer");
            }
            parent::Request::Faulted { fault, .. } => {
                assert!(self.answer.is_none());
                assert!(self.fault.replace(fault).is_none());
            }
            parent::Request::Gone { end, detail, .. } => {
                if end == End::Stopped {
                    assert!(contained, "Gone needs the kind's actual containment proof");
                    assert!(lower_settled, "Gone retains every lower terminal");
                    assert!(self.calls.is_empty(), "Gone retains every parent call");
                    assert!(self.turns.is_empty(), "Gone retains every told turn");
                }
                assert!(self.gone.replace(end).is_none());
                assert!(self.gone_detail.replace(detail).is_none());
            }
            parent::Request::MessageRefused { reason, .. } => self.bounces.push(reason),
            parent::Request::Rejected { account, generation, .. } => {
                self.rejected.push((account, generation));
            }
            parent::Request::Told { .. } => self.told += 1,
            parent::Request::Waiting { read, .. } => self.read = read,
            parent::Request::Long { .. } | parent::Request::LongDone { .. } | parent::Request::Exhausted { .. } => {}
        }
    }
}
