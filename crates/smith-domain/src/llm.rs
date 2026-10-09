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
//! Contract: domain/session.md, section 4; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;

use smith_domain_run as run;
use smith_domain_tools as tools;

pub use smith_domain_session::llm::{Endpoint, Evidence, Failure, Problem, Replay, Role, Stop, Usage};

/// One call to an LLM: everything it needs to produce the next assistant
/// message.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Prompt {
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
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Served {
    /// Main-only exclusive settled-wait descriptor, fixed name wait and no arguments.
    Wait,

    /// Main-only host declaration, handed unchanged to the provider schema layer.
    Host(
        /// Whole admitted bounded declaration, copied without schema or policy interpretation.
        run::HostTool,
    ),
    /// Main-only mid-run delivery descriptor, offered only by a separate grant.
    /// This fixed descriptor names a tool; operation names come from transcript origin.
    Deliver,
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
    /// Ordered provider-neutral blocks; boxed storage and owned payloads count
    /// against the receiving session's aggregate ownership limits.
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
    /// Explicit provider refusal, distinct from ordinary text and preserved in place.
    Refusal {
        /// Provider-attested UTF-8 refusal bytes, bounded with enclosing content.
        text: Box<[u8]>,
        /// Complete optional provider replay envelope, preserved opaquely.
        replay: Option<Replay>,
    },
    /// Owned bounded text in its original provider position.
    Text { text: Box<[u8]>, replay: Option<Replay> },
    /// A tool call the LLM made, sent back as it wrote it: `id` is the
    /// provider's name for the call, `name` and `input` what the LLM wrote.
    ToolCall {
        id: Box<[u8]>,
        name: Box<[u8]>,
        /// Exact provider-written argument bytes, retained for concrete replay and
        /// counted with their enclosing message against session ownership limits.
        input: Box<[u8]>,
        replay: Option<Replay>,
    },
    /// What came of the tool call `id`.
    ToolResult {
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
    /// Actual concrete result, copied verbatim with error and optional opaque
    /// record metadata. Canonical live run feedback carries replay None; an
    /// adapter unable to carry restored metadata refuses before provider work.
    Text {
        /// Attested UTF-8 result bytes, bounded before effects.
        text: Box<[u8]>,
        /// Actual caller error classification.
        error: bool,
        /// Complete optional concrete replay metadata, never silently discarded.
        replay: Option<Replay>,
    },
    /// Actual cancellation won the delegated operation; distinct from error text.
    Withdrawn,

    /// The tools' outcome: a success, or a failure, one that ran out of time
    /// included.
    Owned {
        /// Typed checkout-tool success or failure returned by the tools child;
        /// it is not a declared run result and confers no new authority.
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
    /// Bytes of each opted-in oversized reasoning item, never retained in history.
    pub reasoning_dropped: Box<[u64]>,
    /// Ordered provider-neutral blocks; boxed storage and owned payloads count
    /// against the receiving session's aggregate ownership limits.
    pub content: Box<[Said]>,
    pub stop: Stop,
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
    /// Explicit provider refusal, distinct from ordinary text and preserved in place.
    Refusal {
        /// Provider-attested UTF-8 refusal bytes, bounded with enclosing content.
        text: Box<[u8]>,
        /// Complete optional provider replay envelope, preserved opaquely.
        replay: Option<Replay>,
    },
    /// Owned bounded text in its original provider position.
    Text { text: Box<[u8]>, replay: Option<Replay> },
    /// The LLM asks for a tool to run. `id` is the provider's name for this
    /// call, which its result echoes; `name` and `input` are what the LLM
    /// wrote, the input a JSON object; `call` is what the protocol layer
    /// decoded from them.
    ToolCall {
        id: Box<[u8]>,
        name: Box<[u8]>,
        /// Exact provider-written argument bytes, retained for concrete replay and
        /// counted with their enclosing message against session ownership limits.
        input: Box<[u8]>,
        /// Full decoded classification: owned checkout call, run-served ask,
        /// or invalid-input problem; the domain executes only admitted classifications.
        call: Decoded,
        replay: Option<Replay>,
    },
}

/// A tool call, as the protocol layer decoded it.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Decoded {
    /// A call to one of the tools a session owns.
    Owned {
        /// Typed checkout-tool call decoded below the domain.
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

impl Decoded {
    /// Checked complete application decoded-call ownership, including its inline
    /// classification cell and every nested owning wrapper/payload. The adapter
    /// and root use the same count for the aggregate receiving allowance before
    /// effects; None means arithmetic overflow. Original provider call/replay
    /// bytes are counted independently with their enclosing Said block.
    #[must_use]
    pub fn owned_bytes(&self) -> Option<u64> {
        let payload = match self {
            Decoded::Owned { call } => {
                call.owned_bytes()?.checked_sub(u64::try_from(size_of::<tools::Call>()).ok()?)?
            }
            Decoded::Served { ask } => {
                crate::peer::ask_cost(ask)?.checked_sub(u64::try_from(size_of::<run::Ask>()).ok()?)?
            }
            Decoded::Invalid { problem } => match problem {
                Problem::UnknownTool
                | Problem::NotAnObject
                | Problem::TooLarge
                | Problem::Oversize { .. }
                | Problem::CutOff { .. } => 0,
                Problem::Missing { field } | Problem::WrongType { field } | Problem::BadValue { field } => {
                    u64::try_from(field.len()).ok()?
                }
            },
        };
        u64::try_from(size_of::<Self>()).ok()?.checked_add(payload)
    }
}
