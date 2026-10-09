//! Host policy retained by one admitted run (domain/run.md, section 3.1).
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
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Charter {
    /// Select supplied concrete history at the root entrance; false starts fresh.
    pub resume: bool,

    /// Idle interval after a settled main wait and yield, bounded by the
    /// receiving waiting limit. Zero parks at once. Wall time continues.
    pub waiting: Duration,

    /// Host-attested UTF-8 role instructions for main, copied verbatim before its
    /// Brief. Empty is allowed; children receive only their own raw task.
    /// Payload counts against receiving `Limits.run_bytes`; excess refuses as
    /// `Invalid::TooLarge` before effects. The rendered session cap still applies.
    pub instructions: Box<[u8]>,

    /// Host-written ordered context for main. Titles and text remain literal;
    /// no role, tools or authority are inferred from them. Count and aggregate
    /// ownership are admitted before discovery, or refuse as `Invalid::TooLarge`.
    pub brief: Brief,

    /// Host-supplied relative guide/check paths, admitted before effects. None
    /// selects AGENTS.md and .smith/check. Both owning paths count in `run_bytes`;
    /// workspace authority and lower IO root confinement still apply.
    pub conventions: Option<Conventions>,

    /// Explicit authority supplied by the opener, never inferred from role or text.
    pub grants: Grants,
    /// Host-supplied permitted result forms and their field/text/item rules;
    /// admission refuses malformed or impossible contracts before effects.
    pub outcome: OutcomeSpec,
    /// What the run may spend, across all its conversations.
    pub budget: Budget,
    /// The LLM the main conversation starts with.
    pub llm: Llm,
    /// The LLMs a sub-agent may be opened on, each named by its model, which
    /// it lists once.
    pub models: Box<[Llm]>,
}

/// Host-written context for one main activation, retained with its Charter.
/// No context is derived or reordered; empty context is valid. Section count
/// and owning bytes are bounded at admission, with `Invalid::TooLarge` terminal
/// refusal before effects. Children receive their caller's raw task instead.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Brief {
    /// Sections in host-supplied order, including empty or duplicate titles.
    /// Array cells and every title/text payload count in `Limits.run_bytes`;
    /// exact count above `Limits.brief_sections` refuses before traversal/effects.
    pub sections: Box<[Section]>,
}

/// One host-attested UTF-8 title and text, rendered literally for main.
/// Empty payloads and repeated titles are valid; owning bytes count against
/// receiving `Limits.run_bytes`, or refuse before effects as `Invalid::TooLarge`.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Section {
    /// Host-written title, emitted after `## ` without interpretation or escaping.
    /// Empty and duplicate titles are allowed; payload counts in `Limits.run_bytes`.
    /// Excess aggregate storage refuses at admission as `Invalid::TooLarge`.
    pub title: Box<[u8]>,

    /// Host-written section body, emitted verbatim with paragraph termination.
    /// Empty text is allowed; payload counts in `Limits.run_bytes`. Excess
    /// aggregate storage refuses before effects as `Invalid::TooLarge`.
    pub text: Box<[u8]>,
}

/// What the LLM may do besides talking. Data, never derived from a role.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Grants {
    /// Main-only waiting authority. When absent the main session cannot call
    /// `wait`; children never inherit it.
    pub wait: bool,

    /// Separately granted main-only delivery with required host field caps.
    /// Final Change permission does not grant this tool; children never inherit it.
    /// Admission checks minimum container/name/value fit before session or IO.
    pub deliver: Option<outcome::ChangeSpec>,
    /// The tools that act on the checkout, which conversations run themselves.
    pub tools: Tools,
    /// Asking for sub-agents: conversations of the LLM's own, opened by the run.
    pub agents: bool,
    /// Host-declared main-only tools. Names, descriptions, schemas, effects and
    /// relay deadlines are admitted as bounded data before session or IO.
    pub host_tools: Box<[HostTool]>,
}

/// The families of tools a conversation has: those it runs on the checkout,
/// and those the run serves. Host declarations stay with main.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Families {
    /// Tools offered or granted by this record; their names confer no additional authority.
    pub tools: Tools,
    /// Whether this conversation may ask its run to open sub-agents.
    pub agents: bool,
}

impl Families {
    /// The families `grants` give.
    pub(crate) fn of(grants: &Grants) -> Families {
        Families { tools: grants.tools, agents: grants.agents }
    }

    /// Whether these families are among `wider`'s.
    pub(crate) fn within(self, wider: Families) -> bool {
        let (Tools { inspect, modify, shell }, wide) = (self.tools, wider.tools);
        (!inspect || wide.inspect)
            && (!modify || wide.modify)
            && (!shell || wide.shell)
            && (!self.agents || wider.agents)
    }
}

/// The tool families a conversation runs on the checkout.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Tools {
    /// Read, list and search.
    pub inspect: bool,
    /// Write and edit, in the writable repositories.
    pub modify: bool,
    /// Run commands.
    pub shell: bool,
}

/// An LLM to talk to.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Llm {
    /// Host model rates; unit zero refuses before discovery.
    pub prices: crate::Prices,

    /// Opaque configured replay dialect; history requires an exact match.
    pub dialect: u32,

    /// The credential account configured for the endpoint.
    pub account: u32,
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    pub model: Box<[u8]>,
    /// Usable input tokens for this conversation.
    pub window: u32,
    /// Output tokens reserved for one completion, thinking included.
    pub output: u32,
}

/// A provider endpoint the agent is configured with: which provider, where,
/// with which credentials. The protocol layer holds all of that; the domain
/// only names it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Endpoint(
    /// Host-configured numeric provider endpoint name, echoed without address resolution or authority inference.
    pub u32,
);

/// Whether a run may start on `charter` under `limits`, or what about it does
/// not fit them. Counts are checked before anything that compares names in
/// pairs.
pub(crate) fn check(charter: &Charter, workspace: Option<&crate::Workspace>, limits: &Limits) -> Result<(), Invalid> {
    let Charter { instructions: _, brief, grants, outcome, budget, llm, models, resume: _, waiting, conventions } =
        charter;
    // Exact conversion: an overflowing count must refuse even at a u32::MAX cap.
    match u32::try_from(brief.sections.len()) {
        Ok(sections) if sections <= limits.brief_sections => {}
        Ok(_) | Err(_) => return Err(Invalid::TooLarge),
    }
    if !budget.is_workable() || !budget.within(&limits.budget) || *waiting > limits.waiting {
        return Err(Invalid::Budget);
    }
    if !valid_llm(llm) || count(models.len()) > limits.models || repeated_model(models) {
        return Err(Invalid::Llm);
    }
    for model in models {
        if !valid_llm(model) {
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
    for Llm { account: _, endpoint: _, model, window: _, output: _, dialect: _, prices: _ } in &charter.models {
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

/// Whether an LLM declares positive quantities and a positive price unit.
fn valid_llm(llm: &Llm) -> bool {
    if llm.prices.unit == 0 {
        return false;
    }
    llm.window > 0 && llm.output > 0
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
pub(crate) fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

pub(crate) fn len(bytes: &[u8]) -> Option<u64> {
    u64::try_from(bytes.len()).ok()
}

/// Smith-owned tool names that a host declaration cannot take (domain/run.md, section 5.1).
pub(crate) const RESERVED_NAMES: [&[u8]; 10] =
    [b"finish", b"deliver", b"wait", b"sub_agent", b"read", b"list", b"search", b"write", b"edit", b"shell"];

fn valid_host_tools(tools: &[HostTool], limits: &Limits) -> bool {
    if !tools.is_empty()
        && (limits.host_timeout == Duration::ZERO
            || limits.host_timeout_max == Duration::ZERO
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
        for reserved in RESERVED_NAMES {
            if tool.name.as_ref() == reserved {
                return false;
            }
        }
    }
    true
}
