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
//!
//! Contract: domain/run.md, section 14; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;

use smith_domain_run as run;
use smith_domain_tools as tools;

pub use smith_domain_session::llm::{Endpoint, Failure, Problem, Role, Stop, Usage};

/// One call to an LLM: everything it needs to produce the next assistant
/// message.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Prompt {
    /// Configured provider endpoint identity; the domain never resolves its address.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub model: Box<[u8]>,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub system: Box<[u8]>,
    /// The families of tools the LLM may call, whose schemas the protocol
    /// layer offers it, and the tools the run serves.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub tools: tools::Grants,
    /// Typed run tools offered for this completion; only the main conversation offers finish.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub served: Box<[Served]>,
    /// The conversation so far, oldest first, ending with a user message.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub messages: Box<[Message]>,
    /// The most tokens the answer may take.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub max_tokens: u32,
}

/// A tool the run serves, which a prompt offers the LLM, and which the
/// protocol layer decodes into a [`run::Ask`].
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Served {
    /// Main-only mid-run delivery descriptor, offered only by a separate grant.
    /// This fixed descriptor names a tool; operation names come from transcript origin.
    /// Contract: domain/run.md, sections 8.2 and 8.4; domain/host.md, section 7.
    Deliver,
    /// Finish the run with an outcome.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Finish,
    /// Ask for a sub-agent.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    SubAgent,
}

/// One ordered conversation message, with sender and owned bounded content.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Message {
    /// Sender of this message in the provider-neutral conversation.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub role: Role,
    /// Ordered provider-neutral blocks; boxed storage and owned payloads count
    /// against the receiving session's aggregate ownership limits.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub content: Box<[Block]>,
}

/// A piece of a message in a prompt. Text is UTF-8, checked by the protocol
/// layer.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Block {
    /// Provider-owned reasoning, preserved in position and replayed verbatim.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        bytes: Box<[u8]>,
    },
    /// Owned bounded text in its original provider position.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        text: Box<[u8]>,
    },
    /// A tool call the LLM made, sent back as it wrote it: `id` is the
    /// provider's name for the call, `name` and `input` what the LLM wrote.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        name: Box<[u8]>,
        /// Exact provider-written argument bytes, retained for concrete replay and
        /// counted with their enclosing message against session ownership limits.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        input: Box<[u8]>,
    },
    /// What came of the tool call `id`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    ToolResult {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        id: Box<[u8]>,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        result: Returned,
    },
}

/// What came of a tool call, for the protocol layer to render as the text the
/// LLM reads.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Returned {
    /// The tools' outcome: a success, or a failure, one that ran out of time
    /// included.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Owned {
        /// Typed checkout-tool success or failure returned by the tools child;
        /// it is not a declared run result and confers no new authority.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        outcome: tools::Outcome,
    },
    /// The run's answer to one of the tools it serves; `error` marks a
    /// failure.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Served {
        /// Typed result of the delegated run tool, rendered below the domain.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        returned: run::Returned,
        /// Whether the returned tool result represents a failure.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        error: bool,
    },
    /// The call was malformed, or too large to hold, and this is why.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Invalid {
        /// Typed reason a call could not become an admitted tool operation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        problem: Problem,
    },
    /// Nothing ran for the call: the LLM stopped for another reason than
    /// calling tools, its answer cut short or its turn ended.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    NotRun,
}

/// The next assistant message.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Completion {
    /// Ordered provider-neutral blocks; boxed storage and owned payloads count
    /// against the receiving session's aggregate ownership limits.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub content: Box<[Said]>,
    /// Why the provider stopped this completion, independently of its content.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub stop: Stop,
    /// Provider-reported token usage, charged exactly once when its completion ends.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub usage: Usage,
}

/// A piece of an assistant message.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Said {
    /// Provider-owned reasoning, preserved in position and replayed verbatim.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        bytes: Box<[u8]>,
    },
    /// Owned bounded text in its original provider position.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        text: Box<[u8]>,
    },
    /// The LLM asks for a tool to run. `id` is the provider's name for this
    /// call, which its result echoes; `name` and `input` are what the LLM
    /// wrote, the input a JSON object; `call` is what the protocol layer
    /// decoded from them.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        name: Box<[u8]>,
        /// Exact provider-written argument bytes, retained for concrete replay and
        /// counted with their enclosing message against session ownership limits.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        input: Box<[u8]>,
        /// Full decoded classification: owned checkout call, run-served ask,
        /// or invalid-input problem; the domain executes only admitted classifications.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        call: Decoded,
    },
}

/// A tool call, as the protocol layer decoded it.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Decoded {
    /// A call to one of the tools a session owns.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Owned {
        /// Typed checkout-tool call decoded below the domain.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        call: tools::Call,
    },
    /// A call to one of the tools the run serves.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Served {
        /// Typed run tool ask, validated against the asker's authority and charter.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        ask: run::Ask,
    },
    /// A call that is no call: it is answered with its problem, and nothing
    /// runs for it.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Invalid {
        /// Typed reason a call could not become an admitted tool operation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        problem: Problem,
    },
}
