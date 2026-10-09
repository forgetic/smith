//! Bounded caller-owned codec data and one actual Client's translation context.
//! No borrowed application value survives an entry point.

use alloc::boxed::Box;

use skein_lib::Token;
use smith_domain::{llm, tools};

use crate::{Limits, Receiving};

/// A preparation refusal reported by the protocol adapter to its caller.
/// Contract: protocol/llm.md, sections 3 and 4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Error {
    /// The caller supplied an invalid application value; no call starts.
    Invalid,

    /// A local application or shared-client bound refused the value; no call starts.
    Limit,

    /// The shared dialect cannot represent the value; no call starts.
    Unsupported,
}

impl Error {
    /// Preserve the adapter's existing refusal class at the shared boundary.
    /// Application ownership limits are distinct from the shared Client's caps.
    #[must_use]
    pub const fn from_shared(error: skein_llm::Error) -> Self {
        match error {
            skein_llm::Error::Invalid => Self::Invalid,
            skein_llm::Error::Limit { which: _, bound: _ } => Self::Limit,
            skein_llm::Error::Unsupported => Self::Unsupported,
        }
    }
}

/// Semantic descriptor supplied by the application codec, never inferred from JSON Schema.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ToolKind {
    /// A concrete checkout operation, offered only under its existing family grant.
    /// Contract: domain/tools.md, section 9.
    Owned(
        /// Exact checkout operation whose typed input the caller decodes.
        tools::Tool,
    ),

    /// The run's offered finish descriptor.
    /// Contract: domain/run.md, section 12.
    Finish,

    /// The separately granted mid-run delivery descriptor.
    /// Contract: domain/run.md, section 8.4.
    Deliver,

    /// The run's offered child-conversation descriptor.
    /// Contract: domain/run.md, section 5.3.
    SubAgent,

    /// The main conversation's offered wait descriptor.
    /// Contract: domain/run.md, section 6.
    Wait,
}

/// One whole caller-supplied schema for a concrete non-host tool.
/// All offered application tools require exactly one descriptor; no schema is invented.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ToolSchema {
    /// Semantic classification checked against the prompt's actual grants/descriptors.
    pub kind: ToolKind,

    /// Exact nonempty tool name, unique across application and host declarations.
    pub name: Box<[u8]>,

    /// Whole application description passed to the shared Client.
    pub description: Box<[u8]>,

    /// Whole JSON Schema object bytes; syntax is attested by Skein JSON before preparation.
    /// This adapter never validates arguments against the schema's meaning.
    pub schema: Box<[u8]>,
}

/// Caller-decoded non-host input for one actual completion block.
/// The adapter checks its descriptor and cannot accept a caller-supplied host operation.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct ResolvedCall {
    /// Actual zero-based assistant block position, including preceding text/refusal/replay blocks.
    /// It is only a lookup key; the run derives durable names from actual transcript origin.
    /// Contract: domain/run.md, sections 3.2 and 5.2.
    pub position: u32,

    /// Exact provider tool name whose bytes were decoded by the caller.
    pub name: Box<[u8]>,

    /// Exact raw provider argument bytes; must match the observed call byte for byte.
    pub input: Box<[u8]>,

    /// Typed application outcome, checked against the exact offered descriptor.
    pub call: llm::Decoded,
}

/// Immutable context for the actual call. No provider schema or application policy is interpreted.
/// Declarations survive consumed prompt preparation and are released after actual terminal translation.
#[derive(Debug)]
pub struct Context {
    pub(crate) owner: Token,
    pub(crate) grants: tools::Grants,
    pub(crate) served: Box<[llm::Served]>,
    pub(crate) application: Box<[ToolSchema]>,
    pub(crate) receiving: Receiving,
    pub(crate) limits: Limits,
}

impl Context {
    /// Limits used for the admitted call and its application decoder.
    #[must_use]
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Exact admitted checkout grants for a caller's static application decoder.
    /// This is immutable preparation data, not an authority to run an effect.
    #[must_use]
    pub const fn grants(&self) -> tools::Grants {
        self.grants
    }

    /// Borrowed actual offered run declarations, valid only during this entry.
    /// Callers use them for explicit application decoding without copying a
    /// consumed prompt; host decoding remains the adapter's own declaration lookup.
    #[must_use]
    pub fn served(&self) -> &[llm::Served] {
        &self.served
    }

    /// Borrowed actual non-host descriptors admitted before the wire attempt.
    /// A static application decoder may inspect these without a callback or
    /// retained borrow; the adapter later checks the exact decoded classification.
    #[must_use]
    pub fn application(&self) -> &[ToolSchema] {
        &self.application
    }

    /// Secured receiving metadata for this actual completion, copied unchanged.
    /// The caller's bounded decoder prices its input under this aggregate cap.
    #[must_use]
    pub const fn receiving(&self) -> Receiving {
        self.receiving
    }
}
