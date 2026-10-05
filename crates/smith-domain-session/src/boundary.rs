//! The records that cross the boundary with the session's parent, the
//! root domain (programming-model.md, section 4.5), which routes them to and from the protocol layer and
//! the session's opener. The session defines them; its parent depends on it.
//!
//! Two shapes cross it. A session's lifecycle, with its opener: an
//! [`Event::Open`] is answered by exactly one [`Request::Ended`], after an
//! [`Request::Opened`] that names the session if it was admitted, and any
//! number of [`Request::Yielded`] and [`Request::Used`] in between; the opener
//! addresses the session by that name, and every record back carries the
//! opener's token (programming-model.md, section 4.2). And requests out with exactly one terminal event in
//! (a [`Request::Complete`] is ended by one of [`Event::Completed`],
//! [`Event::Failed`] or [`Event::Cancelled`]; a [`Request::Delegate`], to the
//! opener, by [`Event::Answered`] or [`Event::AnswerCancelled`]; a
//! [`Request::Io`], the tools' file and process operations passed on as they
//! are, by [`Event::Done`]). A request's `owner` is echoed on its terminal
//! event: the session's token for a call to the LLM, a tool run's own for a
//! delegated call, and the tools' own for an operation.

use alloc::boxed::Box;

use skein_lib::{Duration, Time, Token};
use smith_domain_tools as tools;

use crate::llm::{Answer, Completion, Descriptor, Endpoint, Failure, Prompt, Usage};

/// parent -> session
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(PartialEq, Eq, Debug)]
pub enum Event {
    /// Open a session for `spec`, on behalf of `opener`. Answered by exactly one
    /// `Ended`, after an `Opened` if the session was admitted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Open {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// The original session opening, validated before replaying any history.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spec: Spec,
    },
    /// Explicit version-two admission, including a fresh session with no history.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    OpenV2 {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// The original session opening, validated before replaying any history.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spec: crate::record::Opening,
    },
    /// A concrete delegated answer, including the spend of a sub-agent served
    /// by this call. The opener charges each child once, at its terminal answer.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    AnsweredV2 {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        text: Box<[u8]>,
        /// Whether the returned tool result represents a failure.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        error: bool,
        /// Accepted cumulative usage across the enclosing run or session.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: u64,
    },
    /// A withdrawn child terminal still reports what it spent before stopping.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    AnswerCancelledV2 {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Accepted cumulative usage across the enclosing run or session.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: u64,
    },
    /// A new user message for a yielded session, which calls the LLM again.
    /// Sent only while the session is yielded; a `session` that has ended
    /// meanwhile is dropped.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Continue {
        /// Parent-supplied session token identifying the owning kit.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        session: Token,
        /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        content: Box<[u8]>,
    },
    /// End the session, whatever its state: it cancels what is in flight, and
    /// ends once that has settled. A `session` that has ended is dropped.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Close {
        /// Parent-supplied session token identifying the owning kit.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        session: Token,
    },
    /// Terminal for `Complete`: the LLM produced its next message.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Completed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Provider completion, validated and charged once before its tools run.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        completion: Completion,
    },
    /// Terminal for `Complete`: the call produced no message.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Failed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Typed reason why the pending operation produced no successful value.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        failure: Failure,
    },
    /// Terminal for `Complete`, after `Cancel`: the call was abandoned.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Cancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
    /// Terminal for `Io`, for the tools.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Done {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        done: tools::Done,
    },
    /// Terminal for `Delegate`: the opener's answer, a success or a failure.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Answered {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Single terminal value returned to the caller.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        answer: Answer,
    },
    /// Terminal for `Delegate`, after `Withdraw`: the call was abandoned.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    AnswerCancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
}

/// session -> parent
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// The session for `opener` was admitted, and `session` names it from now
    /// on.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Opened {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Parent-supplied session token identifying the owning kit.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        session: Token,
    },
    /// A settled turn, copied at emission. Its values contain no session tickets.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Turn {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Settled concrete transcript turn handed to the opener.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        turn: crate::record::Turn,
    },
    /// Version two's cumulative deployment-unit spend, including child calls.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Priced {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Accepted cumulative usage across the enclosing run or session.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: u64,
        /// Whether checked integer pricing overflowed rather than producing a charge.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        overflow: bool,
    },
    /// The LLM stopped calling tools, saying `text` (its message's text blocks,
    /// one after another). The session waits for `Continue` or `Close`, and
    /// its time budget keeps running.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Yielded {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Why the provider stopped this completion, independently of its content.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        stop: Yield,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        text: Box<[u8]>,
    },
    /// A completion came back: one turn, and `usage` as the provider counts
    /// it. One per completion, the ones that win a race with a cancel
    /// included.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Used {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        usage: Usage,
    },
    /// The session for `opener` has ended, after `turns` completions that used
    /// `usage` (what its `Used` add up to): exactly one per `Open`, once
    /// nothing the session asked for is in flight.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Ended {
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        end: End,
        /// Completion count, bounded across the enclosing run or session.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        turns: u32,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        usage: Usage,
    },
    /// Ask an LLM for the next assistant message, giving up after `timeout`.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Complete {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// First user message supplied by the opener.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        prompt: Prompt,
        /// Maximum time allowed for this completion or contained command.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        timeout: Duration,
    },
    /// Abandon the `Complete` in flight for `owner`. Its terminal event still
    /// comes: `Cancelled`, or whichever outcome won the race.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Cancel {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
    /// Ask io for `op` for the tools, giving up at `deadline`: io runs the
    /// race (programming-model.md, section 5.3).
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Io {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Typed IO operation carrying only the authority and bounds required for it.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        op: tools::Op,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        deadline: Time,
    },
    /// Abandon the `Io` in flight for `owner`. Its terminal event still comes:
    /// `Done` with `Cancelled`, or whichever outcome won the race.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CancelIo {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
    /// Ask the opener to serve the delegated call `call`, a ticket, and to
    /// answer it by `deadline`, when the session's time runs out: the opener
    /// runs the race, within limits of its own if it has shorter ones, and
    /// answers a call that loses as a failure.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Delegate {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Parent-issued session or conversation name, echoed unchanged.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Typed checkout-tool call decoded below the domain.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        call: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        deadline: Time,
    },
    /// Abandon the `Delegate` in flight for `owner`, as the session closes.
    /// Its terminal event still comes: `AnswerCancelled`, or `Answered` if
    /// the answer won the race.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Withdraw {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
}

/// What a session is opened for.
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(PartialEq, Eq, Debug)]
pub struct Spec {
    /// Configured provider endpoint identity; the domain never resolves its address.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub model: Box<[u8]>,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub system: Box<[u8]>,
    /// What the session's own tools may do and where (the families the LLM
    /// may call among them, the checkout's repositories, what commands run
    /// with), copied into its kit; and the tools its opener serves.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub authority: tools::Authority,
    /// Opener-owned tool descriptors; tickets are resolved only by the opener.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub delegated: Box<[Descriptor]>,
    /// The first user message.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub prompt: Box<[u8]>,
    /// The most tokens each answer may take, and fewer once the output budget
    /// has less left.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub max_tokens: u32,
    /// Allowance supplied at admission, checked before starting more work.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub budget: Budget,
}

/// What a session may spend, from the moment it opens. Every dimension is
/// within the session's `Limits`, or the spec is refused.
///
/// A session starts a completion only while it has turns, input and output
/// tokens left (what it spent is below the budget), its cache reads and writes
/// are within their budget, and its `time` has not run out; when it needs one
/// it may not start, it ends. A completion's tokens are known only once it
/// comes back, so it may take input and cache tokens past their budget: its
/// turn still runs the tools it asked for, and the session ends where it
/// would have started the next. The output budget it cannot pass, as the
/// answer's `max_tokens` is cut to what is left. A zero cache budget
/// therefore ends a session only once a completion touches the cache. Time
/// does not wait for the turn: when it runs out, the session closes at once.
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Budget {
    /// Completion count, bounded across the enclosing run or session.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub turns: u32,
    /// Fresh input-token allowance or accepted count, as the provider reports it.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub input: u64,
    /// Output-token allowance or accepted count, as the provider reports it.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub output: u64,
    /// Cache-read token count or allowance as the provider reports it.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub cache_read: u64,
    /// Cache-write token count or allowance as the provider reports it.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub cache_write: u64,
    /// Monotonic wall-time allowance from admission.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub time: Duration,
}

/// A dimension of a [`Budget`].
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dimension {
    /// The completion-count allowance is exhausted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Turns,
    /// The fresh input-token allowance is exhausted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Input,
    /// The output-token allowance is exhausted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Output,
    /// The cache-read token allowance is exhausted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CacheRead,
    /// The cache-write token allowance is exhausted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CacheWrite,
    /// The injected monotonic deadline is reached.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Time,
    /// The checked integer-spend allowance is exhausted.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Unit,
}

/// Why a session yielded: how the LLM stopped calling tools.
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Yield {
    /// The LLM finished its turn.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Done,
    /// The LLM ran out of tokens mid-answer.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Truncated,
    /// The LLM declined to answer.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Refused,
    /// The LLM asked for tools and named none.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Malformed,
}

/// How a session ended.
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum End {
    /// Refused at the entrance: every session slot is taken.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Busy,
    /// Refused at the entrance: the spec does not fit the limits.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Invalid,
    /// Its opener closed it.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Closed,
    /// A call failed, for good or after its retries ran out.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Failed {
        /// Typed reason why the pending operation produced no successful value.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        failure: Failure,
    },
    /// The session's budget ran out in the `spent` dimension: it needed a
    /// completion the budget does not leave room for, or its time is up.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Budget {
        /// Accepted cumulative usage across the enclosing run or session.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: Dimension,
    },
    /// The conversation outgrew the session's message or byte limit.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    TranscriptFull,
    /// A transcript cannot be resumed. The next attempt must start fresh;
    /// this is a transient run failure (domain/session.md, sections 3 and 12).
    TranscriptRefused {
        /// Typed terminal reason supplied by the lower layer.
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        reason: crate::record::Refusal,
    },
    /// A checked price or cumulative spend did not fit the deployment counter.
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    PriceOverflow,
}
