//! The agent capability's parent face (domain/host.md, sections 2, 3 and 9).
//! This stateless vocabulary owns commands and notices, without process
//! mechanics. A parent sends Spawn and receives Gone after retained rights
//! settle; Started binds an agent independently of Admitted.

use crate::{Answer, Ask, CallName, End, Fault, Grant, MessageRefusal, Reply, Start, Turn};
use alloc::boxed::Box;
use skein_lib::{Time, Token};

/// Commands sent by a parent to either kind of agent capability.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "sealed payloads are priced by bounded queues and state")]
pub enum Event {
    /// Parent requests one agent capability; exactly one Gone.
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
}

/// Notices sent by an agent capability to its parent; Spawn ends in Gone.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// Capability started; parent may now address its agent.
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
    /// A bounded long operation began; the parent may show its notice.
    Long { client: Token, span: skein_lib::Duration },
    /// One prior long operation ended.
    LongDone { client: Token },
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
}
