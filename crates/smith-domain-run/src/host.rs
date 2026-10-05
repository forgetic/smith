//! Generic host-tool declarations and bounded relay lifecycle (domain/run.md,
//! sections 3, 5.2 and 12). State retains one immutable operation name, tool,
//! effect and protocol-attested input through bounded retries. It never knows
//! schemas' meaning, host policy, provider IDs or delivery state. A withdrawal
//! requests relay settlement; only an actual terminal releases that relay.

use crate::delivery::CallName;
use alloc::boxed::Box;
use skein_lib::{Duration, Time, Token};

/// Host-declared scheduling effect. Main inherits declarations; children never do.
/// Contract: domain/run.md, sections 5.1–5.3; domain/session.md, section 5.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HostEffect {
    /// Adjacent reads may run together. Contract: domain/session.md, section 5.
    Read,

    /// Runs exclusively with writes and sub-agents. Contract: domain/session.md, section 5.
    Write,
}

/// Host-supplied opaque tool contract, admitted under count and aggregate charter
/// ownership limits before any session or IO. Names are unique across sources.
/// Contract: domain/run.md, sections 3, 5.1, 5.2 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HostTool {
    /// Nonempty protocol-attested UTF-8 tool name, compared byte for byte; never a policy label.
    /// Contract: domain/run.md, sections 5.1 and 5.2.
    pub name: Box<[u8]>,

    /// Nonempty protocol-attested UTF-8 description handed unchanged to the provider.
    /// Contract: domain/run.md, sections 3 and 5.2.
    pub description: Box<[u8]>,

    /// Nonempty protocol-attested UTF-8 JSON Schema bytes, bounded by charter storage. The protocol
    /// owns validation and passes these bytes whole; the domain never interprets them.
    /// Contract: domain/run.md, sections 5.2 and 12.
    pub schema: Box<[u8]>,

    /// Declared immutable scheduling effect, checked again against each call.
    /// Contract: domain/run.md, section 5.2; domain/session.md, section 5.
    pub effect: HostEffect,

    /// Positive per-relay allowance, clamped to receiving limits and remaining
    /// caller/run time; a retry receives a fresh relay deadline.
    /// Contract: domain/run.md, section 5.2.
    pub timeout: Duration,
}

/// Protocol-attested JSON-object input. Private bytes retain that attestation
/// through routing; domains check storage and the object exterior only.
/// Complete JSON syntax validation belongs below the domain, not this constructor.
/// Contract: domain/run.md, sections 5.2 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HostInput {
    bytes: Box<[u8]>,
}

impl HostInput {
    /// Absolute vocabulary cap; receiving limits may be smaller.
    /// Contract: domain/run.md, sections 5.2 and 12.
    pub const CAPACITY: usize = 65_536;

    /// Called by the protocol only after complete JSON parsing established one
    /// UTF-8 object. Refuses oversized or non-object-exterior values. Passing
    /// malformed text or interior violates the sender's attestation contract.
    /// Contract: domain/run.md, sections 5.2 and 12.
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
    /// Contract: domain/run.md, section 5.2.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Protocol-attested UTF-8 host result or error text, sealed by size before crossing
/// the domain boundary. The sender owns text validation.
/// Contract: domain/run.md, sections 5.2 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct HostAnswer {
    text: Box<[u8]>,
    error: bool,
}

impl HostAnswer {
    /// Absolute text cap; the run also enforces its receiving reply limit.
    /// Contract: domain/run.md, sections 5.2 and 12.
    pub const CAPACITY: usize = 65_536;

    /// Seal protocol-attested UTF-8 host text up to the vocabulary cap. The sender
    /// validates encoding before this size-only constructor. Empty result/error text is
    /// allowed. An error is the host's feedback, never a retry classification.
    /// Contract: domain/run.md, sections 5.2 and 12.
    #[must_use]
    pub fn new(text: Box<[u8]>, error: bool) -> Option<Self> {
        if text.len() > Self::CAPACITY {
            return None;
        }
        Some(Self { text, error })
    }

    /// Move the sealed actual text and host error bit to concrete transcript
    /// feedback without copying or interpreting either value.
    /// Contract: domain/run.md, sections 5.2 and 14.
    #[must_use]
    pub fn into_parts(self) -> (Box<[u8]>, bool) {
        (self.text, self.error)
    }

    /// Exact bounded host text. Contract: domain/run.md, section 5.2.
    #[must_use]
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// Whether the host classified this text as a correctable error.
    /// Contract: domain/run.md, section 5.2.
    #[must_use]
    pub const fn error(&self) -> bool {
        self.error
    }
}

/// One live relay attempt, separate from immutable durable operation identity.
/// Old attempts and stale callback generations are inert after their terminal.
/// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RelayName {
    /// Live run-call callback generation; never a durable host operation name.
    /// Contract: domain/run.md, section 5.2.
    pub owner: Token,

    /// One-based bounded attempt number, incremented only after actual settlement.
    /// Contract: domain/run.md, section 5.2.
    pub attempt: u32,
}

/// Why a relay settled without an answer. Neither variant asserts that the
/// durable host operation was undecided; retries retrieve its existing record.
/// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Unanswered {
    /// The transport lost the answer. Contract: domain/run.md, section 5.2.
    Lost,

    /// The relay withdrawal settled. Contract: domain/run.md, section 5.2.
    Withdrawn,
}

/// Exactly one actual host terminal per relay. Before Busy the host made no
/// decision; after Unanswered the relay and its effects have settled. A decided
/// durable name always replays its first answer, even after a lost response.
/// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum HostReply {
    /// Exact recorded result or error; neither is retried.
    /// Contract: domain/run.md, section 5.2.
    Answered(
        /// Exact bounded first-record text/error, replayed by the host for a decided name.
        /// Contract: domain/run.md, section 5.2.
        HostAnswer,
    ),

    /// Predecision capacity refusal; eligible for bounded retry after backoff.
    /// Contract: domain/run.md, section 5.2.
    Busy,

    /// The actual relay terminal carries no learnable outcome yet.
    /// Contract: domain/run.md, section 5.2.
    Unanswered(
        /// Actual settled relay status, preserving possible committed effects.
        /// Contract: domain/run.md, section 5.2.
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
/// Contract: domain/run.md, sections 5.1, 5.2 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HostProblem {
    /// No main-only declaration grants this name. Contract: domain/run.md, section 5.2.
    Undeclared,

    /// Protocol-selected effect differs from the host declaration. Contract: domain/run.md, section 5.2.
    Effect,

    /// Owned name/input exceeds the receiving caps. Contract: domain/run.md, sections 5.2 and 12.
    TooLarge,
}
