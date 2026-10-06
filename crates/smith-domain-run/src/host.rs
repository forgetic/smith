//! Generic host-tool declarations and bounded relay lifecycle (domain/run.md,
//! sections 3, 5.2 and 12). State retains one immutable operation name, tool,
//! effect and protocol-attested input through bounded retries. It never knows
//! schemas' meaning, host policy, provider IDs or delivery state. A withdrawal
//! requests relay settlement; only a terminal releases that relay.

use crate::delivery::CallName;
use alloc::boxed::Box;
use skein_lib::{Duration, Time, Token};

/// Host-declared scheduling effect. Main inherits declarations; children never do.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HostEffect {
    /// Adjacent reads may run together.
    Read,

    /// Runs exclusively with writes and sub-agents.
    Write,
}

/// Host-supplied opaque tool contract, admitted under count and aggregate charter
/// ownership limits before any session or IO. Names are unique across sources.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HostTool {
    /// Nonempty protocol-attested UTF-8 tool name, compared byte for byte; never a policy label.
    pub name: Box<[u8]>,

    /// Nonempty protocol-attested UTF-8 description handed unchanged to the provider.
    pub description: Box<[u8]>,

    /// Nonempty protocol-attested UTF-8 JSON Schema bytes, bounded by charter storage. The protocol
    /// owns validation and passes these bytes whole; the domain never interprets them.
    pub schema: Box<[u8]>,

    /// Declared immutable scheduling effect, checked again against each call.
    pub effect: HostEffect,

    /// Positive per-relay allowance, clamped to receiving limits and remaining
    /// caller/run time; a retry receives a fresh relay deadline.
    pub timeout: Duration,
}

/// Protocol-attested JSON-object input. Private bytes retain that attestation
/// through routing; domains check storage and the object exterior only.
/// Complete JSON syntax validation belongs below the domain, not this constructor.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HostInput {
    bytes: Box<[u8]>,
}

impl HostInput {
    /// Absolute vocabulary cap; receiving limits may be smaller.
    pub const CAPACITY: usize = 65_536;

    /// Called by the protocol only after complete JSON parsing established one
    /// UTF-8 object. Refuses oversized or non-object-exterior values. Passing
    /// malformed text or interior violates the sender's attestation contract.
    #[must_use]
    pub fn attested(bytes: Box<[u8]>) -> Option<Self> {
        if bytes.len() > Self::CAPACITY {
            return None;
        }
        let mut first = None;
        let mut last = None;
        for byte in &bytes {
            if *byte != b' ' && *byte != b'\t' && *byte != b'\r' && *byte != b'\n' {
                if first.is_none() {
                    first = Some(*byte);
                }
                last = Some(*byte);
            }
        }
        if first != Some(b'{') || last != Some(b'}') {
            return None;
        }
        Some(Self { bytes })
    }

    /// The exact immutable provider-written bytes, whitespace included.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Protocol-attested UTF-8 host result or error text, sealed by size before crossing
/// the domain boundary. The sender owns text validation.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HostAnswer {
    text: Box<[u8]>,
    error: bool,
}

impl HostAnswer {
    /// Absolute text cap; the run also enforces its receiving reply limit.
    pub const CAPACITY: usize = 65_536;

    /// Seal protocol-attested UTF-8 host text up to the vocabulary cap. The sender
    /// validates encoding before this size-only constructor. Empty result/error text is
    /// allowed. An error is the host's feedback, never a retry classification.
    #[must_use]
    pub fn new(text: Box<[u8]>, error: bool) -> Option<Self> {
        if text.len() > Self::CAPACITY {
            return None;
        }
        Some(Self { text, error })
    }

    /// Move the sealed text and host error bit to concrete transcript
    /// feedback without copying or interpreting either value.
    #[must_use]
    pub fn into_parts(self) -> (Box<[u8]>, bool) {
        (self.text, self.error)
    }

    /// Exact bounded host text.
    #[must_use]
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// Whether the host classified this text as a correctable error.
    #[must_use]
    pub const fn error(&self) -> bool {
        self.error
    }
}

/// One live relay attempt, separate from immutable durable operation identity.
/// Old attempts and stale callback generations are inert after their terminal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RelayName {
    /// Live run-call callback generation; never a durable host operation name.
    pub owner: Token,

    /// One-based bounded attempt number, incremented only after settlement.
    pub attempt: u32,
}

/// Why a relay settled without an answer. Neither variant asserts that the
/// durable host operation was undecided; retries retrieve its existing record.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Unanswered {
    /// The transport lost the answer.
    Lost,

    /// The relay withdrawal settled.
    Withdrawn,
}

/// Exactly one host terminal per relay. Before Busy the host made no
/// decision; after Unanswered the relay and its effects have settled. A decided
/// durable name always replays its first answer, even after a lost response.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum HostReply {
    /// Exact recorded result or error; neither is retried.
    Answered(
        /// Exact bounded first-record text/error, replayed by the host for a decided name.
        HostAnswer,
    ),

    /// Predecision capacity refusal; eligible for bounded retry after backoff.
    Busy,

    /// The relay terminal carries no learnable outcome yet.
    Unanswered(
        /// Actual settled relay status, preserving possible committed effects.
        Unanswered,
    ),
}

#[derive(Debug)]
pub(crate) struct Relay {
    pub(crate) name: CallName,
    pub(crate) tool: Box<[u8]>,
    pub(crate) effect: HostEffect,
    pub(crate) input: HostInput,
    pub(crate) timeout: Duration,
    pub(crate) caller_deadline: Time,
    pub(crate) stopped: Option<crate::call::Withdrawal>,
    pub(crate) unknown: bool,
    pub(crate) stage: Stage,
}

#[derive(Debug)]
pub(crate) enum Stage {
    Sending { attempt: u32, deadline: Time },
    Withdrawing { attempt: u32 },
    Backoff { attempt: u32, at: Time },
    Closed,
}

/// Domain admission feedback before a host effect. No host policy is inferred.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HostProblem {
    /// No main-only declaration grants this name.
    Undeclared,

    /// Protocol-selected effect differs from the host declaration.
    Effect,

    /// Owned name/input exceeds the receiving caps.
    TooLarge,
}
