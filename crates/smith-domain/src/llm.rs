//! The provider-neutral vocabulary of a conversation with an LLM, as the domain
//! speaks it to the protocol layer.
//!
//! It is the session child domain's ([`smith_domain_session::llm`]),
//! with what the session carries as tickets resolved into the values they stand
//! for: the tools the run serves, the run's typed asks the LLM makes of them,
//! and the run's answers. The protocol layer turns it into each provider's wire
//! format and back, as it does the session's: it owns the schemas of the tools
//! a prompt offers, decodes the JSON the LLM writes as a tool's input into a
//! typed call, or into the [`Problem`] that keeps it from being one, and
//! renders what comes of a call as the text the LLM reads.
//! Contract: domain/run.md, section 14; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;

use smith_domain_run as run;
use smith_domain_tools as tools;

pub use smith_domain_session::llm::{Endpoint, Failure, Problem, Role, Stop, Usage};

/// One call to an LLM: everything it needs to produce the next assistant
/// message.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Prompt {
    /// Configured provider endpoint identity; the domain never resolves its address.
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    pub model: Box<[u8]>,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    pub system: Box<[u8]>,
    /// The families of tools the LLM may call, whose schemas the protocol
    /// layer offers it, and the tools the run serves.
    pub tools: tools::Grants,
    /// Typed run tools offered for this completion; only the main conversation offers finish.
    pub served: Box<[Served]>,
    /// The conversation so far, oldest first, ending with a user message.
    pub messages: Box<[Message]>,
    /// The most tokens the answer may take.
    pub max_tokens: u32,
}

/// A tool the run serves, which a prompt offers the LLM, and which the
/// protocol layer decodes into a [`run::Ask`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Served {
    /// Finish the run with an outcome.
    Finish,
    /// Ask for a sub-agent.
    SubAgent,
}

/// One ordered conversation message, with sender and owned bounded content.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Message {
    /// Sender of this message in the provider-neutral conversation.
    pub role: Role,
    /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
    pub content: Box<[Block]>,
}

/// A piece of a message in a prompt. Text is UTF-8, checked by the protocol
/// layer.
#[derive(PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Block {
    /// Provider-owned reasoning, preserved in position and replayed verbatim.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        bytes: Box<[u8]>,
    },
    /// Owned bounded text in its original provider position.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        text: Box<[u8]>,
    },
    /// A tool call the LLM made, sent back as it wrote it: `id` is the
    /// provider's name for the call, `name` and `input` what the LLM wrote.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        name: Box<[u8]>,
        /// Provider-written tool arguments or token allowance, according to the enclosing record.
        input: Box<[u8]>,
    },
    /// What came of the tool call `id`.
    ToolResult {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        id: Box<[u8]>,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        result: Returned,
    },
}

/// What came of a tool call, for the protocol layer to render as the text the
/// LLM reads.
#[derive(PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Returned {
    /// The tools' outcome: a success, or a failure, one that ran out of time
    /// included.
    Owned {
        /// Declared or accepted result, validated against the host's contract.
        outcome: tools::Outcome,
    },
    /// The run's answer to one of the tools it serves; `error` marks a
    /// failure.
    Served {
        /// Typed result of the delegated run tool, rendered below the domain.
        returned: run::Returned,
        /// Whether the returned tool result represents a failure.
        error: bool,
    },
    /// The call was malformed, or too large to hold, and this is why.
    Invalid {
        /// Typed reason a call could not become an admitted tool operation.
        problem: Problem,
    },
    /// Nothing ran for the call: the LLM stopped for another reason than
    /// calling tools, its answer cut short or its turn ended.
    NotRun,
}

/// The next assistant message.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Completion {
    /// Owned content blocks or file bytes, bounded by the enclosing record's byte cap.
    pub content: Box<[Said]>,
    /// Why the provider stopped this completion, independently of its content.
    pub stop: Stop,
    /// Provider-reported token usage, charged exactly once when its completion ends.
    pub usage: Usage,
}

/// A piece of an assistant message.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Said {
    /// Provider-owned reasoning, preserved in position and replayed verbatim.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        bytes: Box<[u8]>,
    },
    /// Owned bounded text in its original provider position.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        text: Box<[u8]>,
    },
    /// The LLM asks for a tool to run. `id` is the provider's name for this
    /// call, which its result echoes; `name` and `input` are what the LLM
    /// wrote, the input a JSON object; `call` is what the protocol layer
    /// decoded from them.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        name: Box<[u8]>,
        /// Provider-written tool arguments or token allowance, according to the enclosing record.
        input: Box<[u8]>,
        /// Typed tool call or caller-issued delegated call identity, as named by the record.
        call: Decoded,
    },
}

/// A tool call, as the protocol layer decoded it.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Decoded {
    /// A call to one of the tools a session owns.
    Owned {
        /// Typed tool call or caller-issued delegated call identity, as named by the record.
        call: tools::Call,
    },
    /// A call to one of the tools the run serves.
    Served {
        /// Typed run tool ask, validated against the asker's authority and charter.
        ask: run::Ask,
    },
    /// A call that is no call: it is answered with its problem, and nothing
    /// runs for it.
    Invalid {
        /// Typed reason a call could not become an admitted tool operation.
        problem: Problem,
    },
}
