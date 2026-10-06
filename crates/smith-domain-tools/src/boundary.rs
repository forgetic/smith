//! The records that cross the boundary with the tools' parent, the session
//! child domain (programming-model.md, section 4.5), which hands the tools its LLM's calls and routes the
//! file operations they ask for out to io and the terminal events back. The
//! tools define them; the session depends on the tools.
//!
//! Three shapes cross it:
//!
//! - A kit opens and closes: an [`Event::Open`] is answered by exactly one
//!   [`Request::Opened`] or [`Request::Refused`], and an [`Event::Close`] by
//!   exactly one [`Request::Closed`], once every call the kit was running has
//!   been answered. The session names its kit by the token `Opened` gives it,
//!   and the tools echo the session's own token.
//! - Calls in, answers out: each [`Event::Call`] is answered by exactly one
//!   [`Request::Answer`], which consumes its `ReplyTo`.
//! - Operations out, one terminal event in: each [`Request::Io`] is ended by
//!   exactly one [`Event::Done`], after a [`Request::CancelIo`] too. Its
//!   `owner` is the tools' token, echoed on the terminal.
//!
//! The tools arm no timers. A call comes with its deadline, which bounds every
//! operation it asks for; io runs the race (programming-model.md, section 5.3) and reports a lost one as
//! [`Done::TimedOut`].

use alloc::boxed::Box;

use skein_lib::{ReplyTo, Time, Token};

use crate::authority::{Authority, Var};
use crate::call::{Call, Entry, Exit, Fault, Hit, Outcome};
use crate::path::Place;

/// session -> tools
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(PartialEq, Eq, Debug)]
pub enum Event {
    /// Open a kit for the session `session`, with `authority`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Open {
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Explicit roots, grants, working directory and environment admitted by the kit.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        authority: Authority,
    },
    /// Run `call` with the kit `kit`, and answer it by `deadline`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Call {
        /// Tools-issued kit identity, valid until its close terminal.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        kit: Token,
        /// Single-use right to answer this call; exactly one terminal consumes it.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        reply_to: ReplyTo,
        /// Parent-issued tool-call token, echoed on the single outcome terminal.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        call: Call,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        deadline: Time,
    },
    /// Close the kit `kit`: cancel what it is running and, once every call
    /// has been answered, end it. No call follows.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Close {
        /// Tools-issued kit identity, valid until its close terminal.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        kit: Token,
    },
    /// Terminal for `Io`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Done {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        done: Done,
    },
}

/// tools -> session
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// The answer to an `Open`: the kit is `kit` from now on.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Opened {
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Tools-issued kit identity, valid until its close terminal.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        kit: Token,
    },
    /// The answer to an `Open`: no kit was opened.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Refused {
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Typed admission refusal; no work runs for a refused entrance.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        refusal: Refusal,
    },
    /// The answer to a `Call`: exactly one per call.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Answer {
        /// Single-use reply right returned to the layer that issued it.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        to: ReplyTo,
        /// Declared or accepted result, validated against the host's contract.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        outcome: Outcome,
    },
    /// The answer to a `Close`: the kit has ended.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Closed {
        /// Parent-supplied session token identifying the owning kit.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
    },
    /// Ask io for `op`, giving up at `deadline`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Io {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        owner: Token,
        /// Typed IO operation carrying only the authority and bounds required for it.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        op: Op,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        deadline: Time,
    },
    /// Abandon the `Io` in flight for `owner`. Its terminal event still comes:
    /// `Cancelled`, or whichever outcome won the race.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    CancelIo {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        owner: Token,
    },
}

/// Why a kit was not opened.
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// Every kit slot is taken.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Busy,
    /// The authority does not fit the limits.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Invalid,
}

/// A file operation, asked of io. io resolves every place beneath its root
/// and refuses one that a symbolic link leads out of (`Escapes`). A store
/// follows no link at all.
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Op {
    /// Read the regular file at `at`, following symbolic links, if it holds
    /// at most `max` bytes. Ends in `Loaded`, `Missing`, `NotFile`,
    /// `NotDirectory`, `TooLarge` or a common terminal.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Load {
        /// IO-confined place relative to the named repository root.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        at: Place,
        /// Inclusive maximum count allowed by this contract.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        max: u32,
    },
    /// List the directory at `at`, following symbolic links: at most `max`
    /// entries within `max_bytes`, the first in name order, without `.` and `..`. Ends in
    /// `Scanned`, `Missing`, `NotDirectory` or a common terminal.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Scan {
        /// IO-confined place relative to the named repository root.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        at: Place,
        /// Inclusive maximum count allowed by this contract.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        max: u32,

        /// Maximum owned listing bytes: entry array cells plus every name's bytes.
        /// The terminal obeys this cap even when it wins cancellation.
        /// An unfit first entry returns an empty prefix; `Scanned.more` counts
        /// all omitted entries, including those excluded by the byte cap.
        /// Contract: domain/tools.md, sections 4, 5 and 9; domain/session.md, section 3.
        max_bytes: u64,
    },
    /// Make the regular file at `at` hold `content`, if it is as `expect`
    /// says. No part of `at` may be a symbolic link (`Linked`): io resolves
    /// it beneath the root following none (`RESOLVE_NO_SYMLINKS`), so a store
    /// lands in the repository its place names and in no other mounted
    /// beneath it. The content is written beside the file and renamed into
    /// place, so that the file is replaced whole or not at all.
    ///
    /// To replace (`Is`), io compares the file's version with `expect` just
    /// before the rename, and makes no directory: a missing one on the way
    /// means the file is missing, a conflict. A change made before the
    /// comparison is caught; one made by a writer outside the agent between
    /// the comparison and the rename is not, for the two are not one atomic
    /// step. (io runs one store at a time per root, so the agent's own kits
    /// do not race each other there.) To create (`Absent`), io makes the
    /// missing directories on the way, then renames the file into place
    /// without replacing one: a file that appeared meanwhile is a conflict,
    /// and the directories stay.
    ///
    /// Ends in `Stored`, `Conflict`, `Linked`, `NotFile`, `NotDirectory` or a
    /// common terminal.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Store {
        /// IO-confined place relative to the named repository root.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        at: Place,
        /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        content: Box<[u8]>,
        /// Expected file version or absence checked before the atomic replacement.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        expect: Expect,
    },
    /// Run `command` with the shell, in the directory at `cwd`, as a process
    /// tree contained by io, which enforces what it may write (with
    /// read-only bind mounts, say):
    ///
    /// - It sees the repositories in `roots`, and may write only those marked
    ///   writable, which are the ones the kit may write, and only if it was
    ///   granted modify. io mounts the rest read-only, a repository mounted
    ///   inside another over it, so that what a process may write beneath a
    ///   root is decided by the deepest root that holds it.
    /// - In every root, writable or not, the git directory (`.git`, in any
    ///   ASCII case) is read-only: the worker commits the tree the checks
    ///   passed on, so the LLM never needs to write it.
    ///
    /// Its environment is `env` and nothing else. Its output, standard output
    /// and standard error together, is captured: the first `head` bytes, the
    /// last `tail`, and a count of those dropped between.
    /// At the deadline io kills the whole tree and ends it as `Exited`, timed
    /// out, with what it captured. Ends in `Exited`, `Missing` or
    /// `NotDirectory` (for `cwd`), `Escapes`, `Failed` (it could not start),
    /// or `Cancelled` (killed by a cancel).
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Spawn {
        /// IO-confined working directory supplied with explicit root authority.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        cwd: Place,
        /// Owned shell command interpreted only by the contained process layer.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        command: Box<[u8]>,
        /// Only environment variables the contained command may see.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        env: Box<[Var]>,
        /// Explicit repository roots and effective write permissions for the process.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        roots: Box<[Root]>,
        /// First output bytes retained under the caller's cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        head: u32,
        /// Last output bytes retained under the caller's cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        tail: u32,
    },
    /// Search the files at and beneath `at` for lines matching `pattern`, in
    /// those whose names match `glob` if given, with rg run as a contained
    /// process, invoked as
    ///
    /// ```text
    /// rg --no-config --json --regexp=PATTERN [--glob=GLOB] -- PATH
    /// ```
    ///
    /// so that neither the pattern nor the glob can be read as an option, and
    /// no configuration file is: with an empty environment, and every root
    /// read-only. rg follows no link beneath `at`, and skips what it skips by
    /// default (hidden and ignored files, binary ones).
    ///
    /// The protocol layer decodes rg's output into hits, in path order: at
    /// most `hits` of them, with at most `bytes` between them of their paths
    /// and their text, the last one's text cut to fit; the hits beyond are
    /// counted. rg's exits: 0 is `Found`; 1, nothing found, is `Found` with no
    /// hits; 2, an error, is `Found` if it found lines all the same (an
    /// unreadable file among many hides nothing found in the others), and
    /// otherwise `Exited`, with at most `bytes` of what it wrote to standard
    /// error in `head` (as for a pattern it cannot read). At the deadline io
    /// kills rg and ends the search as `Found`, timed out, with what it had
    /// found. Ends in `Found`, `Exited`, `Missing`, `NotDirectory`, `Escapes`,
    /// or a common terminal.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Search {
        /// IO-confined place relative to the named repository root.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        at: Place,
        /// Search expression passed as data to the contained search process.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        pattern: Box<[u8]>,
        /// Optional search filename filter, passed as data rather than an option.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        glob: Option<Box<[u8]>>,
        /// Bounded search results, or the caller's maximum retained count.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        hits: u32,
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        bytes: u32,
    },
}

/// A repository a command sees: io's name for its root, and whether the
/// command may write in it (but never in its git directory).
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Root {
    /// IO-issued repository root token, stored and echoed without interpreting it.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub root: Token,
    /// Whether this mount permits modification; protected git paths remain unwritable.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    pub writable: bool,
}

/// What a `Store` expects to replace.
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Expect {
    /// Nothing: the file is created.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Absent,
    /// The file at `version`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Is {
        /// IO-issued file version or record encoding version, as named by the enclosing type.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        version: Version,
    },
}

/// io's terminal for an operation. Each operation ends in those its
/// documentation names, or in one of the common terminals: `Escapes`,
/// `Failed`, `TimedOut` and `Cancelled`.
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Done {
    /// Load: the file holds `content`, at `version`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Loaded {
        /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        content: Box<[u8]>,
        /// IO-issued file version or record encoding version, as named by the enclosing type.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        version: Version,
    },
    /// Scan: the directory's first `entries` in name order, and how many
    /// `more` it has.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Scanned {
        /// First bounded directory entries in name order.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        entries: Box<[Entry]>,
        /// Problems or matches omitted after the retained bound.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        more: u64,
    },
    /// Store: the file holds the content, at `version`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Stored {
        /// IO-issued file version or record encoding version, as named by the enclosing type.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        version: Version,
    },
    /// Store: the file is not as expected. It is at `now`, or absent.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Conflict {
        /// Current optional file version observed by IO after an optimistic-write conflict.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        now: Option<Version>,
    },
    /// Spawn: the command ended so, with the output captured. Search: rg
    /// failed so.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Exited {
        /// Terminal process classification; only a zero exit code passes checks.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        exit: Exit,
        /// First output bytes retained under the caller's cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        head: Box<[u8]>,
        /// Last output bytes retained under the caller's cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        tail: Box<[u8]>,
        /// Bytes omitted between the retained process output head and tail.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        dropped: u64,
    },
    /// Search: the lines found, and how many more matched; `timed_out` if
    /// the deadline cut it short, and these are what it had found.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Found {
        /// Bounded search results, or the caller's maximum retained count.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        hits: Box<[Hit]>,
        /// Problems or matches omitted after the retained bound.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        more: u64,
        /// Whether IO ended the contained process or search at its deadline.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        timed_out: bool,
    },
    /// Load, Scan: nothing is at the place.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Missing,
    /// Load, Store: what is at the place is not a regular file.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotFile,
    /// Store: a part of the place is a symbolic link, which a store does not
    /// follow.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Linked,
    /// Scan: what is at the place is not a directory. Any operation: a
    /// directory on the way to the place is not one.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotDirectory,
    /// Load: the file is `size` bytes, more than asked for.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TooLarge {
        /// Observed file size which exceeds the receiving byte cap.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        size: u64,
    },
    /// A symbolic link on the way leads out of the root.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Escapes,
    /// io failed with `fault`.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Failed {
        /// Typed IO failure; this boundary carries no OS error text.
        ///
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        fault: Fault,
    },
    /// The deadline passed first.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TimedOut,
    /// The cancel won the race.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Cancelled,
}

/// Which version of a file io found, or made by storing it. io makes versions
/// so that a file changed in any way has a new one, and the domain compares
/// them and never looks inside. In production a version is the file's device,
/// inode, change time and size; every store renames a new file into place, so
/// its inode changes.
///
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Version([u64; 4]);

impl Version {
    /// Made below the domain, from what io found.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    #[must_use]
    pub const fn new(raw: [u64; 4]) -> Version {
        Version(raw)
    }

    /// What it was made from, for the layer that made it.
    ///
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    #[must_use]
    pub const fn raw(self) -> [u64; 4] {
        self.0
    }
}
