//! What the tools tell whoever watches the agent (domain/tools.md, section 4):
//! a fact for each thing that happened, content-free (tokens, counts and
//! classifications, never a path, a file or what a command wrote), in a
//! bounded queue the parent drains at its own pace.
//!
//! Facts are a separate step output. The parent reserves [`crate::max_facts`]
//! slots before each entrance, holding work while its drain is behind; every
//! observation is preserved and none changes a tool's decision.

use skein_lib::{Queue, Time, Token};

use crate::boundary::Refusal;
use crate::call::{Effect, Exit, Fault, Outcome, Tool};

/// An observation stamped with the injected time of its emitting step.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Fact {
    /// Emission time, preserved when parents drain this observation later.
    pub at: Time,
    /// The content-free observation.
    pub kind: FactKind,
    /// Parent call context when the caller has a provider observation.
    pub call: Option<CallInfo>,
}

/// Something that happened to a kit, emitted by the tools for its parent.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FactKind {
    /// The kit opened.
    Opened { session: Token },
    /// No kit was opened.
    Refused {
        session: Token,
        /// Typed admission refusal; no work runs for a refused entrance.
        refusal: Refusal,
    },
    /// A call passed the entrance, and runs.
    Started {
        session: Token,
        /// Content-free classification of the checkout tool called.
        tool: Tool,
    },
    /// A call was answered, so, with an outcome carrying `bytes` of payload
    /// (what was read, listed, found, or kept of a command's output). A call
    /// refused at the entrance is answered without having started.
    Answered {
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
        session: Token,
        /// Number of pending calls whose terminals the closing kit still awaits.
        running: u32,
    },
    /// The kit closed.
    Closed { session: Token },
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
pub(crate) fn answered(session: Token, tool: Tool, outcome: &Outcome) -> FactKind {
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
    FactKind::Answered { session, tool, verdict, bytes }
}

fn len(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

/// Reserved step observations not yet drained by the parent.
#[derive(Debug)]
pub(crate) struct Facts {
    queue: Queue<Fact>,
    now: Time,
}

impl Facts {
    pub(crate) fn with_capacity(capacity: u32) -> Facts {
        Facts { queue: Queue::with_capacity(capacity), now: Time::ZERO }
    }

    pub(crate) fn begin(&mut self, now: Time) {
        self.now = now;
    }

    /// Keeps one fact in the room the parent reserved for this entry point.
    pub(crate) fn push(&mut self, kind: FactKind) {
        let fact = Fact { at: self.now, kind, call: None };
        self.queue.push(fact);
    }

    pub(crate) fn push_call(&mut self, kind: FactKind, call: Option<CallInfo>) {
        self.queue.push(Fact { at: self.now, kind, call });
    }

    pub(crate) fn pop(&mut self) -> Option<Fact> {
        self.queue.pop()
    }

    pub(crate) fn room(&self) -> u32 {
        self.queue.room()
    }
}

/// Provider-parent identity and measurements, carried without owning content.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CallInfo {
    /// The parent's call token, distinct from the affine answer right.
    pub owner: Token,
    pub effect: Effect,
    pub deadline: Time,
    /// Exact raw provider argument bytes, measured before decoding.
    pub input_bytes: u64,
}
