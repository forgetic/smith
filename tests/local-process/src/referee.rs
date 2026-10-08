//! Independent policy over terminal bytes, decoded peer requests and service
//! facts. Repository access is only head/message/files; the referee never
//! reads Sim or Machine (protocol/hosts.md, section 7).

use skein_fake_checkout::git::Tree;
use skein_fake_llm_domain::api::Query;
use skein_io::kernel;
use skein_lib::{Duration, Time};
use skein_world::domain::{Expectations, Judge, Referee};
use smith_local_domain::Fact;

/// Repository observations implemented by a fake checkout or a future real one.
pub trait CheckoutRead {
    fn head(&self) -> Option<u64>;
    fn message(&self, commit: u64) -> Vec<u8>;
    fn files(&self, commit: u64) -> Tree;
}

/// Concrete observations collected independently of process state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub facts: Vec<Fact>,
    pub shown: Vec<u8>,
    pub errors: Vec<u8>,
    pub queries: Vec<Query>,
    pub exit: Option<kernel::Exit>,
    pub pushed: bool,
}

/// One expected story ending supplied by its author.
#[derive(Clone, Debug)]
pub enum Ending {
    Report(Vec<u8>),
    Cancelled,
    Change,
}

/// Evidence handed to the shared judgment kit, never internal process cells.
#[derive(Clone, Debug)]
pub enum Observation {
    Fact(Fact),
    Terminal { shown: Vec<u8>, errors: Vec<u8>, exit: Option<kernel::Exit>, queries: usize },
    Commit { head: Option<u64>, message: Vec<u8>, files: Tree, pushed: bool },
}

/// Obligations and ordering from one invocation's visible facts.
pub struct Meeting {
    ending: Ending,
    activation: Option<u64>,
    next_turn: u32,
    answered: bool,
    shown: bool,
}

impl Expectations for Meeting {
    type Seen = Observation;
    type Name = &'static str;
    type Stimulus = ();

    fn observe(&mut self, observation: Observation, judge: &mut Judge<Self::Name, Self::Stimulus>) {
        match observation {
            Observation::Fact(fact) => match fact {
                Fact::Started { activation } => {
                    judge.check(self.activation.is_none(), "one activation per invocation");
                    self.activation = Some(activation);
                    judge.expect("terminal", Duration::from_secs(60));
                }
                Fact::Message { .. } => {
                    judge.check(self.activation.is_some() && !self.answered, "message belongs to a live activation")
                }
                Fact::Turn { number } => {
                    judge.check(
                        self.activation.is_some() && !self.answered && number == self.next_turn,
                        "consecutive activation-local turns before answer",
                    );
                    self.next_turn += 1;
                }
                Fact::Answered { activation } => {
                    judge
                        .check(self.activation == Some(activation) && !self.answered, "one answer for this activation");
                    self.answered = true;
                }
                Fact::Shown { activation } => {
                    judge.check(
                        self.activation == Some(activation) && self.answered && !self.shown,
                        "one shown answer follows its answer fact",
                    );
                    self.shown = true;
                }
                Fact::DeliveryReturned { name } => {
                    judge.check(self.activation == Some(name.activation), "delivery belongs to this activation")
                }
            },
            Observation::Terminal { shown, errors, exit, queries } => {
                let expected = match &self.ending {
                    Ending::Report(text) => text.as_slice(),
                    Ending::Cancelled => b"Run cancelled",
                    Ending::Change => b"Change delivered",
                };
                judge.check(self.shown && self.answered, "the invocation actually answered and showed it");
                judge.check(exit == Some(kernel::Exit::Code(0)), "successful shell exit");
                judge.check(queries > 0, "the fake provider saw an actual request");
                judge.check(errors.is_empty(), "no startup error output");
                judge.check(shown.windows(expected.len()).any(|part| part == expected), "expected terminal text");
                judge.check(
                    !shown.windows(7).any(|part| part == b"refresh") && !shown.windows(5).any(|part| part == b"token"),
                    "terminal never reveals credentials",
                );
                let met = judge.meet(&"terminal");
                judge.check(met, "terminal settles the activation");
            }
            Observation::Commit { head, message, files, pushed } => {
                if matches!(self.ending, Ending::Change) {
                    judge.check(head.is_some_and(|head| head > 1), "a new local commit exists");
                    judge.check(
                        message.starts_with(b"Updated result\n\nCreated the result file"),
                        "result fields form the message",
                    );
                    judge.check(
                        message.windows(15).any(|part| part == b"Smith-Delivery:"),
                        "commit carries its delivery identity",
                    );
                    judge.check(
                        files.get(b"result.txt".as_slice()).is_some_and(|bytes| bytes == b"new\n"),
                        "commit contains the agent's change",
                    );
                } else {
                    judge.check(head.is_none(), "report or cancellation has no repository effect");
                }
                judge.check(!pushed, "no push without configuration");
            }
        }
    }
}

/// Judge only supplied observations and the checkout's narrow read interface.
#[must_use]
pub fn review(seen: &Seen, checkout: &impl CheckoutRead, ending: Ending) -> Referee<Meeting> {
    let mut referee = Referee::new(Meeting { ending, activation: None, next_turn: 1, answered: false, shown: false });
    let mut stimuli = Vec::new();
    for fact in &seen.facts {
        referee.observe(Time::ZERO, Observation::Fact(*fact), &mut stimuli);
    }
    referee.observe(
        Time::ZERO,
        Observation::Terminal {
            shown: seen.shown.clone(),
            errors: seen.errors.clone(),
            exit: seen.exit,
            queries: seen.queries.len(),
        },
        &mut stimuli,
    );
    let head = checkout.head();
    referee.observe(
        Time::ZERO,
        Observation::Commit {
            head,
            message: head.map_or_else(Vec::new, |head| checkout.message(head)),
            files: head.map_or_else(Tree::new, |head| checkout.files(head)),
            pushed: seen.pushed,
        },
        &mut stimuli,
    );
    referee
}
