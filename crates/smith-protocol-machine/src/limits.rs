//! Bounded machine ownership (programming-model.md, section 6.3;
//! protocol/agent.md, section 2).

use core::mem::size_of;
use skein_lib::{List, Map, Token};

use crate::component::{Pending, Root};

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
pub const fn max_out(_limits: &Limits) -> MaxOut {
    MaxOut { to_domain: 1, below: 1 }
}

/// Memory retained by the component and one translating scan.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let roots = List::<Root>::worst_case(limits.roots)?;
    let operations = Map::<Token, Pending>::worst_case(limits.operations)?;
    let scan = u64::from(limits.entries).checked_mul(u64::try_from(size_of::<smith_domain::tools::Entry>()).ok()?)?;
    roots.checked_add(operations)?.checked_add(scan)?.checked_add(limits.entry_bytes)
}
