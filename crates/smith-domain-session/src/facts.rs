//! What the sessions tell whoever watches the agent (domain/run.md, section
//! 7): a fact for each thing that happened, content-free (tokens, counts and
//! classifications, never what the LLM or the opener said), in a bounded
//! queue the parent drains at its own pace.
//!
//! Facts are outside the boundary's flow control: they are not requests, take
//! no room in `out`, and when the queue is full they are dropped and counted.
//! Nothing the session decides depends on whether a fact was kept.

use skein_lib::{Duration, Queue, Token};
use smith_domain_tools as tools;

use crate::boundary::{End, Yield};
use crate::llm::{Failure, Stop, Usage};

/// Something that happened in the session opened for `opener`.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fact {
    /// The session was admitted.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Opened {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
    },
    /// A completion was asked for, after `attempt` retries, with `messages`
    /// messages and room for `max_tokens` in its answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CompletionStarted {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Retry number for the pending provider completion.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        attempt: u32,
        /// Oldest-first conversation messages, with provider call/result pairing preserved.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        messages: u32,
        /// Maximum output tokens requested for one completion.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        max_tokens: u32,
    },
    /// The completion came back with an answer of `blocks` blocks, `calls` of
    /// them tool calls, of which `invalid` could not be decoded.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CompletionAnswered {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        stop: Stop,
        /// Count of provider answer blocks, independent of their content.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        blocks: u32,
        /// Maximum provider calls held at once.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        calls: u32,
        /// Count of calls which could not be decoded or admitted.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        invalid: u32,
    },
    /// The completion produced no answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CompletionFailed {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        failure: Failure,

        /// Content-free actual lower transport evidence; diagnostic text is not retained.
        /// Contract: domain/session.md, sections 4, 5 and 12.
        evidence: crate::llm::Evidence,
    },
    /// The completion was abandoned.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CompletionCancelled {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
    },
    /// The completion that failed is tried again after `delay`, as retry
    /// `attempt`.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    CompletionRetried {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Retry number for the pending provider completion.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        attempt: u32,
        /// Injected backoff interval before retrying the failed completion.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        delay: Duration,
    },
    /// What the session's own tools told, of the kit the session has: its
    /// calls starting and being answered, and the kit opening and closing.
    /// The tools name the session by its own token; `opener` names its
    /// conversation, as every other fact does.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Tools {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Content-free child observation, dropped and counted if the queue is full.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        fact: tools::Fact,
    },
    /// The tool call at `block` of the last message was delegated to the
    /// opener.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    DelegateStarted {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Index of the delegated tool-call block in its completion.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        block: u32,
    },
    /// The opener answered, with `bytes`; `error` marks a failure.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    DelegateAnswered {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        bytes: u64,
        /// Whether the returned tool result represents a failure.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        error: bool,
    },
    /// The delegated call was withdrawn.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    DelegateCancelled {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
    },
    /// The session yielded.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Yielded {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        stop: Yield,
    },
    /// A completion came back, and used one turn and `usage`.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Used {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        usage: Usage,
        /// Cumulative raw usage overflow attestation; this completion stays exact.
        /// Contract: domain/session.md, section 6; domain/run.md, section 9.
        usage_overflow: bool,
    },
    /// The session ended, or was refused at the entrance.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Ended {
        /// Parent-issued session or conversation name, echoed unchanged.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        opener: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        end: End,
        /// Completion count, bounded across the enclosing run or session.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        turns: u32,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        usage: Usage,
        /// True when cumulative usage overflowed; the whole exact prefix
        /// is copied from the actual Ended terminal.
        /// Contract: domain/session.md, section 6; domain/run.md, section 10.
        usage_overflow: bool,
    },
}

/// The facts not yet drained, and how many did not fit.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
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
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
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
