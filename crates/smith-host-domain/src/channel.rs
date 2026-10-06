//! V2 typed host channel (domain/host.md, sections 2, 3, 6 and 7).
//! Opaque payloads move; metadata controls sequence, spend, read fences and ACKs.
//! No framing, secrets, policy decoding or V1 compatibility. All payloads are
//! checked against receiving Limits before retained state changes. Durable decisions
//! belong to the parent, which scopes `CallName` by the same logical run after restart.
use crate::Delivery;
use alloc::boxed::Box;
use skein_lib::{Duration, Time, Token};

/// Parent start moved to the first channel Send; process spawn has its own deadline (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub struct Start {
    /// Stable parent run identity across restart, uninterpreted here (domain/host.md, sections 2–7).
    pub logical_run: Token,
    /// Positive parent-issued number unique for every activation of `logical_run`.
    /// Contract: domain/host.md, sections 2 and 4.
    pub activation: u64,
    /// Optional prepared workspace resolved by the lower process adapter (domain/host.md, sections 2–7).
    pub workspace: Option<Token>,
    /// Opaque charter at most `Limits::charter_bytes` (domain/host.md, sections 2–7).
    pub charter: Box<[u8]>,
    /// Opaque V2 transcript at most `Limits::transcript_bytes` (domain/host.md, sections 2–7).
    pub transcript: Option<Box<[u8]>>,
    /// Opaque calls answered after transcript, at most `Limits::answered_bytes` (domain/host.md, sections 2–7).
    pub answered: Box<[u8]>,
    /// At most `Limits::directories` unique named mounts (domain/host.md, sections 2–7).
    pub directories: Box<[Directory]>,
    /// At most `Limits::accounts` distinct credential names; no values (domain/host.md, sections 2–7).
    pub grants: Box<[Grant]>,
}

/// Parent mount descriptor forwarded without filesystem or delivery policy (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub struct Directory {
    /// Unique safe single component bounded by `Limits::name_bytes`; the host
    /// attests text encoding, and bytes move unchanged (domain/host.md, section 4).
    pub name: Box<[u8]>,
    /// Parent write authority for the mount (domain/host.md, sections 2–7).
    pub writable: bool,
    /// Parent declares a git working tree; plain directories have no conflicts
    /// (domain/host.md, section 4).
    pub git: bool,
    /// At most `Limits::conflicts` unique bounded relative paths per git mount.
    /// Read-only git conflicts are informative and grant no writes. The host
    /// attests text encoding (domain/host.md, section 4).
    pub conflicts: Box<[Box<[u8]>]>,
}

/// Credential name forwarded to the protocol; never credential bytes (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Grant {
    /// Known parent credential account (domain/host.md, sections 2–7).
    pub account: u32,
    /// Positive increasing credential generation; rejected notices echo it (domain/host.md, sections 2–7).
    pub generation: u64,
    /// Relative lifetime; protocol resolves the actual secret (domain/host.md, sections 2–7).
    pub valid: Duration,
}

/// Durable transcript-derived operation identity, separate from callback Token (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CallName {
    /// Host-supplied activation of the logical run (domain/host.md, sections 2 and 4).
    pub activation: u64,
    /// One-based accepted completion sequence, including restored V2 prefix (domain/host.md, sections 2–7).
    pub completion: u32,
    /// Zero-based assistant block ordinal; checked by the agent before effects (domain/host.md, sections 2–7).
    pub position: u32,
}

/// Agent turn moved once to parent, which owns payload until exact commitment ACK (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub struct Turn {
    /// Positive consecutive turn number from one; checked increment (domain/host.md, sections 2–7).
    pub number: u32,
    /// Global activation spend supplied by the agent in the host's unit; must
    /// not fall;
    /// the opaque body separately retains the session's inclusive child bill.
    /// Contract: domain/host.md, sections 6 and 9; domain/run.md, section 9.
    pub spent: u64,

    /// Last named message actually sent and read; never a queued or unknown name (domain/host.md, sections 2–7).
    pub read: Option<Token>,
    /// Opaque transcript turn at most `Limits::turn_bytes` (domain/host.md, sections 2–7).
    pub body: Box<[u8]>,
}

/// Agent-described host tool effect forwarded to durable parent policy (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Effect {
    /// Read-only host tool; parent decides its meaning (domain/host.md, sections 2–7).
    Read,
    /// Writing host tool; parent must keep its durable decision (domain/host.md, sections 2–7).
    Write,
}

/// Agent operation forwarded once to parent and answered once even after withdrawal (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub enum Ask {
    /// Generic host tool; kit never decodes schema, body or policy (domain/host.md, sections 2–7).
    Host {
        /// Nonempty tool label bounded by `Limits::name_bytes` (domain/host.md, sections 2–7).
        tool: Box<[u8]>,
        /// Read/write classification supplied by the agent (domain/host.md, sections 2–7).
        effect: Effect,
        /// Opaque owned arguments bounded by `Limits::call_bytes` (domain/host.md, sections 2–7).
        body: Box<[u8]>,
    },
    /// Actual delivery of checked workspace; no cancellation after submission (domain/host.md, sections 2–7).
    Deliver {
        /// Opaque generic metadata bounded by `Limits::call_bytes`, with no title/body assumption (domain/host.md, sections 2–7).
        fields: Box<[u8]>,
    },
}

/// Parent terminal forwarded through Send; delivery keeps its actual evidence (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Reply {
    /// Text tool result or error (domain/host.md, sections 2–7).
    Host {
        /// Parent classified error flag; body remains opaque (domain/host.md, sections 2–7).
        error: bool,
        /// Owned text result bounded by `Limits::answer_bytes` (domain/host.md, sections 2–7).
        body: Box<[u8]>,
    },
    /// One full actual delivered/nothing/refused/failed/stale terminal (domain/host.md, sections 2–7).
    Delivery(
        /// Sealed typed boundary value; no hidden policy (domain/host.md, section 2).
        Delivery,
    ),
    /// Predecision capacity refusal; callback still gets exactly one response (domain/host.md, sections 2–7).
    Busy,
    /// Transport could not provide an answer; durable effects are unknown here (domain/host.md, sections 2–7).
    Unavailable,
    /// Transport withdrawal terminal for generic tools; durable effects remain parent-owned (domain/host.md, sections 2–7).
    Withdrawn,
    /// Ordinary tool response exceeded receiving bytes (domain/host.md, sections 2–7).
    TooLarge,
}

/// Exact neutral completion failure after retries or nonretryable refusal.
/// Diagnostic bytes were consumed by session policy; this record remains content-free.
/// Contract: domain/host.md, sections 2, 5 and 10.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionFailure {
    /// Shared-client receiving allowance exceeded.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Limit,

    /// Shared protocol response contract violated.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Protocol,

    /// Unsolicited actual lower cancellation.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Cancelled,

    /// Provider capacity refused the request.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Overloaded,

    /// Provider could not be reached or failed.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Unavailable,

    /// The actual completion deadline elapsed.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    TimedOut,

    /// Provider context allowance was exceeded.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    ContextTooLong,

    /// Provider rejected the request shape.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Invalid,

    /// Provider rejected the credential.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Unauthorized,

    /// Provider rate allowance requires the retained cooldown.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    RateLimited {
        /// Exact lower cooldown, without scheduling or recovery policy.
        /// Contract: domain/host.md, sections 2, 5 and 10.
        retry_after: Duration,
    },

    /// Provider account allowance requires the retained cooldown.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Exhausted {
        /// Exact lower cooldown, without scheduling or recovery policy.
        /// Contract: domain/host.md, sections 2, 5 and 10.
        retry_after: Duration,
    },
}

/// Actual transport evidence retained alongside a neutral completion failure.
/// No domain infers this from an error label or from requested cancellation.
/// Contract: domain/host.md, sections 2, 5 and 10.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionEvidence {
    /// The lower proves no request bytes were sent.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Unsent,

    /// The request may have reached the peer; outcome is unknown.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Unknown,

    /// An actual peer response, including a refusal, was received.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Response,
}

/// What kept an LLM from going on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ModelFault {
    /// Full neutral actual failure and evidence after session retry policy.
    /// This is distinct from local model stopping rules and requested run Cancel.
    /// Contract: domain/host.md, sections 2, 5 and 10.
    Completion {
        /// Exact content-free shared-client classification and cooldown.
        /// Contract: domain/host.md, sections 2, 5 and 10.
        failure: CompletionFailure,

        /// Exact actual transport evidence, including across interrupted Delivery.
        /// Contract: domain/host.md, sections 2, 5 and 10.
        evidence: CompletionEvidence,
    },

    /// The account spent its provider allowance. The host decides whether to retry after cooldown.
    Exhausted,
    /// Its provider failed for good: unreachable, overloaded past the
    /// retries, or refusing the call or its credentials.
    Provider,
    /// The conversation outgrew the model's context, or the bytes a
    /// conversation may hold.
    ContextFull,
    /// It kept declining to answer.
    Refused,
    /// It kept running out of tokens mid-answer.
    Truncated,
    /// It kept asking for tools and naming none.
    Malformed,
}

/// Run-wide allowance exhausted, reported by the agent as a typed failure.
/// The host decides whether another activation receives a new allowance;
/// per-kind receiving token ceilings remain separate classifications.
/// Contract: domain/host.md, sections 2 and 6; domain/run.md, section 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exhausted {
    /// Global completion count reached its cap; admitted current calls settle.
    /// Contract: domain/run.md, section 9; domain/host.md, section 6.
    Turns,

    /// Global host-unit charge reached its cap; admitted current calls settle.
    /// Contract: domain/run.md, section 9; domain/host.md, section 6.
    Spend,

    /// Monotonic run deadline expired; immediate close remains independent.
    /// Contract: domain/run.md, section 9; domain/host.md, sections 4 and 6.
    Time,
}

/// Session receiving token ceiling reported by the agent, separate from the
/// run's scalar financial allowance. Host policy decides the next activation.
/// Contract: domain/host.md, sections 2 and 6; domain/session.md, section 6.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ReceivingLimit {
    /// Fresh input-token receiving allowance exhausted.
    /// Contract: domain/session.md, section 6; domain/host.md, section 6.
    Input,

    /// Output-token receiving allowance exhausted.
    /// Contract: domain/session.md, section 6; domain/host.md, section 6.
    Output,

    /// Cache-read-token receiving allowance exhausted.
    /// Contract: domain/session.md, section 6; domain/host.md, section 6.
    CacheRead,

    /// Cache-write-token receiving allowance exhausted.
    /// Contract: domain/session.md, section 6; domain/host.md, section 6.
    CacheWrite,
}

/// Exact transient V2 history admission refusal, without provider effects or
/// silent fresh-start fallback. The host decides whether a later activation
/// receives corrected history. Contract: domain/host.md, sections 2 and 9;
/// domain/run.md, sections 3, 6 and 10.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TranscriptRefusal {
    /// Unsupported durable record version. Contract: domain/run.md, section 6.
    Version,

    /// Configured endpoint differs. Contract: domain/run.md, section 6.
    Endpoint,

    /// Opaque replay dialect differs. Contract: domain/run.md, section 6.
    Dialect,

    /// Invalid sequence, structure or call/result pairing. Contract: domain/run.md, section 6.
    Malformed,

    /// A local live ticket survives in durable history. Contract: domain/run.md, section 6.
    Unresolved,

    /// History or required receiving reserve exceeds caps. Contract: domain/run.md, section 6.
    TooLarge,
}

/// Why a run ended without an outcome: what the host parent acts on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RunFailure {
    /// Agent currency arithmetic failed; the reported spend is what was
    /// charged before the failing completion.
    /// Actual already-landed delivery remains separate durable evidence.
    /// Contract: domain/run.md, section 9; domain/host.md, sections 6 and 9.
    PriceOverflow,

    /// Agent raw-usage arithmetic failed before the completion's turn was told.
    /// Contract: domain/run.md, section 9; domain/host.md, sections 6 and 9.
    UsageOverflow,

    /// A session receiving token cap stopped another completion; this does not
    /// reinterpret that cap as a run-wide scalar financial budget.
    /// Contract: domain/session.md, section 6; domain/host.md, section 6.
    Receiving(
        /// Exact per-kind ceiling reported by the agent; no host repricing.
        /// Contract: domain/session.md, section 6; domain/host.md, section 6.
        ReceivingLimit,
    ),

    /// Exact transient history refusal before unsupported work starts.
    /// Contract: domain/host.md, sections 2 and 9; domain/run.md, section 6.
    Transcript(
        /// Exact content-free history-admission classification sent by the agent
        /// to the host; no fresh-start fallback or provider effect is implied.
        /// Contract: domain/host.md, sections 2 and 9; domain/run.md, section 6.
        TranscriptRefusal,
    ),

    /// The LLM could not do the work.
    Model(ModelFault),
    /// The run's budget ran out.
    Budget(Exhausted),
    /// The LLM did not keep to the run's rules.
    Policy(Policy),
    /// The host parent cancelled the run.
    Cancelled,
    /// The host's delivery context moved: no later delivery from this run can
    /// land. The host decides what context a new run receives.
    Stale,
}

/// The run's rules, as the LLM broke them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Policy {
    /// It kept stopping without finishing, through `nudges` nudges, having
    /// called `finish` with `rejected` outcomes that were refused.
    Unfinished {
        /// Number of run nudges already given after unfinished turns.
        nudges: u32,
        /// Number of declared outcomes rejected by the charter contract.
        rejected: u32,
    },
}

/// Agent last-word result; Accepted bytes remain opaque to kit (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub enum RunResult {
    /// Run refused before Admitted; requires zero turns/spend (domain/host.md, sections 2–7).
    Refused {
        /// Opaque admission refusal bounded by `Limits::outcome_bytes` (domain/host.md, sections 2–7).
        detail: Box<[u8]>,
    },
    /// Agent-declared result allowed by its contract (domain/host.md, sections 2–7).
    Accepted {
        /// Opaque result bounded by `Limits::outcome_bytes` (domain/host.md, sections 2–7).
        outcome: Box<[u8]>,
    },
    /// V2 parking resumes from transcript, with no snapshot (domain/host.md, sections 2–7).
    Parked,
    /// Typed run failure (domain/host.md, sections 2–7).
    Failed {
        /// Agent-declared stop kind (domain/host.md, sections 2–7).
        failure: RunFailure,
    },
}

/// Agent final word forwarded once; process still owes Gone (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub struct Answer {
    /// Exactly the observed numbered turn count (domain/host.md, sections 2–7).
    pub turns: u32,

    /// Final global host-unit spend, at least the previous Turn's spend;
    /// refused starts have zero.
    /// Contract: domain/host.md, sections 6 and 9; domain/run.md, section 9.
    pub spent: u64,

    /// Opaque accepted result or typed refusal/parking/failure/actual delivery evidence (domain/host.md, sections 2–7).
    pub result: RunResult,
}

/// Decoded V2 agent records; one record terminates each Read (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
pub enum Up {
    /// Independent run admission after process Started, exactly once (domain/host.md, sections 2–7).
    Admitted,
    /// Generic named host operation; parent owes actual terminal (domain/host.md, sections 2–7).
    Call {
        /// Agent callback identity; separate from durable operation name (domain/host.md, sections 2–7).
        call: Token,
        /// Positive completion and assistant-block position, scoped by logical run (domain/host.md, sections 2–7).
        name: CallName,
        /// Agent deadline bounds watchdog pause, not abandonment (domain/host.md, sections 2–7).
        deadline: Time,
        /// Typed effect and opaque metadata (domain/host.md, sections 2–7).
        ask: Ask,
    },
    /// Call withdrawn once; parent still owes terminal (domain/host.md, sections 2–7).
    Withdraw {
        /// Agent callback identity; separate from durable operation name (domain/host.md, sections 2–7).
        call: Token,
    },
    /// Validated numbered turn moved to parent (domain/host.md, sections 2–7).
    Turn {
        /// Bounded turn metadata and owned transcript body (domain/host.md, sections 2–7).
        turn: Turn,
    },
    /// Best-effort observation; valid receipt counts as progress (domain/host.md, sections 2–7).
    Fact {
        /// Opaque fact bounded by `Limits::fact_bytes` (domain/host.md, sections 2–7).
        body: Box<[u8]>,
    },
    /// Bounded long operation stretches progress clock (domain/host.md, sections 2–7).
    Long {
        /// At most `Limits::long_span` (domain/host.md, sections 2–7).
        span: Duration,
    },
    /// Ends the previously announced progress stretch (domain/host.md, sections 2–7).
    LongDone,
    /// Waiting with a known read prefix pauses the watchdog until a message arrives
    /// (domain/host.md, sections 2 and 4).
    Waiting {
        /// None before first message; known sent message thereafter (domain/host.md, sections 2–7).
        read: Option<Token>,
    },
    /// Protocol rejected a named grant (domain/host.md, sections 2–7).
    Rejected {
        /// Known credential account (domain/host.md, sections 2–7).
        account: u32,
        /// Known positive generation (domain/host.md, sections 2–7).
        generation: u64,
    },
    /// Protocol exhausted a known account (domain/host.md, sections 2–7).
    Exhausted {
        /// Known credential account (domain/host.md, sections 2–7).
        account: u32,
        /// Parent refresh/retry hint (domain/host.md, sections 2–7).
        retry_after: Duration,
    },
    /// Agent last word; no further records may follow before EOF (domain/host.md, sections 2 and 4).
    Answer {
        /// Checked final counts and opaque or typed result (domain/host.md, sections 2–7).
        answer: Answer,
    },
}

/// Owned V2 downlink; Start first, ordered controls and at most one Cancel (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Down {
    /// First and exactly once; opaque payloads moved from Spawn (domain/host.md, sections 2–7).
    Start {
        /// Validated parent start including post-transcript answers (domain/host.md, sections 2–7).
        start: Start,
    },
    /// Named ordered parent message (domain/host.md, sections 2–7).
    Message {
        /// Opaque parent name; reserved until read fence covers it (domain/host.md, sections 2–7).
        name: Token,
        /// Opaque text bounded by `Limits::message_bytes` (domain/host.md, sections 2–7).
        body: Box<[u8]>,
    },
    /// One actual response to agent callback (domain/host.md, sections 2–7).
    Answer {
        /// Agent callback identity; separate from durable operation name (domain/host.md, sections 2–7).
        call: Token,
        /// Exactly one matching actual parent terminal; never abandons delivery (domain/host.md, sections 2–7).
        reply: Reply,
    },
    /// Exact parent commitment of one forwarded turn (domain/host.md, sections 2–7).
    Acknowledge {
        /// Outstanding exact numbered turn, never an inferred prefix (domain/host.md, sections 2–7).
        turn: u32,
    },
    /// Coalesced known-account credential refresh (domain/host.md, sections 2–7).
    Grant {
        /// Names only; generation increases (domain/host.md, sections 2–7).
        grant: Grant,
    },
    /// First polite stop, once; actual operations retain terminal rights (domain/host.md, sections 2–7).
    Cancel,
}
