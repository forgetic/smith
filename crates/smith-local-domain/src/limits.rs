//! Checked ownership bound for local policy and its in-process agent
//! (domain/host.md, sections 6 and 8; programming-model.md, section 6.3).

use alloc::boxed::Box;
use core::mem::size_of;
use skein_lib::{Map, Queue, Token};
use smith_domain::{self as agent, run};

use crate::{AgentIo, DeliveryRecord, Fact, IntentDirectory, Request};

/// Capacity of one local chat and its child agent.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Receiving limits of the child agent.
    pub agent: agent::Limits,
    /// Endpoint names permitted by the child agent's configuration.
    pub endpoints: Box<[run::charter::Endpoint]>,
    /// Maximum chat store-key bytes.
    pub chat_bytes: u32,
    /// Maximum local instructions and brief text bytes.
    pub text_bytes: u32,
    /// Maximum model choices, main included.
    pub models: u32,
    /// Maximum terminal line bytes.
    pub line_bytes: u32,
    /// Maximum rendered text bytes per notice.
    pub show_bytes: u32,
    /// Maximum queued person lines.
    pub lines: u32,
    /// Maximum turns awaiting durable acknowledgement.
    pub unsaved: u32,
    /// Maximum content-free observations.
    pub facts: u32,
}

/// Bound on owned local state, including the child domain.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.models == 0 || limits.lines == 0 || limits.unsaved == 0 || limits.show_bytes < 64 {
        return None;
    }
    let agent = agent::worst_case(&limits.agent)?;
    let endpoints = u64::from(limits.models)
        .checked_mul(u64::try_from(size_of::<run::charter::Llm>()).ok()?)?
        .checked_add(u64::from(limits.models).checked_mul(u64::from(limits.text_bytes))?)?;
    let accounts = u64::from(limits.agent.accounts).checked_mul(u64::try_from(size_of::<u32>()).ok()?)?;
    let configuration = u64::from(limits.chat_bytes)
        .checked_add(u64::from(limits.text_bytes).checked_mul(2)?)?
        .checked_add(endpoints)?
        .checked_add(accounts)?
        .checked_add(limits.agent.run.run_bytes)?;
    let lines = Queue::<Token>::worst_case(limits.lines)?
        .checked_add(u64::from(limits.lines).checked_mul(u64::from(limits.line_bytes))?)?;
    let record = limits.agent.session.session_bytes.checked_add(
        u64::from(limits.agent.session.messages).checked_mul(u64::try_from(size_of::<agent::Turn>()).ok()?)?,
    )?;
    let unsaved = u64::from(limits.unsaved).checked_mul(record)?;
    let out = Queue::<Request>::worst_case(agent::max_out(&limits.agent).saturating_add(limits.lines))?
        .checked_add(u64::from(limits.show_bytes))?;
    let child_out = Queue::<agent::Request>::worst_case(agent::max_out(&limits.agent))?
        .checked_add(u64::from(agent::max_out(&limits.agent)).checked_mul(record.max(limits.agent.run.run_bytes))?)?;
    let held = Queue::<AgentIo>::worst_case(1)?.checked_add(limits.agent.session.completion_bytes)?;
    let answered = Map::<run::CallName, DeliveryRecord>::worst_case(limits.agent.run.answered_calls)?
        .checked_add(u64::from(limits.agent.run.answered_calls).checked_mul(run::Delivered::worst_case())?)?;
    let delivery = skein_lib::List::<run::Receipt>::worst_case(run::MAX_DIRECTORIES)?
        .checked_add(run::Delivered::worst_case().checked_mul(4)?)?
        .checked_add(limits.agent.run.outcome_bytes.checked_mul(2)?)?
        .checked_add(skein_lib::List::<IntentDirectory>::worst_case(run::MAX_DIRECTORIES)?.checked_mul(3)?)?
        .checked_add(
            u64::from(run::MAX_DIRECTORIES).checked_mul(u64::try_from(run::Receipt::CAPACITY).ok()?)?.checked_mul(3)?,
        )?
        .checked_add(run::Delivered::worst_case().checked_mul(2)?)?;
    agent
        .checked_add(configuration)?
        .checked_add(lines)?
        .checked_add(unsaved)?
        .checked_add(out)?
        .checked_add(child_out)?
        .checked_add(held)?
        .checked_add(delivery)?
        .checked_add(answered)?
        .checked_add(Queue::<Fact>::worst_case(limits.facts)?)
}
