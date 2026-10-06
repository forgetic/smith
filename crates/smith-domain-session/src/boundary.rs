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
//! [`Event::Failed`] or [`Event::Cancelled`], or by [`Event::BudgetDenied`]
//! when the root withholds it before any provider lease, or [`Event::UnsentClosed`]
//! when the run closes before publication; a [`Request::Delegate`], to the
//! opener, by [`Event::Answered`] or [`Event::AnswerCancelled`]; a
//! [`Request::Io`], the tools' file and process operations passed on as they
//! are, by [`Event::Done`]). A request's `owner` is echoed on its terminal
//! event: the session's token for a call to the LLM, a tool run's own for a
//! delegated call, and the tools' own for an operation.

use alloc::boxed::Box;

use skein_lib::{Duration, Time, Token};
use smith_domain_tools as tools;

use crate::llm::{Completion, Descriptor, Endpoint, Failure, Prompt, Usage};

/// parent -> session
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(PartialEq, Eq, Debug)]
pub enum Event {
    /// Parent admission of bounded concrete history, including a fresh session.
    /// Ends exactly once after all provider, tools and delegated work settles.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Open {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Parent-owned spec, prices and optional concrete history. Admission
        /// checks receiving caps, transcript identity and provider/result space
        /// before any effect; refusal returns one `Ended` without `Opened`.
        /// Exactly one fixed `Opening` node is owned in transit, in addition
        /// to its bounded payload/envelopes. The entry consumes its ownership
        /// as admission validates and moves content; `worst_case` separately
        /// counts the node while it may coexist with admission state.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opening: Box<crate::record::Opening>,
    },
    /// Parent terminal for Delegate, possibly winning a Withdraw race. Its
    /// concrete payload consumes the result space reserved before that call.
    /// The current live identity charges the child bill once; stale repeats
    /// neither retain bytes nor change own or inclusive pricing.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Answered {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Parent-owned result bytes, at most `Limits::delegated_result_bytes`.
        /// Moved into concrete history even while closing; no answer ticket remains.
        /// Contract: domain/session.md, sections 3, 5 and 12.
        text: Box<[u8]>,
        /// Whether the returned tool result represents a failure.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        error: bool,
        /// Child activation's inclusive cumulative bill, in deployment units;
        /// historical spend is excluded. Only the inclusive parent prefix changes.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: u64,
    },
    /// Parent terminal acknowledging Withdraw. Its current delegated identity
    /// settles exactly once and releases its reserved result space. A withdrawn
    /// child still reports its activation bill; stale repeats remain inert.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    AnswerCancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Child activation's inclusive cumulative bill, in deployment units;
        /// historical spend is excluded. Only the inclusive parent prefix changes.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: u64,
    },
    /// A new user message for a yielded session, which calls the LLM again.
    /// Sent only while the session is yielded; a `session` that has ended
    /// meanwhile is dropped.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Continue {
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        session: Token,
        /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        content: Box<[u8]>,
    },
    /// End the session, whatever its state: it cancels what is in flight, and
    /// ends once that has settled. A `session` that has ended is dropped.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Close {
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        session: Token,
    },
    /// Terminal for `Complete`: the LLM produced its next message.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Completed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Provider completion, validated and charged once before its tools run.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        completion: Completion,
    },
    /// The parent received a completion whose checked run-wide charge or usage
    /// cannot be added. Its content and calls are discarded before any effect.
    /// Contract: domain/session.md, section 6; domain/run.md, section 9.
    Overflowed {
        /// The pending provider request whose terminal was received.
        owner: Token,
        /// Exact typed arithmetic failure, either price or usage overflow.
        end: End,
    },
    /// The root denied this requested completion before publishing it to a
    /// provider. Only its current Calling owner releases credit and ends the
    /// session, after kit settlement. Stale or duplicate denials are inert;
    /// no cancellation, retry, usage or Turn is produced.
    /// Contract: domain/session.md, sections 3, 5 and 6; domain/run.md, section 9.
    BudgetDenied {
        /// Session-issued pending completion owner, echoed without a provider lease.
        /// Contract: domain/session.md, sections 3 and 5; domain/run.md, section 9.
        owner: Token,
        /// Exact exhausted root allowance, mapped to the settled Budget terminal.
        /// Contract: domain/session.md, section 6; domain/run.md, section 9.
        reason: BudgetDenial,
    },
    /// The root closed the run before publishing this requested completion.
    /// Only the current Calling owner releases its reservation and settles
    /// Closed after the kit closes. Stale, duplicate and already-closing
    /// events are inert; an actual provider terminal remains owed once started.
    /// No cancellation, retry, usage or Turn is invented.
    /// Contract: domain/session.md, sections 3, 5 and 6; domain/run.md, section 9.
    UnsentClosed {
        /// Session-issued pending completion owner, without a provider lease.
        /// Contract: domain/session.md, sections 3 and 5; domain/run.md, section 9.
        owner: Token,
    },
    /// Terminal for `Complete`: the call produced no message.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Failed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        failure: Failure,

        /// Exact content-free transport evidence carried by the actual terminal.
        /// Contract: domain/session.md, sections 4, 5 and 12.
        evidence: crate::llm::Evidence,

        /// Bounded exact shared-client diagnostic, consumed and dropped by policy.
        /// It never controls text-based retry decisions or enters saved history.
        /// Contract: domain/session.md, sections 4, 5 and 12.
        detail: Box<[u8]>,
    },
    /// Terminal for `Complete`, after `Cancel`: the call was abandoned.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Cancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
    /// Terminal for `Io`, for the tools.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Done {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        done: tools::Done,
    },
}

/// session -> parent
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// The session for `opener` was admitted, and `session` names it from now
    /// on.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Opened {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        session: Token,
    },
    /// A settled turn, copied at emission. Its values contain no session tickets.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Turn {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Settled concrete transcript turn handed to the opener.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        turn: crate::record::Turn,
    },
    /// The cumulative deployment-unit spend, including child calls.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Priced {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// This activation's exact inclusive subtree charge. Historical turns
        /// do not seed it; own completions and delegated terminal bills do.
        /// Contract: domain/session.md, section 6; domain/run.md, section 9.
        spent: u64,
        /// Cumulative price of this activation's own completions only. The
        /// root counts its checked deltas; delegated bills never change it.
        /// Contract: domain/session.md, section 6; domain/run.md, section 9.
        own_spent: u64,
    },
    /// The LLM stopped calling tools, saying `text` (its message's text blocks,
    /// one after another). The session waits for `Continue` or `Close`, and
    /// its time budget keeps running.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Yielded {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        stop: Yield,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        text: Box<[u8]>,
    },
    /// A completion came back: one turn, and `usage` as the provider counts
    /// it. One per completion, the ones that win a race with a cancel
    /// included.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Used {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        usage: Usage,
    },
    /// The session for `opener` has ended, after `turns` completions that used
    /// `usage`, their exact sum: exactly one
    /// per `Open`, once nothing the session asked for is in flight.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Ended {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        end: End,
        /// Completion count, bounded across the enclosing run or session.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        turns: u32,
        /// Exact cumulative raw usage of charged completions.
        /// Contract: domain/session.md, section 6; domain/run.md, section 10.
        usage: Usage,
    },
    /// Ask an LLM for the next assistant message, giving up after `timeout`.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Complete {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// First user message supplied by the opener.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        prompt: Prompt,
        /// Maximum time allowed for this completion or contained command.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        timeout: Duration,

        /// Maximum owned translated completion bytes, including block cells,
        /// replay envelopes and decoded calls. The adapter verifies its configured
        /// bound before preparing the provider request; actual terminals obey it.
        /// Contract: domain/session.md, sections 3, 5 and 12.
        max_completion_bytes: u64,

        /// Maximum translated completion blocks, reserved with result skeletons
        /// before this request. One actual terminal remains owed after Cancel.
        /// Contract: domain/session.md, sections 3, 5 and 12.
        max_completion_blocks: u32,

        /// Maximum exact shared-client failure diagnostic bytes. The adapter
        /// verifies compatibility before prepare; policy consumes the actual
        /// terminal and drops detail without retaining text in facts/history.
        /// Contract: domain/session.md, sections 4, 5 and 12.
        max_failure_bytes: u32,
    },
    /// Abandon the `Complete` in flight for `owner`. Its terminal event still
    /// comes: `Cancelled`, or whichever outcome won the race.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Cancel {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
    /// Ask io for `op` for the tools, giving up at `deadline`: io runs the
    /// race (programming-model.md, section 5.3).
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Io {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Typed IO operation carrying only the authority and bounds required for it.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        op: tools::Op,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        deadline: Time,
    },
    /// Abandon the `Io` in flight for `owner`. Its terminal event still comes:
    /// `Done` with `Cancelled`, or whichever outcome won the race.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CancelIo {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
    /// Ask the opener to serve the delegated call `call`, a ticket, and to
    /// answer it by `deadline`, when the session's time runs out: the opener
    /// runs the race, within limits of its own if it has shorter ones, and
    /// answers a call that loses as a failure.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Delegate {
        /// Concrete transcript origin supplied by the session, preserved by the opener.
        /// It is fixed-size; its sequence is checked before any delegated effect.
        ///
        /// Contract: domain/session.md, sections 3 and 5; domain/run.md, section 8.2.
        origin: crate::record::Origin,
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Typed checkout-tool call decoded below the domain.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        call: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        deadline: Time,
    },
    /// Abandon the `Delegate` in flight for `owner`, as the session closes.
    /// Its terminal event still comes: `AnswerCancelled`, or `Answered` if
    /// the answer won the race.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Withdraw {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        owner: Token,
    },
}

/// What a session is opened for.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(PartialEq, Eq, Debug)]
pub struct Spec {
    /// Configured provider endpoint identity; the domain never resolves its address.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub model: Box<[u8]>,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub system: Box<[u8]>,
    /// What the session's own tools may do and where (the families the LLM
    /// may call among them, the checkout's repositories, what commands run
    /// with), copied into its kit; and the tools its opener serves.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub authority: tools::Authority,
    /// Opener-owned tool descriptors; tickets are resolved only by the opener.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub delegated: Box<[Descriptor]>,
    /// The first user message.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub prompt: Box<[u8]>,
    /// The most tokens each answer may take, and fewer once the output budget
    /// has less left.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub max_tokens: u32,
    /// Allowance supplied at admission, checked before starting more work.
    ///
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
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Budget {
    /// Completion count, bounded across the enclosing run or session.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub turns: u32,
    /// Fresh input-token allowance or accepted count, as the provider reports it.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub input: u64,
    /// Output-token allowance or accepted count, as the provider reports it.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub output: u64,
    /// Cache-read token count or allowance as the provider reports it.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub cache_read: u64,
    /// Cache-write token count or allowance as the provider reports it.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub cache_write: u64,
    /// Monotonic wall-time allowance from admission.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub time: Duration,
}

/// A dimension of a [`Budget`].
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dimension {
    /// The completion-count allowance is exhausted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Turns,
    /// The fresh input-token allowance is exhausted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Input,
    /// The output-token allowance is exhausted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Output,
    /// The cache-read token allowance is exhausted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CacheRead,
    /// The cache-write token allowance is exhausted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CacheWrite,
    /// The injected monotonic deadline is reached.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Time,
    /// The checked integer-spend allowance is exhausted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Unit,
}

/// Why a session yielded: how the LLM stopped calling tools.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Yield {
    /// The LLM finished its turn.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Done,
    /// The LLM ran out of tokens mid-answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Truncated,
    /// The LLM declined to answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Refused,
    /// The LLM asked for tools and named none.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Malformed,
}

/// How a session ended.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum End {
    /// Refused at the entrance: every session slot is taken.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Busy,
    /// Refused at the entrance: the spec does not fit the limits.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Invalid,
    /// Its opener closed it.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Closed,
    /// A call failed, for good or after its retries ran out.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Failed {
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        failure: Failure,

        /// Exact content-free transport evidence carried by the actual terminal.
        /// Contract: domain/session.md, sections 4, 5 and 12.
        evidence: crate::llm::Evidence,
    },
    /// The session's budget ran out in the `spent` dimension: it needed a
    /// completion the budget does not leave room for, or its time is up.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Budget {
        /// Exhausted receiving or deployment-unit dimension, reported to the
        /// parent only after all started provider, tool and child work settles.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        spent: Dimension,
    },
    /// The conversation outgrew the session's message or byte limit.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    TranscriptFull,
    /// A transcript cannot be resumed. The next attempt must start fresh;
    /// this is a transient run failure (domain/session.md, sections 3 and 12).
    TranscriptRefused {
        /// Typed terminal reason supplied by the lower layer.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        reason: crate::record::Refusal,
    },
    /// A checked price or cumulative spend did not fit the deployment counter.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    PriceOverflow,
    /// Cumulative raw usage did not fit; the rejected completion is not
    /// charged or told. `PriceOverflow` takes precedence when both fail.
    /// Contract: domain/session.md, section 6; domain/run.md, sections 9 and 10.
    UsageOverflow,
}

/// A root allowance exhausted before publishing a requested completion.
/// This carries no provider policy and allocates no payload.
/// Contract: domain/session.md, section 6; domain/run.md, section 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BudgetDenial {
    /// No global completion remains; ends as `Dimension::Turns` after kit close.
    /// Contract: domain/session.md, section 6; domain/run.md, section 9.
    Turns,
    /// No global deployment-unit allowance remains; ends as `Dimension::Unit`.
    /// Contract: domain/session.md, section 6; domain/run.md, section 9.
    Spend,
}
