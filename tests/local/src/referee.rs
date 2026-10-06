//! Independent checks over the local host's typed boundaries and content-free
//! facts (domain/host.md, sections 2, 6 and 8–10).

use std::collections::{BTreeMap, BTreeSet};

use skein_lib::Token;
use skein_world::domain::{Expectations, Judge};

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
}

impl Meeting {
    /// Start from a store's last durable counters when invoking a fresh domain.
    #[must_use]
    pub fn after(activation: u64, message: u64) -> Self {
        Self { last_activation: activation, last_message: message, ..Self::default() }
    }
}

impl Expectations for Meeting {
    type Seen = Seen;
    type Name = &'static str;
    type Stimulus = ();

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
        }
    }
}
