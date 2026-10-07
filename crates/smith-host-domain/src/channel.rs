//! V2 typed host channel (domain/host.md, sections 2, 3, 6 and 7;
//! protocol/channel.md, sections 3 and 7).
//! Opaque payloads move; metadata controls sequence, spend, read fences and ACKs.
//! No framing, secrets, policy decoding or V1 compatibility. All payloads are
//! checked against receiving Limits before retained state changes. Durable decisions
//! belong to the parent, which scopes `CallName` by the same logical run after restart.
use crate::Delivery;
use alloc::boxed::Box;
use skein_lib::{Duration, Time, Token};

/// Parent start moved to the first channel Send; process spawn has its own deadline.
#[derive(PartialEq, Eq, Debug)]
pub struct Start {
    /// Stable parent run identity across restart, uninterpreted here.
    pub logical_run: Token,
    /// Positive parent-issued number unique for every activation of `logical_run`.
    pub activation: u64,
    /// Optional prepared workspace resolved by the lower process adapter.
    pub workspace: Option<Token>,
    /// Opaque charter at most `Limits::charter_bytes`.
    pub charter: Box<[u8]>,
    /// Opaque V2 transcript at most `Limits::transcript_bytes`.
    pub transcript: Option<Box<[u8]>>,
    /// Opaque calls answered after transcript, at most `Limits::answered_bytes`.
    pub answered: Box<[u8]>,
    /// At most `Limits::directories` unique named mounts.
    pub directories: Box<[Directory]>,
    /// At most `Limits::accounts` distinct credential names; no values.
    pub grants: Box<[Grant]>,
}

/// Host credit sent with the start for turns awaiting durable acknowledgement.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Window {
    /// Maximum unacknowledged turns.
    pub turns: u32,
    /// Maximum unacknowledged turn bytes.
    pub bytes: u64,
}

/// Parent mount descriptor forwarded without filesystem or delivery policy.
#[derive(PartialEq, Eq, Debug)]
pub struct Directory {
    /// Unique safe single component bounded by `Limits::name_bytes`; the host
    /// attests text encoding, and bytes move unchanged.
    pub name: Box<[u8]>,
    /// Parent write authority for the mount.
    pub writable: bool,
    /// Parent declares a git working tree; plain directories have no conflicts
    ///.
    pub git: bool,
    /// At most `Limits::conflicts` unique bounded relative paths per git mount.
    /// Read-only git conflicts are informative and grant no writes. The host
    /// attests text encoding.
    pub conflicts: Box<[Box<[u8]>]>,
}

/// Credential name forwarded to the protocol; never credential bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Grant {
    /// Known parent credential account.
    pub account: u32,
    /// Positive increasing credential generation; rejected notices echo it.
    pub generation: u64,
    /// Relative lifetime; protocol resolves the secret.
    pub valid: Duration,
}

/// Durable transcript-derived operation identity, separate from callback Token.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CallName {
    /// Host-supplied activation of the logical run.
    pub activation: u64,
    /// One-based accepted completion sequence, including restored V2 prefix.
    pub completion: u32,
    /// Zero-based assistant block ordinal; checked by the agent before effects.
    pub position: u32,
}

/// Agent turn moved once to parent, which owns payload until exact commitment ACK.
#[derive(PartialEq, Eq, Debug)]
pub struct Turn {
    /// Positive consecutive turn number from one; checked increment.
    pub number: u32,
    /// Global activation spend supplied by the agent in the host's unit; must
    /// not fall;
    /// the opaque body separately retains the session's inclusive child bill.
    pub spent: u64,

    /// Last named message actually sent and read; never a queued or unknown name.
    pub read: Option<Token>,
    /// Opaque transcript turn at most `Limits::turn_bytes`.
    pub body: Box<[u8]>,
}

/// Agent-described host tool effect forwarded to durable parent policy.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Effect {
    /// Read-only host tool; parent decides its meaning.
    Read,
    /// Writing host tool; parent must keep its durable decision.
    Write,
}

/// Agent operation forwarded once to parent and answered once even after withdrawal.
#[derive(PartialEq, Eq, Debug)]
pub enum Ask {
    /// Generic host tool; kit never decodes schema, body or policy.
    Host {
        /// Nonempty tool label bounded by `Limits::name_bytes`.
        tool: Box<[u8]>,
        /// Read/write classification supplied by the agent.
        effect: Effect,
        /// Opaque owned arguments bounded by `Limits::call_bytes`.
        body: Box<[u8]>,
    },
    /// Actual delivery of checked workspace; no cancellation after submission.
    Deliver {
        /// Opaque generic metadata bounded by `Limits::call_bytes`, with no title/body assumption.
        fields: Box<[u8]>,
    },
}

/// Parent terminal forwarded through Send; delivery keeps its evidence.
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Reply {
    /// Text tool result or error.
    Host {
        /// Parent classified error flag; body remains opaque.
        error: bool,
        /// Owned text result bounded by `Limits::answer_bytes`.
        body: Box<[u8]>,
    },
    /// One full delivered/nothing/refused/failed/stale terminal.
    Delivery(
        /// Sealed typed boundary value; no hidden policy.
        Delivery,
    ),
    /// Predecision capacity refusal; callback still gets exactly one response.
    Busy,
    /// Transport could not provide an answer; durable effects are unknown here.
    Unavailable,
    /// Transport withdrawal terminal for generic tools; durable effects remain parent-owned.
    Withdrawn,
    /// Ordinary tool response exceeded receiving bytes.
    TooLarge,
}

/// Exact neutral completion failure after retries or nonretryable refusal.
/// Diagnostic bytes were consumed by session policy; this record remains content-free.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionFailure {
    /// Shared-client receiving allowance exceeded.
    Limit,

    /// Shared protocol response contract violated.
    Protocol,

    /// Unsolicited lower cancellation.
    Cancelled,

    /// Provider capacity refused the request.
    Overloaded,

    /// Provider could not be reached or failed.
    Unavailable,

    /// The completion deadline elapsed.
    TimedOut,

    /// Provider context allowance was exceeded.
    ContextTooLong,

    /// Provider rejected the request shape.
    Invalid,

    /// Provider rejected the credential.
    Unauthorized,

    /// Provider rate allowance requires the retained cooldown.
    RateLimited {
        /// Exact lower cooldown, without scheduling or recovery policy.
        retry_after: Duration,
    },

    /// Provider account allowance requires the retained cooldown.
    Exhausted {
        /// Exact lower cooldown, without scheduling or recovery policy.
        retry_after: Duration,
    },
}

/// Actual transport evidence retained alongside a neutral completion failure.
/// No domain infers this from an error label or from requested cancellation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionEvidence {
    /// The lower proves no request bytes were sent.
    Unsent,

    /// The request may have reached the peer; outcome is unknown.
    Unknown,

    /// A peer response, including a refusal, was received.
    Response,
}

/// What kept an LLM from going on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ModelFault {
    /// Full neutral failure and evidence after session retry policy.
    /// This is distinct from local model stopping rules and requested run Cancel.
    Completion {
        /// Exact content-free shared-client classification and cooldown.
        failure: CompletionFailure,

        /// Exact transport evidence, including across interrupted Delivery.
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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exhausted {
    /// Global completion count reached its cap; admitted current calls settle.
    Turns,

    /// Global host-unit charge reached its cap; admitted current calls settle.
    Spend,

    /// Monotonic run deadline expired; immediate close remains independent.
    Time,
    /// A session exceeded its receiving token ceiling for one kind.
    Tokens(ReceivingLimit),
    /// Checked priced or raw usage arithmetic could not represent the total.
    Overflow(Overflow),
}

/// Which checked budget arithmetic overflowed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Overflow {
    /// Priced spend could not be represented.
    Spend,
    /// Raw completion usage could not be represented.
    Usage,
}

/// Session receiving token ceiling reported by the agent, separate from the
/// run's scalar financial allowance. Host policy decides the next activation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ReceivingLimit {
    /// Fresh input-token receiving allowance exhausted.
    Input,

    /// Output-token receiving allowance exhausted.
    Output,

    /// Cache-read-token receiving allowance exhausted.
    CacheRead,

    /// Cache-write-token receiving allowance exhausted.
    CacheWrite,
}

/// Exact transient V2 history admission refusal, without provider effects or
/// silent fresh-start fallback. The host decides whether a later activation
/// receives corrected history.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TranscriptRefusal {
    /// Unsupported durable record version.
    Version,

    /// Configured endpoint differs.
    Endpoint,

    /// Opaque replay dialect differs.
    Dialect,

    /// Invalid sequence, structure or call/result pairing.
    Malformed,

    /// A local live ticket survives in durable history.
    Unresolved,

    /// History or required receiving reserve exceeds caps.
    TooLarge,
}

/// Why a run ended without an outcome: what the host parent acts on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RunFailure {
    /// Exact transient history refusal before unsupported work starts.
    Transcript(
        /// Exact content-free history-admission classification sent by the agent
        /// to the host; no fresh-start fallback or provider effect is implied.
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

/// Why the agent refused a start before admission (domain/run.md, section 10).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// The agent has no capacity for the start.
    Busy,
    /// The agent rejected the start for a typed contract reason.
    Invalid(RunInvalid),
}

/// The agent's reason for rejecting a start before admission.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RunInvalid {
    /// The agent does not read the charter's version.
    CharterVersion,
    /// The activation name is invalid.
    Activation,
    /// The acknowledgement window cannot hold one largest permitted turn.
    Window,
    /// A convention path is invalid.
    Conventions,
    /// The charter owns too many bytes.
    TooLarge,
    /// A workspace mount is invalid.
    Workspace,
    /// Tool grants or declarations are invalid.
    Grants,
    /// The result contract is invalid.
    Outcome,
    /// The budget is invalid.
    Budget,
    /// A model declaration is invalid.
    Llm,
    /// The main conversation cannot fit.
    Conversation,
    /// An endpoint name is not configured.
    Endpoint,
}

/// Agent last-word result; Accepted bytes remain opaque to kit.
#[derive(PartialEq, Eq, Debug)]
pub enum RunResult {
    /// Run refused before Admitted; requires zero turns/spend.
    Refused {
        /// Typed agent refusal in the kit's own vocabulary.
        refusal: Refusal,
    },
    /// Agent-declared result allowed by its contract.
    Accepted {
        /// Opaque result bounded by `Limits::outcome_bytes`.
        outcome: Box<[u8]>,
    },
    /// V2 parking resumes from transcript, with no snapshot.
    Parked,
    /// Typed run failure.
    Failed {
        /// Agent-declared stop kind.
        failure: RunFailure,
    },
}

/// Agent final word forwarded once; process still owes Gone.
#[derive(PartialEq, Eq, Debug)]
pub struct Answer {
    /// Exactly the observed numbered turn count.
    pub turns: u32,

    /// Final global host-unit spend, at least the previous Turn's spend;
    /// refused starts have zero.
    pub spent: u64,

    /// Opaque accepted result or typed refusal/parking/failure/delivery evidence.
    pub result: RunResult,
}

/// Decoded V2 agent records; one record terminates each Read.
#[derive(PartialEq, Eq, Debug)]
pub enum Up {
    /// Independent run admission after process Started, exactly once.
    Admitted,
    /// Generic named host operation; parent owes terminal.
    Call {
        call: Token,
        /// Positive completion and assistant-block position, scoped by logical run.
        name: CallName,
        /// Agent deadline bounds watchdog pause, not abandonment.
        deadline: Time,
        /// Typed effect and opaque metadata.
        ask: Ask,
    },
    /// Call withdrawn once; parent still owes terminal.
    Withdraw { call: Token },
    /// Validated numbered turn moved to parent.
    Turn {
        /// Bounded turn metadata and owned transcript body.
        turn: Turn,
    },
    /// Best-effort observation; valid receipt counts as progress.
    Fact {
        /// Opaque fact bounded by `Limits::fact_bytes`.
        body: Box<[u8]>,
    },
    /// Bounded long operation stretches progress clock.
    Long {
        /// At most `Limits::long_span`.
        span: Duration,
    },
    /// Ends the previously announced progress stretch.
    LongDone,
    /// Waiting with a known read prefix pauses the watchdog until a message arrives
    ///.
    Waiting {
        /// None before first message; known sent message thereafter.
        read: Option<Token>,
    },
    /// Protocol rejected a named grant.
    Rejected {
        /// Known credential account.
        account: u32,
        /// Known positive generation.
        generation: u64,
    },
    /// Protocol exhausted a known account.
    Exhausted {
        /// Known credential account.
        account: u32,
        /// Parent refresh/retry hint.
        retry_after: Duration,
    },
    /// Agent last word; no further records may follow before EOF.
    Answer {
        /// Checked final counts and opaque or typed result.
        answer: Answer,
    },
}

/// Owned V2 downlink; Start first, ordered controls and at most one Cancel.
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Down {
    /// First and exactly once; opaque payloads moved from Spawn.
    Start {
        /// Validated parent start including post-transcript answers.
        start: Start,
        /// Credit derived from the host kit's limits.
        window: Window,
    },
    /// Named ordered parent message.
    Message {
        /// Opaque parent name; reserved until read fence covers it.
        name: Token,
        /// Opaque sender label; label and text together fit `Limits::message_bytes`.
        label: Box<[u8]>,
        /// Opaque text; label and text together fit `Limits::message_bytes`.
        text: Box<[u8]>,
    },
    /// One response to agent callback.
    Answer {
        call: Token,
        /// Exactly one matching parent terminal; never abandons delivery.
        reply: Reply,
    },
    /// Exact parent commitment of one forwarded turn.
    Acknowledge {
        /// Outstanding exact numbered turn, never an inferred prefix.
        turn: u32,
    },
    /// Coalesced known-account credential refresh.
    Grant {
        /// Names only; generation increases.
        grant: Grant,
    },
    /// First polite stop, once; operations retain terminal rights.
    Cancel,
}
