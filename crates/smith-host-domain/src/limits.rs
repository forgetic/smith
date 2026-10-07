//! Checked capacities and ownership (domain/host.md, sections 3, 4 and 6;
//! programming-model.md, section 6). Parent owns forwarded turn payloads; kit
//! owns metadata and queued responses through final Answer.
use crate::domain::{Account, Agent, Alarm, Call, Sent, TurnMeta};
use crate::{AnsweredCall, Delivered, Directory, Down, Fact, Grant};
use alloc::boxed::Box;
use core::mem::size_of;
use skein_lib::{Deadlines, Duration, Map, Queue, Slab, Token};

/// Immutable parent bounds; invalid arithmetic refuses construction.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Contained process slots.
    pub agents: u32,
    /// Mount descriptors per start.
    pub directories: u32,
    /// Conflict paths per mount.
    pub conflicts: u32,
    /// Relative conflict path bytes, at most 4096; larger configurations are
    /// checked by admission and `worst_case`.
    pub path_bytes: u32,
    /// Mount and generic tool label bytes.
    pub name_bytes: u32,
    /// Distinct credential names.
    pub accounts: u32,
    /// Opaque start charter bytes.
    pub charter_bytes: u64,
    /// Opaque transcript bytes.
    pub transcript_bytes: u64,
    /// Opaque post-transcript answer bytes.
    pub answered_bytes: u64,
    /// One inbound message's label and text bytes together.
    pub message_bytes: u64,
    /// Queued plus sent-unread message names.
    pub messages: u32,
    /// Outstanding parent operation rights and reply transmissions.
    pub calls: u32,
    /// Generic tool or delivery argument bytes.
    pub call_bytes: u64,
    /// Generic tool result and sealed delivery ownership cap.
    pub answer_bytes: u64,
    /// Forwarded turn metadata including queued ACK transmissions.
    pub turns: u32,
    /// Maximum one turn payload.
    pub turn_bytes: u64,
    /// Total parent-owned turn payload credit.
    pub unacknowledged_bytes: u64,
    /// Opaque agent fact bytes.
    pub fact_bytes: u64,
    /// Opaque declared answer bytes.
    pub outcome_bytes: u64,
    /// Operator process-tail bytes.
    pub detail_bytes: u32,
    /// Lower spawn terminal deadline.
    pub spawn_timeout: Duration,
    /// Unpaused progress deadline.
    pub no_progress: Duration,
    /// Maximum declared long-operation extension.
    pub long_span: Duration,
    /// Independent working wall deadline.
    pub wall_time: Duration,
    /// Cancel/answer/exit grace before tree termination.
    pub grace: Duration,
    /// Terminate grace before tree kill.
    pub kill_after: Duration,
    /// Content-free diagnostic queue capacity.
    pub facts: u32,
}

/// Count domain containers, retained payloads, bounded scratch and each retained
/// receipt copy. Start stays here only before Started, which emits its Send;
/// Spawning rejects messages, so retained Start and queued messages are exclusive.
/// The disconnect snapshot follows draining queued payloads and is counted separately.
/// Emitted/lower-owned bytes are the caller's to count. None for
/// invalid/unrepresentable limits.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.path_bytes > 4096 {
        return None;
    }
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
        .checked_add(64_u64.checked_mul(u64::try_from(size_of::<Box<[u8]>>()).ok()?)?)?
        .checked_add(limits.answered_bytes)?
        .checked_add(128_u64.checked_mul(u64::try_from(size_of::<AnsweredCall>()).ok()?)?)?
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
