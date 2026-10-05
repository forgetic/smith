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
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fact {
    /// The kit opened.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Opened {
        /// Parent-supplied session token identifying the owning kit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
    },
    /// No kit was opened.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Refused {
        /// Parent-supplied session token identifying the owning kit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Typed admission refusal; no work runs for a refused entrance.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        refusal: Refusal,
    },
    /// A call passed the entrance, and runs.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Started {
        /// Parent-supplied session token identifying the owning kit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Content-free classification of the checkout tool called.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        tool: Tool,
    },
    /// A call was answered, so, with an outcome carrying `bytes` of payload
    /// (what was read, listed, found, or kept of a command's output). A call
    /// refused at the entrance is answered without having started.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Answered {
        /// Parent-supplied session token identifying the owning kit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Content-free classification of the checkout tool called.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        tool: Tool,
        /// Content-free classification of the checkout call terminal.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        verdict: Verdict,
        /// Owned payload bytes charged against the enclosing session limit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        bytes: u64,
    },
    /// The kit is closing, with `running` calls to settle.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Closing {
        /// Parent-supplied session token identifying the owning kit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
        /// Number of pending calls whose terminals the closing kit still awaits.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        running: u32,
    },
    /// The kit closed.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Closed {
        /// Parent-supplied session token identifying the owning kit.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        session: Token,
    },
}

/// How a call ended, without what it said: an outcome's kind.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Verdict {
    /// Read-only call or successful read fact, according to the enclosing classification.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Read,
    /// The directory listing completed.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Listed,
    /// The contained search completed with bounded matches.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Found,
    /// The version-aware write completed.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Written {
        /// Whether this write created a previously absent file.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        created: bool,
    },
    /// The version-aware edit completed.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Edited {
        /// Number of edit occurrences successfully replaced.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        replaced: u32,
    },
    /// The contained process ended with its classified exit.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Exited {
        /// Terminal process classification; only a zero exit code passes checks.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        exit: Exit,
    },
    /// The kit has no authority for this tool family.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotGranted,
    /// The path is outside the granted repository roots.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Outside,
    /// The resolved repository forbids writes.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    ReadOnly,
    /// The resolved path exceeds the kit path cap.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TooLong,
    /// No entry exists at the requested path.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotFound,
    /// The entry is not a regular file.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotFile,
    /// A write path traverses a symbolic link and is refused.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Linked,
    /// A write would touch protected git metadata.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Protected,
    /// A path prefix or requested listing is not a directory.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotDirectory,
    /// A configured ownership, count or encoded-byte cap would be exceeded.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TooLarge,
    /// A write or edit lacks a previously observed file version.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NotRead,
    /// The file version changed since it was read.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Stale,
    /// The edit snippet has no occurrence.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NoMatch,
    /// A single-occurrence edit found more than one match.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Ambiguous {
        /// Number of matched occurrences which made a single-occurrence edit ambiguous.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        count: u32,
    },
    /// The edit or write would change no file bytes.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Unchanged,
    /// One failed terminal with bounded diagnostics.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Failed {
        /// Typed IO failure; this boundary carries no OS error text.
        /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
        fault: Fault,
    },
    /// The injected operation deadline won the race.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    TimedOut,
    /// The caller cancelled and the terminal settled.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Cancelled,
    /// No capacity is currently available; a later call may fit.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    Busy,
    /// The path or process input contains a forbidden NUL byte.
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
    NulByte,
}

/// The fact of `outcome` answering a call of `tool` from `session`.
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
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
/// Contract: domain/tools.md, sections 4, 5, 6 and 9.
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
    /// Contract: domain/tools.md, sections 4, 5, 6 and 9.
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
