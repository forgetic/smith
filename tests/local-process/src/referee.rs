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
    require_facts: bool,
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
                } else if self.require_facts {
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
                if self.require_facts && !matches!(self.ending, Ending::Unavailable) {
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
    review_with_facts(seen, checkout, ending, true)
}

/// Judge binary observations without requiring in-process service facts.
#[must_use]
pub fn review_binary(seen: &Seen, checkout: &impl CheckoutRead, ending: Ending) -> Referee<Meeting> {
    review_with_facts(seen, checkout, ending, false)
}

fn review_with_facts(
    seen: &Seen,
    checkout: &impl CheckoutRead,
    ending: Ending,
    require_facts: bool,
) -> Referee<Meeting> {
    let mut referee =
        Referee::new(Meeting { ending, require_facts, activation: None, next_turn: 1, answered: false, shown: false });
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

/// Capture only terminal, emitted facts and independent peer observations.
#[must_use]
pub fn seen(procs: &[crate::process::Proc], saved_before_query: bool, pushed: bool) -> Seen {
    use crate::process::Proc;
    let terminal =
        procs.iter().find_map(|p| if let Proc::Terminal(p) = p { Some(p.as_ref()) } else { None }).expect("terminal");
    let peer =
        procs.iter().find_map(|p| if let Proc::Peer(p) = p { Some(p.as_ref()) } else { None }).expect("provider");
    let facts = procs
        .iter()
        .find_map(|p| if let Proc::Local(p) = p { Some(p.facts().to_vec()) } else { None })
        .unwrap_or_default();
    let browser = procs.iter().find_map(|p| if let Proc::Browser(p) = p { Some(p.as_ref()) } else { None });
    let issuer = procs.iter().find_map(|p| if let Proc::Issuer(p) = p { Some(p.as_ref()) } else { None });
    Seen {
        facts,
        shown: terminal.shown().to_vec(),
        errors: terminal.errors().to_vec(),
        queries: crate::llm::queries(peer).cloned().collect(),
        exit: terminal.exit(),
        pushed,
        oauth: issuer.map(|issuer| OAuthSeen {
            posts: crate::oauth::posts(issuer.peer()),
            pages: browser.map_or(0, crate::process::BrowserProcess::pages),
            browser_replied: browser.is_some_and(crate::process::BrowserProcess::replied),
            saved_before_query,
        }),
    }
}

/// The same outside referee driven beside processes in either kernel loop.
pub struct Run {
    seed: u64,
    meeting: Referee<Meeting>,
    checkout: Box<dyn CheckoutRead>,
    token_directory: std::path::PathBuf,
    interrupt_on_query: bool,
    interrupted: bool,
    facts: usize,
    saved_before_query: bool,
    reviewed: bool,
    next_act: Option<Time>,
    shutdown: bool,
    end: Option<Time>,
}
impl Run {
    /// Prepare the scenario's policy, repository face and observed-token check.
    #[must_use]
    pub fn new(
        seed: u64,
        ending: Ending,
        checkout: Box<dyn CheckoutRead>,
        token_directory: std::path::PathBuf,
        interrupt_on_query: bool,
        keep_facts: bool,
    ) -> Self {
        Self {
            seed,
            meeting: Referee::new(Meeting {
                ending,
                require_facts: keep_facts,
                activation: None,
                next_turn: 1,
                answered: false,
                shown: false,
            }),
            checkout,
            token_directory,
            interrupt_on_query,
            interrupted: false,
            facts: 0,
            saved_before_query: false,
            reviewed: false,
            next_act: None,
            shutdown: false,
            end: None,
        }
    }
}
impl skein_world::Referee<crate::process::Proc> for Run {
    fn act(&mut self, now: Time, procs: &mut [crate::process::Proc]) {
        use crate::process::Proc;
        self.end.get_or_insert_with(|| now.saturating_add(Duration::from_secs(120)));
        self.next_act = None;
        let query = procs.iter().any(|p| matches!(p, Proc::Peer(peer) if crate::llm::queries(peer).next().is_some()));
        let exited = procs.iter().any(|p| matches!(p, Proc::Terminal(t) if t.exit().is_some()));
        let url = procs.iter().find_map(|p| {
            if let Proc::Terminal(t) = p {
                let shown = t.shown();
                shown.windows(9).position(|part| part == b"Sign in: ").and_then(|at| {
                    let url = &shown[at + 9..];
                    url.iter().position(|b| *b == b'\n').map(|end| &url[..end])
                })
            } else {
                None
            }
        });
        // Copy into the referee's stack; process storage was allocated at startup.
        let mut bytes = [0; 8192];
        let length = url.map_or(0, |url| {
            assert!(url.len() <= bytes.len());
            bytes[..url.len()].copy_from_slice(url);
            url.len()
        });
        if exited {
            self.shutdown = true;
        }
        for proc in procs {
            match proc {
                Proc::Terminal(t) => {
                    if self.interrupt_on_query && query && !self.interrupted {
                        t.interrupt();
                        self.interrupted = true;
                    }
                }
                Proc::Peer(peer) => {
                    if exited {
                        peer.shutdown();
                    }
                }
                Proc::Issuer(issuer) => {
                    if exited {
                        issuer.shutdown();
                    }
                }
                Proc::Browser(browser) => {
                    if browser.pages() == 0 && length > 0 {
                        browser.visit(&bytes[..length]);
                    }
                    if exited {
                        browser.shutdown();
                    }
                }
                Proc::Local(_) | Proc::Agent(_, _) | Proc::Git(_) => {}
            }
        }
    }
    fn observe(&mut self, now: Time, procs: &[crate::process::Proc]) {
        use crate::process::Proc;
        use skein_world::Host;
        let mut stimuli = Vec::new();
        if let Some(local) = procs.iter().find_map(|p| if let Proc::Local(p) = p { Some(p.as_ref()) } else { None }) {
            for fact in &local.facts()[self.facts..] {
                self.meeting.observe(now, Observation::Fact(*fact), &mut stimuli);
            }
            self.facts = local.facts().len();
        }
        let peer =
            procs.iter().find_map(|p| if let Proc::Peer(p) = p { Some(p.as_ref()) } else { None }).expect("provider");
        assert!(crate::llm::queries(peer).count() <= 64, "bounded provider work");
        let authenticated = procs.iter().any(|p| matches!(p, Proc::Issuer(_)));
        if authenticated && !self.saved_before_query && crate::llm::queries(peer).next().is_some() {
            let tokens = smith_local_shell::local_tokens::Tokens::new(
                &self.token_directory,
                smith_local_shell::local_host::token_limits(),
            )
            .expect("private store");
            assert_eq!(
                tokens.load(0).expect("record").expect("saved before grant use").access_token.as_ref(),
                b"access-new"
            );
            self.saved_before_query = true;
        }
        let terminal = procs
            .iter()
            .find_map(|p| if let Proc::Terminal(p) = p { Some(p.as_ref()) } else { None })
            .expect("terminal");
        let visit = procs.iter().any(|p| matches!(p,Proc::Browser(browser) if browser.pages()==0))
            && terminal
                .shown()
                .windows(9)
                .position(|bytes| bytes == b"Sign in: ")
                .is_some_and(|at| terminal.shown()[at + 9..].contains(&b'\n'));
        if (self.interrupt_on_query && !self.interrupted && crate::llm::queries(peer).next().is_some())
            || visit
            || (terminal.exit().is_some() && !self.shutdown)
        {
            self.next_act = Some(now);
        }
        let local_settled = procs.iter().any(|p| matches!(p,Proc::Local(local) if local.is_empty()));
        let browser_settled = procs.iter().filter(|p| matches!(p, Proc::Browser(_))).all(Host::is_empty);
        if !self.reviewed && terminal.is_empty() && local_settled && browser_settled {
            let observed = seen(procs, self.saved_before_query, false);
            self.meeting.observe(
                now,
                Observation::Terminal {
                    shown: observed.shown,
                    errors: observed.errors,
                    exit: observed.exit,
                    queries: observed.queries.len(),
                },
                &mut stimuli,
            );
            self.meeting.observe(
                now,
                Observation::Peer { queries: observed.queries, oauth: observed.oauth },
                &mut stimuli,
            );
            let head = self.checkout.head();
            self.meeting.observe(
                now,
                Observation::Commit {
                    message: head.as_ref().map_or_else(Vec::new, |head| self.checkout.message(head)),
                    files: head.as_ref().map_or_else(Tree::new, |head| self.checkout.files(head)),
                    head,
                    pushed: false,
                },
                &mut stimuli,
            );
            self.reviewed = true;
        }
        if self.meeting.is_due(now) {
            self.meeting.fire(now, &mut stimuli);
        }
        if let skein_world::domain::Verdict::Failed(why) = self.meeting.verdict() {
            panic!("seed {}: {why:?}", self.seed);
        }
    }
    fn next_deadline(&self) -> Option<Time> {
        [self.next_act, self.meeting.next_deadline(), self.end.filter(|_| !self.reviewed)].into_iter().flatten().min()
    }
    fn overdue(&self, now: Time) -> Option<String> {
        (self.end.is_some_and(|end| now >= end) && !self.reviewed)
            .then(|| "local invocation did not settle its observed terminal".into())
    }
    fn passed(&self) -> bool {
        self.reviewed && matches!(self.meeting.verdict(), skein_world::domain::Verdict::Passed)
    }
}
