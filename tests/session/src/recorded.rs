//! A scripted parent/provider world for concrete version-two sessions. The
//! referee observes only requests: provider bytes, turns and spend, never
//! session state. The harness separately checks boundary and settle contracts.

use skein_lib::{Env, Queue, Time, Token, Wall};
use smith_domain_session::{self as session, llm, record};
use smith_domain_tools::{Authority, Effect, Grants};

/// Integer prices used by the concrete transcript scenarios; pricing performs no live account lookup.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
pub const PRICES: record::Prices = record::Prices { input: 7, cached: 3, output: 11, unit: 10 };

/// Fixed provider usage supplied by concrete transcript scenarios.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
pub const USAGE: llm::Usage =
    llm::Usage { input_tokens: 11, output_tokens: 4, cache_read_tokens: 5, cache_write_tokens: 3 };

/// Fixed opaque provider bytes whose exact replay preservation is asserted.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
pub const OPAQUE: &[u8] = b"\0provider reasoning\xffsignature";

/// Builds the concrete version-two opening with the supplied transcript and integer spend cap.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
#[must_use]
pub fn opening(transcript: Option<record::Transcript>, budget: u64) -> record::Opening {
    record::Opening {
        spec: session::Spec {
            endpoint: llm::Endpoint(7),
            model: b"model".as_slice().into(),
            system: b"instructions".as_slice().into(),
            authority: Authority {
                cwd: Box::default(),
                repos: Box::default(),
                grants: Grants { inspect: false, modify: false, shell: false },
                env: Box::default(),
            },
            delegated: Box::new([llm::Descriptor { ticket: Token::new(19), effect: Effect::Write }]),
            prompt: b"wake".as_slice().into(),
            max_tokens: 128,
            budget: crate::BUDGET,
        },
        dialect: 23,
        prices: PRICES,
        budget,
        transcript,
    }
}

/// Real domain plus scripted typed peers, pending terminals and external observations for this component story.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
pub struct World {
    /// Real session state under test, advanced only through its declared boundary.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub domain: session::Domain,
    /// Injected monotonic and wall times together with immutable session limits.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub env: Env<session::Limits>,
    /// Bounded queue of owned session requests awaiting scripted delivery.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub out: Queue<session::Request>,
    /// Current admitted session identity, or none before admission or after its terminal.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub session: Option<Token>,
    /// Pending provider completion identity; its single terminal consumes it.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub completing: Option<Token>,
    /// Pending opener-served call identities, each requiring one terminal.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub delegated: Vec<Token>,
    /// Actual delegated transcript origins in emitted order, independent of live callbacks.
    /// Contract: domain/session.md, sections 3, 5 and 12; testing-strategy.md, section 7.
    pub origins: Vec<record::Origin>,
    /// Pending checkout-operation identities and their owned typed requests.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub operations: Vec<(Token, smith_domain_tools::Op)>,
    /// Identities for which the domain requested cancellation; their terminals remain owed.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub cancelled_operations: Vec<Token>,
    /// Complete deterministic state snapshots at loop boundaries, separate
    /// from the referee's external observations.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub snapshots: Vec<String>,
    /// Concrete provider prompts in their observed request order.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub prompts: Vec<llm::Prompt>,
    /// Concrete transcript turns emitted by the session, in order.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub turns: Vec<record::Turn>,
    /// Accepted cumulative integer charges and their overflow flags, in request order.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub spend: Vec<(u64, bool)>,
    /// The single settled session terminal, or none while work remains.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub end: Option<session::End>,
    /// Ordered deterministic boundary record renderings used for replay comparisons.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub trace: Vec<String>,
}

impl World {
    /// Constructs empty bounded state under the supplied immutable limits; no IO or clocks are consulted.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn new(seed: u64, facts: u32) -> World {
        let mut limits = crate::Settings::calm(seed).agent;
        limits.sessions = 1;
        limits.spend = u64::MAX;
        limits.messages = 32;
        limits.facts = facts;
        World {
            domain: session::Domain::new(&limits, seed),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            out: Queue::with_capacity(session::max_out(&limits)),
            session: None,
            completing: None,
            delegated: vec![],
            origins: vec![],
            operations: vec![],
            cancelled_operations: vec![],
            snapshots: vec![],
            prompts: vec![],
            turns: vec![],
            spend: vec![],
            end: None,
            trace: vec![],
        }
    }

    /// Delivers one typed terminal or entrance, drains the emitted requests and checks terminal ownership.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub fn step(&mut self, event: session::Event) {
        match &event {
            session::Event::Completed { owner, .. }
            | session::Event::Cancelled { owner }
            | session::Event::Failed { owner, .. } => {
                assert_eq!(self.completing.take(), Some(*owner), "one terminal per completion");
            }
            session::Event::AnsweredV2 { owner, .. }
            | session::Event::AnswerCancelledV2 { owner, .. }
            | session::Event::AnswerCancelled { owner } => {
                let position =
                    self.delegated.iter().position(|pending| pending == owner).expect("one terminal per delegate");
                self.delegated.remove(position);
            }
            session::Event::Done { owner, .. } => {
                let position = self
                    .operations
                    .iter()
                    .position(|(pending, _)| pending == owner)
                    .expect("one terminal per owned operation");
                self.operations.remove(position);
            }
            session::Event::Open { .. }
            | session::Event::OpenV2 { .. }
            | session::Event::Continue { .. }
            | session::Event::Close { .. }
            | session::Event::Answered { .. } => {}
        }
        self.domain.reclaim();
        session::step(&mut self.domain, &self.env, event, &mut self.out);
        assert!(self.out.len() <= session::max_out(&self.env.limits));
        while let Some(request) = self.out.pop() {
            self.trace.push(format!("{request:?}"));
            match request {
                session::Request::Opened { opener, session } => {
                    assert_eq!(opener, Token::new(31));
                    assert!(self.session.replace(session).is_none());
                }
                session::Request::Complete { owner, prompt, .. } => {
                    assert!(self.completing.replace(owner).is_none());
                    self.prompts.push(prompt);
                }
                session::Request::Delegate { owner, origin, .. } => {
                    assert!(!self.delegated.contains(&owner));
                    self.delegated.push(owner);
                    self.origins.push(origin);
                }
                session::Request::Turn { opener, turn } => {
                    assert_eq!(opener, Token::new(31));
                    self.turns.push(turn);
                }
                session::Request::Priced { opener, spent, overflow } => {
                    assert_eq!(opener, Token::new(31));
                    self.spend.push((spent, overflow));
                }
                session::Request::Ended { opener, end, .. } => {
                    assert_eq!(opener, Token::new(31));
                    assert!(self.end.replace(end).is_none());
                }
                session::Request::Yielded { .. }
                | session::Request::Used { .. }
                | session::Request::Cancel { .. }
                | session::Request::Withdraw { .. } => {}
                session::Request::Io { owner, op, .. } => {
                    assert!(!self.operations.iter().any(|(pending, _)| pending == &owner));
                    self.operations.push((owner, op));
                }
                session::Request::CancelIo { owner } => {
                    assert!(self.operations.iter().any(|(pending, _)| pending == &owner));
                    self.cancelled_operations.push(owner);
                }
            }
        }
        while self.domain.pop_fact().is_some() {}
        self.snapshots.push(format!("{:?}", self.domain));
    }

    /// Delivers a new typed opening to the scripted peer or real session and retains its pending terminal obligations.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub fn open(&mut self, opening: record::Opening) {
        self.step(session::Event::OpenV2 { opener: Token::new(31), spec: opening });
    }

    /// Answers the pending provider completion with the supplied blocks and fixed usage.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub fn complete(&mut self, content: Box<[llm::Block]>, stop: llm::Stop, usage: llm::Usage) {
        self.step(session::Event::Completed {
            owner: self.completing.expect("a completion is in flight"),
            completion: llm::Completion { content, stop, usage },
        });
    }

    /// Requests session closure; lower terminals must still be delivered before settlement.
    ///
    /// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
    pub fn close(&mut self) {
        self.step(session::Event::Close { session: self.session.expect("admitted") });
        if let Some(owner) = self.completing {
            self.step(session::Event::Cancelled { owner });
        }
        self.domain.reclaim();
        assert!(self.end.is_some());
        assert_eq!(self.domain.sessions(), 0);
        assert_eq!(self.domain.runs(), 0);
        assert_eq!(self.domain.kits(), 0);
        assert!(self.completing.is_none() && self.delegated.is_empty() && self.operations.is_empty());
    }
}

/// Builds a concrete provider call block naming the opener-served fixture tool.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
#[must_use]
pub fn called() -> Box<[llm::Block]> {
    Box::new([
        llm::Block::Opaque { bytes: OPAQUE.into() },
        llm::Block::ToolCall {
            id: b"provider-call".as_slice().into(),
            name: b"subagent".as_slice().into(),
            input: br#"{"task":"review"}"#.as_slice().into(),
            call: llm::Decoded::Delegated { ticket: Token::new(991), effect: Effect::Write },
        },
    ])
}

/// Runs the fixed concrete transcript story and returns its external observations.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
#[must_use]
pub fn scenario(seed: u64, facts: u32) -> World {
    let mut world = World::new(seed, facts);
    world.open(opening(None, 100));
    world.complete(called(), llm::Stop::ToolUse, USAGE);
    assert!(world.turns.is_empty(), "the turn waits for its tool results");
    let owner = world.delegated[0];
    world.step(session::Event::AnsweredV2 { owner, text: b"child finished".as_slice().into(), error: false, spent: 9 });
    // Independent referee arithmetic: ceil((14*7 + 5*3 + 4*11)/10)=16,
    // then the child contributes 9. One terminal response charges it once.
    assert_eq!(world.spend, [(16, false), (25, false)]);
    assert_eq!(world.turns.len(), 1);
    assert_eq!(world.turns[0].spent, 25);
    assert_eq!(world.turns[0].messages.len(), 3);
    let blocks = &world.turns[0].messages[1].content;
    assert_eq!(blocks[0], llm::Block::Opaque { bytes: OPAQUE.into() });
    assert!(matches!(blocks[1], llm::Block::ToolCall { call: llm::Decoded::Historical, .. }));
    assert_eq!(
        world.turns[0].messages[2].content[0],
        llm::Block::ToolResult {
            id: b"provider-call".as_slice().into(),
            result: llm::Returned::Text { text: b"child finished".as_slice().into(), error: false }
        }
    );
    world.complete(
        Box::new([llm::Block::Text { text: b"done".as_slice().into() }]),
        llm::Stop::EndTurn,
        llm::Usage { output_tokens: 1, ..llm::Usage::ZERO },
    );
    assert_eq!(world.turns[1].spent, 27); // ceil(11/10)=2, per completion.
    world.close();
    world
}

/// Copies the settled concrete turns into a replayable version-two transcript.
///
/// World contract: domain/session.md, sections 10 and 12; testing-strategy.md, section 2.2.
#[must_use]
pub fn transcript(world: &World) -> record::Transcript {
    record::Transcript {
        version: record::VERSION,
        endpoint: llm::Endpoint(7),
        dialect: 23,
        turns: world.turns.clone().into(),
        after: Box::default(),
    }
}
