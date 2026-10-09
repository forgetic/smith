//! Parent and lower process faces (domain/host.md, sections 2–7).
//! Spawn ends in Gone; Started only names the process binding, Admitted names
//! the run's independent acceptance. Parent call rights survive channel loss;
//! Gone waits for IO proof, all IO terminals and exact parent commitments.
//! This module retains no runtime state and knows no charter semantics or
//! process internals; [`Event`] enters the stateful host kit, which emits
//! [`Request`] values with one terminal right each.
use crate::{Answer, Ask, CallName, Down, Grant, Reply, Start, Turn, Up};
use alloc::boxed::Box;
use skein_lib::{Time, Token};

/// Parent notices and lower request terminals consumed by step.
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Event {
    /// Parent requests one contained process; exactly one Gone.
    Spawn {
        client: Token,
        /// Bounded V2 start admitted before process IO.
        start: Start,
    },
    /// Parent message, ending refused if the ingress cannot admit it.
    Message {
        agent: Token,
        /// Opaque parent message name.
        name: Token,
        /// Sender's bounded label, forwarded without interpretation.
        label: Box<[u8]>,
        /// Sender's bounded text, forwarded without interpretation.
        text: Box<[u8]>,
    },
    /// Parent consumes one outstanding operation right, even after channel shutdown.
    Answer {
        agent: Token,
        call: Token,
        /// Exactly one matching parent terminal; never abandons delivery.
        reply: Reply,
    },
    /// Parent committed exactly this forwarded turn.
    Acknowledge {
        agent: Token,
        /// Exact forwarded turn number; duplicate kept ACK is inert.
        turn: u32,
    },
    /// Parent replaces a known credential name.
    Grant {
        agent: Token,
        /// Positive advancing generation and relative validity.
        grant: Grant,
    },
    /// Parent politely stops once; no deadline reset.
    Stop { agent: Token },
    /// Terminal of lower Spawn; process owns its tree.
    Spawned { owner: Token, process: Token },
    /// Terminal of lower Spawn; no process resources remain.
    Unspawned {
        owner: Token,
        /// Operator tail kept up to `Limits::detail_bytes`.
        detail: Box<[u8]>,
    },
    /// Terminal of Send; release its exact reservation.
    Sent { owner: Token },
    /// Terminal of Send; drain what agent already wrote.
    Unsent { owner: Token },
    /// Terminal of Read; decoded V2 record.
    Received {
        owner: Token,
        /// Bounded agent metadata and payload.
        message: Up,
    },
    /// Terminal of Read; undecodable/version-mismatched or oversized channel frame.
    Malformed { owner: Token },
    /// Terminal of Read; channel EOF.
    Hangup { owner: Token },
    /// Terminal of Signal, whether or not signal reached process.
    Signalled { owner: Token },
    /// Terminal of Wait; descendants may still run.
    Exited { owner: Token },
    /// Terminal of Reap after Exited; proves `TreeEmpty`.
    Reaped {
        owner: Token,
        /// Operator tail only, never LLM feedback.
        detail: Box<[u8]>,
    },
}

/// Parent notifications and effects; each lower request retains one terminal right.
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Request {
    /// Process spawned; parent may now address kit agent.
    Started { client: Token, agent: Token },
    /// Agent independently accepted run.
    Admitted { client: Token },
    /// One parent operation; exact Answer required.
    Called {
        client: Token,
        /// Stable parent scope, not interpreted.
        logical_run: Token,
        call: Token,
        /// Positive completion and assistant-block position, scoped by logical run.
        name: CallName,
        /// Actual operation deadline and bounded pause.
        deadline: Time,
        /// Generic tool/effect or delivery fields.
        ask: Ask,
    },
    /// Agent/process withdrew call once; parent terminal still required.
    Withdrawn { client: Token, call: Token },
    /// Parent takes owned payload; kit retains exact ACK metadata.
    Turn {
        client: Token,
        /// Validated numbered turn.
        turn: Turn,
    },
    /// Validated read fence; crossed queued message prevents pause.
    Waiting {
        client: Token,
        /// Last sent message read.
        read: Option<Token>,
    },
    /// Known-account credential rejection notice.
    Rejected {
        client: Token,
        /// Credential account.
        account: u32,
        /// Rejected generation.
        generation: u64,
    },
    /// Known-account quota notice.
    Exhausted {
        client: Token,
        /// Credential account.
        account: u32,
        /// Retry hint.
        retry_after: skein_lib::Duration,
    },
    /// Best-effort fact moved to parent.
    Told {
        client: Token,
        /// Bounded opaque agent fact.
        body: Box<[u8]>,
    },
    /// One run answer; process cleanup still owes Gone.
    Answered {
        client: Token,
        /// Validated last word.
        answer: Answer,
    },
    /// One typed failure before the run answer.
    Faulted {
        client: Token,
        /// Process/channel failure, not a fabricated run result.
        fault: Fault,
    },
    /// Message rejected before queue mutation.
    MessageRefused {
        client: Token,
        /// Original rejected message name.
        name: Token,
        /// Admission reason.
        reason: MessageRefusal,
    },
    /// Spawn terminal; all process and parent rights settled.
    Gone {
        client: Token,
        /// Entrance or containment completion.
        end: End,
        /// Bounded operator detail.
        detail: Box<[u8]>,
    },
    /// Lower contained process spawn with credential-free environment.
    Spawn {
        owner: Token,
        /// Optional prepared workspace handle.
        workspace: Option<Token>,
        /// Lower Spawn must settle by this deadline.
        deadline: Time,
    },
    /// Lower channel write; exactly one Sent/Unsent.
    Send {
        owner: Token,
        process: Token,
        /// Owned downlink payload counted by receiving lower layer.
        message: Down,
    },
    /// Demand one record; exactly one Received/Malformed/Hangup.
    Read { owner: Token, process: Token },
    /// Signal every process-tree member; exactly one Signalled.
    Signal {
        owner: Token,
        process: Token,
        /// Terminate or Kill.
        signal: Signal,
    },
    /// Wait for main process exit; exactly one Exited.
    Wait { owner: Token, process: Token },
    /// Await empty contained tree after exit; exactly one Reaped.
    Reap { owner: Token, process: Token },
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
