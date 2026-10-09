//! A scripted conversation partner: what the run's conversations would be,
//! the session child domain and the LLM behind it, played from a seed.
//!
//! It speaks the run's conversation vocabulary, as the top level will once it
//! translates the session's, and plays the conversations' contract:
//!
//! - An opening is refused (`Ended` as busy or invalid, with no `Started`) or
//!   started at once.
//! - A started conversation requests a pure run completion permit before each
//!   actual turn. After its latency, the turn emits its cumulative own/subtree
//!   `Priced` and exact single-completion `Used`. After each, the script draws what the LLM does next: fail
//!   (`Ended` with a fault), call `finish` (`Delegated`, then wait for its
//!   `Return`), ask for sub-agents, call a declared host tool, deliver a
//!   separate mid-run change, wait, yield (`Yielded`, then wait for `Say` or
//!   `Close`), or carry on. A finish declares an
//!   outcome that fits the host's charters or one that breaks them, a change,
//!   verdict, report or declared failure. An ask may want more than the asker has, or an LLM
//!   the charter does not list, and may ask for a small share. A sub-agent
//!   may not finish: where main would, it yields its answer.
//! - A write runs alone, as a session runs it: a finish, or an ask for a
//!   sub-agent that may modify or run commands. A granted delivery and a host
//!   write use that same exclusive schedule. Asks for sub-agents that may
//!   only look are read-only, and a turn may make several, which the session
//!   runs side by side: the LLM carries on once they have all returned.
//! - It keeps to its share of the budget as a session keeps to its ceilings:
//!   it requests a turn only while its own turns and inclusive spend have
//!   room; the run additionally denies starts at its shared scalar ceiling.
//!   After a turn that went past any part of its share, it
//!   settles that turn's call, if it made one, and ends out of budget. It
//!   expires, out of time, when its time runs out.
//! - `wait` first receives its own result; only then does the partner yield so
//!   the run can park. Host calls may return busy, lose a decided answer, or
//!   return the recorded answer on the original or a retry relay.
//! - A call carries the conversation's expiry as its deadline, and the run
//!   runs the race: the conversation waits for the call to return, past its
//!   expiry too, then carries on, or ends out of time if it has expired.
//! - `Say` comes only while it is yielded; `Close` at any time. Closed, it
//!   withdraws its call in flight and waits for it to return, settles for
//!   a while (a turn in flight may win the race with the close and be spent),
//!   then sends its one `Ended`. A `Say` or `Close` for a conversation that
//!   has ended is dropped, as a stale handle is.
//!
//! What it does at a later time it asks the world to wake it for. Each wake
//! names the conversation and which of its wakes it is, so a wake that a
//! later one replaced is ignored.

use std::collections::{BTreeMap, BTreeSet};

use skein_lib::{Duration, Rng, Time, Token};
use smith_domain_run::charter::Families;
use smith_domain_run::outcome::{Change, Declared, DeclaredFailure, Field, Item, Report, Verdict};
use smith_domain_run::{
    Ask, Budget, End, Event, Exhausted, Fault, HostEffect, HostInput, Opening, Returned, Spend, Stop,
};

use skein_world::domain::Span;

/// How the partner behaves.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Script {
    /// Conversations at once. An opening beyond them is refused as busy.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub conversations: u32,
    /// The chance, per mille, that an opening is refused as invalid.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub invalid: u32,
    /// How long a turn takes.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub turn: Span,
    /// The tokens a turn spends: input and output drawn from `1..=` the
    /// largest of each, cache reads and writes each from `0..=` the largest.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub input: u64,
    /// Inclusive largest output-token count generated for a turn.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub output: u64,
    /// Inclusive largest cache-read and cache-write token count generated for a turn.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub cache: u64,
    /// After each turn, the chance, per mille, that the conversation fails,
    /// that the LLM calls `finish`, and that it yields; otherwise it carries
    /// on.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub faults: u32,
    /// Chance per mille of a finish after a generated turn.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub finishes: u32,
    /// Chance per mille of a sub-agent ask after a generated turn.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub asks: u32,
    /// Chance per mille of a granted host-tool call after a turn.
    pub host_calls: u32,
    /// Chance per mille of a granted mid-run delivery after a turn.
    pub deliveries: u32,
    /// Chance per mille of a granted wait after a turn.
    pub waits: u32,
    /// Chance per mille of a yield after a generated turn.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub yields: u32,
    /// Of asks, the chance, per mille, that it is one the run cannot grant,
    /// and that it asks for a small share.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub bad_asks: u32,
    /// Chance per mille of an explicit smaller share on a sub-agent ask.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub shares: u32,
    /// The most read-only asks a turn makes at once.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub parallel: u32,
    /// Of finishes, the chance, per mille, that the outcome is a change
    /// rather than a verdict, and that it fits the host's charters.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub changes: u32,
    /// Chance per mille of a report form; zero preserves the earlier fixture's random draws.
    ///
    /// Scripted-world contract: domain/run.md, sections 7.1 and 13; testing-strategy.md, section 2.2.
    pub reports: u32,
    /// Chance per mille of a declared failure after report selection; zero preserves earlier random draws.
    ///
    /// Scripted-world contract: domain/run.md, sections 7.1 and 13; testing-strategy.md, section 2.2.
    pub failures: u32,
    /// Chance per mille that a generated finish meets the charter.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub good: u32,
    /// The chance, per mille, that a yield stops for something other than the
    /// end of a turn.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub odd_stops: u32,
    /// How long a closed conversation takes to settle.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub settle: Span,
    /// The chance, per mille, that a turn in flight wins the race with a close.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub races: u32,
}

/// What the partner asks of the world.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Out {
    /// Simulated session requests the pure run admission gate before
    /// starting a completion. Contract: domain/run.md, sections 9 and 14.
    Permit {
        /// Concrete conversation. Contract: domain/run.md, section 14.
        conversation: Token,
        /// Simulated session owner. Contract: domain/run.md, section 14.
        peer: Token,
    },

    /// An event for the run.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Event(Event),
    /// Wake the partner at `at` for `peer`'s wake numbered `wake`.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Wake {
        /// Injected monotonic time at which this wake becomes due.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
        at: Time,
        /// Opaque conversation identity to wake.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
        peer: Token,
        /// Seeded wake sequence distinguishing the peer's current alarm from a stale one.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
        wake: u64,
    },
}

/// What the partner counted.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Tally {
    /// Actual successful settled wait results, independent of accepted outcomes.
    /// Contract: domain/run.md, section 6.
    pub waiting: u32,
    /// Mid-run delivery results returned to a continuing conversation.
    pub delivered: u32,
    /// Host results, unknown outcomes and pre-relay refusals returned to the partner.
    pub host_answered: u32,
    pub host_unknown: u32,
    pub host_rejected: u32,

    /// Count of opened observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub opened: u32,
    /// Count of refused observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub refused: u32,
    /// Count of turns observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub turns: u32,
    /// Count of yields observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub yields: u32,
    /// Count of nudged observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub nudged: u32,
    /// Count of faults observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub faults: u32,
    /// Finishes called, sub-agents asked for, and how they returned.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub finishes: u32,
    /// Count of asks observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub asks: u32,
    /// Count of answered observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub answered: u32,
    /// Count of unanswered observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub unanswered: u32,
    /// Count of ask refused observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub ask_refused: u32,
    /// Count of accepted observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub accepted: u32,
    /// Count of rejected observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub rejected: u32,
    /// Count of checks failed observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub checks_failed: u32,
    /// Count of moved observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub moved: u32,
    /// Count of unpushed observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub unpushed: u32,
    /// Count of cancelled observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub cancelled: u32,
    /// Count of timed out observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub timed_out: u32,
    /// Count of busy observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub busy: u32,
    /// Calls withdrawn as the conversation closed.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub withdrawn: u32,
    /// Conversations that ended at a ceiling of their share, or out of time.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub ceilings: u32,
    /// Count of expired observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub expired: u32,
    /// Count of closed observed at this scripted boundary.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub closed: u32,
    /// Turns in flight that won the race with a close.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub races: u32,
    /// `Say` and `Close` for conversations that had ended.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub stale: u32,
}

/// Seeded scripted conversation peer retaining pending talks, calls and close races.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
pub struct Partner {
    script: Script,
    rng: Rng,
    talks: BTreeMap<Token, Talk>,
    /// Conversations that have ended, which a stale `Say` or `Close` may name.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    ended: BTreeSet<Token>,
    /// The conversation each finish in flight is of.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    calls: BTreeMap<Token, Token>,
    deliver_calls: BTreeSet<Token>,
    /// Names for conversations and calls.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    serial: u64,
    /// What every conversation has spent, by its `Used`.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    spent: Spend,
    tally: Tally,
    tell_turns: bool,
}

/// A started conversation.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
struct Talk {
    /// The run's name for it.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    conversation: Token,
    /// Whether it may finish, and its families.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    finish: bool,
    wait: bool,
    deliver: bool,
    host_tools: Vec<(Box<[u8]>, HostEffect)>,
    families: Families,
    budget: Budget,
    expires: Time,
    spent: Spend,
    subtree_spent: u64,
    phase: Phase,
    /// The wake that counts; earlier ones are stale.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    wake: u64,
    told: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Current completion requested but unsent; no wake or lower right exists.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    Requesting,

    /// A turn is in flight: its wake ends it.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Turning,
    /// Waiting for `Say` or `Close`: its wake is the expiry.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Yielded,
    /// Its `pending` calls (a finish, or asks for sub-agents) are in flight,
    /// with no wake: the run returns them by the conversation's expiry.
    /// `over` is the part of the share the turn that called went past, if
    /// any.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Calling { pending: u32, over: Option<Exhausted> },
    /// Closed, it withdrew its `pending` calls, and waits for them to return;
    /// then it settles.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Withdrawn { pending: u32 },
    /// Closed, settling: its wake ends it. `in_flight` says a turn was.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    Closing { in_flight: bool },
}

impl Partner {
    /// Constructs empty bounded state under the supplied immutable limits; no IO or clocks are consulted.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn new(script: Script, seed: u64) -> Partner {
        Partner {
            script,
            rng: Rng::new(seed),
            talks: BTreeMap::new(),
            ended: BTreeSet::new(),
            calls: BTreeMap::new(),
            deliver_calls: BTreeSet::new(),
            serial: 0,
            spent: Spend::ZERO,
            tally: Tally::default(),
            tell_turns: false,
        }
    }

    /// Enable opaque settled Turn records for message-fence component stories.
    pub fn tell_turns(&mut self) {
        self.tell_turns = true;
    }

    /// Conversations started and not ended, and finishes in flight.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn live(&self) -> usize {
        self.talks.len() + self.calls.len()
    }

    /// What every conversation has spent.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn spent(&self) -> Spend {
        self.spent
    }

    /// Observed counters retained by this scripted peer.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn tally(&self) -> Tally {
        self.tally
    }

    /// The most a turn may spend.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    #[must_use]
    pub fn turn_max(&self) -> Spend {
        let Script { input, output, cache, .. } = self.script;
        Spend { units: input, turns: 1, input, output, cache_read: cache, cache_write: cache }
    }

    /// Delivers a new typed opening to the scripted peer or session and retains its pending terminal obligations.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub fn open(&mut self, now: Time, conversation: Token, opening: &Opening, out: &mut Vec<Out>) {
        let full = self.talks.len() >= usize::try_from(self.script.conversations).expect("a u32 fits");
        if full || self.rng.chance(self.script.invalid) {
            let end = if full { End::Busy } else { End::Invalid };
            out.push(Out::Event(Event::Ended { conversation, end, spend: Spend::ZERO }));
            self.tally.refused += 1;
            return;
        }
        let peer = self.mint();
        let expires = now.saturating_add(opening.budget.time);
        let talk = Talk {
            conversation,
            finish: opening.finish,
            wait: opening.wait,
            deliver: opening.deliver,
            host_tools: opening.host_tools.iter().map(|tool| (tool.name.clone(), tool.effect)).collect(),
            families: opening.families,
            budget: opening.budget,
            expires,
            spent: Spend::ZERO,
            subtree_spent: 0,
            phase: Phase::Requesting,
            wake: 0,
            told: 0,
        };
        self.talks.insert(peer, talk);
        out.push(Out::Event(Event::Started { conversation, peer }));
        self.tally.opened += 1;
        self.carry_on(peer, out);
    }

    /// Schedules another seeded turn for a live conversation.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub fn say(&mut self, peer: Token, out: &mut Vec<Out>) {
        let Some(talk) = self.talks.get(&peer) else {
            assert!(self.ended.contains(&peer), "a stale say names a conversation that ended");
            self.tally.stale += 1;
            return;
        };
        assert_eq!(talk.phase, Phase::Yielded, "the run says something only to a yielded conversation");
        self.tally.nudged += 1;
        self.carry_on(peer, out);
    }

    /// Requests session closure; lower terminals must still be delivered before settlement.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub fn close(&mut self, now: Time, peer: Token, out: &mut Vec<Out>) {
        let settle = self.draw(self.script.settle);
        let Some(talk) = self.talks.get_mut(&peer) else {
            assert!(self.ended.contains(&peer), "a stale close names a conversation that ended");
            self.tally.stale += 1;
            return;
        };
        let conversation = talk.conversation;
        let in_flight = match talk.phase {
            Phase::Turning => true,
            Phase::Yielded | Phase::Requesting => false,
            // It withdraws its calls in flight and waits for them to return,
            // with no wake.
            Phase::Calling { pending, over: _ } => {
                talk.phase = Phase::Withdrawn { pending };
                for (call, of) in &self.calls {
                    if *of == peer {
                        out.push(Out::Event(Event::Withdraw { conversation, call: *call }));
                        self.tally.withdrawn += 1;
                    }
                }
                return;
            }
            Phase::Withdrawn { .. } | Phase::Closing { .. } => panic!("the run closes a conversation once"),
        };
        talk.phase = Phase::Closing { in_flight };
        self.wake(now.saturating_add(settle), peer, out);
    }

    /// The run's answer to the call `call`.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub fn returned(&mut self, now: Time, call: Token, result: &Returned, bill: u64, out: &mut Vec<Out>) {
        let peer = self.calls.remove(&call).expect("a return names a call in flight");
        let mid_delivery = self.deliver_calls.remove(&call);
        match result {
            Returned::HostAnswered(_) => self.tally.host_answered += 1,
            Returned::HostUnknown => self.tally.host_unknown += 1,
            Returned::HostTooLarge { .. } | Returned::HostReportedTooLarge | Returned::HostRejected(_) => {
                self.tally.host_rejected += 1;
            }
            Returned::Waiting => self.tally.waiting += 1,
            Returned::Delivered(_) if mid_delivery => self.tally.delivered += 1,
            Returned::Accepted | Returned::Delivered(_) => self.tally.accepted += 1,
            Returned::Nothing | Returned::DeliveryRefused(_) | Returned::DeliveryFailed { .. } => {
                self.tally.unpushed += 1;
            }
            Returned::Rejected { .. } => self.tally.rejected += 1,
            Returned::ChecksFailed { .. } => self.tally.checks_failed += 1,
            Returned::Stale => self.tally.moved += 1,
            Returned::Cancelled => self.tally.cancelled += 1,
            Returned::TimedOut => self.tally.timed_out += 1,
            Returned::Busy => self.tally.busy += 1,
            Returned::Answered { .. } => self.tally.answered += 1,
            Returned::Unanswered { .. } => self.tally.unanswered += 1,
            Returned::Refused { .. } => self.tally.ask_refused += 1,
        }
        let talk = self.talks.get_mut(&peer).expect("a conversation outlives its calls");
        talk.subtree_spent = talk.subtree_spent.checked_add(bill).expect("bounded child bill");
        out.push(Out::Event(Event::Priced {
            conversation: talk.conversation,
            own_spent: talk.spent.units,
            subtree_spent: talk.subtree_spent,
        }));
        let expires = talk.expires;
        if *result == Returned::TimedOut {
            assert!(now >= expires, "a call times out only once its deadline has passed");
        }
        let phase = talk.phase;
        if matches!(phase, Phase::Calling { pending: 1, .. } | Phase::Withdrawn { pending: 1 }) {
            self.tell(peer, out);
        }
        match phase {
            Phase::Calling { pending, over } if pending > 1 => {
                self.talks.get_mut(&peer).expect("a pending talk").phase = Phase::Calling { pending: pending - 1, over }
            }
            Phase::Withdrawn { pending } if pending > 1 => {
                self.talks.get_mut(&peer).expect("a pending talk").phase = Phase::Withdrawn { pending: pending - 1 }
            }
            // Its last call in flight returned.
            Phase::Calling { pending: _, over } => match over {
                Some(exhausted) => self.end(peer, End::Budget(exhausted), out),
                None if now >= expires => self.end(peer, End::Budget(Exhausted::Time), out),
                None if *result == Returned::Waiting => {
                    let talk = self.talks.get_mut(&peer).expect("a waiting main");
                    talk.phase = Phase::Yielded;
                    out.push(Out::Event(Event::Yielded {
                        conversation: talk.conversation,
                        stop: Stop::EndTurn,
                        text: b"Waiting for a message."[..].into(),
                    }));
                    self.tally.yields += 1;
                    self.wake(expires, peer, out);
                }
                None => self.carry_on(peer, out),
            },
            Phase::Withdrawn { pending: _ } => {
                let settle = self.draw(self.script.settle);
                let talk = self.talks.get_mut(&peer).expect("a live conversation");
                talk.phase = Phase::Closing { in_flight: false };
                self.wake(now.saturating_add(settle), peer, out);
            }
            Phase::Requesting | Phase::Turning | Phase::Yielded | Phase::Closing { .. } => {
                panic!("a return comes while its call is in flight")
            }
        }
    }

    /// The wake numbered `wake` for `peer` has come.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    pub fn woken(&mut self, now: Time, peer: Token, wake: u64, out: &mut Vec<Out>) {
        let Some(talk) = self.talks.get_mut(&peer) else { return };
        if talk.wake != wake {
            return;
        }
        let (phase, expires) = (talk.phase, talk.expires);
        match phase {
            Phase::Turning if now >= expires => self.end(peer, End::Budget(Exhausted::Time), out),
            Phase::Turning => self.turn(peer, out),
            Phase::Yielded => {
                assert!(now >= expires, "a yielded conversation wakes only when it expires");
                self.end(peer, End::Budget(Exhausted::Time), out);
            }
            Phase::Requesting | Phase::Calling { .. } | Phase::Withdrawn { .. } => {
                unreachable!("a call in flight waits without a wake")
            }
            Phase::Closing { in_flight } => {
                if in_flight && self.rng.chance(self.script.races) {
                    self.spend(peer, out);
                    self.tally.races += 1;
                }
                self.end(peer, End::Closed, out);
            }
        }
    }

    /// A turn in flight completes; the script draws what comes next.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn turn(&mut self, peer: Token, out: &mut Vec<Out>) {
        self.spend(peer, out);
        let talk = self.talks.get(&peer).expect("a turn is of a live conversation");
        let over = overspent(&talk.budget, talk.spent);
        let (finish, agents, wait, deliver, host_tools) =
            (talk.finish, talk.families.agents, talk.wait, talk.deliver, !talk.host_tools.is_empty());
        let roll = u32::try_from(self.rng.below(1000)).expect("below 1000");
        let Script { faults, finishes, asks, host_calls, deliveries, waits, yields, .. } = self.script;
        let finishing = faults.saturating_add(finishes);
        let asking = finishing.saturating_add(asks);
        let hosting = asking.saturating_add(host_calls);
        let delivering = hosting.saturating_add(deliveries);
        let waiting = delivering.saturating_add(waits);
        if roll < faults {
            let fault = if self.rng.chance(500) { Fault::Provider } else { Fault::ContextFull };
            self.tally.faults += 1;
            self.end(peer, End::Fault(fault), out);
        } else if roll < finishing && finish {
            self.finish(peer, over, out);
        } else if roll >= finishing && roll < asking && agents && over.is_none() {
            self.ask(peer, out);
        } else if roll >= asking && roll < hosting && host_tools && over.is_none() {
            self.host_call(peer, out);
        } else if roll >= hosting && roll < delivering && deliver && over.is_none() {
            self.deliver(peer, out);
        } else if roll >= delivering && roll < waiting && wait && over.is_none() {
            self.call(peer, None, [Ask::Wait], out);
        } else if let Some(exhausted) = over {
            // Past its share with no call to settle: it ends.
            self.tally.ceilings += 1;
            self.end(peer, End::Budget(exhausted), out);
        } else if roll < waiting.saturating_add(yields) || roll < finishing {
            let stop = if self.rng.chance(self.script.odd_stops) {
                match self.rng.below(3) {
                    0 => Stop::MaxTokens,
                    1 => Stop::Refusal,
                    _ => Stop::NoCalls,
                }
            } else {
                Stop::EndTurn
            };
            self.tell(peer, out);
            let talk = self.talks.get_mut(&peer).expect("a turn is of a live conversation");
            talk.phase = Phase::Yielded;
            let (conversation, expires) = (talk.conversation, talk.expires);
            out.push(Out::Event(Event::Yielded { conversation, stop, text: b"I have looked into it."[..].into() }));
            self.tally.yields += 1;
            self.wake(expires, peer, out);
        } else {
            self.carry_on(peer, out);
        }
    }

    /// The LLM calls `finish`, with an outcome drawn from the script.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn finish(&mut self, peer: Token, over: Option<Exhausted>, out: &mut Vec<Out>) {
        let outcome = self.outcome();
        self.tally.finishes += 1;
        let asks = [Ask::Finish { outcome }];
        self.call(peer, over, asks, out);
    }

    /// The LLM asks for sub-agents, as the script draws them: one, or several
    /// that may only look.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn ask(&mut self, peer: Token, out: &mut Vec<Out>) {
        let count = self.rng.between(1, u64::from(self.script.parallel.max(1)));
        let asks: Vec<Ask> = (0..count).map(|_| self.sub_agent(peer, count > 1)).collect();
        self.tally.asks += u32::try_from(count).expect("a few");
        self.call(peer, None, asks, out);
    }

    fn host_call(&mut self, peer: Token, out: &mut Vec<Out>) {
        let tools = &self.talks[&peer].host_tools;
        let index = usize::try_from(self.rng.below(tools.len() as u64)).expect("bounded declarations");
        let (tool, effect) = tools[index].clone();
        let input = HostInput::attested(b"{}".as_slice().into()).expect("bounded object");
        self.call(peer, None, [Ask::Host { tool, effect, input }], out);
    }

    fn deliver(&mut self, peer: Token, out: &mut Vec<Out>) {
        let change = Change {
            fields: Box::new([
                Field { name: b"title".as_slice().into(), value: b"Update".as_slice().into() },
                Field { name: b"body".as_slice().into(), value: b"The parser now passes.".as_slice().into() },
            ]),
        };
        self.call(peer, None, [Ask::Deliver { change }], out);
    }

    /// An ask for a sub-agent, as the script draws it: `read_only`, if it may
    /// only look.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn sub_agent(&mut self, peer: Token, read_only: bool) -> Ask {
        let own = self.talks[&peer].families;
        let bad = self.rng.chance(self.script.bad_asks);
        let mut families = Families { agents: self.rng.chance(500) && own.agents, ..own };
        if read_only {
            families.tools.modify = false;
            families.tools.shell = false;
        }
        let mut llm = if self.rng.chance(300) { Some(b"fake-2"[..].into()) } else { None };
        if bad {
            // More than it has, if it lacks anything and may want it; else an
            // unknown LLM.
            if read_only || (own.tools.shell && own.tools.modify) {
                llm = Some(b"fake-9"[..].into());
            } else {
                families.tools.shell = true;
                families.tools.modify = true;
            }
        }
        let share = if self.rng.chance(self.script.shares) {
            Some(smith_domain_run::Share {
                turns: u32::try_from(self.rng.between(1, 3)).expect("small"),
                spend: self.rng.between(100, 4_000),
            })
        } else {
            None
        };
        let brief = b"Look into the parser, and say what you found."[..].into();
        Ask::SubAgent { brief, families, llm, share }
    }

    /// The LLM's turn called the run for `asks`, side by side, `over` its
    /// share if it went past it. Each call is due by the conversation's
    /// expiry.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn call(&mut self, peer: Token, over: Option<Exhausted>, asks: impl IntoIterator<Item = Ask>, out: &mut Vec<Out>) {
        let mut pending = 0;
        for ask in asks {
            let call = self.mint();
            if matches!(ask, Ask::Deliver { .. }) {
                self.deliver_calls.insert(call);
            }
            let talk = &self.talks[&peer];
            let (conversation, deadline) = (talk.conversation, talk.expires);
            self.calls.insert(call, peer);
            out.push(Out::Event(Event::Delegated {
                conversation,
                call,
                ask,
                deadline,
                name: smith_domain_run::CallName { activation: 1, completion: talk.spent.turns, position: pending },
            }));
            pending += 1;
        }
        let talk = self.talks.get_mut(&peer).expect("a live conversation calls");
        talk.phase = Phase::Calling { pending, over };
    }

    /// An outcome to declare, from any of four forms, that fits the host's
    /// charters or one that does not.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn outcome(&mut self) -> Declared {
        if self.script.reports > 0 && self.rng.chance(self.script.reports) {
            let good = self.rng.chance(self.script.good);
            return Declared::Report(Report {
                text: Box::new([]),
                fields: Box::new([Field {
                    name: b"source".as_slice().into(),
                    value: if good { b"README.md".as_slice().into() } else { Box::new([]) },
                }]),
            });
        }
        if self.script.failures > 0 && self.rng.chance(self.script.failures) {
            let good = self.rng.chance(self.script.good);
            return Declared::Failure(DeclaredFailure {
                reason: if good { b"Required access is unavailable.".as_slice().into() } else { Box::new([]) },
                fields: Box::new([Field {
                    name: b"cause".as_slice().into(),
                    value: b"missing-access".as_slice().into(),
                }]),
            });
        }
        let change = self.rng.chance(self.script.changes);
        let good = self.rng.chance(self.script.good);
        if change {
            let title = if good { b"Fix the parser"[..].into() } else { Box::default() };
            return Declared::Change(Change {
                fields: Box::new([
                    smith_domain_run::outcome::Field { name: b"title".as_slice().into(), value: title },
                    smith_domain_run::outcome::Field {
                        name: b"body".as_slice().into(),
                        value: b"It accepts tabs now."[..].into(),
                    },
                ]),
            });
        }
        let comment = |kind: &[u8], fields: &[&[u8]]| Item {
            kind: kind.into(),
            fields: fields.iter().map(|name| Field { name: (*name).into(), value: b"...".as_slice().into() }).collect(),
        };
        let (name, children): (&[u8], Box<[Item]>) = match (good, self.rng.below(3)) {
            (true, 0) => (b"approve", Box::new([])),
            (true, _) => {
                let count = self.rng.between(1, 3);
                (b"request-changes", (0..count).map(|_| comment(b"nit", &[b"path", b"body"])).collect())
            }
            (false, 0) => (b"reject", Box::new([])),
            (false, 1) => (b"request-changes", Box::new([])),
            (false, _) => (b"request-changes", Box::new([comment(b"praise", &[b"path"])])),
        };
        Declared::Verdict(Verdict {
            name: name.into(),
            text: b"See the comments."[..].into(),
            items: children,
            fields: Box::new([]),
        })
    }

    /// The LLM goes on: another turn, unless the share leaves no room for one.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn carry_on(&mut self, peer: Token, out: &mut Vec<Out>) {
        let talk = self.talks.get_mut(&peer).expect("a live conversation");
        if let Some(exhausted) = ceiling(&talk.budget, talk.spent) {
            self.tally.ceilings += 1;
            self.end(peer, End::Budget(exhausted), out);
            return;
        }
        talk.phase = Phase::Requesting;
        out.push(Out::Permit { conversation: talk.conversation, peer });
    }

    /// Start only after the actual pure gate accepts; denial invents no usage.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub fn permitted(
        &mut self,
        now: Time,
        peer: Token,
        permit: smith_domain_run::CompletionPermit,
        out: &mut Vec<Out>,
    ) {
        // A prior Close owns its delayed settlement; an old queued
        // permission cannot revive it or invent a second terminal.
        let Some(talk) = self.talks.get(&peer) else {
            return;
        };
        if talk.phase != Phase::Requesting {
            return;
        }
        match permit {
            smith_domain_run::CompletionPermit::Denied(exhausted) => {
                self.end(peer, End::Budget(exhausted), out);
                return;
            }
            smith_domain_run::CompletionPermit::Closing => {
                self.end(peer, End::Closed, out);
                return;
            }
            smith_domain_run::CompletionPermit::Allowed => {}
            smith_domain_run::CompletionPermit::Held => return,
        }
        let latency = self.draw(self.script.turn);
        let talk = self.talks.get_mut(&peer).expect("a live admitted conversation");
        talk.phase = Phase::Turning;
        let at = now.saturating_add(latency).min(talk.expires);
        self.wake(at, peer, out);
    }

    fn spend(&mut self, peer: Token, out: &mut Vec<Out>) {
        let Script { input, output, cache, .. } = self.script;
        let mut spend = Spend {
            units: 0,
            turns: 1,
            input: self.rng.between(1, input),
            output: self.rng.between(1, output),
            cache_read: self.rng.below(cache.saturating_add(1)),
            cache_write: self.rng.below(cache.saturating_add(1)),
        };
        spend.units = spend.input;
        let talk = self.talks.get_mut(&peer).expect("a live conversation spends");
        talk.spent = talk.spent.accumulate(spend).expect("bounded scripted usage");
        self.spent = self.spent.accumulate(spend).expect("bounded scripted usage");
        talk.subtree_spent = talk.subtree_spent.checked_add(spend.units).expect("bounded fixture bill");
        self.tally.turns += 1;
        out.push(Out::Event(Event::Priced {
            conversation: talk.conversation,
            own_spent: talk.spent.units,
            subtree_spent: talk.subtree_spent,
        }));
        out.push(Out::Event(Event::Used { conversation: talk.conversation, spend }));
    }

    fn tell(&mut self, peer: Token, out: &mut Vec<Out>) {
        if !self.tell_turns {
            return;
        }
        let talk = self.talks.get_mut(&peer).expect("a settled live talk");
        if talk.told >= talk.spent.turns {
            return;
        }
        talk.told += 1;
        out.push(Out::Event(Event::Turn {
            conversation: talk.conversation,
            record: Token::new(u64::from(talk.told)),
            sequence: talk.told,
        }));
    }

    fn end(&mut self, peer: Token, end: End, out: &mut Vec<Out>) {
        let talk = self.talks.remove(&peer).expect("a conversation ends once");
        self.ended.insert(peer);
        if end == End::Closed {
            self.tally.closed += 1;
        }
        if end == End::Budget(Exhausted::Time) {
            self.tally.expired += 1;
        }
        out.push(Out::Event(Event::Ended { conversation: talk.conversation, end, spend: talk.spent }));
    }

    /// Replaces `peer`'s wake with one at `at`.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
    fn wake(&mut self, at: Time, peer: Token, out: &mut Vec<Out>) {
        let talk = self.talks.get_mut(&peer).expect("a live conversation is woken");
        talk.wake += 1;
        out.push(Out::Wake { at, peer, wake: talk.wake });
    }

    fn mint(&mut self) -> Token {
        self.serial += 1;
        Token::new(self.serial)
    }

    fn draw(&mut self, span: Span) -> Duration {
        span.draw(&mut self.rng)
    }
}

/// The part of `budget` that leaves no room to start another turn after
/// `spent`: no turn, input or output left.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
fn ceiling(budget: &Budget, spent: Spend) -> Option<Exhausted> {
    if spent.turns >= budget.turns {
        Some(Exhausted::Turns)
    } else if spent.units >= budget.spend {
        Some(Exhausted::Spend)
    } else {
        None
    }
}

/// The first part of `budget` that `spent` has gone past, if any.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 2.2.
fn overspent(budget: &Budget, spent: Spend) -> Option<Exhausted> {
    if spent.turns > budget.turns {
        Some(Exhausted::Turns)
    } else if spent.units > budget.spend {
        Some(Exhausted::Spend)
    } else {
        None
    }
}
