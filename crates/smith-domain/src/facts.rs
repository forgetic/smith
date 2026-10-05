//! What the agent tells whoever watches it (domain/run.md, section 14): the
//! child domains' facts, content-free, gathered after every entry point into one
//! bounded queue the loop drains at its own pace. What does not fit is dropped
//! and counted, and nothing the domain decides depends on it.
//!
//! The run names a conversation by its token for it, and the sessions name
//! theirs by their opener's, which is the same token: the facts of a
//! conversation and of its session go together.

use alloc::boxed::Box;
use skein_lib::Token;
use smith_domain_run::facts as run;
use smith_domain_session as session;

/// Content the protocol projects into the channel's fact payload. The engine
/// applies capture policy; the agent's queue drops and counts overflow.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Debug)]
pub enum Content {
    /// Owned bounded text in its original provider position.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Text {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        text: Box<[u8]>,
    },
    /// Provider tool-call content observed at the root boundary; capture never authorizes or executes it.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Call {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        name: Box<[u8]>,
        /// Provider-written tool-argument bytes, retained exactly for replay.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        input: Box<[u8]>,
    },
    /// Content observation of a typed IO terminal; capture may drop it without changing decisions.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Tool {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        done: crate::tools::Done,
    },
    /// Content observation of provider-reported usage; charging does not depend on capture.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Usage {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        usage: crate::llm::Usage,
    },
}

pub(crate) fn done_bytes(done: &crate::tools::Done) -> u64 {
    use crate::tools::Done;
    match done {
        Done::Loaded { content, .. } => bytes(content),
        Done::Scanned { entries, .. } => {
            let mut held = fixed(entries.len(), size_of::<crate::tools::Entry>());
            for entry in entries {
                held = held.saturating_add(bytes(entry.name.as_bytes()));
            }
            held
        }
        Done::Exited { head, tail, .. } => bytes(head).saturating_add(bytes(tail)),
        Done::Found { hits, .. } => {
            let mut held = fixed(hits.len(), size_of::<crate::tools::Hit>());
            for hit in hits {
                held = held.saturating_add(bytes(&hit.path)).saturating_add(bytes(&hit.text));
            }
            held
        }
        Done::Stored { .. }
        | Done::Conflict { .. }
        | Done::Missing
        | Done::NotFile
        | Done::Linked
        | Done::NotDirectory
        | Done::TooLarge { .. }
        | Done::Escapes
        | Done::Failed { .. }
        | Done::TimedOut
        | Done::Cancelled => 0,
    }
}

pub(crate) fn bytes(value: &[u8]) -> u64 {
    u64::try_from(value.len()).unwrap_or(u64::MAX)
}

fn fixed(count: usize, size: usize) -> u64 {
    u64::try_from(count).unwrap_or(u64::MAX).saturating_mul(u64::try_from(size).unwrap_or(u64::MAX))
}

/// Something that happened in a child domain.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Fact {
    /// In the run child domain.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Run {
        /// Content-free child observation, dropped and counted if the queue is full.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        fact: run::Fact,
    },
    /// In the session child domain, or its tools.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Session {
        /// Content-free child observation, dropped and counted if the queue is full.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        fact: session::Fact,
    },
}
