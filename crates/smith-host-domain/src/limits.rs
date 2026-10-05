//! Checked capacities and ownership (domain/host.md, sections 3, 4 and 6;
//! programming-model.md, section 6). Parent owns forwarded turn payloads; kit
//! owns metadata, queued responses and actual landed proof through final Answer.
use crate::domain::{Account, Agent, Alarm, Call, Proof, Sent, TurnMeta};
use crate::{Delivered, Directory, Down, Fact, Grant};
use alloc::boxed::Box;
use core::mem::size_of;
use skein_lib::{Deadlines, Duration, Map, Queue, Slab, Token};

/// Immutable parent bounds; invalid arithmetic refuses construction (domain/host.md, sections 2–7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Contained process slots (domain/host.md, sections 2–7).
    pub agents: u32,
    /// Mount descriptors per start (domain/host.md, sections 2–7).
    pub directories: u32,
    /// Conflict paths per mount (domain/host.md, sections 2–7).
    pub conflicts: u32,
    /// Relative conflict path bytes (domain/host.md, sections 2–7).
    pub path_bytes: u32,
    /// Mount and generic tool label bytes (domain/host.md, sections 2–7).
    pub name_bytes: u32,
    /// Distinct credential names (domain/host.md, sections 2–7).
    pub accounts: u32,
    /// Opaque start charter bytes (domain/host.md, sections 2–7).
    pub charter_bytes: u64,
    /// Opaque transcript bytes (domain/host.md, sections 2–7).
    pub transcript_bytes: u64,
    /// Opaque post-transcript answer bytes (domain/host.md, sections 2–7).
    pub answered_bytes: u64,
    /// One inbound message bytes (domain/host.md, sections 2–7).
    pub message_bytes: u64,
    /// Queued plus sent-unread message names (domain/host.md, sections 2–7).
    pub messages: u32,
    /// Outstanding parent operation rights and reply transmissions (domain/host.md, sections 2–7).
    pub calls: u32,
    /// Generic tool or delivery argument bytes (domain/host.md, sections 2–7).
    pub call_bytes: u64,
    /// Generic tool result and sealed delivery ownership cap (domain/host.md, sections 2–7).
    pub answer_bytes: u64,
    /// Forwarded turn metadata including queued ACK transmissions (domain/host.md, sections 2–7).
    pub turns: u32,
    /// Maximum one turn payload (domain/host.md, sections 2–7).
    pub turn_bytes: u64,
    /// Total parent-owned turn payload credit (domain/host.md, sections 2–7).
    pub unacknowledged_bytes: u64,
    /// Opaque agent fact bytes (domain/host.md, sections 2–7).
    pub fact_bytes: u64,
    /// Opaque declared answer bytes (domain/host.md, sections 2–7).
    pub outcome_bytes: u64,
    /// Operator process-tail bytes (domain/host.md, sections 2–7).
    pub detail_bytes: u32,
    /// Lower spawn terminal deadline (domain/host.md, sections 2–7).
    pub spawn_timeout: Duration,
    /// Unpaused progress deadline (domain/host.md, sections 2–7).
    pub no_progress: Duration,
    /// Maximum declared long-operation extension (domain/host.md, sections 2–7).
    pub long_span: Duration,
    /// Independent working wall deadline (domain/host.md, sections 2–7).
    pub wall_time: Duration,
    /// Cancel/answer/exit grace before tree termination (domain/host.md, sections 2–7).
    pub grace: Duration,
    /// Terminate grace before tree kill (domain/host.md, sections 2–7).
    pub kill_after: Duration,
    /// Content-free diagnostic queue capacity (domain/host.md, sections 2–7).
    pub facts: u32,
}

/// Count domain containers, retained payloads, bounded scratch and each retained
/// receipt copy. Start stays here only before Started, which emits its Send;
/// Spawning rejects messages, so retained Start and queued messages are exclusive.
/// A replacement proof clone uses its Parent-stage call's unoccupied reply reserve.
/// The disconnect snapshot follows draining queued payloads and is counted separately.
/// Emitted/lower-owned bytes are the caller's to count. None for
/// invalid/unrepresentable limits (domain/host.md, sections 3, 4 and 6).
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.turns == 0
        || limits.turn_bytes == 0
        || limits.unacknowledged_bytes < limits.turn_bytes
        || limits.answer_bytes < Delivered::worst_case()
    {
        return None;
    }
    limits.calls.checked_add(8)?;
    let outbox = outbox(limits)?;
    let alarms = limits.agents.checked_mul(3)?;
    let directories = u64::from(limits.directories).checked_mul(
        u64::try_from(size_of::<Directory>())
            .ok()?
            .checked_add(u64::from(limits.name_bytes))?
            .checked_add(u64::from(limits.conflicts).checked_mul(
                u64::try_from(size_of::<Box<[u8]>>()).ok()?.checked_add(u64::from(limits.path_bytes))?,
            )?)?,
    )?;
    let start = limits
        .charter_bytes
        .checked_add(limits.transcript_bytes)?
        .checked_add(limits.answered_bytes)?
        .checked_add(directories)?
        .checked_add(u64::from(limits.accounts).checked_mul(u64::try_from(size_of::<Grant>()).ok()?)?)?;
    let queued = u64::from(limits.messages)
        .checked_mul(limits.message_bytes)?
        .checked_add(u64::from(limits.calls).checked_mul(limits.answer_bytes)?)?;
    let per_agent = Queue::<Down>::worst_case(outbox)?
        .checked_add(Queue::<Token>::worst_case(limits.messages)?)?
        .checked_add(Queue::<Sent>::worst_case(2)?)?
        .checked_add(Map::<Token, Call>::worst_case(limits.calls)?)?
        .checked_add(Map::<u32, TurnMeta>::worst_case(limits.turns)?)?
        .checked_add(Map::<u32, Account>::worst_case(limits.accounts)?)?
        .checked_add(u64::try_from(size_of::<Proof>()).ok()?)?
        .checked_add(Delivered::worst_case())?
        .checked_add(start.max(queued))?
        .checked_add(u64::from(limits.detail_bytes))?;
    Slab::<Agent>::worst_case(limits.agents)?
        .checked_add(skein_lib::List::<Token>::worst_case(limits.calls)?)?
        .checked_add(Deadlines::<Alarm>::worst_case(alarms)?)?
        .checked_add(Queue::<Fact>::worst_case(limits.facts)?)?
        .checked_add(u64::from(limits.agents).checked_mul(per_agent)?)
}

pub(crate) fn outbox(limits: &Limits) -> Option<u32> {
    limits.messages.checked_add(limits.calls)?.checked_add(limits.turns)?.checked_add(limits.accounts)?.checked_add(1)
}
