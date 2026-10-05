//! Expectations for the copied agent on its scripted host (domain/run.md,
//! sections 13 and 14; testing-strategy.md, section 7). This module retains
//! only observed starts, provider usage, check results, push snapshots and
//! answers. It cannot inspect agent state, private conversations or fixtures.
//! The shared referee owns deadlines and reports the first broken expectation.

use std::collections::BTreeSet;

use skein_lib::{Duration, Token};
use skein_world::domain::{Expectations, Judge};
use smith_domain::run::{Answer, Exit, Push, Spend, outcome::Declared};

/// A host or provider observation, independent of the agent's private state.
#[derive(Debug)]
pub enum Seen {
    /// The scripted host starts its one request; an answer is due within `within`.
    Started {
        /// Whether the charter permits a change.
        change: bool,
        /// Whether a change must pass the fixture's checks.
        checks: bool,
        /// The host's liveness allowance, including cancellation settlement.
        within: Duration,
    },
    /// The agent asks the provider for a completion; its owner may be reused
    /// only after the previous terminal has been observed.
    Completing {
        /// The agent's name for the pending provider request.
        owner: Token,
    },
    /// One accepted provider terminal, counted once in the final spend.
    Completed {
        /// The agent's completion owner, unique until that completion ends.
        owner: Token,
        /// The provider's independently reported usage for that completion.
        spent: Spend,
    },
    /// A provider request ended without usage, by failure or cancellation.
    CompletionEnded {
        /// The agent's name for the provider request being ended.
        owner: Token,
    },
    /// The checks' terminal as IO sends it to the agent.
    Checked {
        /// The finishing call that asked for the check.
        owner: Token,
        /// Zero is the only passing code; timeout and cancellation never pass.
        exit: Exit,
    },
    /// The host sees a push request and snapshots exactly the checkout bytes.
    Pushing {
        /// The finishing call that asked for the push.
        owner: Token,
        /// The independently observed checkout snapshot at the request.
        tree: Vec<u8>,
    },
    /// The host's terminal for a push, and what it actually retained as landed.
    Pushed {
        /// The finishing call that asked for the push.
        owner: Token,
        /// Scripted host outcome; a successful push retains the snapshot.
        push: Push,
        /// Landed bytes for `Done`; empty for a refusal or stale branch.
        tree: Vec<u8>,
    },
    /// The host receives the one answer to its start.
    Answered {
        /// The answer received at the boundary, never read from domain state.
        answer: Answer,
        /// Number of agent requests still awaiting terminals at that boundary.
        pending: usize,
    },
}

/// Scenario policy; its state is populated only by [`Seen`] observations.
#[derive(Debug)]
pub struct Meeting {
    started: bool,
    answered: bool,
    change: bool,
    checks: bool,
    completing: BTreeSet<Token>,
    spent: Spend,
    passed: BTreeSet<Token>,
    pushing: std::collections::BTreeMap<Token, Vec<u8>>,
    landed: u32,
}

impl Default for Meeting {
    fn default() -> Self {
        Self {
            started: false,
            answered: false,
            change: false,
            checks: false,
            completing: BTreeSet::new(),
            spent: Spend::ZERO,
            passed: BTreeSet::new(),
            pushing: std::collections::BTreeMap::new(),
            landed: 0,
        }
    }
}

impl Expectations for Meeting {
    type Seen = Seen;

    type Name = &'static str;

    type Stimulus = ();

    fn observe(&mut self, seen: Seen, judge: &mut Judge<Self::Name, Self::Stimulus>) {
        match seen {
            Seen::Started { change, checks, within } => {
                judge.check(!self.started, "one host start per scenario");
                if !self.started {
                    judge.expect("host answer", within);
                }
                self.started = true;
                self.change = change;
                self.checks = checks;
            }
            Seen::Completing { owner } => {
                judge.check(self.started && !self.answered, "a completion belongs to a live host request");
                judge.check(self.completing.insert(owner), "one completion in flight per owner");
            }
            Seen::Completed { owner, spent } => {
                judge.check(self.started && !self.answered, "usage belongs to a live host request");
                judge.check(self.completing.remove(&owner), "one completion terminal per pending owner");
                self.spent = self.spent.saturating_add(spent);
            }
            Seen::CompletionEnded { owner } => {
                judge.check(self.completing.remove(&owner), "one completion terminal per pending owner");
            }
            Seen::Checked { owner, exit } => {
                judge.check(self.started && !self.answered, "checks precede the host answer");
                if exit == (Exit::Code { code: 0 }) {
                    self.passed.insert(owner);
                }
            }
            Seen::Pushing { owner, tree } => {
                judge.check(self.started && !self.answered && self.change, "only a live change charter pushes");
                judge.check(!self.checks || self.passed.remove(&owner), "checks pass before each push");
                judge.check(self.pushing.insert(owner, tree).is_none(), "one push in flight per owner");
            }
            Seen::Pushed { owner, push, tree } => {
                let asked = self.pushing.remove(&owner);
                judge.check(asked.is_some(), "a host push terminal names a pending push");
                match push {
                    Push::Done => {
                        judge.check(asked.as_ref() == Some(&tree), "the host lands exactly the tree the agent left");
                        self.landed += 1;
                    }
                    Push::Moved | Push::Failed { .. } | Push::Nothing => {
                        judge.check(tree.is_empty(), "an unsuccessful push lands nothing");
                    }
                }
            }
            Seen::Answered { answer, pending } => {
                judge.check(self.started && !self.answered, "exactly one answer per host start");
                judge.check(
                    pending == 0 && self.pushing.is_empty() && self.completing.is_empty(),
                    "an answer waits for every request terminal",
                );
                let (spent, change) = match answer {
                    Answer::Refused(_) => (Spend::ZERO, false),
                    Answer::Accepted { spent, outcome } => (spent, matches!(outcome, Declared::Change(_))),
                    Answer::Failed { spent, .. } => (spent, false),
                };
                judge.check(spent == self.spent, "the answer accounts for every accepted provider turn exactly once");
                judge.check(!change || self.landed == 1, "an accepted change landed exactly once");
                judge.check(change || self.landed == 0, "a verdict or failure lands no change");
                self.answered = true;
                let met = judge.meet(&"host answer");
                judge.check(met, "the start's answer obligation is met once");
            }
        }
    }
}
