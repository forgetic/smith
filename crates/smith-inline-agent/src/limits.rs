//! Checked slot and routing bounds (domain/host.md, sections 9.1 and 9.4;
//! programming-model.md, section 6.3). This module keeps no runtime state.
//! `worst_case` prices each root and every retained routing container;
//! caller-owned emitted payloads remain covered by the root's outgoing bound.

use core::mem::size_of;
use skein_lib::{Duration, Id, List, Map, Queue, Slab, Token};
use smith_domain as smith;

use crate::domain::{Domain, Relay, Slot};
use crate::{Fact, Output};

/// Startup allowances supplied by the parent, shared by every inline slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Limits {
    pub slots: u32,
    pub smith: smith::Limits,
    /// Exact concrete turn ownership awaiting the parent's acknowledgements.
    pub window: smith::Window,
    /// Independent wall allowance; progress and waits never pause it.
    pub wall_time: Duration,
    pub facts: u32,
}

/// Maximum ordered outputs from one entry point, including cleanup fanout.
#[must_use]
pub fn max_out(limits: &Limits) -> u32 {
    output_count(limits).expect("worst_case accepted the output count")
}

fn output_count(limits: &Limits) -> Option<u32> {
    smith::max_out(&limits.smith)
        .checked_add(limits.smith.run.calls)?
        .checked_add(limits.smith.run.conversations)?
        .checked_add(3)
}

/// Retained roots, configuration, routing, scratch and caller queue ownership.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.facts < max_facts()
        || limits.window.turns == 0
        || limits.window.bytes < smith::max_turn_bytes(&limits.smith)?
        || limits.wall_time == Duration::ZERO
    {
        return None;
    }
    let outputs = output_count(limits)?;
    let per_slot = smith::worst_case(&limits.smith)?
        .checked_add(Map::<Token, Relay>::worst_case(limits.smith.run.calls)?)?
        .checked_add(Map::<Token, bool>::worst_case(limits.smith.run.conversations)?)?
        .checked_add(Map::<u32, u64>::worst_case(limits.window.turns)?)?;
    Slab::<Slot>::worst_case(limits.slots)?
        .checked_add(Map::<Token, Id<Slot>>::worst_case(limits.slots)?)?
        .checked_add(u64::from(limits.slots).checked_mul(per_slot)?)?
        .checked_add(Queue::<smith::Request>::worst_case(smith::max_out(&limits.smith))?)?
        .checked_add(Queue::<Output>::worst_case(outputs)?)?
        .checked_add(Queue::<Fact>::worst_case(limits.facts)?)?
        .checked_add(List::<Token>::worst_case(limits.slots)?)?
        .checked_add(List::<Token>::worst_case(limits.smith.run.calls)?)?
        .checked_add(List::<Token>::worst_case(limits.smith.run.conversations)?)?
        .checked_add(List::<smith::run::Message>::worst_case(limits.smith.run.messages)?)?
        .checked_add(List::<smith::Grant>::worst_case(limits.smith.accounts)?)?
        .checked_add(List::<smith::AnsweredCall>::worst_case(limits.smith.run.answered_calls)?)?
        .checked_add(u64::from(limits.smith.run.answered_calls).checked_mul(smith::run::Delivered::worst_case())?)?
        .checked_add(limits.smith.configured_model_bytes)?
        .checked_add(
            u64::from(limits.smith.endpoints)
                .checked_mul(u64::try_from(size_of::<smith::run::charter::Endpoint>()).ok()?)?,
        )?
        .checked_add(u64::try_from(size_of::<Domain>()).ok()?)
}

/// Maximum lifecycle facts emitted by any one entry point.
#[must_use]
pub const fn max_facts() -> u32 {
    3
}
