//! What the tools tell whoever watches the agent (domain/tools.md, section 9):
//! a fact for each thing that happened, content-free (tokens, counts and
//! classifications, never a path, a file or what a command wrote), in a
//! bounded queue the parent drains at its own pace.
//!
//! Facts are outside the boundary's flow control: they are not requests, take
//! no room in `out`, and when the queue is full they are dropped and counted.
//! Nothing the tools decide depends on whether a fact was kept.

use skein_lib::{Queue, Token};

use crate::boundary::Refusal;
use crate::call::{Exit, Fault, Outcome, Tool};

/// Something that happened to the kit of the session `session`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fact {
    /// The kit opened.
    Opened {
        /// The session child's limits or opaque session identity, according to the enclosing record.
        session: Token,
    },
    /// No kit was opened.
    Refused {
        /// The session child's limits or opaque session identity, according to the enclosing record.
        session: Token,
        /// Typed admission refusal; no work runs for a refused entrance.
        refusal: Refusal,
    },
    /// A call passed the entrance, and runs.
    Started {
        /// The session child's limits or opaque session identity, according to the enclosing record.
        session: Token,
        /// Content-free classification of the checkout tool called.
        tool: Tool,
    },
    /// A call was answered, so, with an outcome carrying `bytes` of payload
    /// (what was read, listed, found, or kept of a command's output). A call
    /// refused at the entrance is answered without having started.
    Answered {
        /// The session child's limits or opaque session identity, according to the enclosing record.
        session: Token,
        /// Content-free classification of the checkout tool called.
        tool: Tool,
        /// Content-free classification of the checkout call terminal.
        verdict: Verdict,
        /// Owned payload bytes charged against the enclosing session limit.
        bytes: u64,
    },
    /// The kit is closing, with `running` calls to settle.
    Closing {
        /// The session child's limits or opaque session identity, according to the enclosing record.
        session: Token,
        /// Number of pending calls whose terminals the closing kit still awaits.
        running: u32,
    },
    /// The kit closed.
    Closed {
        /// The session child's limits or opaque session identity, according to the enclosing record.
        session: Token,
    },
}

/// How a call ended, without what it said: an outcome's kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Verdict {
    /// Read-only call or successful read fact, according to the enclosing classification.
    Read,
    /// The directory listing completed.
    Listed,
    /// The contained search completed with bounded matches.
    Found,
    /// The version-aware write completed.
    Written {
        /// Whether this write created a previously absent file.
        created: bool,
    },
    /// The version-aware edit completed.
    Edited {
        /// Number of edit occurrences successfully replaced.
        replaced: u32,
    },
    /// The contained process ended with its classified exit.
    Exited {
        /// Terminal process classification; only a zero exit code passes checks.
        exit: Exit,
    },
    /// The kit has no authority for this tool family.
    NotGranted,
    /// The path is outside the granted repository roots.
    Outside,
    /// The resolved repository forbids writes.
    ReadOnly,
    /// The resolved path exceeds the kit path cap.
    TooLong,
    /// No entry exists at the requested path.
    NotFound,
    /// The entry is not a regular file.
    NotFile,
    /// A write path traverses a symbolic link and is refused.
    Linked,
    /// A write would touch protected git metadata.
    Protected,
    /// A path prefix or requested listing is not a directory.
    NotDirectory,
    /// A configured ownership, count or encoded-byte cap would be exceeded.
    TooLarge,
    /// A write or edit lacks a previously observed file version.
    NotRead,
    /// The file version changed since it was read.
    Stale,
    /// The edit snippet has no occurrence.
    NoMatch,
    /// A single-occurrence edit found more than one match.
    Ambiguous {
        /// Number of matched occurrences which made a single-occurrence edit ambiguous.
        count: u32,
    },
    /// The edit or write would change no file bytes.
    Unchanged,
    /// One failed terminal with bounded diagnostics.
    Failed {
        /// Typed IO failure; this boundary carries no OS error text.
        fault: Fault,
    },
    /// The injected operation deadline won the race.
    TimedOut,
    /// The caller cancelled and the terminal settled.
    Cancelled,
    /// No capacity is currently available; a later call may fit.
    Busy,
    /// The path or process input contains a forbidden NUL byte.
    NulByte,
}

/// The fact of `outcome` answering a call of `tool` from `session`.
pub(crate) fn answered(session: Token, tool: Tool, outcome: &Outcome) -> Fact {
    let (verdict, bytes) = match outcome {
        Outcome::Read { content, .. } => (Verdict::Read, len(content)),
        Outcome::Listed { entries, .. } => {
            let mut bytes: u64 = 0;
            for entry in entries {
                bytes = bytes.saturating_add(len(entry.name.as_bytes()));
            }
            (Verdict::Listed, bytes)
        }
        Outcome::Found { hits, .. } => {
            let mut bytes: u64 = 0;
            for hit in hits {
                bytes = bytes.saturating_add(len(&hit.path)).saturating_add(len(&hit.text));
            }
            (Verdict::Found, bytes)
        }
        Outcome::Written { created } => (Verdict::Written { created: *created }, 0),
        Outcome::Edited { replaced } => (Verdict::Edited { replaced: *replaced }, 0),
        Outcome::Exited { exit, head, tail, dropped: _ } => {
            (Verdict::Exited { exit: *exit }, len(head).saturating_add(len(tail)))
        }
        Outcome::NotGranted => (Verdict::NotGranted, 0),
        Outcome::Outside => (Verdict::Outside, 0),
        Outcome::ReadOnly => (Verdict::ReadOnly, 0),
        Outcome::TooLong => (Verdict::TooLong, 0),
        Outcome::NotFound => (Verdict::NotFound, 0),
        Outcome::NotFile => (Verdict::NotFile, 0),
        Outcome::Linked => (Verdict::Linked, 0),
        Outcome::Protected => (Verdict::Protected, 0),
        Outcome::NotDirectory => (Verdict::NotDirectory, 0),
        Outcome::TooLarge { .. } => (Verdict::TooLarge, 0),
        Outcome::NotRead => (Verdict::NotRead, 0),
        Outcome::Stale => (Verdict::Stale, 0),
        Outcome::NoMatch => (Verdict::NoMatch, 0),
        Outcome::Ambiguous { count, .. } => (Verdict::Ambiguous { count: *count }, 0),
        Outcome::Unchanged => (Verdict::Unchanged, 0),
        Outcome::Failed { fault } => (Verdict::Failed { fault: *fault }, 0),
        Outcome::TimedOut => (Verdict::TimedOut, 0),
        Outcome::Cancelled => (Verdict::Cancelled, 0),
        Outcome::Busy => (Verdict::Busy, 0),
        Outcome::NulByte => (Verdict::NulByte, 0),
    };
    Fact::Answered { session, tool, verdict, bytes }
}

fn len(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

/// The facts not yet drained, and how many did not fit.
#[derive(Debug)]
pub(crate) struct Facts {
    queue: Queue<Fact>,
    lost: u64,
}

impl Facts {
    pub(crate) fn with_capacity(capacity: u32) -> Facts {
        Facts { queue: Queue::with_capacity(capacity), lost: 0 }
    }

    /// Keeps `fact` if there is room for it, and counts it otherwise.
    pub(crate) fn push(&mut self, fact: Fact) {
        if self.queue.try_push(fact).is_err() {
            self.lost = self.lost.saturating_add(1);
        }
    }

    pub(crate) fn pop(&mut self) -> Option<Fact> {
        self.queue.pop()
    }

    pub(crate) fn lost(&self) -> u64 {
        self.lost
    }
}
