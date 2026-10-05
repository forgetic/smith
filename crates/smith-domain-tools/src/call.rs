//! The calls a session's LLM makes and what comes of them: the vocabulary the
//! session and the protocol layer share with the tools.
//!
//! The protocol layer owns each tool's schema, decodes the JSON the LLM wrote
//! into a [`Call`], valid by construction, and renders an [`Outcome`] as the
//! text the LLM reads. A malformed call never gets here: the session answers
//! it. What an outcome holds, such as how much of a file comes back, is the
//! tools' decision.
//!
//! Contract: domain/tools.md, section 9; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;

use skein_lib::Duration;

use crate::path::{Name, Path};

/// A call the LLM made to one of the tools.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Call {
    /// Read the file at `path`: its lines after the first `skip`, at most
    /// `lines` of them if given, within the tools' limit on what a read
    /// answers with. Any read counts as reading the file at the version io
    /// loaded, however little of it the window shows: a change needs the
    /// current version seen, not all of it.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Read {
        /// Path relative to the named root, resolved and confined by the receiving IO layer.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        path: Path,
        /// Number of whole lines skipped before the returned read window.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        skip: u32,
        /// Scripted answer parts, emitted in their declared order.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        lines: Option<u32>,
    },
    /// List the directory at `path`.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    List {
        /// Path relative to the named root, resolved and confined by the receiving IO layer.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        path: Path,
    },
    /// Search the files at and beneath `path` for `pattern`, a regular
    /// expression as rg reads it, in those whose names match `glob` if given.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Search {
        /// Path relative to the named root, resolved and confined by the receiving IO layer.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        path: Path,
        /// Search expression passed as data to the contained search process.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        pattern: Box<[u8]>,
        /// Optional search filename filter, passed as data rather than an option.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        glob: Option<Box<[u8]>>,
    },
    /// Make the file at `path` hold `content`, creating it if there is none.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Write {
        /// Path relative to the named root, resolved and confined by the receiving IO layer.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        path: Path,
        /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        content: Box<[u8]>,
    },
    /// Replace `old` with `new` in the file at `path`: its one occurrence, or
    /// every one if `all`.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Edit {
        /// Path relative to the named root, resolved and confined by the receiving IO layer.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        path: Path,
        /// Exact bytes whose occurrence the edit must match in a previously read file.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        old: Box<[u8]>,
        /// Replacement bytes for the matching edit occurrence.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        new: Box<[u8]>,
        /// Whether to replace every occurrence rather than require exactly one.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        all: bool,
    },
    /// Run `command` with the shell, in the working directory, for at most
    /// `timeout` if given and within the tools' own limit.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Shell {
        /// Owned shell command interpreted only by the contained process layer.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        command: Box<[u8]>,
        /// Maximum time allowed for this completion or contained command.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        timeout: Option<Duration>,
    },
}

/// Which tool a call is for, without its arguments.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tool {
    /// Read-only call or successful read fact, according to the enclosing classification.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Read,
    /// Directory-listing tool.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    List,
    /// Contained-search tool.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Search,
    /// A call that may modify state and must run alone.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Write,
    /// Version-aware file-edit tool.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Edit,
    /// Contained shell-command tool.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Shell,
}

/// The tool `call` is for.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[must_use]
pub const fn tool(call: &Call) -> Tool {
    match call {
        Call::Read { .. } => Tool::Read,
        Call::List { .. } => Tool::List,
        Call::Search { .. } => Tool::Search,
        Call::Write { .. } => Tool::Write,
        Call::Edit { .. } => Tool::Edit,
        Call::Shell { .. } => Tool::Shell,
    }
}

/// What a call does to the checkout. Calls that only read may run side by
/// side; a call that writes runs alone.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Effect {
    /// Read-only call or successful read fact, according to the enclosing classification.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Read,
    /// A call that may modify state and must run alone.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Write,
}

/// The effect of `call`. A command may do anything, so it writes.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[must_use]
pub const fn effect(call: &Call) -> Effect {
    match call {
        Call::Read { .. } | Call::List { .. } | Call::Search { .. } => Effect::Read,
        Call::Write { .. } | Call::Edit { .. } | Call::Shell { .. } => Effect::Write,
    }
}

/// What came of a call: exactly one per call.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Outcome {
    /// A read: `content` is the lines after the first `skipped`, `lines` of
    /// them, of the `total` the file has. They are whole lines, but for a
    /// line longer than what a read answers with, which is `cut` at that
    /// limit; a read moves by whole lines, so the rest of a cut line cannot
    /// be read (a search, or a command, can reach it).
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Read {
        /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        content: Box<[u8]>,
        /// Whole file lines skipped before this read window.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        skipped: u32,
        /// Scripted answer parts, emitted in their declared order.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        lines: u32,
        /// Total file lines observed at the loaded version.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        total: u32,
        /// Bytes omitted outside the retained output window.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        cut: bool,
    },
    /// A listing: the directory's first entries in name order, and how many
    /// `more` it has.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Listed {
        /// First bounded directory entries in name order.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        entries: Box<[Entry]>,
        /// Problems or matches omitted after the retained bound.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        more: u64,
    },
    /// A search: the first lines that matched, in path order, and how many
    /// `more` did; `timed_out` if the search ran out of time, and these are
    /// what it had found by then.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Found {
        /// Bounded search results, or the caller's maximum retained count.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        hits: Box<[Hit]>,
        /// Problems or matches omitted after the retained bound.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        more: u64,
        /// Whether IO ended the contained process or search at its deadline.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        timed_out: bool,
    },
    /// The file holds what was written; it was `created` if there was none.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Written {
        /// Whether this write created a previously absent file.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        created: bool,
    },
    /// The edit `replaced` that many occurrences.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Edited {
        /// Number of edit occurrences successfully replaced.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        replaced: u32,
    },
    /// The command ended so, having written its output (standard output and
    /// standard error together, as they came): the first bytes of it in
    /// `head`, the last in `tail`, and `dropped` bytes between them that were
    /// not kept.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Exited {
        /// Terminal process classification; only a zero exit code passes checks.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        exit: Exit,
        /// First output bytes retained under the caller's cap.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        head: Box<[u8]>,
        /// Last output bytes retained under the caller's cap.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        tail: Box<[u8]>,
        /// Bytes omitted between the retained process output head and tail.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        dropped: u64,
    },

    /// The kit was not granted the call's family of tools.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotGranted,
    /// The path is outside every repository of the checkout, or a symbolic
    /// link on the way to it leads out of its repository.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Outside,
    /// The path is in a repository the kit may not write.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    ReadOnly,
    /// The path is longer than the tools take.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TooLong,
    /// Nothing is at the path.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotFound,
    /// What is at the path is not a regular file: a directory, a device.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotFile,
    /// The path of a write or an edit goes through a symbolic link, which
    /// they do not follow, so that a change lands only in the repository its
    /// path names. Reads follow links that stay inside the path's own
    /// repository.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Linked,
    /// The path of a write or an edit is in a repository's git directory: it
    /// has `.git`, in any ASCII case, among its names. The tools do not know
    /// git directories under other names (a `.git` file pointing elsewhere, a
    /// separate git directory, hooks configured to live in the tree), so the
    /// worker must not trust a repository's own configuration either.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Protected,
    /// What is at the path, or a directory on the way to it, is not a
    /// directory.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotDirectory,
    /// The file, or the content to write, is `size` bytes: more than the
    /// tools load or store.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TooLarge {
        /// Observed file size which exceeds the receiving byte cap.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        size: u64,
    },
    /// The file exists and the LLM has not read it, so it may not be changed.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotRead,
    /// The file changed since the LLM read it, so it may not be changed
    /// until the LLM reads it again.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Stale,
    /// The snippet to replace is not in the file (an empty one never is).
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NoMatch,
    /// The snippet to replace is in the file `count` times, and the edit
    /// was for one: `lines` are where the first few start, counting from 1.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Ambiguous {
        /// Number of matched occurrences which made a single-occurrence edit ambiguous.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        count: u32,
        /// Scripted answer parts, emitted in their declared order.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        lines: Box<[u32]>,
    },
    /// The edit would leave the file as it is: the snippet and its
    /// replacement are the same.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Unchanged,
    /// io failed.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Failed {
        /// Typed IO failure; this boundary carries no OS error text.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        fault: Fault,
    },
    /// The call did not finish by its deadline.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TimedOut,
    /// The kit closed while the call was running.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Cancelled,
    /// The kit has as many calls running as it may.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Busy,
    /// The command, the pattern or the glob holds a NUL byte, which no
    /// argument to a process can.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NulByte,
}

/// A line a search found.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Hit {
    /// The file it is in, beneath the path searched: names joined by `/`,
    /// empty if the path searched is the file.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub path: Box<[u8]>,
    /// Its number, counting from 1.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub line: u32,
    /// The line, without its end, cut where the search's byte limit fell.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub text: Box<[u8]>,
}

/// An entry of a listed directory.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Entry {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub name: Name,
    /// Typed entry classification or byte label required by the enclosing contract.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub kind: Kind,
}

/// What an entry is, as io found it, without following symbolic links.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    /// Regular file.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    File,
    /// Directory.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Directory,
    /// Symbolic link.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Link,
    /// A device, a socket, a pipe.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Other,
}

/// How a command ended.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exit {
    /// It exited with `code`.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Code {
        /// Typed or byte-valued terminal classification in the enclosing boundary.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        code: u8,
    },
    /// A signal killed it.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Signal {
        /// Signal number which ended the contained process.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        signal: u8,
    },
    /// It ran past its deadline, and io killed it, with every process it
    /// started.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TimedOut,
}

/// Why io failed, as the protocol layer classifies the error.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// Permission denied.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Denied,
    /// No space left, or over a quota.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NoSpace,
    /// Anything else.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Other,
}
