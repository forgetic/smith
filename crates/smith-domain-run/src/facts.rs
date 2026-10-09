//! What the runs tell whoever watches the agent (domain/run.md, section 11):
//! a fact for each thing that happened, content-free (tokens, counts and
//! classifications, never what an LLM, the charter or the host said), in a
//! bounded queue the parent drains at its own pace.
//!
//! Facts are a separate step output. The parent reserves [`crate::max_facts`]
//! slots before each entrance, holding work while its drain is behind; every
//! observation is preserved. Facts change no decision: the host's watchdog
//! hears of a check's deadline from `Request::Checking`.
//!
//! A start refused at the entrance tells nothing: no run was. Each step of an
//! admitted run says which run it is about; the facts its requests tell are
//! derived from them in one place ([`tell`]), and the facts of what happened
//! to the run without a request (a conversation ending, a call made, checks
//! or a delivery ending) are told where they happen.

use skein_lib::{Queue, Slab, Time, Token};

use crate::boundary::{Answer, End, Exit, Failure, Refusal, Request, Returned};
use crate::run::{self, Conversation, Run};

/// An observation stamped with the injected time of its emitting step.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Fact {
    /// Emission time, preserved when parents drain this observation later.
    pub at: Time,
    /// The content-free observation.
    pub kind: FactKind,
}

/// Something that happened in the run `run` (the run's token for it, as
/// `Admitted` gives it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FactKind {
    /// Each host message covered by an actual told main turn.
    MessageRead { run: Token, name: Token, turn: u32 },
    /// The final ordered read fence of one actual told main turn.
    MessageFence { run: Token, turn: u32, read: Option<Token> },
    /// An accepted host message ended unread at the run answer.
    MessageUnread { run: Token, name: Token },
    /// A host message entered the bounded inbox, counted as rendered bytes.
    MessageReceived { run: Token, name: Token, bytes: u64 },
    /// A host message ended refused before retention, counted as rendered bytes.
    MessageRefused { run: Token, name: Token, bytes: u64, reason: crate::MessageRefusal },
    /// The run was admitted.
    Admitted { run: Token, resumed: bool },
    /// It looked in its checkout, and found `guides` guides and `checks`
    /// repositories with checks.
    Prepared {
        run: Token,
        /// Number of repository guides retained during preparation.
        guides: u32,
        /// Number of prepared repository check executables.
        checks: u32,
    },
    /// It opened the main conversation or a child conversation.
    Opened {
        run: Token,
        conversation: Token,
        /// Whether this is a child conversation.
        child: bool,
        parent: Option<Token>,
        call: Option<Token>,
    },
    /// The conversation ended, or was refused at its entrance.
    Ended {
        run: Token,
        conversation: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        end: End,
    },
    /// A conversation made the call `call` of the run.
    Called {
        run: Token,
        conversation: Token,
        /// Run-issued call token, identifying the observed finish or sub-agent ask.
        call: Token,
        /// Typed run tool ask, validated against the asker's authority and charter.
        ask: Asked,
    },
    /// The call returned.
    Returned {
        run: Token,
        /// Run-issued call token, identifying the observed finish or sub-agent ask.
        call: Token,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        result: Return,
    },
    /// Checks started, to be stopped at `deadline` at the latest.
    CheckStarted { run: Token, deadline: Time },
    /// Checks ended.
    CheckFinished {
        run: Token,
        /// Terminal process classification; only a zero exit code passes checks.
        exit: Exit,
    },
    /// An host delivery ended with its content-free classification.
    Delivered { run: Token, status: crate::DeliveryStatus },
    /// The run answered.
    Answered {
        run: Token,
        /// Single terminal value returned to the caller.
        answer: Answered,
    },
}

/// What a call asked for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Asked {
    /// Main requested settled waiting.
    Wait,
    /// An opaque declared host-tool call.
    Host,
    /// Main requested separately granted delivery.
    Deliver,
    /// The LLM declared a run result.
    Finish,
    /// The LLM asked its run to open a sub-agent.
    SubAgent,
}

/// How a call returned.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Return {
    /// Main wait intent accepted.
    Waiting,
    /// Actual bounded host text reached the caller.
    HostAnswered,
    /// The host decided, but its answer exceeded the receiving cap.
    HostTooLarge,
    /// Relay settled but no permitted recovery can learn its outcome.
    HostUnknown,
    /// Host declaration/input refused before effects.
    HostRejected,
    /// Actual host delivery evidence.
    Delivered,
    /// No changed directory.
    Nothing,
    /// Named correctable host refusal.
    DeliveryRefused,
    /// The run accepted its declared result.
    Accepted,
    /// The declared result failed the charter's contract.
    Rejected,
    /// Required checks did not pass.
    ChecksFailed,
    /// The host found its delivery context stale.
    Stale,
    /// The host delivery failed and returned bounded feedback.
    DeliveryFailed,
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
    /// Settled idle main parked.
    Parked,
    /// The entrance or operation was refused with the enclosing typed reason.
    Refused(Refusal),
    /// The run accepted its declared result.
    Accepted,
    /// One failed terminal with bounded diagnostics.
    Failed(Failure),
}

/// The facts not yet drained and the run the step
/// being taken is about.
#[derive(Debug)]
pub(crate) struct Facts {
    queue: Queue<Fact>,
    now: Time,
    about: Option<Token>,
}

impl Facts {
    pub(crate) fn with_capacity(capacity: u32) -> Facts {
        Facts { queue: Queue::with_capacity(capacity), now: Time::ZERO, about: None }
    }

    /// Keeps one fact in the room the parent reserved for this entry point.
    pub(crate) fn push(&mut self, kind: FactKind) {
        let fact = Fact { at: self.now, kind };
        self.queue.push(fact);
    }

    pub(crate) fn pop(&mut self) -> Option<Fact> {
        self.queue.pop()
    }

    pub(crate) fn room(&self) -> u32 {
        self.queue.room()
    }

    /// The step being taken is about the run `run`.
    pub(crate) fn about(&mut self, run: Token) {
        self.about = Some(run);
    }

    /// A step begins, about no run yet.
    pub(crate) fn begin(&mut self, now: Time) {
        self.now = now;
        self.about = None;
    }
}

/// What the step that made the requests in `out` from `mark` on tells, a fact
/// for each but the requests whose outcome is told when it comes (closes,
/// says, cancels, deliveries) or that tell nothing of their own (looks).
pub(crate) fn tell(
    facts: &mut Facts,
    runs: &Slab<Run>,
    conversations: &Slab<Conversation>,
    calls: &crate::call::Calls,
    out: &Queue<Request>,
    mark: u32,
) {
    let Some(run) = facts.about else {
        return;
    };
    let made = usize::try_from(mark).expect("a u32 fits in a usize");
    for request in out.iter().skip(made) {
        let fact = match request {
            Request::Admitted { host_run: _, run } => {
                FactKind::Admitted { run: *run, resumed: run::resumed(runs, *run) }
            }
            Request::Open { conversation, opening: _ } => {
                let run::OpeningFact { child, parent, call } = run::opened(conversations, calls, *conversation);
                FactKind::Opened { run, conversation: *conversation, child, parent, call }
            }
            Request::Return { call, result, .. } => FactKind::Returned { run, call: *call, result: result_of(result) },
            Request::Check { owner: _, program: _, deadline, tail: _ } => {
                FactKind::CheckStarted { run, deadline: *deadline }
            }
            Request::Answer { to: _, answer, read: _ } => FactKind::Answered { run, answer: answered(answer) },
            Request::MessageRefused { .. }
            | Request::Waiting { .. }
            | Request::Turn { .. }
            | Request::HostCall { .. }
            | Request::WithdrawHost { .. }
            | Request::Say { .. }
            | Request::Close { .. }
            | Request::Read { .. }
            | Request::Probe { .. }
            | Request::Abort { .. }
            | Request::Checking { .. }
            | Request::ChecksEnded { .. }
            | Request::Deliver { .. } => continue,
        };
        facts.push(fact);
    }
}

fn result_of(result: &Returned) -> Return {
    match result {
        Returned::Waiting => Return::Waiting,
        Returned::HostAnswered(_) => Return::HostAnswered,
        Returned::HostTooLarge { .. } | Returned::HostReportedTooLarge => Return::HostTooLarge,
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
