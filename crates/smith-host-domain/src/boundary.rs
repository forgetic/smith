//! Parent and lower process faces (domain/host.md, sections 2–7).
//! Spawn ends in Gone; Started only names the process binding, Admitted names
//! the run's independent acceptance. Parent call rights survive channel loss;
//! Gone waits for IO proof, all IO terminals and exact parent commitments.
use crate::{Answer, Ask, CallName, Down, Grant, MessageRefusal, Reply, Start, Turn, Up};
use alloc::boxed::Box;
use skein_lib::{Time, Token};

/// Parent notices and lower request terminals consumed by step (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Event {
    /// Parent requests one contained process; exactly one Gone (domain/host.md, sections 2–7).
    Spawn {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Bounded V2 start admitted before process IO (domain/host.md, sections 2–7).
        start: Start,
    },
    /// Parent notice, bounced if full/ending/reused (domain/host.md, sections 2–7).
    Message {
        /// Host kit handle from Started; stale parent notices are inert (domain/host.md, sections 2–7).
        agent: Token,
        /// Opaque parent message name (domain/host.md, sections 2–7).
        name: Token,
        /// Bounded parent message (domain/host.md, sections 2–7).
        body: Box<[u8]>,
    },
    /// Parent consumes one outstanding operation right, even after channel shutdown (domain/host.md, sections 2–7).
    Answer {
        /// Host kit handle from Started; stale parent notices are inert (domain/host.md, sections 2–7).
        agent: Token,
        /// Agent callback identity; separate from durable operation name (domain/host.md, sections 2–7).
        call: Token,
        /// Exactly one matching actual parent terminal; never abandons delivery (domain/host.md, sections 2–7).
        reply: Reply,
    },
    /// Parent committed exactly this forwarded turn (domain/host.md, sections 2–7).
    Acknowledge {
        /// Host kit handle from Started; stale parent notices are inert (domain/host.md, sections 2–7).
        agent: Token,
        /// Exact forwarded turn number; duplicate kept ACK is inert (domain/host.md, sections 2–7).
        turn: u32,
    },
    /// Parent replaces a known credential name (domain/host.md, sections 2–7).
    Grant {
        /// Host kit handle from Started; stale parent notices are inert (domain/host.md, sections 2–7).
        agent: Token,
        /// Positive advancing generation and relative validity (domain/host.md, sections 2–7).
        grant: Grant,
    },
    /// Parent politely stops once; no deadline reset (domain/host.md, sections 2–7).
    Stop {
        /// Host kit handle from Started; stale parent notices are inert (domain/host.md, sections 2–7).
        agent: Token,
    },
    /// Terminal of lower Spawn; process owns its tree (domain/host.md, sections 2–7).
    Spawned {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Lower process-tree handle from Spawned (domain/host.md, sections 2–7).
        process: Token,
    },
    /// Terminal of lower Spawn; no process resources remain (domain/host.md, sections 2–7).
    Unspawned {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Operator tail kept up to `Limits::detail_bytes` (domain/host.md, sections 2–7).
        detail: Box<[u8]>,
    },
    /// Terminal of Send; release its exact reservation (domain/host.md, sections 2–7).
    Sent {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
    },
    /// Terminal of Send; drain what agent already wrote (domain/host.md, sections 2–7).
    Unsent {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
    },
    /// Terminal of Read; decoded V2 record (domain/host.md, sections 2–7).
    Received {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Bounded agent metadata and payload (domain/host.md, sections 2–7).
        message: Up,
    },
    /// Terminal of Read; undecodable/version-mismatched or oversized channel frame (domain/host.md, sections 2–7).
    Malformed {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
    },
    /// Terminal of Read; channel EOF (domain/host.md, sections 2–7).
    Hangup {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
    },
    /// Terminal of Signal, whether or not signal reached process (domain/host.md, sections 2–7).
    Signalled {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
    },
    /// Terminal of Wait; descendants may still run (domain/host.md, sections 2–7).
    Exited {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
    },
    /// Terminal of Reap after Exited; proves `TreeEmpty` (domain/host.md, sections 2–7).
    Reaped {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Operator tail only, never LLM feedback (domain/host.md, sections 2–7).
        detail: Box<[u8]>,
    },
}

/// Parent notifications and effects; each lower request retains one terminal right (domain/host.md, sections 2–7).
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "sealed host diagnostic keeps a fixed inline tail; queue/state bounds price the full variant"
)]
pub enum Request {
    /// Process spawned; parent may now address kit agent (domain/host.md, sections 2–7).
    Started {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Host kit handle from Started; stale parent notices are inert (domain/host.md, sections 2–7).
        agent: Token,
    },
    /// Agent independently accepted run (domain/host.md, sections 2–7).
    Admitted {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
    },
    /// One parent operation; exact actual Answer required (domain/host.md, sections 2–7).
    Called {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Stable parent scope, not interpreted (domain/host.md, sections 2–7).
        logical_run: Token,
        /// Agent callback identity; separate from durable operation name (domain/host.md, sections 2–7).
        call: Token,
        /// Positive completion and assistant-block position, scoped by logical run (domain/host.md, sections 2–7).
        name: CallName,
        /// Actual operation deadline and bounded pause (domain/host.md, sections 2–7).
        deadline: Time,
        /// Generic tool/effect or delivery fields (domain/host.md, sections 2–7).
        ask: Ask,
    },
    /// Agent/process withdrew call once; actual parent terminal still required (domain/host.md, sections 2–7).
    Withdrawn {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Agent callback identity; separate from durable operation name (domain/host.md, sections 2–7).
        call: Token,
    },
    /// Parent takes owned payload; kit retains exact ACK metadata (domain/host.md, sections 2–7).
    Turn {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Validated numbered turn (domain/host.md, sections 2–7).
        turn: Turn,
    },
    /// Validated read fence; crossed queued message prevents pause (domain/host.md, sections 2–7).
    Waiting {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Last actual sent message read (domain/host.md, sections 2–7).
        read: Option<Token>,
    },
    /// Known-account credential rejection notice (domain/host.md, sections 2–7).
    Rejected {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Credential account (domain/host.md, sections 2–7).
        account: u32,
        /// Rejected generation (domain/host.md, sections 2–7).
        generation: u64,
    },
    /// Known-account quota notice (domain/host.md, sections 2–7).
    Exhausted {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Credential account (domain/host.md, sections 2–7).
        account: u32,
        /// Retry hint (domain/host.md, sections 2–7).
        retry_after: skein_lib::Duration,
    },
    /// Best-effort fact moved to parent (domain/host.md, sections 2–7).
    Told {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Bounded opaque agent fact (domain/host.md, sections 2–7).
        body: Box<[u8]>,
    },
    /// One run answer; process cleanup still owes Gone (domain/host.md, sections 2–7).
    Answered {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Validated last word (domain/host.md, sections 2–7).
        answer: Answer,
    },
    /// One typed failure before the run answer (domain/host.md, sections 2–7).
    Faulted {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Process/channel failure, not a fabricated run result (domain/host.md, sections 2–7).
        fault: Fault,
    },
    /// Message rejected before queue mutation (domain/host.md, sections 2–7).
    Bounced {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Original rejected message name (domain/host.md, sections 2–7).
        name: Token,
        /// Admission reason (domain/host.md, sections 2–7).
        bounce: Bounce,
    },
    /// Actual agent refusal settles one issued message credit; independent from
    /// local prequeue Bounced and from the lower Send terminal (domain/host.md, section 4.2).
    MessageBounced {
        /// Original parent owner, echoed even during channel draining (domain/host.md, section 4.2).
        client: Token,
        /// Exact issued opaque name whose one credit was retired (domain/host.md, section 4.2).
        name: Token,
        /// Actual agent reason; never inferred from host phase or payload (domain/host.md, section 4.2).
        reason: MessageRefusal,
    },
    /// Spawn terminal; all process and parent rights settled (domain/host.md, sections 2–7).
    Gone {
        /// Opaque parent owner from Spawn; echoed on notifications (domain/host.md, sections 2–7).
        client: Token,
        /// Entrance or containment completion (domain/host.md, sections 2–7).
        end: End,
        /// Bounded operator detail (domain/host.md, sections 2–7).
        detail: Box<[u8]>,
    },
    /// Lower contained process spawn with credential-free environment (domain/host.md, sections 2–7).
    Spawn {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Optional prepared workspace handle (domain/host.md, sections 2–7).
        workspace: Option<Token>,
        /// Lower Spawn must settle by this deadline (domain/host.md, sections 2–7).
        deadline: Time,
    },
    /// Lower channel write; exactly one Sent/Unsent (domain/host.md, sections 2–7).
    Send {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Lower process-tree handle from Spawned (domain/host.md, sections 2–7).
        process: Token,
        /// Owned downlink payload counted by receiving lower layer (domain/host.md, sections 2–7).
        message: Down,
    },
    /// Demand one record; exactly one Received/Malformed/Hangup (domain/host.md, sections 2–7).
    Read {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Lower process-tree handle from Spawned (domain/host.md, sections 2–7).
        process: Token,
    },
    /// Signal every process-tree member; exactly one Signalled (domain/host.md, sections 2–7).
    Signal {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Lower process-tree handle from Spawned (domain/host.md, sections 2–7).
        process: Token,
        /// Terminate or Kill (domain/host.md, sections 2–7).
        signal: Signal,
    },
    /// Wait for main process exit; exactly one Exited (domain/host.md, sections 2–7).
    Wait {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Lower process-tree handle from Spawned (domain/host.md, sections 2–7).
        process: Token,
    },
    /// Await empty contained tree after exit; exactly one Reaped (domain/host.md, sections 2–7).
    Reap {
        /// Kit-issued live handle echoed by the matching lower terminal (domain/host.md, sections 2–7).
        owner: Token,
        /// Lower process-tree handle from Spawned (domain/host.md, sections 2–7).
        process: Token,
    },
}

/// Lower contained-tree signal (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Signal {
    /// Polite tree-wide termination (domain/host.md, sections 2–7).
    Terminate,
    /// Final tree-wide kill (domain/host.md, sections 2–7).
    Kill,
}

/// Typed failure while no agent final answer was accepted (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// Channel/process ended without answer (domain/host.md, sections 2–7).
    Exited,
    /// Agent broke sequence, ownership or typed channel rules (domain/host.md, sections 2–7).
    Rules,
    /// Decoded payload exceeded receiving bound (domain/host.md, sections 2–7).
    TooLarge,
    /// Unpaused progress clock expired (domain/host.md, sections 2–7).
    NoProgress,
    /// Independent wall bound expired (domain/host.md, sections 2–7).
    WallTime,
}

/// Parent message admission refusal (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Bounce {
    /// Payload exceeded bytes (domain/host.md, sections 2–7).
    TooLarge,
    /// Bounded queued or unread messages full (domain/host.md, sections 2–7).
    Full,
    /// Agent no longer accepts messages (domain/host.md, sections 2–7).
    Ending,
    /// Outstanding name or current read watermark reused (domain/host.md, sections 2–7).
    ReusedName,
}

/// Start refused before process resources or payload copying (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Invalid {
    /// Opaque charter too large (domain/host.md, sections 2–7).
    Charter,
    /// Opaque transcript too large (domain/host.md, sections 2–7).
    Transcript,
    /// Post-transcript answer bytes too large (domain/host.md, sections 2–7).
    Answered,
    /// Invalid bounded mount/conflict descriptors (domain/host.md, sections 2–7).
    Directories,
    /// Duplicate/invalid/excess credential names (domain/host.md, sections 2–7).
    Grants,
    /// Configured arithmetic or mandatory turn capacity invalid (domain/host.md, sections 2–7).
    Limits,
}

/// Terminal disposition of one parent Spawn (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum End {
    /// No agent slab slot available (domain/host.md, sections 2–7).
    Busy,
    /// Start refused before IO (domain/host.md, sections 2–7).
    Invalid(
        /// Sealed typed boundary value; no hidden policy (domain/host.md, section 2).
        Invalid,
    ),
    /// Lower could not spawn; no resources remain (domain/host.md, sections 2–7).
    Unspawned,
    /// Process exited, tree empty, EOF and all rights settled (domain/host.md, sections 2–7).
    Stopped,
}
