//! Ordered routing for the agent capability (domain/host.md, sections 3, 4 and 9).
//! The parent and process faces own their independent vocabularies. These
//! stateless envelopes preserve emission order for a composed step and know
//! neither payload semantics nor process resources. `step` consumes Input
//! and emits Output; an inline capability uses the parent face alone.

use crate::{parent, process};

/// A routed parent command or lower process terminal consumed by the host kit.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "sealed payloads are priced by bounded queues and state")]
pub enum Input {
    /// A command sent by the parent.
    Parent(parent::Event),
    /// A terminal sent by the lower process adapter.
    Process(process::Event),
}

/// An ordered parent notice or lower process effect emitted by the host kit.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "sealed payloads are priced by bounded queues and state")]
pub enum Output {
    /// A notice sent to the parent.
    Parent(parent::Request),
    /// An effect sent to the lower process adapter.
    Process(process::Request),
}

/// Lower contained-tree signal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Signal {
    /// Polite tree-wide termination.
    Terminate,
    /// Final tree-wide kill.
    Kill,
}

/// Typed failure while no agent final answer was accepted.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// Channel/process ended without answer.
    Exited,
    /// Agent broke sequence, ownership or typed channel rules.
    Rules,
    /// Decoded payload exceeded receiving bound.
    TooLarge,
    /// Unpaused progress clock expired.
    NoProgress,
    /// Independent wall bound expired.
    WallTime,
}

/// Parent message admission refusal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MessageRefusal {
    /// Payload exceeded bytes.
    TooLarge,
    /// Bounded queued or unread messages full.
    Full,
    /// Agent no longer accepts messages.
    Ending,
    /// Outstanding name or current read watermark reused.
    NameInUse,
}

/// Start refused before process resources or payload copying.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Invalid {
    /// Carried messages exceed the inbox or rendered byte bound, or repeat a name.
    Messages,
    /// A parent Start with activation zero is refused before process work.
    Activation,

    /// Opaque charter too large.
    Charter,
    /// Opaque transcript too large.
    Transcript,
    /// Post-transcript answer bytes too large.
    Answered,
    /// Invalid bounded mount/conflict descriptors.
    Directories,
    /// Duplicate/invalid/excess credential names.
    Grants,
    /// Configured arithmetic or mandatory turn capacity invalid.
    Limits,
}

/// Terminal disposition of one parent Spawn.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum End {
    /// No agent slab slot available.
    Busy,
    /// Start refused before IO.
    Invalid(
        /// Sealed typed boundary value; no hidden policy.
        Invalid,
    ),
    /// Lower could not spawn; no resources remain.
    Unspawned,
    /// Process exited, tree empty, EOF and all rights settled.
    Stopped,
}
