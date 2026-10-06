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
//! or a delivery ending) are told where they happen.

use skein_lib::{Queue, Slab, Time, Token};

use crate::boundary::{Answer, End, Exit, Failure, Refusal, Request, Returned};
use crate::run::{self, Conversation, Run};

/// Something that happened in the run `run` (the run's token for it, as
/// `Admitted` gives it).
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fact {
    /// The run was admitted.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Admitted {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
    },
    /// It looked in its checkout, and found `guides` guides and `checks`
    /// repositories with checks.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Prepared {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Number of repository guides retained during preparation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        guides: u32,
        /// Number of prepared repository check executables.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        checks: u32,
    },
    /// It opened the conversation `conversation`, at `depth`: zero for main.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Opened {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Nesting or JSON depth, bounded by the enclosing immutable limits.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        depth: u32,
    },
    /// The conversation ended, or was refused at its entrance.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Ended {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        end: End,
    },
    /// A conversation made the call `call` of the run.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Called {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Run-issued call token, identifying the observed finish or sub-agent ask.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        call: Token,
        /// Typed run tool ask, validated against the asker's authority and charter.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        ask: Asked,
    },
    /// The call returned.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Returned {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Run-issued call token, identifying the observed finish or sub-agent ask.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        call: Token,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        result: Return,
    },
    /// Checks started, to be stopped at `deadline` at the latest.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    CheckStarted {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        deadline: Time,
    },
    /// Checks ended.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    CheckFinished {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Terminal process classification; only a zero exit code passes checks.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        exit: Exit,
    },
    /// An actual host delivery ended with its content-free classification.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Delivered {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Typed terminal for the host's change-delivery request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        push: crate::DeliveryStatus,
    },
    /// The run answered.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Answered {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
        /// Single terminal value returned to the caller.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        answer: Answered,
    },
}

/// What a call asked for.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Asked {
    /// Main requested settled waiting. Contract: domain/run.md, section 6.
    Wait,
    /// An opaque declared host-tool call. Contract: domain/run.md, sections 5.2 and 11.
    Host,
    /// Main requested separately granted delivery. Contract: domain/run.md, sections 8.4 and 12.
    Deliver,
    /// The LLM declared a run result.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Finish,
    /// The LLM asked its run to open a sub-agent.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    SubAgent,
}

/// How a call returned.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Return {
    /// Main wait intent accepted. Contract: domain/run.md, section 6.
    Waiting,
    /// Actual bounded host text reached the caller. Contract: domain/run.md, sections 5.2 and 11.
    HostAnswered,
    /// Relay settled but no permitted recovery can learn its outcome. Contract: domain/run.md, sections 5.2 and 11.
    HostUnknown,
    /// Host declaration/input refused before effects. Contract: domain/run.md, sections 5.2 and 11.
    HostRejected,
    /// Actual host delivery evidence. Contract: domain/run.md, sections 8.2 and 12.
    Delivered,
    /// No changed directory. Contract: domain/run.md, sections 8.2 and 12.
    Nothing,
    /// Named correctable host refusal. Contract: domain/run.md, sections 8.2 and 12.
    DeliveryRefused,
    /// The run accepted its declared result.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Accepted,
    /// The declared result failed the charter's contract.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Rejected,
    /// Required checks did not pass.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    ChecksFailed,
    /// The host found its delivery context stale.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Stale,
    /// The actual host delivery failed and returned bounded feedback.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    DeliveryFailed,
    /// The caller cancelled and the terminal settled.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Cancelled,
    /// The injected operation deadline won the race.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TimedOut,
    /// No capacity is currently available; a later call may fit.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Busy,
    /// The sub-agent answered or the run supplied its terminal, as classified by the enclosing fact.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Answered,
    /// The sub-agent ended without an accepted answer.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Unanswered,
    /// The entrance or operation was refused with the enclosing typed reason.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Refused,
}

/// How a run answered.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Answered {
    /// Settled idle main parked. Contract: domain/run.md, section 6.
    Parked,
    /// The entrance or operation was refused with the enclosing typed reason.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Refused(Refusal),
    /// The run accepted its declared result.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Accepted,
    /// One failed terminal with bounded diagnostics.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Failed(Failure),
}

/// The facts not yet drained, how many did not fit, and the run the step
/// being taken is about.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
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
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
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
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn about(&mut self, run: Token) {
        self.about = Some(run);
    }

    /// A step begins, about no run yet.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn begin(&mut self) {
        self.about = None;
    }
}

/// What the step that made the requests in `out` from `mark` on tells, a fact
/// for each but the requests whose outcome is told when it comes (closes,
/// says, cancels, deliveries) or that tell nothing of their own (looks).
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
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
            Request::Return { call, result, .. } => Fact::Returned { run, call: *call, result: result_of(result) },
            Request::Check { owner: _, program: _, deadline, tail: _ } => {
                Fact::CheckStarted { run, deadline: *deadline }
            }
            Request::Answer { to: _, answer } => Fact::Answered { run, answer: answered(answer) },
            Request::Waiting { .. }
            | Request::Turn { .. }
            | Request::HostCall { .. }
            | Request::WithdrawHost { .. }
            | Request::Say { .. }
            | Request::Close { .. }
            | Request::Read { .. }
            | Request::Probe { .. }
            | Request::Abort { .. }
            | Request::Checking { .. }
            | Request::Deliver { .. } => continue,
        };
        facts.push(fact);
    }
}

fn result_of(result: &Returned) -> Return {
    match result {
        Returned::Waiting => Return::Waiting,
        Returned::HostAnswered(_) => Return::HostAnswered,
        Returned::HostUnknown => Return::HostUnknown,
        Returned::HostRejected(_) => Return::HostRejected,
        Returned::Delivered(_) => Return::Delivered,
        Returned::Nothing => Return::Nothing,
        Returned::DeliveryRefused(_) => Return::DeliveryRefused,
        Returned::Accepted => Return::Accepted,
        Returned::Rejected { .. } => Return::Rejected,
        Returned::ChecksFailed { .. } => Return::ChecksFailed,
        Returned::Stale => Return::Stale,
        Returned::DeliveryFailed { .. } => Return::DeliveryFailed,
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
        Answer::Parked { .. } => Answered::Parked,
        Answer::Refused(refusal) => Answered::Refused(*refusal),
        Answer::Accepted { .. } => Answered::Accepted,
        Answer::Failed { failure, .. } => Answered::Failed(*failure),
    }
}
