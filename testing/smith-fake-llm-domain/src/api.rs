//! The provider's API, as its domain layer sees it once the protocol layer has
//! parsed a request.
//!
//! Contract: domain/session.md, sections 4 and 12; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;

use skein_lib::Duration;

/// Sender of one provider conversation message.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Role {
    /// User or tool-result side of the conversation.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    User,
    /// Provider-produced assistant side of the conversation.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Assistant,
}

/// One bounded block accepted from the provider stream or neutral fake API.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    /// Provider-owned replay data, preserved verbatim and never interpreted by the domain.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        bytes: Box<[u8]>,
    },
    /// Owned bounded text in its original provider position.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        text: Box<[u8]>,
    },
    /// The model calls a tool with `arguments`, a JSON object.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        name: Box<[u8]>,
        /// Provider-written JSON tool arguments, retained within the enclosing byte cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        arguments: Box<[u8]>,
    },
    /// The client's answer to the tool call `id`.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    ToolOutput {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        id: Box<[u8]>,
        /// Owned output bytes retained within the enclosing output cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        output: Box<[u8]>,
        /// Whether the supplied tool result represents a failure.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        is_error: bool,
    },
}

/// One ordered conversation message, with sender and owned bounded content.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Message {
    /// Sender of this message in the provider-neutral conversation.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub role: Role,
    /// Maximum message parts, tool definitions or retained output parts.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub parts: Box<[Part]>,
}

/// A tool the client offers. `parameters` is a JSON schema.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ToolSpec {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub name: Box<[u8]>,
    /// Owned display text for the offered tool; the model alone interprets it.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub description: Box<[u8]>,
    /// Owned bounded JSON schema for the tool's arguments.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub parameters: Box<[u8]>,
}

/// A request for the next assistant message.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Query {
    /// Provider model name, treated as bytes by the domain.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub model: Box<[u8]>,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub system: Box<[u8]>,
    /// Tools offered or granted by this record; their names confer no additional authority.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub tools: Box<[ToolSpec]>,
    /// Oldest-first conversation messages, with provider call/result pairing preserved.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub messages: Box<[Message]>,
    /// Maximum output tokens requested for one completion.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub max_tokens: u32,
}

/// One fake-provider terminal, carrying ordered response parts, stop classification and independent token usage.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Answer {
    /// Maximum message parts, tool definitions or retained output parts.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub parts: Box<[Part]>,
    /// Provider stop classification or permission to offer finish, as named by the enclosing record.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub finish: Finish,
    /// Provider-reported token usage, charged exactly once when its completion ends.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub usage: Usage,
}

/// Why the fake provider stopped; the agent translator supplies its own corresponding vocabulary.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Finish {
    /// The fake finished its assistant turn.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Stop,
    /// The fake asks for tools, including deliberately malformed empty batches.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    ToolCalls,
    /// The fake cut its answer at the requested output-token limit.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Length,
    /// The fake refused by its scripted content filter.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    ContentFilter,
}

/// Tokens a call took: the prompt's, read afresh or from the cache, the
/// cache's new entries, and the answer's.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Usage {
    /// Fresh provider prompt tokens reported for this completion.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub prompt_tokens: u64,
    /// Provider prompt tokens served from cache.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub cached_tokens: u64,
    /// Provider prompt tokens written to cache.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub cache_creation_tokens: u64,
    /// Provider output tokens reported for this completion.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub completion_tokens: u64,
}

/// Typed refusal or terminal failure of the fake peer, independent of the agent's policy.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Error {
    /// The provider is temporarily overloaded.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Overloaded,
    /// The provider asks the client to wait before retrying.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    RateLimited {
        /// Optional provider cooldown, retained in the representation of this boundary.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        retry_after: Duration,
    },
    /// The service failed, or could not be reached.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Unavailable,
    /// The query does not fit the model's context window.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    ContextTooLong,
    /// The client's credentials were refused.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Unauthorized,
    /// The provider reports its account allowance spent.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Exhausted {
        /// Optional provider cooldown, retained in the representation of this boundary.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        retry_after: Duration,
    },
    /// The neutral fake rejected the client's conversation structure.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    InvalidRequest,
}

/// A conversation the fake plays from a script rather than at random: one
/// whose system text holds `cue`.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Script {
    /// Script-selection bytes matched at the earliest position in system text.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub cue: Box<[u8]>,
    /// The answers, in order: the first answers a conversation with no
    /// assistant message yet, the next one with one, and so on.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub turns: Box<[Turn]>,
}

/// One scripted answer: what it says, why it stops, and the tokens it takes
/// to say.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Turn {
    /// Scripted answer parts, emitted in their declared order.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub lines: Box<[Line]>,
    /// Provider stop classification or permission to offer finish, as named by the enclosing record.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub finish: Finish,
    /// Maximum JSON tokens or scripted output tokens, as named by the enclosing record.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub tokens: u64,
}

/// A piece of a scripted answer. The fake names its calls.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Line {
    /// Owned bounded text in its original provider position.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        text: Box<[u8]>,
    },
    /// A call to the tool `name` with `arguments`, which may be anything: a
    /// script may call a tool that was not offered, or write what is not an
    /// object.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Call {
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        name: Box<[u8]>,
        /// Provider-written JSON tool arguments, retained within the enclosing byte cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        arguments: Box<[u8]>,
    },
}
