//! Ownership caps and checked worst-case arithmetic. This module retains no runtime state.
//! `worst_case` projects immutable limits into container and payload bounds, returning `None` on overflow.
//!
//! Contract: domain/session.md, section 12; programming-model.md, sections 4.4, 4.5 and 6.3.

use skein_lib::{Deadlines, Duration, List, Queue, Slab};
use smith_domain_tools as tools;

use crate::boundary::Budget;
use crate::facts::Fact;
use crate::llm::{Block, Message};
use crate::record::{Opening, Turn};
use crate::session::{Alarm, Ready, Run, Session, Slot};

/// The most tool calls a session runs at once: what `Limits::parallel_tools`
/// may be, and what bounds the requests a step emits.
pub const MAX_PARALLEL: u32 = 8;

/// The session child domain's limits (programming-model.md, sections 4.5 and 6.3), handed by its parent to every
/// step read-only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Sessions at once. An `Open` beyond them is refused as busy.
    pub sessions: u32,
    /// Largest deployment-unit budget admitted by the parent opening.
    pub spend: u64,
    /// Messages a session's transcript holds, the spec's prompt included: at
    /// least two, the prompt and an answer.
    pub messages: u32,
    /// Bytes a session holds: its spec, its transcript and the tool results
    /// it is collecting, each block counted at its fixed size plus its payload.
    pub session_bytes: u64,

    /// Maximum owned content of one provider completion, including
    /// block cells, replay envelopes and decoded owned-call fields. The session
    /// reserves this and every possible unstarted result before asking the provider.
    /// Contract: domain/session.md, sections 3, 5 and 12.
    pub completion_bytes: u64,

    /// Maximum blocks in a completion; result-slot storage is
    /// reserved independently before the provider request. The adapter's full
    /// translated completion bound must obey both receiving caps.
    /// Contract: domain/session.md, sections 3, 5 and 12.
    pub completion_blocks: u32,

    /// Maximum exact failure diagnostic bytes. Provider receiving credit
    /// covers this terminal before work; policy drops detail after consumption.
    /// Contract: domain/session.md, sections 4, 5 and 12.
    pub failure_bytes: u32,

    /// Maximum concrete opener-result payload per admitted delegated call.
    /// A whole batch reserves its possible results before any effect; each
    /// terminal owns its reservation through close and turn emission.
    /// Contract: domain/session.md, sections 3, 5 and 12.
    pub delegated_result_bytes: u64,

    /// The largest budget a spec may ask for, dimension by dimension. Its time
    /// is the longest a session may live.
    pub budget: Budget,
    /// Owned tool calls a session runs at once: adjacent calls that read run
    /// together, up to this many, and a call that writes runs alone. Between
    /// one and [`MAX_PARALLEL`].
    pub parallel_tools: u32,
    /// The largest `max_tokens` a spec may ask for.
    pub max_tokens: u32,
    /// Retries of a call that failed transiently, after which the session
    /// fails.
    pub retries: u32,
    /// The wait before the first retry, doubled for each one after it, with
    /// jitter.
    pub backoff_base: Duration,
    /// The longest wait before a retry, unless the provider asks for longer.
    pub backoff_max: Duration,
    /// How long the protocol layer gives each call.
    pub call_timeout: Duration,
    /// How long a tool call may run, and no later than the session's time
    /// runs out. Its deadline goes with it, and a call that runs out of time
    /// is answered as such, to the LLM. A delegated call has no timeout here:
    /// the opener races it against the session's time.
    pub tool_timeout: Duration,
    /// Facts kept until the parent drains them, the tools' passed on among
    /// them. Beyond them, facts are dropped and counted.
    pub facts: u32,
    /// The tools child domain's, which the session owns: a kit for each
    /// session, with room for its widest batch.
    pub tools: tools::Limits,
}

/// Logical room secured before each provider request: maximum completion,
/// copied result IDs/details and independent Block/Slot skeleton wrappers.
/// This does not allocate memory or lower the transcript payload ceiling; it
/// prevents a provider effect whose in-cap terminal cannot be retained.
/// It also covers the full bounded failure terminal; diagnostic policy consumes
/// and drops that transient detail. Returns None on zero completion caps or
/// checked arithmetic overflow.
/// Contract: domain/session.md, sections 3, 5 and 12.
#[must_use]
pub fn completion_reserve(limits: &Limits) -> Option<u64> {
    crate::session::provider_reserve(limits)
}

/// The most memory the domain holds under `limits`, in bytes (programming-model.md, section 6.3), or `None`
/// if it does not fit a `u64` or the limits cannot be honoured: a parallel
/// batch wider than [`MAX_PARALLEL`] or than the tools run for a kit at once
/// (so that a read never meets `Busy`), fewer kits than sessions, or a
/// transcript too short for a prompt and its answer.
///
/// It is the session's own, the tools' it owns, and the queue that holds what
/// the tools emit in a step. It counts the containers, their bookkeeping
/// included, and the payloads, not allocator overhead. The prompts of calls in
/// flight are copies held by the protocol layer, and the texts of yields copies
/// held by the opener, which count them. Facts own nothing beyond their queue.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let parallel = limits.parallel_tools;
    if !(1..=MAX_PARALLEL).contains(&parallel) || parallel > limits.tools.calls || limits.messages < 2 {
        return None;
    }
    if limits.tools.kits < limits.sessions {
        return None;
    }
    let sessions = Slab::<Session>::worst_case(limits.sessions)?;
    let runs = Slab::<Run>::worst_case(runs(limits)?)?;
    let alarms = Deadlines::<Alarm>::worst_case(alarms(limits)?)?;
    let ready = Ready::worst_case(limits.sessions)?;
    let facts = Queue::<Fact>::worst_case(limits.facts)?;
    let tools = tools::worst_case(&limits.tools)?;
    let tools_out = Queue::<tools::Request>::worst_case(tools::max_out(&limits.tools))?;
    // Each session owns its transcript's list and up to its byte limit.
    // Result slots coexist with the assembled result Block array. The latter
    // is charged in session_bytes; the full Slot container is additional.
    // The whole byte-cap quotient and configured completion block cap form
    // a conservative bound for any admitted live batch, also across restore.
    // Keeping their maximum avoids assuming Slot <= Block.
    let block = u64::try_from(size_of::<Block>()).ok()?;
    let calls = u32::try_from(limits.session_bytes.checked_div(block)?).ok()?.max(limits.completion_blocks);
    let slots = List::<Slot>::worst_case(calls)?;
    let session =
        List::<Message>::worst_case(limits.messages)?.checked_add(slots)?.checked_add(limits.session_bytes)?;
    let held = u64::from(limits.sessions).checked_mul(session)?;
    // One parent handoff owns exactly one fixed boxed Opening node. Its
    // validated payload is charged by the session byte cap, and its restore
    // envelopes may coexist while messages move into the transcript list.
    // Count the node throughout input validation and transfer, independently
    // of its release timing and all payload/record cells.
    let opening = u64::try_from(size_of::<Opening>()).ok()?;
    let staging = List::<Turn>::worst_case(limits.messages)?
        .checked_add(List::<Message>::worst_case(limits.messages)?)?
        .checked_add(opening)?;
    sessions
        .checked_add(runs)?
        .checked_add(alarms)?
        .checked_add(ready)?
        .checked_add(facts)?
        .checked_add(tools)?
        .checked_add(tools_out)?
        .checked_add(held)?
        .checked_add(staging)
}

/// The run slab's capacity: two batches a session. A session starts at most
/// one batch in a step, and a batch the tools answered in the step that
/// started it goes on from the ready list, after the reclaim point (see the
/// session module). So in one iteration a session holds the runs of at most
/// two batches: one ending, and the next it starts.
pub(crate) fn runs(limits: &Limits) -> Option<u32> {
    limits.sessions.checked_mul(limits.parallel_tools)?.checked_mul(2)
}

/// The alarm table's capacity: every session may have two alarms armed.
pub(crate) fn alarms(limits: &Limits) -> Option<u32> {
    limits.sessions.checked_mul(2)
}
