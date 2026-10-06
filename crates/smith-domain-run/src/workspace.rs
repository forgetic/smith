//! Host-owned optional workspace metadata (domain/run.md, sections 3.2, 8.3 and 14).
//! Admission bounds counts and payloads before comparing names or paths. Retained
//! roots and write authority are immutable; this module never knows filesystem
//! contents, host delivery policy or credentials. `check` validates before effects.

use crate::boundary::Invalid;
use crate::charter::{Families, Tools, len};
use crate::delivery;
use crate::limits::Limits;
use alloc::boxed::Box;
use core::mem::size_of;
use skein_lib::Token;

/// Host-selected directories, moved with Start and retained until the run ends.
/// None at Start means no workspace; a present empty workspace is refused.
/// Contract: domain/run.md, sections 3.2, 8.3 and 14; domain/tools.md, section 2.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Workspace {
    /// Ordered unique mounts, bounded by receiving `Limits::directories`.
    /// Contract: domain/run.md, sections 3.2 and 14.
    pub directories: Box<[Directory]>,
}

/// Immutable host mount and initial git merge evidence, admitted before IO.
/// Contract: domain/run.md, sections 3.2, 8.1, 8.3 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Directory {
    /// Unique safe single component, bounded by `Limits::directory_name_bytes`.
    /// The host attests textual encoding; bytes are compared and rendered
    /// unchanged without domain decoding or replacement.
    /// Contract: domain/run.md, sections 3.2 and 12; domain/tools.md, section 2.
    pub name: Box<[u8]>,

    /// Unique opaque lower IO root, carrying no filesystem knowledge here.
    /// Contract: domain/run.md, sections 3.2 and 12.
    pub root: Token,

    /// Host write authority; discovery and checks never widen it.
    /// Contract: domain/run.md, sections 2, 3.2 and 8.1.
    pub writable: bool,

    /// Whether the host prepared this directory as a git working tree.
    /// Contract: domain/run.md, sections 3.2 and 8.3.
    pub git: bool,

    /// Initial relative conflict paths, unique within this git directory and
    /// bounded by receiving count/path caps. The host attests text encoding;
    /// paths are preserved unchanged. Plain directories require none;
    /// read-only git conflict evidence remains informative and grants no writes.
    /// Contract: domain/run.md, sections 3.2, 8.3, 12 and 14.
    pub conflicts: Box<[Box<[u8]>]>,
}

pub(crate) fn directories(workspace: Option<&Workspace>) -> &[Directory] {
    match workspace {
        Some(workspace) => &workspace.directories,
        None => &[],
    }
}

pub(crate) fn families(workspace: Option<&Workspace>, families: Families) -> Families {
    match workspace {
        Some(_) => families,
        None => Families { tools: Tools { inspect: false, modify: false, shell: false }, agents: families.agents },
    }
}

pub(crate) fn admit(workspace: Option<&Workspace>, limits: &Limits) -> Result<(), Invalid> {
    let mounted = directories(workspace);
    match workspace {
        Some(_) if mounted.is_empty() => return Err(Invalid::Workspace),
        Some(_) | None => {}
    }
    if exceeds(mounted.len(), limits.directories)
        || limits.conflict_path_bytes > u32::try_from(crate::Marker::CAPACITY).expect("fixed marker cap")
    {
        return Err(Invalid::Workspace);
    }
    // All receiving count and byte limits precede pairwise work.
    for directory in mounted {
        if exceeds(directory.name.len(), limits.directory_name_bytes)
            || exceeds(directory.conflicts.len(), limits.conflicts)
        {
            return Err(Invalid::Workspace);
        }
        for path in &directory.conflicts {
            if exceeds(path.len(), limits.conflict_path_bytes) {
                return Err(Invalid::Workspace);
            }
        }
    }
    Ok(())
}

pub(crate) fn check(workspace: Option<&Workspace>, limits: &Limits) -> Result<(), Invalid> {
    admit(workspace, limits)?;
    let mounted = directories(workspace);
    for (position, directory) in mounted.iter().enumerate() {
        if !safe_name(&directory.name) || (!directory.git && !directory.conflicts.is_empty()) {
            return Err(Invalid::Workspace);
        }
        for earlier in mounted.get(..position).expect("enumerated directory") {
            if earlier.name == directory.name || earlier.root == directory.root {
                return Err(Invalid::Workspace);
            }
        }
        for (position, path) in directory.conflicts.iter().enumerate() {
            if !delivery::relative_path(path) {
                return Err(Invalid::Workspace);
            }
            for earlier in directory.conflicts.get(..position).expect("enumerated conflict") {
                if earlier == path {
                    return Err(Invalid::Workspace);
                }
            }
        }
    }
    Ok(())
}

fn exceeds(length: usize, maximum: u32) -> bool {
    // An unrepresentable caller length exceeds every receiving u32 cap.
    match u32::try_from(length) {
        Ok(length) => length > maximum,
        Err(_) => true,
    }
}

fn safe_name(name: &[u8]) -> bool {
    if name.is_empty() || name == b"." || name == b".." {
        return false;
    }
    for byte in name {
        if *byte == 0 || *byte == b'/' {
            return false;
        }
    }
    true
}

pub(crate) fn cost(workspace: Option<&Workspace>) -> Option<u64> {
    let mut cost = 0_u64;
    let cell = u64::try_from(size_of::<Directory>()).ok()?;
    let path_cell = u64::try_from(size_of::<Box<[u8]>>()).ok()?;
    for directory in directories(workspace) {
        cost = cost.checked_add(cell)?.checked_add(len(&directory.name)?)?;
        for path in &directory.conflicts {
            cost = cost.checked_add(path_cell)?.checked_add(len(path)?)?;
        }
    }
    Some(cost)
}
