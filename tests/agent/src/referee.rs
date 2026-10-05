//! Expectations for the copied agent on its scripted host (domain/run.md,
//! sections 13 and 14; testing-strategy.md, section 7). This module retains
//! only observed starts, provider usage, check results, push snapshots and
//! answers. It cannot inspect agent state, private conversations or fixtures.
//! The shared referee owns deadlines and reports the first broken expectation.

use std::collections::BTreeSet;

use skein_lib::{Duration, Token};
use skein_world::domain::{Expectations, Judge};
use smith_domain::run::{
    Answer, Delivery, Exit, Spend,
    outcome::{Declared, Field, FieldRule, Item, OutcomeSpec, TextSpec},
};

/// A host or provider observation, independent of the agent's private state.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "observed push terminals retain the copied fixed diagnostic tail without changing their boundary value"
)]
pub enum Seen {
    /// Public host shutdown observation, independent of private run state.
    /// Contract: domain/run.md, sections 10 and 13; testing-strategy.md, section 7.
    Stopped {
        /// The already-decided typed stop whose evidence interrupted delivery preserves.
        /// Contract: domain/run.md, sections 8.4 and 10.
        failure: smith_domain::run::Failure,
    },
    /// IO snapshots checkout bytes at the check request, before its terminal.
    /// Contract: domain/run.md, sections 8.1 and 13; testing-strategy.md, section 7.
    Checking {
        /// Live callback owner, not the durable host name. Contract: domain/run.md, section 8.2.
        owner: Token,
        /// Public IO snapshot to compare with actual delivery submission. Contract: domain/run.md, section 8.1.
        tree: Vec<u8>,
    },
    /// The scripted host starts its one request; an answer is due within `within`.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Started {
        /// Independently observed separate main delivery grant, not inferred from final Change permission.
        /// Contract: domain/run.md, sections 8.4 and 13; testing-strategy.md, section 7.
        delivery: bool,
        /// Host-supplied rules observed at the typed Start boundary, retained by this independent referee.
        ///
        /// Contract: domain/run.md, sections 7.1 and 13; testing-strategy.md, section 7.
        contract: OutcomeSpec,
        /// Immutable aggregate result ownership cap supplied alongside the Start.
        ///
        /// Contract: domain/run.md, sections 7.1 and 13; testing-strategy.md, section 7.
        outcome_bytes: u64,
        /// Whether a change must pass the fixture's checks.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        checks: bool,
        /// The host's liveness allowance, including cancellation settlement.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        within: Duration,
    },
    /// The agent asks the provider for a completion; its owner may be reused
    /// only after the previous terminal has been observed.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Completing {
        /// The agent's name for the pending provider request.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        owner: Token,
    },
    /// One accepted provider terminal, counted once in the final spend.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Completed {
        /// The agent's completion owner, unique until that completion ends.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        owner: Token,
        /// The provider's independently reported usage for that completion.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        spent: Spend,
    },
    /// A provider request ended without usage, by failure or cancellation.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    CompletionEnded {
        /// The agent's name for the provider request being ended.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        owner: Token,
    },
    /// The checks' terminal as IO sends it to the agent.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Checked {
        /// The finishing call that asked for the check.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        owner: Token,
        /// Zero is the only passing code; timeout and cancellation never pass.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        exit: Exit,
    },
    /// The host sees a push request and snapshots exactly the checkout bytes.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Pushing {
        /// Public durable transcript origin, separate from the live callback owner.
        /// Contract: domain/run.md, sections 8.2 and 13; testing-strategy.md, section 7.
        name: smith_domain::run::CallName,
        /// The finishing call that asked for the push.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        owner: Token,
        /// The independently observed checkout snapshot at the request.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        tree: Vec<u8>,
    },
    /// The host's terminal for a push, and what it actually retained as landed.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Delivered {
        /// The finishing call that asked for the push.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        owner: Token,
        /// Scripted host outcome; a successful push retains the snapshot.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        push: Delivery,
        /// Landed bytes for `Done`; empty for a refusal or stale branch.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        tree: Vec<u8>,
    },
    /// The host receives the one answer to its start.
    ///
    /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
    Answered {
        /// The answer received at the boundary, never read from domain state.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        answer: Answer,
        /// Number of agent requests still awaiting terminals at that boundary.
        ///
        /// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
        pending: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Waiting,
    Running,
    Answered,
}

/// Scenario policy; its state is populated only by [`Seen`] observations.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
#[derive(Debug)]
pub struct Meeting {
    phase: Phase,
    contract: Option<OutcomeSpec>,
    outcome_bytes: u64,
    checks: bool,
    delivery: bool,
    stopped: Option<smith_domain::run::Failure>,
    checking: std::collections::BTreeMap<Token, Vec<u8>>,
    names: std::collections::HashSet<smith_domain::run::CallName>,
    last_landed: Option<(smith_domain::run::CallName, smith_domain::run::Delivered)>,
    completing: BTreeSet<Token>,
    spent: Spend,
    passed: std::collections::BTreeMap<Token, Vec<u8>>,
    pushing: std::collections::BTreeMap<Token, (smith_domain::run::CallName, Vec<u8>)>,
    landed: u32,
}

impl Default for Meeting {
    fn default() -> Self {
        Self {
            phase: Phase::Waiting,
            contract: None,
            outcome_bytes: 0,
            checks: false,
            delivery: false,
            stopped: None,
            checking: std::collections::BTreeMap::new(),
            names: std::collections::HashSet::new(),
            last_landed: None,
            completing: BTreeSet::new(),
            spent: Spend::ZERO,
            passed: std::collections::BTreeMap::new(),
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
            Seen::Stopped { failure } => {
                if self.stopped.is_none() {
                    self.stopped = Some(failure);
                }
            }
            Seen::Checking { owner, tree } => {
                judge.check(self.checking.insert(owner, tree).is_none(), "one check request per pending owner");
            }
            Seen::Started { contract, outcome_bytes, checks, within, delivery } => {
                judge.check(self.phase == Phase::Waiting, "one host start per scenario");
                if self.phase == Phase::Waiting {
                    judge.expect("host answer", within);
                }
                if self.phase == Phase::Waiting {
                    self.phase = Phase::Running;
                }
                self.contract = Some(contract);
                self.outcome_bytes = outcome_bytes;
                self.checks = checks;
                self.delivery = delivery;
            }
            Seen::Completing { owner } => {
                judge.check(self.phase == Phase::Running, "a completion belongs to a live host request");
                judge.check(self.completing.insert(owner), "one completion in flight per owner");
            }
            Seen::Completed { owner, spent } => {
                judge.check(self.phase == Phase::Running, "usage belongs to a live host request");
                judge.check(self.completing.remove(&owner), "one completion terminal per pending owner");
                self.spent = self.spent.saturating_add(spent);
            }
            Seen::CompletionEnded { owner } => {
                judge.check(self.completing.remove(&owner), "one completion terminal per pending owner");
            }
            Seen::Checked { owner, exit } => {
                judge.check(self.phase == Phase::Running, "checks precede the host answer");
                let tree = self.checking.remove(&owner);
                judge.check(tree.is_some(), "check terminal names pending IO");
                if exit == (Exit::Code { code: 0 }) {
                    self.passed.insert(owner, tree.unwrap_or_default());
                }
            }
            Seen::Pushing { owner, tree, name } => {
                judge.check(
                    self.phase == Phase::Running
                        && (self.delivery || self.contract.as_ref().is_some_and(|contract| contract.change.is_some())),
                    "only a live change charter pushes",
                );
                judge.check(
                    !self.checks || self.passed.remove(&owner).as_ref() == Some(&tree),
                    "delivery is the exclusive checked snapshot",
                );
                judge.check(
                    name.completion > 0 && self.names.insert(name),
                    "durable names are nonzero and distinct across actual submissions",
                );
                judge.check(self.pushing.insert(owner, (name, tree)).is_none(), "one push in flight per owner");
            }
            Seen::Delivered { owner, push, tree } => {
                let asked = self.pushing.remove(&owner);
                judge.check(asked.is_some(), "a host push terminal names a pending push");
                match push {
                    Delivery::Delivered(receipts) => {
                        judge.check(
                            asked.as_ref().map(|(_, snapshot)| snapshot) == Some(&tree),
                            "the host lands exactly the tree the agent left",
                        );
                        if let Some((name, _)) = &asked {
                            self.last_landed = Some((*name, receipts));
                        }
                        self.landed += 1;
                    }
                    Delivery::Stale | Delivery::Failed(_) | Delivery::Refused(_) | Delivery::Nothing => {
                        judge.check(tree.is_empty(), "an unsuccessful push lands nothing");
                    }
                }
            }
            Seen::Answered { answer, pending } => {
                judge.check(self.phase == Phase::Running, "exactly one answer per host start");
                judge.check(
                    pending == 0 && self.pushing.is_empty() && self.completing.is_empty() && self.checking.is_empty(),
                    "an answer waits for every request terminal",
                );
                let (spent, change) = match &answer {
                    Answer::Delivered { spent, name, receipts, stopped } => {
                        judge.check(
                            self.delivery && self.last_landed.as_ref() == Some(&(*name, receipts.clone())),
                            "interrupted mid delivery preserves its actual name and receipts",
                        );
                        judge.check(
                            self.stopped == Some(*stopped),
                            "interrupted delivery preserves the already observed stop",
                        );
                        (*spent, false)
                    }
                    Answer::Refused(_) => (Spend::ZERO, false),
                    Answer::Accepted { spent, outcome } => (*spent, matches!(outcome, Declared::Change(_))),
                    Answer::Failed { spent, .. } => (*spent, false),
                };
                judge.check(spent == self.spent, "the answer accounts for every accepted provider turn exactly once");
                judge.check(!change || self.landed == 1, "an accepted change landed exactly once");
                judge.check(
                    change || self.delivery || self.landed == 0,
                    "a non-change result only lands under a separate main grant",
                );
                if let Answer::Accepted { outcome, .. } = &answer {
                    judge.check(
                        self.contract.as_ref().is_some_and(|contract| accepted(contract, outcome, self.outcome_bytes)),
                        "an accepted result meets the host contract and byte cap",
                    );
                }
                self.phase = Phase::Answered;
                let met = judge.meet(&"host answer");
                judge.check(met, "the start's answer obligation is met once");
            }
        }
    }
}

// An independent boundary oracle: this uses only the host's Start and the
// final Answer, never the production judge or its ownership calculation.
fn accepted(contract: &OutcomeSpec, value: &Declared, max: u64) -> bool {
    let (valid, owned) = match value {
        Declared::Change(change) => (
            contract.change.as_ref().is_some_and(|rule| fields_fit(&rule.fields, &change.fields)),
            fields_owned(&change.fields),
        ),
        Declared::Report(report) => (
            contract.report.as_ref().is_some_and(|rule| text_fit(rule, &report.text, &report.fields)),
            fields_owned(&report.fields).and_then(|cost| cost.checked_add(length(report.text.len()))),
        ),
        Declared::Failure(failure) => (
            contract.failure.as_ref().is_some_and(|rule| text_fit(rule, &failure.reason, &failure.fields)),
            fields_owned(&failure.fields).and_then(|cost| cost.checked_add(length(failure.reason.len()))),
        ),
        Declared::Verdict(verdict) => {
            let valid = contract.verdicts.iter().find(|rule| rule.name == verdict.name).is_some_and(|rule| {
                length(verdict.text.len()) <= u64::from(rule.text_max)
                    && fields_fit(&rule.fields, &verdict.fields)
                    && (u64::from(rule.items.min)..=u64::from(rule.items.max)).contains(&length(verdict.items.len()))
                    && verdict.items.iter().all(|item| {
                        rule.items
                            .kinds
                            .iter()
                            .find(|kind| kind.kind == item.kind)
                            .is_some_and(|kind| fields_fit(&kind.fields, &item.fields))
                    })
            });
            let owned = fields_owned(&verdict.fields)
                .and_then(|cost| cost.checked_add(length(verdict.name.len())))
                .and_then(|cost| cost.checked_add(length(verdict.text.len())))
                .and_then(|cost| {
                    verdict.items.iter().try_fold(cost, |cost, item| {
                        cost.checked_add(length(std::mem::size_of::<Item>()))?
                            .checked_add(length(item.kind.len()))?
                            .checked_add(fields_owned(&item.fields)?)
                    })
                });
            (valid, owned)
        }
    };
    valid && owned.is_some_and(|cost| cost <= max)
}

fn text_fit(rule: &TextSpec, text: &[u8], fields: &[Field]) -> bool {
    (u64::from(rule.min)..=u64::from(rule.max)).contains(&length(text.len())) && fields_fit(&rule.fields, fields)
}

fn fields_fit(rules: &[FieldRule], fields: &[Field]) -> bool {
    fields.iter().enumerate().all(|(at, field)| fields[..at].iter().all(|previous| previous.name != field.name))
        && rules.iter().all(|rule| {
            fields
                .iter()
                .find(|field| field.name == rule.name)
                .is_some_and(|field| !field.value.is_empty() && length(field.value.len()) <= u64::from(rule.max))
        })
}

fn fields_owned(fields: &[Field]) -> Option<u64> {
    fields.iter().try_fold(0_u64, |cost, field| {
        cost.checked_add(length(std::mem::size_of::<Field>()))?
            .checked_add(length(field.name.len()))?
            .checked_add(length(field.value.len()))
    })
}

fn length(value: usize) -> u64 {
    u64::try_from(value).expect("the fixture architecture's byte length fits u64")
}
