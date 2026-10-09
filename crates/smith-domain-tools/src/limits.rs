//! Ownership caps and checked worst-case arithmetic. This module retains no runtime state.
//! `worst_case` projects immutable limits into container and payload bounds, returning `None` on overflow.
//!
//! Contract: domain/tools.md, section 4; programming-model.md, sections 4.4, 4.5 and 6.3.

use skein_lib::{Duration, Id, List, Map, Queue, Set, Slab};

use crate::authority::{Mount, Var};
use crate::boundary::Root;
use crate::call::{Call, Entry, Hit};
use crate::facts::Fact;
use crate::job::{self, Job};
use crate::kit::Kit;
use crate::knowledge::Seen;
use crate::path::{Name, Place};

/// The tools child domain's limits (programming-model.md, sections 4.5 and 6.3), handed by its parent to every
/// step read-only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Kits at once, one per session. An open beyond them is refused as busy.
    pub kits: u32,
    /// Calls a kit runs at once. A call beyond them is answered `Busy`.
    pub calls: u32,
    /// Repositories an authority may name.
    pub repos: u32,
    /// The longest path the tools take, in bytes, as an absolute path's names
    /// joined by `/`: a path a call names, a mount, the working directory.
    /// A call's spelling before normalisation must also fit, including dots.
    pub path_bytes: u32,
    /// Files a kit remembers the LLM read. Past them, the one read longest
    /// ago is forgotten, and must be read again before it is changed.
    pub known_files: u32,
    /// The largest file the tools load or store.
    pub file_bytes: u32,
    /// The most content a read answers with.
    pub read_bytes: u32,
    /// The most entries a listing answers with.
    pub list_entries: u32,

    /// Maximum listing ownership: each entry's fixed size plus its name bytes.
    /// The lower scan returns a name-order prefix within this and `list_entries`,
    /// preserving the count of omitted entries. This also bounds late results.
    pub list_bytes: u64,

    /// The most line numbers an ambiguous edit answers with.
    pub match_lines: u32,
    /// How long a file operation may take, within its call's deadline.
    pub file_timeout: Duration,
    /// The most an authority's environment may hold: its names and values,
    /// with a byte for each `=`.
    pub env_bytes: u32,
    /// How long a command may run if its call does not say, and the longest
    /// it may ask for; both within the call's deadline.
    pub shell_timeout: Duration,
    /// Maximum accepted explicit shell timeout; larger requests are refused.
    pub shell_timeout_max: Duration,
    /// How much of a command's output is kept: its first bytes, and its last.
    pub shell_head: u32,
    /// Last output bytes retained from a contained shell process.
    pub shell_tail: u32,
    /// The most lines a search answers with, and the most text in them.
    pub search_hits: u32,
    /// Maximum aggregate retained search-path and text bytes.
    pub search_bytes: u32,
    /// How long a search may take, within its call's deadline.
    pub search_timeout: Duration,
    /// Reserved observations buffered until the parent drains them; at least [`crate::max_facts`].
    pub facts: u32,
}

/// The most memory the domain holds under `limits`, in bytes (programming-model.md, section 6.3), or `None`
/// if it does not fit a `u64` or the limits cannot be honoured.
///
/// It counts the containers, their bookkeeping included, and the payloads, not
/// allocator overhead. What travels in events and requests is counted by the
/// layer that holds it: the content of a file loaded or to be stored, and the
/// outcomes answered.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.facts < crate::max_facts(limits) {
        return None;
    }
    let smallest_entry = u64::try_from(size_of::<Entry>()).ok()?.checked_add(1)?;
    if limits.list_bytes < smallest_entry {
        return None;
    }
    let kits = Slab::<Kit>::worst_case(limits.kits)?.checked_add(u64::from(limits.kits).checked_mul(kit(limits)?)?)?;
    // Only running jobs hold a place, at most `calls` a kit; the slab has more
    // slots, for the jobs answered in an iteration.
    let running = u64::from(limits.kits).checked_mul(u64::from(limits.calls))?;
    let jobs = Slab::<Job>::worst_case(job::slots(limits)?)?.checked_add(running.checked_mul(job::held(limits)?)?)?;
    // Facts own nothing beyond their queue.
    let facts = Queue::<Fact>::worst_case(limits.facts)?;
    // One call normalises a path at a time: flags for its raw components,
    // references to those and the working directory, then the joined path
    // and its copy beneath the chosen repository.
    let parts = limits.path_bytes.checked_add(1)? / 2;
    let scratch = List::<bool>::worst_case(parts)?
        .checked_add(List::<&Name>::worst_case(parts.checked_mul(2)?)?)?
        .checked_add(u64::from(limits.path_bytes).checked_mul(2)?)?;
    kits.checked_add(jobs)?.checked_add(facts)?.checked_add(scratch)
}

/// Maximum dynamic ownership of this call's outcome, excluding its
/// enclosing result block and copied provider ID, which the session reserves
/// separately. The parent uses this before dispatch, including for a terminal
/// that wins cancellation; `None` means a configured bound overflowed.
#[must_use]
pub fn result_worst_case(call: &Call, limits: &Limits) -> Option<u64> {
    match call {
        Call::Read { .. } => Some(u64::from(limits.read_bytes)),
        Call::List { .. } => Some(limits.list_bytes),
        Call::Search { .. } => {
            let cells = u64::try_from(size_of::<Hit>()).ok()?;
            cells.checked_mul(u64::from(limits.search_hits))?.checked_add(u64::from(limits.search_bytes))
        }
        Call::Write { .. } => Some(0),
        Call::Edit { .. } => {
            let ambiguity_slot_bytes = u64::try_from(size_of::<u32>()).ok()?;
            ambiguity_slot_bytes.checked_mul(u64::from(limits.match_lines))
        }
        Call::Shell { .. } => u64::from(limits.shell_head).checked_add(u64::from(limits.shell_tail)),
    }
}

/// What one kit holds beyond its slot: its authority, each path in it at most
/// `path_bytes` joined; what its LLM knows, a place for each file; and its
/// jobs' names.
fn kit(limits: &Limits) -> Option<u64> {
    let known = u64::from(limits.known_files).checked_mul(u64::from(limits.path_bytes))?;
    let knowledge = Map::<Place, Seen>::worst_case(limits.known_files)?.checked_add(known)?;
    let jobs = Set::<Id<Job>>::worst_case(limits.calls)?;
    authority(limits)?.checked_add(knowledge)?.checked_add(jobs)
}

/// What a kit's authority holds, each path in it at most `path_bytes` joined.
fn authority(limits: &Limits) -> Option<u64> {
    let path_bytes = u64::from(limits.path_bytes);
    // A name is at least a byte, and a slash parts it from the next.
    let cwd_names = limits.path_bytes.checked_add(1)? / 2;
    let cwd = List::<Name>::worst_case(cwd_names)?.checked_add(path_bytes)?;
    let mount_bytes = u64::from(limits.repos).checked_mul(path_bytes)?;
    let mounts = List::<Mount>::worst_case(limits.repos)?.checked_add(mount_bytes)?;
    let roots = List::<Root>::worst_case(limits.repos)?;
    // A variable costs at least two bytes: a name and its `=`.
    let vars = limits.env_bytes / 2;
    let env = List::<Var>::worst_case(vars)?.checked_add(u64::from(limits.env_bytes))?;
    cwd.checked_add(mounts)?.checked_add(roots)?.checked_add(env)
}

/// Numeric declarations needed by this layer's transitional derivation.
/// The service supplies only owned numbers and this domain's own child limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Derivation {
    pub conversations: u32,
    pub calls_per_response: u32,
    pub tool_payload: u32,
    pub read_window: u32,
    pub shell_head: u32,
    pub shell_tail: u32,
    pub search_hits: u32,
    pub search_bytes: u32,
    pub list_entries: u32,
    pub environment_bytes: u32,
    pub tool_deadline: Duration,
    pub shell_timeout: Duration,
}

/// Derive this layer from declarations, refusing checked arithmetic overflow.
/// Contract: protocol/limits.md, sections 2 and 3.
#[must_use]
pub fn derive(inputs: &Derivation) -> Option<Limits> {
    if inputs.conversations == 0 || inputs.calls_per_response == 0 {
        return None;
    }
    Some(Limits {
        kits: inputs.conversations,
        calls: inputs.calls_per_response.min(4),
        repos: 2,
        path_bytes: 256,
        known_files: 16,
        file_bytes: inputs.tool_payload,
        read_bytes: inputs.read_window,
        list_entries: inputs.list_entries,
        // A listing uses one default read window of ownership, with room for
        // at least one entry cell and a nonempty name.
        list_bytes: u64::from(inputs.read_window).max(u64::try_from(size_of::<Entry>()).ok()?.checked_add(1)?),
        match_lines: 8,
        file_timeout: Duration::from_secs(30).min(inputs.tool_deadline),
        env_bytes: inputs.environment_bytes,
        shell_timeout: inputs.shell_timeout,
        shell_timeout_max: inputs.tool_deadline,
        shell_head: inputs.shell_head,
        shell_tail: inputs.shell_tail,
        search_hits: inputs.search_hits,
        search_bytes: inputs.search_bytes,
        search_timeout: Duration::from_secs(30).min(inputs.tool_deadline),
        facts: 1024,
    })
}
