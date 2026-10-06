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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fact {
    /// The session was admitted.
    Opened {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
    },
    /// A completion was asked for, after `attempt` retries, with `messages`
    /// messages and room for `max_tokens` in its answer.
    CompletionStarted {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Retry number for the pending provider completion.
        attempt: u32,
        /// Oldest-first conversation messages, with provider call/result pairing preserved.
        messages: u32,
        /// Maximum output tokens requested for one completion.
        max_tokens: u32,
    },
    /// The completion came back with an answer of `blocks` blocks, `calls` of
    /// them tool calls, of which `invalid` could not be decoded.
    CompletionAnswered {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Why the provider stopped this completion, independently of its content.
        stop: Stop,
        /// Count of provider answer blocks, independent of their content.
        blocks: u32,
        /// Maximum provider calls held at once.
        calls: u32,
        /// Count of calls which could not be decoded or admitted.
        invalid: u32,
    },
    /// The completion produced no answer.
    CompletionFailed {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Typed reason why the pending operation produced no successful value.
        failure: Failure,

        /// Content-free lower transport evidence; diagnostic text is not retained.
        /// Contract: domain/session.md, sections 4, 5 and 12.
        evidence: crate::llm::Evidence,
    },
    /// The completion was abandoned.
    CompletionCancelled {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
    },
    /// The completion that failed is tried again after `delay`, as retry
    /// `attempt`.
    CompletionRetried {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Retry number for the pending provider completion.
        attempt: u32,
        /// Injected backoff interval before retrying the failed completion.
        delay: Duration,
    },
    /// What the session's own tools told, of the kit the session has: its
    /// calls starting and being answered, and the kit opening and closing.
    /// The tools name the session by its own token; `opener` names its
    /// conversation, as every other fact does.
    Tools {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Content-free child observation, dropped and counted if the queue is full.
        fact: tools::Fact,
    },
    /// The tool call at `block` of the last message was delegated to the
    /// opener.
    DelegateStarted {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Index of the delegated tool-call block in its completion.
        block: u32,
    },
    /// The opener answered, with `bytes`; `error` marks a failure.
    DelegateAnswered {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Owned payload bytes charged against the enclosing session limit.
        bytes: u64,
        /// Whether the returned tool result represents a failure.
        error: bool,
    },
    /// The delegated call was withdrawn.
    DelegateCancelled {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
    },
    /// The session yielded.
    Yielded {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Why the provider stopped this completion, independently of its content.
        stop: Yield,
    },
    /// A completion came back, and used one turn and `usage`.
    Used {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        usage: Usage,
    },
    /// The session ended, or was refused at the entrance.
    Ended {
        /// Parent-issued session or conversation name, echoed unchanged.
        opener: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        end: End,
        /// Completion count, bounded across the enclosing run or session.
        turns: u32,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        usage: Usage,
    },
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
