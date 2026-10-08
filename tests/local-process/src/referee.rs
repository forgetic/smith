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
    fn head(&self) -> Option<Vec<u8>>;
    fn message(&self, commit: &[u8]) -> Vec<u8>;
    fn files(&self, commit: &[u8]) -> Tree;
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
    pub oauth: Option<OAuthSeen>,
}

/// Facts observed by the fake issuer, browser and first provider request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthSeen {
    pub posts: u64,
    pub pages: u32,
    pub browser_replied: bool,
    pub saved_before_query: bool,
}

/// One expected story ending supplied by its author.
#[derive(Clone, Debug)]
pub enum Ending {
    Report(Vec<u8>),
    Cancelled,
    Change { before: Vec<u8> },
    Unavailable,
}

/// Evidence handed to the shared judgment kit, never internal process cells.
#[derive(Clone, Debug)]
pub enum Observation {
    Fact(Fact),
    Peer { queries: Vec<Query>, oauth: Option<OAuthSeen> },
    Terminal { shown: Vec<u8>, errors: Vec<u8>, exit: Option<kernel::Exit>, queries: usize },
    Commit { head: Option<Vec<u8>>, message: Vec<u8>, files: Tree, pushed: bool },
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
                    Ending::Change { .. } => b"Change delivered",
                    Ending::Unavailable => b"account is unavailable",
                };
                if matches!(self.ending, Ending::Unavailable) {
                    judge.check(
                        self.activation.is_none() && !self.answered && !self.shown,
                        "a refused account starts no run",
                    );
                } else {
                    judge.check(self.shown && self.answered, "the invocation actually answered and showed it");
                }
                judge.check(exit == Some(kernel::Exit::Code(0)), "successful shell exit");
                judge.check(
                    if matches!(self.ending, Ending::Unavailable) { queries == 0 } else { queries > 0 },
                    "provider work matches account availability",
                );
                judge.check(errors.is_empty(), "no startup error output");
                judge.check(shown.windows(expected.len()).any(|part| part == expected), "expected terminal text");
                judge.check(
                    !shown.windows(7).any(|part| part == b"refresh") && !shown.windows(5).any(|part| part == b"token"),
                    "terminal never reveals credentials",
                );
                if !matches!(self.ending, Ending::Unavailable) {
                    let met = judge.meet(&"terminal");
                    judge.check(met, "terminal settles the activation");
                }
            }
            Observation::Peer { queries, oauth } => {
                judge.check(queries.len() <= 64, "bounded observed provider requests");
                for query in &queries {
                    judge.check(
                        query.model.as_ref() == b"fake" && query.max_tokens > 0,
                        "configured model reaches the provider and its declared ceiling is positive",
                    );
                    judge.check(
                        query.system.windows(12).any(|part| part == b"@local-shell"),
                        "configured instructions reach the provider",
                    );
                }
                if let Some(oauth) = oauth {
                    judge.check(oauth.posts == 1 && oauth.pages <= 1, "one token exchange and at most one page visit");
                    judge.check(oauth.pages == 0 || oauth.browser_replied, "the browser heard the loopback reply");
                    judge.check(
                        queries.is_empty() || oauth.saved_before_query,
                        "token saved before the peer saw grant use",
                    );
                }
            }
            Observation::Commit { head, message, files, pushed } => {
                if let Ending::Change { before } = &self.ending {
                    judge.check(head.as_ref().is_some_and(|head| head != before), "a new local commit exists");
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
    referee.observe(
        Time::ZERO,
        Observation::Peer { queries: seen.queries.clone(), oauth: seen.oauth.clone() },
        &mut stimuli,
    );
    let head = checkout.head();
    referee.observe(
        Time::ZERO,
        Observation::Commit {
            message: head.as_ref().map_or_else(Vec::new, |head| checkout.message(head)),
            files: head.as_ref().map_or_else(Tree::new, |head| checkout.files(head)),
            head,
            pushed: seen.pushed,
        },
        &mut stimuli,
    );
    referee
}
