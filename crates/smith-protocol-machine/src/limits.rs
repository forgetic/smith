//! Bounded machine ownership (programming-model.md, section 6.3;
//! protocol/agent.md, section 2).

use core::mem::size_of;
use skein_lib::{Duration, List, Map, Token};

use crate::component::{Pending, Root};
use crate::process::Process;

/// File translation capacity, checked before effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum simultaneous file requests.
    pub operations: u32,
    /// Maximum admitted workspace roots.
    pub roots: u32,
    /// Maximum relative path bytes.
    pub path_bytes: u32,
    /// Maximum content bytes for load or store.
    pub file_bytes: u32,
    /// Maximum entries retained from a scan.
    pub entries: u32,
    /// Maximum retained entry cells and names.
    pub entry_bytes: u64,
    /// Maximum simultaneous child processes.
    pub processes: u32,
    /// Upper bound for either command output window.
    pub output_bytes: u32,
    /// Upper bound for retained search hits.
    pub search_hits: u32,
    /// Upper bound for retained search path and text bytes.
    pub search_bytes: u32,
    /// Maximum one rg JSON line accepted for parsing.
    pub search_line_bytes: u32,
    /// Maximum configured command environment bytes.
    pub env_bytes: u32,
    /// Grace between a polite signal and a kill.
    pub stop_grace: Duration,
}

/// The most records one step can emit into each queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxOut {
    /// Domain terminals.
    pub to_domain: u32,
    /// IO requests.
    pub below: u32,
}

/// One terminal or one request per entry point.
#[must_use]
pub const fn max_out(limits: &Limits) -> MaxOut {
    let below = if limits.processes > 3 { limits.processes } else { 3 };
    MaxOut { to_domain: 1, below }
}

/// Memory retained by the component and one translating scan.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let roots = List::<Root>::worst_case(limits.roots)?;
    let operations = Map::<Token, Pending>::worst_case(limits.operations)?;
    let scan = u64::from(limits.entries).checked_mul(u64::try_from(size_of::<smith_domain::tools::Entry>()).ok()?)?;
    let processes = Map::<Token, Process>::worst_case(limits.processes)?;
    let guides = Map::<Token, crate::guide::Guide>::worst_case(limits.operations)?;
    let pipes = Map::<Token, Token>::worst_case(limits.processes.checked_mul(2)?)?;
    let output = u64::from(limits.output_bytes).checked_mul(2)?.checked_mul(u64::from(limits.processes))?;
    let search = u64::from(limits.search_bytes)
        .checked_add(u64::from(limits.search_line_bytes))?
        .checked_add(
            u64::from(limits.search_hits).checked_mul(u64::try_from(size_of::<smith_domain::tools::Hit>()).ok()?)?,
        )?
        .checked_mul(u64::from(limits.processes))?;
    roots
        .checked_add(operations)?
        .checked_add(scan)?
        .checked_add(limits.entry_bytes)?
        .checked_add(processes)?
        .checked_add(guides)?
        .checked_add(pipes)?
        .checked_add(output)?
        .checked_add(search)?
        .checked_add(u64::from(limits.env_bytes))
}
