//! Independent checks over the local host's typed boundaries and content-free
//! facts (domain/host.md, sections 2, 6 and 8–10).

use std::collections::{BTreeMap, BTreeSet};

use skein_fake_checkout::git::Tree;
use skein_lib::Token;
use skein_world::domain::{Expectations, Judge};
use smith_domain::run::CallName;
use smith_local_domain::{DeliveryRecord, DeliveryState};

/// A boundary observation made by the scripted person, store or provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// The store was asked to save a new activation and message counter.
    Activation { number: u64, message: u64 },
    /// A durable message was handed to the agent.
    Message { name: Token },
    /// The agent told a turn.
    Turn { number: u32 },
    /// The host asked the store to save a turn.
    SaveTurn { number: u32, read: Option<Token> },
    /// Delivery records remaining after an atomic turn save.
    StoredAnswers { activation: u64, sequence: u32, names: Vec<CallName> },
    /// The store acknowledged a turn.
    TurnSaved { number: u32 },
    /// The agent produced one answer.
    Answered { activation: u64 },
    /// The host showed that answer to the person.
    Shown { activation: u64 },
    /// A provider request was forwarded to the scripted provider.
    Complete { owner: Token },
    /// One provider terminal was delivered to the host.
    Completed { owner: Token },
    /// One delivery answer reached the typed store.
    DeliveryRecorded { name: CallName, intent: bool, receipts: Vec<u32> },
    /// A delivered target's remote names the local committed head.
    DeliveredTarget { remote_matches: bool },
    /// The store confirmed the record is durable.
    DeliverySaved { name: CallName, intent: bool },
    /// The local domain then returned that answer to the child.
    DeliveryReturned { name: CallName },
    /// A writable directory's check passed with this working tree.
    Checked { directory: u32, tree: Tree },
    /// The host committed this working tree in the named directory.
    Committed { name: CallName, directory: u32, tree: Tree },
    /// The agent successfully stored a file beneath this root.
    Wrote { root: u64 },
}

/// Scenario policy populated only from observations, never domain state.
#[derive(Debug, Default)]
pub struct Meeting {
    last_activation: u64,
    has_activation: bool,
    last_message: u64,
    last_relayed: u64,
    last_read: u64,
    next_turn: u32,
    told: BTreeSet<u32>,
    pending_turns: BTreeSet<u32>,
    saved: BTreeSet<u32>,
    relayed: BTreeSet<Token>,
    completions: BTreeSet<Token>,
    answered: BTreeMap<u64, u32>,
    shown: BTreeSet<u64>,
    recorded: BTreeSet<(u64, u32, u32)>,
    intents: BTreeSet<(u64, u32, u32)>,
    pending_intents: BTreeSet<(u64, u32, u32)>,
    pending_answers: BTreeSet<(u64, u32, u32)>,
    saved_deliveries: BTreeSet<(u64, u32, u32)>,
    commits: BTreeMap<(u64, u32, u32), BTreeSet<u32>>,
    checked: BTreeMap<u32, Tree>,
    writable: BTreeSet<u32>,
    writable_roots: BTreeSet<u64>,
}

impl Meeting {
    /// Start from a store's last durable counters when invoking a fresh domain.
    #[must_use]
    pub fn after(activation: u64, message: u64) -> Self {
        Self { last_activation: activation, last_message: message, ..Self::default() }
    }

    /// Supply the fixture's writable mount ordinals and IO roots.
    #[must_use]
    pub fn writable(mut self, directories: &[u32], roots: &[u64]) -> Self {
        self.writable.extend(directories);
        self.writable_roots.extend(roots);
        self
    }

    /// Seed independent observations that survived an earlier invocation.
    #[must_use]
    pub fn prior_delivery<'a>(
        mut self,
        records: impl Iterator<Item = &'a DeliveryRecord>,
        commits: &[(CallName, u32)],
    ) -> Self {
        for record in records {
            let key = call_key(record.name);
            match &record.state {
                DeliveryState::Intent(_) => {
                    self.intents.insert(key);
                }
                DeliveryState::Answer(_) => {
                    self.recorded.insert(key);
                    self.saved_deliveries.insert(key);
                }
            }
        }
        for (name, directory) in commits {
            self.commits.entry(call_key(*name)).or_default().insert(*directory);
        }
        self
    }
}

impl Expectations for Meeting {
    type Seen = Seen;
    type Name = &'static str;
    type Stimulus = ();

    #[expect(clippy::too_many_lines, reason = "one referee keeps every local boundary event in order")]
    fn observe(&mut self, seen: Seen, judge: &mut Judge<Self::Name, Self::Stimulus>) {
        match seen {
            Seen::Activation { number, message } => {
                judge.check(number >= self.last_activation, "activation never goes backwards");
                judge.check(
                    self.has_activation || number > self.last_activation,
                    "activations strictly increase across invocations",
                );
                judge.check(message == self.last_message + 1, "each saved line gets one increasing name");
                if number != self.last_activation {
                    judge.check(number == self.last_activation + 1, "activations strictly increase across invocations");
                    self.last_activation = number;
                    self.next_turn = 0;
                    self.told.clear();
                    self.pending_turns.clear();
                    self.saved.clear();
                }
                self.has_activation = true;
                self.last_message = message;
            }
            Seen::Message { name } => {
                judge.check(self.relayed.insert(name), "a person line is relayed only once");
                judge.check(name.raw() <= self.last_message, "a relayed line was saved first");
                judge.check(name.raw() > self.last_relayed, "person lines reach the agent in order");
                self.last_relayed = name.raw();
            }
            Seen::Turn { number } => {
                judge.check(!self.shown.contains(&self.last_activation), "nothing is told after the shown answer");
                judge.check(number == self.next_turn + 1, "turns form an ordered prefix without gaps");
                self.next_turn = number;
                self.told.insert(number);
            }
            Seen::SaveTurn { number, read } => {
                judge.check(!self.shown.contains(&self.last_activation), "nothing is saved after the shown answer");
                judge.check(self.told.remove(&number), "every save answers one told turn");
                judge.check(self.pending_turns.insert(number), "a turn has one save request");
                if let Some(read) = read {
                    judge.check(self.relayed.contains(&read), "a read fence names a relayed person line");
                    judge.check(read.raw() >= self.last_read, "the read fence never moves backwards");
                    self.last_read = read.raw();
                }
            }
            Seen::StoredAnswers { activation, sequence, names } => {
                judge.check(
                    names.iter().all(|name| name.activation == activation && name.completion > sequence),
                    "no answer remains after a saved turn follows it",
                );
            }
            Seen::TurnSaved { number } => {
                judge.check(self.pending_turns.remove(&number), "one store terminal per turn save");
                judge.check(
                    number == u32::try_from(self.saved.len()).unwrap_or(u32::MAX) + 1,
                    "turns become durable in order",
                );
                self.saved.insert(number);
            }
            Seen::Answered { activation } => {
                judge.check(activation == self.last_activation, "the answer names the active run");
                judge.check(self.answered.insert(activation, self.next_turn).is_none(), "one answer per run");
            }
            Seen::Shown { activation } => {
                judge.check(self.answered.contains_key(&activation), "only an answer is shown as a result");
                judge.check(self.pending_turns.is_empty(), "the answer is shown after all turns are saved");
                judge.check(self.shown.insert(activation), "an answer is shown once");
            }
            Seen::Complete { owner } => {
                judge.check(self.completions.insert(owner), "one completion request per pending owner");
            }
            Seen::Completed { owner } => {
                judge.check(self.completions.remove(&owner), "one terminal per completion request");
            }
            Seen::DeliveryRecorded { name, intent, receipts } => {
                let key = call_key(name);
                if intent {
                    judge.check(self.intents.insert(key), "one durable intent per delivery name");
                    self.pending_intents.insert(key);
                } else {
                    judge.check(self.recorded.insert(key), "one durable decision per delivery name");
                    let included: BTreeSet<u32> = receipts.into_iter().collect();
                    let committed = self.commits.get(&key).cloned().unwrap_or_default();
                    judge.check(committed.is_subset(&included), "every named commit is in the saved delivery answer");
                    self.pending_answers.insert(key);
                }
            }
            Seen::DeliveredTarget { remote_matches } => {
                judge.check(remote_matches, "delivered target names its commit on the fake remote");
            }
            Seen::DeliverySaved { name, intent } => {
                let key = call_key(name);
                if intent {
                    judge.check(self.pending_intents.remove(&key), "a delivery intent has one store terminal");
                } else {
                    judge.check(self.pending_answers.remove(&key), "a delivery answer has one store terminal");
                    judge.check(self.saved_deliveries.insert(key), "one store terminal per delivery record");
                }
            }
            Seen::DeliveryReturned { name } => {
                judge.check(
                    self.saved_deliveries.contains(&call_key(name)),
                    "delivery is recorded before the child hears it",
                );
            }
            Seen::Checked { directory, tree } => {
                self.checked.insert(directory, tree);
            }
            Seen::Committed { name, directory, tree } => {
                let key = call_key(name);
                judge.check(
                    self.intents.contains(&key) && !self.pending_intents.contains(&key),
                    "a named commit follows its durable intent",
                );
                judge
                    .check(self.commits.entry(key).or_default().insert(directory), "one commit per delivery directory");
                judge.check(self.writable.contains(&directory), "delivery writes only writable directories");
                if let Some(checked) = self.checked.get(&directory) {
                    judge.check(&tree == checked, "delivery commits exactly the checked tree");
                }
            }
            Seen::Wrote { root } => {
                judge.check(self.writable_roots.contains(&root), "agent writes only writable roots");
            }
        }
    }
}

fn call_key(name: CallName) -> (u64, u32, u32) {
    (name.activation, name.completion, name.position)
}
