//! Host policy retained by one admitted run (domain/run.md, sections 3.1 and 14).
//! Admission checks counts and owned bytes before effects; this module knows no
//! task vocabulary, provider credentials or meaning behind the supplied text.
//!
//! It is policy as data. The run interprets no workflow vocabulary: the names
//! in a charter (of repositories, host declarations, verdicts, kinds and fields) are
//! labels, compared byte for byte, and text is the LLM's to read.

use alloc::boxed::Box;
use core::mem::size_of;

use skein_lib::Duration;

use crate::boundary::Invalid;
use crate::budget::Budget;
use crate::limits::Limits;
use crate::outcome::{self, OutcomeSpec};

pub use crate::conventions::Conventions;

pub use crate::host::{HostEffect, HostTool};

/// What a run is given when it starts.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Charter {
    /// Select supplied concrete history at the root entrance; false starts fresh.
    /// Contract: domain/run.md, sections 3 and 13.
    pub resume: bool,

    /// Positive idle interval after a settled main wait and yield, bounded by
    /// the receiving waiting limit. Wall time continues independently.
    /// Contract: domain/run.md, sections 6 and 10.
    pub waiting: Duration,

    /// Host-attested UTF-8 role instructions for main, copied verbatim before its
    /// Brief. Empty is allowed; children receive only their own raw task.
    /// Payload counts against receiving `Limits.run_bytes`; excess refuses as
    /// `Invalid::TooLarge` before effects. The rendered session cap still applies.
    /// Contract: domain/run.md, sections 3.1, 3.3, 5.3, 13 and 14.
    pub instructions: Box<[u8]>,

    /// Host-written ordered context for main. Titles and text remain literal;
    /// no role, tools or authority are inferred from them. Count and aggregate
    /// ownership are admitted before discovery, or refuse as `Invalid::TooLarge`.
    /// Contract: domain/run.md, sections 3.1, 3.3, 5.3, 13 and 14.
    pub brief: Brief,

    /// Host-supplied relative guide/check paths, admitted before effects. None
    /// selects AGENTS.md and .smith/check. Both owning paths count in `run_bytes`;
    /// workspace authority and lower IO root confinement still apply.
    /// Contract: domain/run.md, sections 3.1, 3.3, 8.1, 12 and 14.
    pub conventions: Option<Conventions>,

    /// Explicit authority supplied by the opener, never inferred from role or text.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub grants: Grants,
    /// Host-supplied permitted result forms and their field/text/item rules;
    /// admission refuses malformed or impossible contracts before effects.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub outcome: OutcomeSpec,
    /// What the run may spend, across all its conversations.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub budget: Budget,
    /// The LLM the main conversation starts with.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub llm: Llm,
    /// The LLMs a sub-agent may be opened on, each named by its model, which
    /// it lists once.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub models: Box<[Llm]>,
}

/// Host-written context for one main activation, retained with its Charter.
/// No context is derived or reordered; empty context is valid. Section count
/// and owning bytes are bounded at admission, with `Invalid::TooLarge` terminal
/// refusal before effects. Children receive their caller's raw task instead.
/// Contract: domain/run.md, sections 3.1, 3.3, 5.3, 13 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Brief {
    /// Sections in host-supplied order, including empty or duplicate titles.
    /// Array cells and every title/text payload count in `Limits.run_bytes`;
    /// exact count above `Limits.brief_sections` refuses before traversal/effects.
    /// Contract: domain/run.md, sections 3.1, 3.3, 13 and 14.
    pub sections: Box<[Section]>,
}

/// One host-attested UTF-8 title and text, rendered literally for main.
/// Empty payloads and repeated titles are valid; owning bytes count against
/// receiving `Limits.run_bytes`, or refuse before effects as `Invalid::TooLarge`.
/// Contract: domain/run.md, sections 3.1, 3.3, 13 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Section {
    /// Host-written title, emitted after `## ` without interpretation or escaping.
    /// Empty and duplicate titles are allowed; payload counts in `Limits.run_bytes`.
    /// Excess aggregate storage refuses at admission as `Invalid::TooLarge`.
    /// Contract: domain/run.md, sections 3.1, 3.3, 13 and 14.
    pub title: Box<[u8]>,

    /// Host-written section body, emitted verbatim with paragraph termination.
    /// Empty text is allowed; payload counts in `Limits.run_bytes`. Excess
    /// aggregate storage refuses before effects as `Invalid::TooLarge`.
    /// Contract: domain/run.md, sections 3.1, 3.3, 13 and 14.
    pub text: Box<[u8]>,
}

/// What the LLM may do besides talking. Data, never derived from a role.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Grants {
    /// Separately granted main-only delivery with required host field caps.
    /// Final Change permission does not grant this tool; children never inherit it.
    /// Admission checks minimum container/name/value fit before session or IO.
    /// Contract: domain/run.md, sections 7.1, 8.1 and 8.4.
    pub deliver: Option<outcome::ChangeSpec>,
    /// The tools that act on the checkout, which conversations run themselves.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub tools: Tools,
    /// Asking for sub-agents: conversations of the LLM's own, opened by the run.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub agents: bool,
    /// Host-declared main-only tools. Names, descriptions, schemas, effects and
    /// relay deadlines are admitted as bounded data before session or IO.
    /// Contract: domain/run.md, sections 3, 5.1, 5.2 and 12.
    pub host_tools: Box<[HostTool]>,
}

/// The families of tools a conversation has: those it runs on the checkout,
/// and those the run serves. Host declarations stay with main.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Families {
    /// Tools offered or granted by this record; their names confer no additional authority.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub tools: Tools,
    /// Whether this conversation may ask its run to open sub-agents.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub agents: bool,
}

impl Families {
    /// The families `grants` give.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn of(grants: &Grants) -> Families {
        Families { tools: grants.tools, agents: grants.agents }
    }

    /// Whether these families are among `wider`'s.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn within(self, wider: Families) -> bool {
        let (Tools { inspect, modify, shell }, wide) = (self.tools, wider.tools);
        (!inspect || wide.inspect)
            && (!modify || wide.modify)
            && (!shell || wide.shell)
            && (!self.agents || wider.agents)
    }
}

/// The tool families a conversation runs on the checkout.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Tools {
    /// Read, list and search.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub inspect: bool,
    /// Write and edit, in the writable repositories.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub modify: bool,
    /// Run commands.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub shell: bool,
}

/// An LLM to talk to.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Llm {
    /// Opaque configured replay dialect; history requires an exact match.
    /// Contract: domain/run.md, sections 3 and 13.
    pub dialect: u32,

    /// The credential account configured for the endpoint.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub account: u32,
    /// Configured provider endpoint identity; the domain never resolves its address.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub model: Box<[u8]>,
    /// The most tokens each answer may take.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub max_tokens: u32,
}

/// A provider endpoint the agent is configured with: which provider, where,
/// with which credentials. The protocol layer holds all of that; the domain
/// only names it.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Endpoint(
    /// Host-configured numeric provider endpoint name, echoed without address resolution or authority inference.
    ///
    /// Contract: domain/run.md, sections 3 and 14.
    pub u32,
);

/// Whether a run may start on `charter` under `limits`, or what about it does
/// not fit them. Counts are checked before anything that compares names in
/// pairs.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn check(charter: &Charter, workspace: Option<&crate::Workspace>, limits: &Limits) -> Result<(), Invalid> {
    let Charter { instructions: _, brief, grants, outcome, budget, llm, models, resume: _, waiting, conventions } =
        charter;
    // Exact conversion: an overflowing count must refuse even at a u32::MAX cap.
    match u32::try_from(brief.sections.len()) {
        Ok(sections) if sections <= limits.brief_sections => {}
        Ok(_) | Err(_) => return Err(Invalid::TooLarge),
    }
    if !budget.is_workable()
        || !budget.within(&limits.budget)
        || *waiting == Duration::ZERO
        || *waiting > limits.waiting
    {
        return Err(Invalid::Budget);
    }
    if !fits(llm, limits) || count(models.len()) > limits.models || repeated_model(models) {
        return Err(Invalid::Llm);
    }
    for model in models {
        if !fits(model, limits) {
            return Err(Invalid::Llm);
        }
    }
    if let Some(conventions) = conventions
        && !crate::conventions::valid(conventions)
    {
        return Err(Invalid::Conventions);
    }
    crate::workspace::admit(workspace, limits)?;
    let owned = match cost(charter) {
        Some(bytes) => match crate::workspace::cost(workspace) {
            Some(workspace_bytes) => bytes.checked_add(workspace_bytes),
            None => None,
        },
        None => None,
    };
    match owned {
        Some(bytes) if bytes <= limits.run_bytes => {}
        Some(_) | None => return Err(Invalid::TooLarge),
    }
    crate::workspace::check(workspace, limits)?;
    if count(grants.host_tools.len()) > limits.host_tools
        || repeated_host_tool(&grants.host_tools)
        || !valid_host_tools(&grants.host_tools, limits)
    {
        return Err(Invalid::Grants);
    }
    if !outcome::is_valid(outcome, limits) {
        return Err(Invalid::Outcome);
    }
    if let Some(spec) = &grants.deliver
        && !outcome::valid_change(spec, limits)
    {
        return Err(Invalid::Grants);
    }
    if (outcome.change.is_some() || grants.deliver.is_some())
        && (count(crate::workspace::directories(workspace).len()) > crate::MAX_DIRECTORIES
            || limits.delivery_timeout == Duration::ZERO)
    {
        return Err(Invalid::Workspace);
    }
    let writable = has_writable(workspace);
    if outcome.change.is_some() && !writable {
        return Err(Invalid::Outcome);
    }
    if grants.deliver.is_some() && !writable {
        return Err(Invalid::Grants);
    }
    Ok(())
}

fn has_writable(workspace: Option<&crate::Workspace>) -> bool {
    for directory in crate::workspace::directories(workspace) {
        if directory.writable {
            return true;
        }
    }
    false
}

/// The bytes a charter holds beyond its fixed size: each part held in a box
/// at its fixed size, plus its payload. `None` past a `u64`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn cost(charter: &Charter) -> Option<u64> {
    let mut cost = len(&charter.instructions)?.checked_add(len(&charter.llm.model)?)?;
    let section_bytes = u64::try_from(size_of::<Section>()).ok()?;
    let sections = u64::try_from(charter.brief.sections.len()).ok()?;
    cost = cost.checked_add(sections.checked_mul(section_bytes)?)?;
    for section in &charter.brief.sections {
        cost = cost.checked_add(len(&section.title)?)?.checked_add(len(&section.text)?)?;
    }
    if let Some(conventions) = &charter.conventions {
        // The two Box wrappers live inline in Charter, priced by Slab<Run>.
        cost = cost.checked_add(len(&conventions.guide)?)?.checked_add(len(&conventions.checks)?)?;
    }
    let llm = u64::try_from(size_of::<Llm>()).ok()?;
    for Llm { account: _, endpoint: _, model, max_tokens: _, dialect: _ } in &charter.models {
        cost = cost.checked_add(llm)?.checked_add(len(model)?)?;
    }
    let host_tool = u64::try_from(size_of::<HostTool>()).ok()?;
    for tool in &charter.grants.host_tools {
        cost = cost
            .checked_add(host_tool)?
            .checked_add(len(&tool.name)?)?
            .checked_add(len(&tool.description)?)?
            .checked_add(len(&tool.schema)?)?;
    }
    if let Some(spec) = &charter.grants.deliver {
        cost = cost.checked_add(outcome::change_cost(spec)?)?;
    }
    cost.checked_add(outcome::cost(&charter.outcome)?)
}

/// Whether an LLM asks for an answer that fits the limits.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn fits(llm: &Llm, limits: &Limits) -> bool {
    llm.max_tokens > 0 && llm.max_tokens <= limits.max_tokens
}

fn repeated_model(models: &[Llm]) -> bool {
    for (index, llm) in models.iter().enumerate() {
        for other in models.get(index.saturating_add(1)..).unwrap_or_default() {
            if other.model == llm.model {
                return true;
            }
        }
    }
    false
}

fn repeated_host_tool(host_tools: &[HostTool]) -> bool {
    for (index, host_tool) in host_tools.iter().enumerate() {
        for other in host_tools.get(index.saturating_add(1)..).unwrap_or_default() {
            if other.name == host_tool.name {
                return true;
            }
        }
    }
    false
}

/// A length as a count, saturating: anything past a `u32` is past every limit.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

pub(crate) fn len(bytes: &[u8]) -> Option<u64> {
    u64::try_from(bytes.len()).ok()
}

fn valid_host_tools(tools: &[HostTool], limits: &Limits) -> bool {
    if !tools.is_empty()
        && (limits.host_attempts == 0
            || limits.host_timeout == Duration::ZERO
            || limits.host_backoff == Duration::ZERO
            || limits.host_input_bytes < 2
            || limits.host_input_bytes > u32::try_from(crate::HostInput::CAPACITY).expect("fixed input cap")
            || limits.host_reply_bytes > u32::try_from(crate::HostAnswer::CAPACITY).expect("fixed answer cap"))
    {
        return false;
    }
    for tool in tools {
        if tool.name.is_empty()
            || tool.description.is_empty()
            || tool.schema.is_empty()
            || tool.timeout == Duration::ZERO
        {
            return false;
        }
        for reserved in [
            b"finish".as_slice(),
            b"deliver",
            b"wait",
            b"subagent",
            b"sub_agent",
            b"read",
            b"list",
            b"search",
            b"write",
            b"edit",
            b"shell",
            b"read_file",
            b"list_dir",
            b"write_file",
            b"edit_file",
            b"run_shell",
        ] {
            if tool.name.as_ref() == reserved {
                return false;
            }
        }
    }
    true
}
