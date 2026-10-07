//! The LLM component's domain and io boundaries.
//! A service supplies the admitted charter's contract with each Complete;
//! the domain retains policy and sees only typed terminals.
//! Contract: protocol/llm.md, sections 2, 3, 6 and 7.

use alloc::boxed::Box;

use skein_lib::{Duration, Token};
use smith_domain::{llm, run};

/// A request routed from the root domain by its owning service.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "the service moves one complete domain request directly into the component"
)]
pub enum FromDomain {
    /// Start one completion under the admitted charter's result rules.
    Complete {
        owner: Token,
        grant: smith_domain::GrantName,
        prompt: llm::Prompt,
        timeout: Duration,
        bounds: crate::Receiving,
        outcome: run::outcome::OutcomeSpec,
        deliver: Option<run::outcome::ChangeSpec>,
    },
    /// Cancel one outstanding completion.
    Cancel { owner: Token },
}

/// One typed answer or text fact returned to the root domain.
#[derive(Debug)]
pub enum ToDomain {
    /// The completion and its decoded calls.
    Completed { owner: Token, completion: llm::Completion },
    /// One failure with the shared client's evidence.
    Failed { owner: Token, failure: llm::Failure, evidence: llm::Evidence, detail: Box<[u8]> },
    /// The accepted call was cancelled.
    Cancelled { owner: Token },
    /// Text arrived in flight; only its byte count crosses the boundary.
    Text { owner: Token, bytes: u32 },
}

/// Io work addressed with the connection component's token.
pub type Below = skein_io::Request;

/// Io's answer routed with the connection component's token.
pub type BelowEvent = skein_io::Event;
