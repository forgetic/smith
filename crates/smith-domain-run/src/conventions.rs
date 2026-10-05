//! Host policy for relative workspace guide/check paths (domain/run.md,
//! sections 3.1, 3.3, 8.1, 12 and 14). Admitted charters keep two owning paths;
//! the absent policy keeps no path allocation and selects Smith defaults.
//! Discovery, system text and exclusive checks borrow the same selection.
//! This module knows no host workflow or filesystem; IO confines resolution,
//! including symlinks, beneath the supplied root. Validation precedes effects.

use alloc::boxed::Box;

use crate::charter::Charter;
use crate::delivery::{Marker, relative_path};

/// Host-supplied workspace conventions, replacing both default relative paths.
/// The host attests UTF-8, as for the charter's text. Admission refuses unsafe
/// or oversized spellings before IO; accepted paths are retained unchanged
/// within Charter's aggregate byte cap. There is no fallback to legacy paths.
/// Contract: domain/run.md, sections 3.1, 3.3, 8.1, 12 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Conventions {
    /// Relative guide path read in every mount and named in its system heading.
    /// Missing/nontext/failed reads yield no guide; at most `guide_bytes` are read.
    /// Contract: domain/run.md, sections 3.3, 8.1, 12 and 14.
    pub guide: Box<[u8]>,

    /// Relative executable path probed and run only in writable mounts when
    /// a Change or separately granted delivery requires checks. No executable
    /// means no checks; an actual failing check returns bounded feedback.
    /// Contract: domain/run.md, sections 8.1, 10, 12 and 14.
    pub checks: Box<[u8]>,
}

impl Conventions {
    /// Maximum bytes per path, sharing the bounded relative marker vocabulary.
    /// Both paths also count against receiving `Limits.run_bytes`; their inline
    /// Box wrappers are part of Charter and the domain's slab bound.
    /// Contract: domain/run.md, sections 3.1, 8.1, 12 and 14.
    pub const PATH_CAPACITY: usize = Marker::CAPACITY;
}

pub(crate) fn valid(conventions: &Conventions) -> bool {
    valid_path(&conventions.guide) && valid_path(&conventions.checks)
}

fn valid_path(path: &[u8]) -> bool {
    if path.len() > Conventions::PATH_CAPACITY || !relative_path(path) {
        return false;
    }
    for byte in path {
        // Portable relative spelling and safe single-line prompt labels.
        if *byte < b' ' || *byte == 127 || *byte == b'\\' {
            return false;
        }
    }
    true
}

pub(crate) fn guide(charter: &Charter) -> &[u8] {
    match &charter.conventions {
        Some(conventions) => &conventions.guide,
        None => b"AGENTS.md",
    }
}

pub(crate) fn checks(charter: &Charter) -> &[u8] {
    match &charter.conventions {
        Some(conventions) => &conventions.checks,
        None => b".smith/check",
    }
}
