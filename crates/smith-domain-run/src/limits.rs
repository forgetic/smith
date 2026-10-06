//! Ownership caps and checked worst-case arithmetic. This module retains no runtime state.
//! `worst_case` projects immutable limits into container and payload bounds, returning `None` on overflow.
//!
//! Contract: domain/run.md, section 13; programming-model.md, sections 4.4, 4.5 and 6.3.

use skein_lib::{Deadlines, Duration, List, Queue, Slab};

use crate::budget::Budget;
use crate::call::Calls;
use crate::facts::Fact;
use crate::prepare::Guide;
use crate::run::{Alarm, Conversation, Run};

/// The run child domain's limits (programming-model.md, sections 4.5 and 6.3), handed by its parent to every
/// step read-only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Live FIFO messages per admitted run. Full admission bounces before retention.
    pub messages: u32,

    /// Maximum attested labelled text bytes retained per queued message.
    pub message_bytes: u32,

    /// Maximum positive charter idle interval; it never pauses the wall budget.
    pub waiting: Duration,

    /// Maximum host operation duration after checked submission. It must
    /// be positive for a delivery-capable charter; caller expiry may be earlier.
    /// The host supplies the bounded host terminal even after run shutdown.
    pub delivery_timeout: Duration,
    /// Runs at once. A start beyond them is refused as busy.
    pub runs: u32,
    /// Conversations at once, across runs. A start with no room for its main
    /// conversation is refused as busy.
    pub conversations: u32,
    /// Bytes a run holds beyond inline slab fields: its charter and separate
    /// workspace, including Brief cells, instructions, title/text payloads and every
    /// Directory cell, name, conflict Box cell and path.
    pub run_bytes: u64,

    /// Host-written main Brief sections. Exact count is checked before section
    /// traversal, rendering or effects; excess refuses as `Invalid::TooLarge`.
    /// Cells, instructions and all title/text payloads also fit `run_bytes`.
    pub brief_sections: u32,
    /// Directories a present workspace may list; empty present workspaces refuse.
    pub directories: u32,

    /// Maximum opaque safe mount component bytes, checked before pairwise admission.
    pub directory_name_bytes: u32,

    /// Maximum initial conflict paths per git directory, checked before comparisons.
    pub conflicts: u32,

    /// Maximum relative conflict path bytes, at most `Marker::CAPACITY` (4096).
    pub conflict_path_bytes: u32,
    /// Host tool declarations a charter may grant.
    pub host_tools: u32,

    /// Maximum immutable protocol-attested host input bytes per call, at most the vocabulary cap.
    pub host_input_bytes: u32,

    /// Maximum host answer text bytes accepted and retained per call.
    pub host_reply_bytes: u32,

    /// Receiving ceiling for each relay timeout; caller and run expiry may be earlier.
    pub host_timeout: Duration,

    /// Positive deterministic backoff between settled retryable relays.
    pub host_backoff: Duration,

    /// Verdicts an outcome spec may list.
    pub verdicts: u32,
    /// Calls of conversations to the run in flight at once, across runs, each
    /// with an alarm for its deadline. A call beyond them is answered as busy.
    pub calls: u32,
    /// The largest budget a charter may ask for, part by part.
    pub budget: Budget,
    /// The largest `max_tokens` a charter's LLM may ask for.
    pub max_tokens: u32,
    /// The LLMs a charter may list for sub-agents.
    pub models: u32,
    /// Conversations a run may have at once, main included.
    pub run_conversations: u32,
    /// The most bytes of a sub-agent's last message its asker is given.
    pub answer_bytes: u32,
    /// Nudges a run gives its LLM when it stops without finishing, after which
    /// the run fails.
    pub nudges: u32,
    /// The most bytes of a repository's selected guide a run reads and puts in
    /// its system text.
    pub guide_bytes: u32,
    /// How long io has for each look in the checkout.
    pub io_timeout: Duration,
    /// Largest aggregate result ownership beyond its inline declaration,
    /// counting every field/item container, name and value as
    /// `outcome::owned_bytes` does. A larger finish is feedback, before
    /// shape judgement or any checks/Delivery.
    pub outcome_bytes: u64,
    /// How long a repository's checks may run.
    pub check_timeout: Duration,
    /// The most bytes of a failed check's output the LLM is shown: its tail.
    pub check_tail: u32,
    /// Facts kept until the parent drains them. Beyond them, facts are
    /// dropped and counted.
    pub facts: u32,
}

/// The most memory the domain holds under `limits`, in bytes (programming-model.md, section 6.3), or `None`
/// if it does not fit a `u64`.
///
/// It counts the containers, their bookkeeping included, and the payloads, not
/// allocator overhead. What a run sends is a copy, which its receiver counts;
/// what it receives and only passes on (a check's output) is the sender's.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.conflict_path_bytes > u32::try_from(crate::Marker::CAPACITY).ok()? {
        return None;
    }
    let runs = Slab::<Run>::worst_case(limits.runs)?;
    let conversations = Slab::<Conversation>::worst_case(limits.conversations)?;
    // A deadline per run, and two per call.
    let alarms =
        Deadlines::<Alarm>::worst_case(limits.runs.checked_mul(2)?.checked_add(limits.calls.checked_mul(2)?)?)?;
    // Each run holds its charter, including both convention path payloads, up
    // to its byte limit (inline path wrappers are in Slab<Run>), and what it found in
    // its checkout: a guide and a mark for checks per repository.
    let guides = List::<Guide>::worst_case(limits.directories)?
        .checked_add(u64::from(limits.directories).checked_mul(u64::from(limits.guide_bytes))?)?;
    let checks = List::<u32>::worst_case(limits.directories)?;
    // A winding run holds the outcome it accepted.
    let run = limits
        .run_bytes
        .checked_add(Queue::<crate::run::Message>::worst_case(limits.messages)?)?
        .checked_add(u64::from(limits.messages).checked_mul(u64::from(limits.message_bytes))?)?
        .checked_add(guides)?
        .checked_add(checks)?
        .checked_add(limits.outcome_bytes.max(crate::Delivered::worst_case()))?;
    let held = u64::from(limits.runs).checked_mul(run)?;
    // A call landing a change holds it; a sub-agent's call, its answer.
    let host = limits.run_bytes.checked_add(u64::from(limits.host_input_bytes))?;
    let call = limits.outcome_bytes.max(u64::from(limits.answer_bytes)).max(host);
    let calls = Calls::worst_case(limits.calls)?.checked_add(u64::from(limits.calls).checked_mul(call)?)?;
    let facts = Queue::<Fact>::worst_case(limits.facts)?;
    runs.checked_add(conversations)?.checked_add(alarms)?.checked_add(held)?.checked_add(calls)?.checked_add(facts)
}
