//! Ownership caps and checked worst-case arithmetic. This module retains no runtime state.
//! `worst_case` projects immutable limits into container and payload bounds, returning `None` on overflow.
//!
//! Contract: domain/run.md, section 14; programming-model.md, sections 4.4, 4.5 and 6.3.

use skein_lib::{Deadlines, Duration, List, Queue, Slab};

use crate::budget::Budget;
use crate::call::Calls;
use crate::facts::Fact;
use crate::prepare::Guide;
use crate::run::{Alarm, Conversation, Run};

/// The run child domain's limits (programming-model.md, sections 4.5 and 6.3), handed by its parent to every
/// step read-only.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Runs at once. A start beyond them is refused as busy.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub runs: u32,
    /// Conversations at once, across runs. A start with no room for its main
    /// conversation is refused as busy.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub conversations: u32,
    /// Bytes a run holds: its charter, each part held in a box counted at its
    /// fixed size plus its payload.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub run_bytes: u64,
    /// Repositories a checkout may list.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub repositories: u32,
    /// Outlets a charter may grant.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub outlets: u32,
    /// Verdicts an outcome spec may list.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub verdicts: u32,
    /// Calls of conversations to the run in flight at once, across runs, each
    /// with an alarm for its deadline. A call beyond them is answered as busy.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub calls: u32,
    /// The largest budget a charter may ask for, part by part.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub budget: Budget,
    /// The largest `max_tokens` a charter's LLM may ask for.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub max_tokens: u32,
    /// The LLMs a charter may list for sub-agents.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub models: u32,
    /// How deep sub-agents may nest: main is at depth zero, a sub-agent one
    /// deeper than its asker.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub depth: u32,
    /// Conversations a run may have at once, main included.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub run_conversations: u32,
    /// The most bytes of a sub-agent's last message its asker is given.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub answer_bytes: u32,
    /// Nudges a run gives its LLM when it stops without finishing, after which
    /// the run fails.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub nudges: u32,
    /// The most bytes of a repository's `AGENTS.md` a run reads and puts in
    /// its system text.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub guide_bytes: u32,
    /// How long io has for each look in the checkout.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub io_timeout: Duration,
    /// The most bytes an outcome declared to `finish` may hold, as a run
    /// counts them. A larger one is rejected.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub outcome_bytes: u64,
    /// How long a repository's checks may run.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub check_timeout: Duration,
    /// The most bytes of a failed check's output the LLM is shown: its tail.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub check_tail: u32,
    /// Facts kept until the parent drains them. Beyond them, facts are
    /// dropped and counted.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub facts: u32,
}

/// The most memory the domain holds under `limits`, in bytes (programming-model.md, section 6.3), or `None`
/// if it does not fit a `u64`.
///
/// It counts the containers, their bookkeeping included, and the payloads, not
/// allocator overhead. What a run sends is a copy, which its receiver counts;
/// what it receives and only passes on (a check's output) is the sender's.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let runs = Slab::<Run>::worst_case(limits.runs)?;
    let conversations = Slab::<Conversation>::worst_case(limits.conversations)?;
    // A deadline per run, and one per call.
    let alarms = Deadlines::<Alarm>::worst_case(limits.runs.checked_add(limits.calls)?)?;
    // Each run holds its charter, up to its byte limit, and what it found in
    // its checkout: a guide and a mark for checks per repository.
    let guides = List::<Guide>::worst_case(limits.repositories)?
        .checked_add(u64::from(limits.repositories).checked_mul(u64::from(limits.guide_bytes))?)?;
    let checks = List::<u32>::worst_case(limits.repositories)?;
    // A winding run holds the outcome it accepted.
    let run = limits.run_bytes.checked_add(guides)?.checked_add(checks)?.checked_add(limits.outcome_bytes)?;
    let held = u64::from(limits.runs).checked_mul(run)?;
    // A call landing a change holds it; a sub-agent's call, its answer.
    let call = limits.outcome_bytes.max(u64::from(limits.answer_bytes));
    let calls = Calls::worst_case(limits.calls)?.checked_add(u64::from(limits.calls).checked_mul(call)?)?;
    let facts = Queue::<Fact>::worst_case(limits.facts)?;
    runs.checked_add(conversations)?.checked_add(alarms)?.checked_add(held)?.checked_add(calls)?.checked_add(facts)
}
