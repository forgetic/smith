//! What the sessions tell whoever watches the agent (domain/session.md,
//! sections 3–5): a fact for each thing that happened: bounded provider identity, counts and
//! classifications, never prompts, tool inputs or results, in a bounded
//! queue the parent drains at its own pace.
//!
//! Facts are a separate step output: the parent reserves [`crate::max_facts`]
//! slots before each entrance and drains them before taking more work.

use alloc::boxed::Box;

use skein_lib::{Duration, Queue, Time, Token};
use smith_domain_tools as tools;

use crate::boundary::{End, Yield};
use crate::llm::{Failure, Stop, Usage};

/// An observation stamped with the injected time of its emitting step.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Fact {
    /// Emission time, preserved when parents drain this observation later.
    pub at: Time,
    /// The content-free observation.
    pub kind: FactKind,
    /// Provider call metadata for a workspace or delegated call observation.
    pub call: Option<ToolCall>,
    /// Conversation-local completion identity and its exact accepted charge.
    pub response: Option<ResponseInfo>,
}

/// Something that happened in the session opened for `opener`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FactKind {
    /// The session was admitted.
    Opened { opener: Token },
    /// A completion was asked for, after `attempt` retries, with `messages`
    /// messages and room for `max_tokens` in its answer.
    CompletionStarted {
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
        opener: Token,
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
        opener: Token,
        failure: Failure,

        /// Content-free lower transport evidence; diagnostic text is not retained.
        evidence: crate::llm::Evidence,
    },
    /// The completion was abandoned.
    CompletionCancelled { opener: Token },
    /// The completion that failed is tried again after `delay`, as retry
    /// `attempt`.
    CompletionRetried {
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
        opener: Token,
        /// Content-free child observation, preserved in reserved output room.
        fact: tools::FactKind,
    },
    /// The tool call at `block` of the last message was delegated to the
    /// opener.
    DelegateStarted {
        opener: Token,
        /// Index of the delegated tool-call block in its completion.
        block: u32,
    },
    /// The opener answered, with `bytes`; `error` marks a failure.
    DelegateAnswered {
        opener: Token,
        /// Owned payload bytes charged against the enclosing session limit.
        bytes: u64,
        /// Whether the returned tool result represents a failure.
        error: bool,
    },
    /// The delegated call was withdrawn.
    DelegateCancelled { opener: Token },
    /// The session yielded.
    Yielded { opener: Token, stop: Yield },
    /// A completion came back, and used one turn and `usage`.
    Used { opener: Token, usage: Usage },
    /// The session ended, or was refused at the entrance.
    Ended {
        opener: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        end: End,
        /// Completion count, bounded across the enclosing run or session.
        turns: u32,
        usage: Usage,
    },
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

    pub(crate) const fn now(&self) -> Time {
        self.now
    }

    pub(crate) fn begin(&mut self, now: Time) {
        self.now = now;
    }

    /// Keeps one fact in the room the parent reserved for this entry point.
    pub(crate) fn push(&mut self, kind: FactKind) {
        let fact = Fact { at: self.now, kind, call: None, response: None };
        self.queue.push(fact);
    }

    pub(crate) fn push_at(&mut self, at: Time, kind: FactKind) {
        self.queue.push(Fact { at, kind, call: None, response: None });
    }

    pub(crate) fn push_call(&mut self, at: Time, kind: FactKind, call: &ToolCall) {
        assert!(self.queue.room() > 0, "parent reserved call observation before cloning");
        self.queue.push(Fact { at, kind, call: Some(call.clone()), response: None });
    }

    pub(crate) fn push_response(&mut self, kind: FactKind, number: u64, spent: Option<u64>) {
        self.queue.push(Fact { at: self.now, kind, call: None, response: Some(ResponseInfo { number, spent }) });
    }

    pub(crate) fn pop(&mut self) -> Option<Fact> {
        self.queue.pop()
    }

    pub(crate) fn room(&self) -> u32 {
        self.queue.room()
    }
}

/// The owner of a tool declaration, supplied by the conversation opener.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToolSource {
    /// The session's checkout tools.
    Workspace,
    /// A run-owned tool such as `sub_agent` or finish.
    Run,
    /// An opaque tool declared by the host.
    Host,
}

/// Exact provider call identity and content-free start measurements.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ToolCall {
    /// Session-issued call identity, also used by a run opening its child.
    pub owner: Token,
    pub id: Box<[u8]>,
    pub name: Box<[u8]>,
    pub source: ToolSource,
    pub effect: tools::Effect,
    pub deadline: Time,
    /// Exact raw provider argument bytes, independent of decoded storage.
    pub input_bytes: u64,
}

/// A completion's number within its conversation and its accepted host-unit charge.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ResponseInfo {
    /// One-based completed turn, reused by retries of that turn.
    pub number: u64,
    /// Accepted charge in the opener's price unit; None before accounting.
    pub spent: Option<u64>,
}
