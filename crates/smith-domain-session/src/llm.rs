//! The provider-neutral vocabulary of a conversation with an LLM.
//!
//! The domain speaks this to every provider. The protocol layer turns it into
//! each provider's wire format (the Anthropic and `OpenAI` APIs, ...) and back,
//! and classifies whatever goes wrong as a [`Failure`]. It owns the schemas of
//! the tools a prompt offers, decodes the JSON the LLM writes as a tool's input
//! into a typed call, or into the [`Problem`] that keeps it from being one, and
//! renders what comes of a call as the text the LLM reads. The domain never
//! parses: it keeps a call's input as the bytes the LLM wrote, to send back
//! verbatim, beside what was decoded from it.
//!
//! Contract: domain/session.md, section 12; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;

use skein_lib::{Duration, Token};
use smith_domain_tools as tools;

/// A provider endpoint the protocol layer is configured with: which provider,
/// where, with which credentials. The domain only names it.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Endpoint(
    /// Host-configured numeric provider endpoint name, echoed without address resolution or authority inference.
    ///
    /// Contract: domain/session.md, sections 4 and 12.
    pub u32,
);

/// Who wrote a message.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Role {
    /// The agent's side: the task, and the results of tool calls.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    User,
    /// The LLM.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Assistant,
}

/// A piece of a message. Text is UTF-8, checked by the protocol layer.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Block {
    /// Provider-owned reasoning, preserved in position and replayed verbatim.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        bytes: Box<[u8]>,
    },
    /// Owned bounded text in its original provider position.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        text: Box<[u8]>,
    },
    /// The LLM asks for a tool to run. `id` is the provider's name for this
    /// call, which its result echoes; `name` and `input` are what the LLM
    /// wrote, the input a JSON object; `call` is what the protocol layer
    /// decoded from them.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        name: Box<[u8]>,
        /// Exact provider-written argument bytes, retained for concrete replay and
        /// counted with their enclosing message against session ownership limits.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        input: Box<[u8]>,
        /// Full decoded classification: owned tool call, opener-served ticket,
        /// invalid-input problem, or non-executable concrete-history replay marker.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        call: Decoded,
    },
    /// What came of the tool call `id`.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    ToolResult {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        id: Box<[u8]>,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        result: Returned,
    },
}

/// A tool call, as the protocol layer decoded it.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Decoded {
    /// A call to one of the tools the session owns.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Owned {
        /// Typed checkout-tool call decoded below the domain.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        call: tools::Call,
    },
    /// A call to one of the tools the opener serves, with `effect`: the typed
    /// call is the opener's, kept under `ticket` (a ticket names a value the
    /// session cannot, and the layer that holds it resolves it).
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Delegated {
        /// Opener-issued opaque name for a tool, delegated call or returned answer.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        ticket: Token,
        /// Whether this delegated call may write, used to serialize writers.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        effect: tools::Effect,
    },
    /// A call that is no call: the session answers it with its problem, and
    /// runs nothing.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Invalid {
        /// Typed reason a call could not become an admitted tool operation.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        problem: Problem,
    },
    /// A prior call, kept by provider name and input in its enclosing block.
    /// It is replayed to the provider and never executed again.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Historical,
}

/// What came of a tool call, for the protocol layer to render as the text the
/// LLM reads.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Returned {
    /// The tools' outcome: a success, or a failure, one that ran out of time
    /// included.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Owned {
        /// Typed checkout-tool success or failure returned by the tools child;
        /// it is not a declared run result and confers no new authority.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        outcome: tools::Outcome,
    },
    /// The opener's answer to a delegated call.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Delegated {
        /// Single terminal value returned to the caller.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        answer: Answer,
    },
    /// The call was malformed, and this is why.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Invalid {
        /// Typed reason a call could not become an admitted tool operation.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        problem: Problem,
    },
    /// Nothing ran for the call: the LLM stopped for another reason than
    /// calling tools, its answer cut short or its turn ended.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    NotRun,
    /// An opener's concrete rendered answer, without a local ticket.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Text {
        /// Opaque opener-provided result bytes, preserved in concrete history.
        /// The constructor does not validate UTF-8; session ownership limits bound retention.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        text: Box<[u8]>,
        /// Whether the returned tool result represents a failure.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        error: bool,
    },
    /// A delegated call whose withdrawal won its terminal race.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Withdrawn,
}

/// A tool the opener serves, which the prompt offers the LLM: `ticket` names
/// it, and its schema is the opener's. Calls to it read or write as `effect`
/// says, and are scheduled so.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Descriptor {
    /// Opener-issued opaque name for a tool, delegated call or returned answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub ticket: Token,
    /// Whether this delegated call may write, used to serialize writers.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub effect: tools::Effect,
}

/// The opener's answer to a delegated call: what it says is the opener's,
/// kept under `ticket`, and takes `bytes`, which count against the session's
/// byte limit as if the session held them; `error` marks a failure, one that
/// ran out of time included.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Answer {
    /// Opener-issued opaque name for a tool, delegated call or returned answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub ticket: Token,
    /// Owned payload bytes charged against the enclosing session limit.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub bytes: u64,
    /// Whether the returned tool result represents a failure.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub error: bool,
}

/// Why a tool call is no call: the protocol layer could not decode it, or the
/// agent could not hold what it decoded.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Problem {
    /// No tool the prompt offered has the call's name.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    UnknownTool,
    /// The input is not a JSON object.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    NotAnObject,
    /// The input has no `field`, which the tool needs.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Missing {
        /// Name of the required input member which failed validation.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        field: Box<[u8]>,
    },
    /// The input's `field` has the wrong type.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    WrongType {
        /// Name of the required input member which failed validation.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        field: Box<[u8]>,
    },
    /// The input's `field` has a value the tool cannot take: a path with an
    /// empty name or a NUL in it, a number out of range.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    BadValue {
        /// Name of the required input member which failed validation.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        field: Box<[u8]>,
    },
    /// The call holds more than the agent takes at once.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    TooLarge,
}

/// One ordered conversation message, with sender and owned bounded content.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Message {
    /// Sender of this message in the provider-neutral conversation.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub role: Role,
    /// Ordered provider-neutral blocks; boxed storage and owned payloads count
    /// against the receiving session's aggregate ownership limits.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub content: Box<[Block]>,
}

/// One call to an LLM: everything it needs to produce the next assistant
/// message.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Prompt {
    /// Configured provider endpoint identity; the domain never resolves its address.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub endpoint: Endpoint,
    /// The provider's name for the model.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub model: Box<[u8]>,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub system: Box<[u8]>,
    /// The families of tools the LLM may call, whose schemas the protocol
    /// layer offers it, and the tools the opener serves.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub tools: tools::Grants,
    /// Opener-owned tool descriptors; tickets are resolved only by the opener.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub delegated: Box<[Descriptor]>,
    /// The conversation so far, oldest first, ending with a user message.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub messages: Box<[Message]>,
    /// The most tokens the answer may take.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub max_tokens: u32,
}

/// The next assistant message.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Completion {
    /// Ordered provider-neutral blocks; boxed storage and owned payloads count
    /// against the receiving session's aggregate ownership limits.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub content: Box<[Block]>,
    /// Why the provider stopped this completion, independently of its content.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub stop: Stop,
    /// Provider-reported token usage, charged exactly once when its completion ends.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub usage: Usage,
}

/// Why the LLM stopped.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stop {
    /// It finished its turn.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    EndTurn,
    /// It wants the tool calls in its message run, and their results back.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    ToolUse,
    /// It ran out of its token budget mid-answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    MaxTokens,
    /// It declined to answer.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Refusal,
}

/// The tokens calls consumed, as the provider counts them: what it read
/// afresh, what it wrote, and what it read from and wrote to its prompt cache.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Usage {
    /// Fresh input tokens reported by the provider.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub input_tokens: u64,
    /// Output tokens reported by the provider.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub output_tokens: u64,
    /// Input tokens served from the provider's prompt cache.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub cache_read_tokens: u64,
    /// Input tokens written to the provider's prompt cache.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub cache_write_tokens: u64,
}

impl Usage {
    /// No accepted completions or token usage yet.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    pub const ZERO: Usage = Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 };

    /// Adds accepted cumulative usage by dimension, saturating rather than wrapping on overflow.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    #[must_use]
    pub const fn saturating_add(self, other: Usage) -> Usage {
        Usage {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
            cache_read_tokens: self.cache_read_tokens.saturating_add(other.cache_read_tokens),
            cache_write_tokens: self.cache_write_tokens.saturating_add(other.cache_write_tokens),
        }
    }
}

/// Why a call produced no message, as the protocol layer classifies the
/// provider's answer, or the lack of one.
///
/// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Failure {
    /// The provider is overloaded (HTTP 529, 503). Transient.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Overloaded,
    /// The provider limits our rate (HTTP 429) and asks us to wait
    /// `retry_after`, zero if it did not say. Transient.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    RateLimited {
        /// Optional provider cooldown, retained in the representation of this boundary.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        retry_after: Duration,
    },
    /// The provider could not be reached, or failed (a connection error,
    /// HTTP 500). Transient.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Unavailable,
    /// No answer within the call's deadline. Transient.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    TimedOut,
    /// The conversation does not fit the model's context window.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    ContextTooLong,
    /// The provider rejected the call as malformed or unsupported (HTTP 400,
    /// 404, 422).
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Invalid,
    /// The provider refused our credentials (HTTP 401, 403).
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Unauthorized,
    /// The account has spent its allowance; the engine waits for its reset.
    ///
    /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
    Exhausted {
        /// Optional provider cooldown, retained in the representation of this boundary.
        ///
        /// Copy baseline: domain/session.md, sections 3, 4, 5, 6 and 12.
        retry_after: Duration,
    },
}
