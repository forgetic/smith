//! What the runs tell whoever watches the agent (domain/run.md, section 14):
//! a fact for each thing that happened, content-free (tokens, counts and
//! classifications, never what an LLM, the charter or the worker said), in a
//! bounded queue the parent drains at its own pace.
//!
//! Facts are outside the boundary's flow control: they are not requests, take
//! no room in `out`, and when the queue is full they are dropped and counted.
//! Nothing the run decides depends on whether a fact was kept, and nothing
//! outside may either: the worker's watchdog hears of a check's deadline from
//! `Request::Checking`, which is not lossy.
//!
//! A start refused at the entrance tells nothing: no run was. Each step of an
//! admitted run says which run it is about; the facts its requests tell are
//! derived from them in one place ([`tell`]), and the facts of what happened
//! to the run without a request (a conversation ending, a call made, checks
//! or a push ending) are told where they happen.

use skein_lib::{Queue, Slab, Time, Token};

use crate::boundary::{Answer, End, Exit, Failure, Push, Refusal, Request, Returned};
use crate::run::{self, Conversation, Run};

/// Something that happened in the run `run` (the run's token for it, as
/// `Admitted` gives it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[expect(clippy::large_enum_variant, reason = "bounded diagnostics stay inline and are included in worst_case")]
pub enum Fact {
    /// The run was admitted.
    Admitted {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
    },
    /// It looked in its checkout, and found `guides` guides and `checks`
    /// repositories with checks.
    Prepared {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Number of repository guides retained during preparation.
        guides: u32,
        /// Number of prepared checks or checks-required policy, according to the record.
        checks: u32,
    },
    /// It opened the conversation `conversation`, at `depth`: zero for main.
    Opened {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        conversation: Token,
        /// Nesting or JSON depth, bounded by the enclosing immutable limits.
        depth: u32,
    },
    /// The conversation ended, or was refused at its entrance.
    Ended {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        conversation: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        end: End,
    },
    /// A conversation made the call `call` of the run.
    Called {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        conversation: Token,
        /// Typed tool call or caller-issued delegated call identity, as named by the record.
        call: Token,
        /// Typed run tool ask, validated against the asker's authority and charter.
        ask: Asked,
    },
    /// The call returned.
    Returned {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Typed tool call or caller-issued delegated call identity, as named by the record.
        call: Token,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        result: Return,
    },
    /// Checks started, to be stopped at `deadline` at the latest.
    CheckStarted {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        deadline: Time,
    },
    /// Checks ended.
    CheckFinished {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Terminal process classification; only a zero exit code passes checks.
        exit: Exit,
    },
    /// A push ended.
    Pushed {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Typed terminal for the host's change-delivery request.
        push: Push,
    },
    /// The run answered.
    Answered {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
        /// Single terminal value returned to the caller.
        answer: Answered,
    },
}

/// What a call asked for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Asked {
    /// The LLM declared a run result.
    Finish,
    /// The LLM asked its run to open a sub-agent.
    SubAgent,
}

/// How a call returned.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Return {
    /// The run accepted its declared result.
    Accepted,
    /// The declared result failed the charter's contract.
    Rejected,
    /// Required checks did not pass.
    ChecksFailed,
    /// The host found the target branch stale.
    Moved,
    /// The host refused or failed the push and returned bounded feedback.
    Unpushed,
    /// The caller cancelled and the terminal settled.
    Cancelled,
    /// The injected operation deadline won the race.
    TimedOut,
    /// No capacity is currently available; a later call may fit.
    Busy,
    /// The sub-agent answered or the run supplied its terminal, as classified by the enclosing fact.
    Answered,
    /// The sub-agent ended without an accepted answer.
    Unanswered,
    /// The entrance or operation was refused with the enclosing typed reason.
    Refused,
}

/// How a run answered.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Answered {
    /// The entrance or operation was refused with the enclosing typed reason.
    Refused(Refusal),
    /// The run accepted its declared result.
    Accepted,
    /// One failed terminal with bounded diagnostics.
    Failed(Failure),
}

/// The facts not yet drained, how many did not fit, and the run the step
/// being taken is about.
#[derive(Debug)]
pub(crate) struct Facts {
    queue: Queue<Fact>,
    lost: u64,
    about: Option<Token>,
}

impl Facts {
    pub(crate) fn with_capacity(capacity: u32) -> Facts {
        Facts { queue: Queue::with_capacity(capacity), lost: 0, about: None }
    }

    /// Keeps `fact` if there is room for it, and counts it otherwise.
    #[expect(clippy::large_types_passed_by_value, reason = "a bounded fact moves into its fixed-capacity queue")]
    pub(crate) fn push(&mut self, fact: Fact) {
        if self.queue.try_push(fact).is_err() {
            self.lost = self.lost.saturating_add(1);
        }
    }

    pub(crate) fn pop(&mut self) -> Option<Fact> {
        self.queue.pop()
    }

    pub(crate) fn lost(&self) -> u64 {
        self.lost
    }

    /// The step being taken is about the run `run`.
    pub(crate) fn about(&mut self, run: Token) {
        self.about = Some(run);
    }

    /// A step begins, about no run yet.
    pub(crate) fn begin(&mut self) {
        self.about = None;
    }
}

/// What the step that made the requests in `out` from `mark` on tells, a fact
/// for each but the requests whose outcome is told when it comes (closes,
/// says, cancels, pushes) or that tell nothing of their own (looks).
pub(crate) fn tell(
    facts: &mut Facts,
    runs: &Slab<Run>,
    conversations: &Slab<Conversation>,
    out: &Queue<Request>,
    mark: u32,
) {
    let Some(run) = facts.about else {
        return;
    };
    let made = usize::try_from(mark).expect("a u32 fits in a usize");
    for request in out.iter().skip(made) {
        let fact = match request {
            Request::Admitted { worker: _, run } => Fact::Admitted { run: *run },
            Request::Open { conversation, opening: _ } => {
                let (depth, prepared) = run::opened(runs, conversations, *conversation);
                // Main opens once its run has prepared.
                if let Some((guides, checks)) = prepared {
                    facts.push(Fact::Prepared { run, guides, checks });
                }
                Fact::Opened { run, conversation: *conversation, depth }
            }
            Request::Return { call, result } => Fact::Returned { run, call: *call, result: result_of(result) },
            Request::Check { owner: _, program: _, deadline, tail: _ } => {
                Fact::CheckStarted { run, deadline: *deadline }
            }
            Request::Answer { to: _, answer } => Fact::Answered { run, answer: answered(answer) },
            Request::Say { .. }
            | Request::Close { .. }
            | Request::Read { .. }
            | Request::Probe { .. }
            | Request::Abort { .. }
            | Request::Checking { .. }
            | Request::Push { .. }
            | Request::CancelHost { .. } => continue,
        };
        facts.push(fact);
    }
}

fn result_of(result: &Returned) -> Return {
    match result {
        Returned::Accepted => Return::Accepted,
        Returned::Rejected { .. } => Return::Rejected,
        Returned::ChecksFailed { .. } => Return::ChecksFailed,
        Returned::Moved => Return::Moved,
        Returned::Unpushed { .. } => Return::Unpushed,
        Returned::Cancelled => Return::Cancelled,
        Returned::TimedOut => Return::TimedOut,
        Returned::Busy => Return::Busy,
        Returned::Answered { .. } => Return::Answered,
        Returned::Unanswered { .. } => Return::Unanswered,
        Returned::Refused { .. } => Return::Refused,
    }
}

fn answered(answer: &Answer) -> Answered {
    match answer {
        Answer::Refused(refusal) => Answered::Refused(*refusal),
        Answer::Accepted { .. } => Answered::Accepted,
        Answer::Failed { failure, spent: _ } => Answered::Failed(*failure),
    }
}
