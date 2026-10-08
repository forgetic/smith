//! Whole files, conditional stores, and bounded scans over skein's file io
//! (protocol/agent.md, section 2; domain/tools.md, sections 4 and 6).

use alloc::boxed::Box;

use skein_io::{digest, file, kernel};
use skein_lib::{List, Token};
use smith_domain::tools;

use crate::component::{Kind, Root};
use crate::limits::Limits;

/// Prepare one file request, or refuse before io sees an unsafe path.
pub(crate) fn request(
    roots: &List<Root>,
    limits: &Limits,
    owner: Token,
    op: tools::Op,
) -> Result<(Kind, file::Request), tools::Done> {
    match op {
        tools::Op::Load { at, max } => {
            check(roots, limits, at.root, &at.path, false)?;
            if max > limits.file_bytes {
                return Err(other());
            }
            Ok((Kind::Load, file::Request::Load { owner, root: at.root, path: at.path, max, no_follow: false }))
        }
        tools::Op::Scan { at, max, max_bytes } => {
            check(roots, limits, at.root, &at.path, false)?;
            if max > limits.entries || max_bytes > limits.entry_bytes {
                return Err(other());
            }
            Ok((
                Kind::Scan,
                file::Request::Scan {
                    owner,
                    root: at.root,
                    path: if at.path.is_empty() { Box::from(&b"."[..]) } else { at.path },
                    max,
                    max_bytes,
                    no_follow: false,
                },
            ))
        }
        tools::Op::Store { at, content, expect } => {
            check(roots, limits, at.root, &at.path, true)?;
            if content.len() > usize::try_from(limits.file_bytes).expect("u32 fits usize") {
                return Err(tools::Done::TooLarge { size: u64::try_from(content.len()).unwrap_or(u64::MAX) });
            }
            let expected = match expect {
                tools::Expect::Absent => None,
                tools::Expect::Is { version } => Some(digest::Digest(version.raw())),
            };
            Ok((
                Kind::Store,
                file::Request::Store { owner, root: at.root, path: at.path, bytes: content, expected, no_follow: true },
            ))
        }
        tools::Op::Spawn { .. } | tools::Op::Search { .. } => Err(other()),
    }
}

pub(crate) fn check(
    roots: &List<Root>,
    limits: &Limits,
    root: Token,
    path: &[u8],
    write: bool,
) -> Result<(), tools::Done> {
    if path.len() > usize::try_from(limits.path_bytes).expect("u32 fits usize") {
        return Err(other());
    }
    if in_git(path) {
        return Err(tools::Done::Failed { fault: tools::Fault::Denied });
    }
    for admitted in roots {
        if admitted.token == root {
            if write && !admitted.writable {
                return Err(tools::Done::Failed { fault: tools::Fault::Denied });
            }
            return Ok(());
        }
    }
    Err(tools::Done::Escapes)
}

fn in_git(path: &[u8]) -> bool {
    let mut start = 0_usize;
    for (index, byte) in path.iter().enumerate() {
        if *byte == b'/' {
            if path.get(start..index).expect("path index is in range").eq_ignore_ascii_case(b".git") {
                return true;
            }
            start = index.checked_add(1).expect("index of a path byte fits");
        }
    }
    path.get(start..).expect("path index is in range").eq_ignore_ascii_case(b".git")
}

fn other() -> tools::Done {
    tools::Done::Failed { fault: tools::Fault::Other }
}

/// Convert the one terminal expected for the admitted operation.
#[expect(clippy::manual_map, reason = "the strict step subset does not use closures")]
pub(crate) fn terminal(kind: Kind, event: file::Event) -> tools::Done {
    match event {
        file::Event::Loaded { bytes, .. } => {
            assert_eq!(kind, Kind::Load, "only a load returns file bytes");
            tools::Done::Loaded { version: tools::Version::new(digest::digest(&bytes).0), content: bytes }
        }
        file::Event::Scanned { entries, more, .. } => {
            assert_eq!(kind, Kind::Scan, "only a scan returns entries");
            let capacity = u32::try_from(entries.len()).expect("bounded scan count");
            let mut translated = List::with_capacity(capacity);
            for entry in entries {
                let name = tools::Name::new(entry.name).expect("kernel entries have safe names");
                let kind = match entry.kind {
                    kernel::Kind::File => tools::Kind::File,
                    kernel::Kind::Directory => tools::Kind::Directory,
                    kernel::Kind::Symlink => tools::Kind::Link,
                    kernel::Kind::Other => tools::Kind::Other,
                };
                translated.push(tools::Entry { name, kind }).expect("one result per entry");
            }
            tools::Done::Scanned { entries: translated.into_boxed(), more }
        }
        file::Event::Stored { digest, .. } => {
            assert_eq!(kind, Kind::Store, "only a store returns a digest");
            tools::Done::Stored { version: tools::Version::new(digest.0) }
        }
        file::Event::Conflict { now, .. } => {
            assert_eq!(kind, Kind::Store, "only a store returns conflict");
            let now = match now {
                Some(digest) => Some(tools::Version::new(digest.0)),
                None => None,
            };
            tools::Done::Conflict { now }
        }
        file::Event::TooLarge { size, .. } => {
            assert_eq!(kind, Kind::Load, "only a load reports observed excess");
            tools::Done::TooLarge { size }
        }
        file::Event::Cancelled { .. } => tools::Done::Cancelled,
        file::Event::Failed { error, .. } => failure(kind, error),
        file::Event::Opened { .. }
        | file::Event::Stated { .. }
        | file::Event::Written { .. }
        | file::Event::Read { .. }
        | file::Event::Synced { .. }
        | file::Event::Closed { .. }
        | file::Event::Renamed { .. }
        | file::Event::Removed { .. }
        | file::Event::Listed { .. } => unreachable!("machine asks io only for whole-file operations"),
    }
}

fn failure(kind: Kind, error: kernel::Error) -> tools::Done {
    match error {
        kernel::Error::NotFound => tools::Done::Missing,
        kernel::Error::NotADirectory => tools::Done::NotDirectory,
        kernel::Error::IsADirectory | kernel::Error::NotAFile => match kind {
            Kind::Load | Kind::Store => tools::Done::NotFile,
            Kind::Scan => tools::Done::NotDirectory,
        },
        kernel::Error::TooManyLinks => match kind {
            Kind::Store => tools::Done::Linked,
            Kind::Load | Kind::Scan => other(),
        },
        kernel::Error::Escape => tools::Done::Escapes,
        kernel::Error::Permission | kernel::Error::ReadOnly => tools::Done::Failed { fault: tools::Fault::Denied },
        kernel::Error::NoSpace => tools::Done::Failed { fault: tools::Fault::NoSpace },
        kernel::Error::TimedOut => tools::Done::TimedOut,
        kernel::Error::Cancelled => tools::Done::Cancelled,
        kernel::Error::Refused
        | kernel::Error::Reset
        | kernel::Error::BrokenPipe
        | kernel::Error::NotConnected
        | kernel::Error::AddressInUse
        | kernel::Error::AddressNotAvailable
        | kernel::Error::Unreachable
        | kernel::Error::TooManyOpenFiles
        | kernel::Error::NoBufferSpace
        | kernel::Error::TooLate
        | kernel::Error::Exists
        | kernel::Error::NotEmpty
        | kernel::Error::NameTooLong
        | kernel::Error::InvalidArgument
        | kernel::Error::Other(_) => other(),
    }
}
