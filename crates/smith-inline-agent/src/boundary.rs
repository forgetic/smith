//! Lower routing for an inline run (domain/host.md, sections 9.1 and 9.4).
//! These stateless records retain owned requests and exact callback owners;
//! no operation is answered here and each provider request owes one terminal.

use alloc::boxed::Box;
use skein_lib::{Duration, Token};
use smith_domain as smith;
use smith_host_domain::parent;

/// One provider terminal sent by the root to its generation-safe agent handle.
#[derive(PartialEq, Eq, Debug)]
pub enum Below {
    /// A provider supplied its bounded completion; ends one Complete.
    Completed { owner: Token, completion: smith::llm::Completion },
    /// A provider failed with actual transport evidence; ends one Complete.
    Failed { owner: Token, failure: smith::llm::Failure, evidence: smith::llm::Evidence, detail: Box<[u8]> },
    /// A provider confirmed cancellation; ends one Complete.
    Cancelled { owner: Token },
}

/// Ordered requests sent to the parent or its lower protocol components.
#[derive(PartialEq, Eq, Debug)]
pub enum Output {
    /// A notice in the same parent vocabulary as the spawned capability.
    Parent(parent::Request),
    /// A root request scoped by its agent handle; its original owner is unchanged.
    Lower { agent: Token, request: Lower },
}

/// An operation for the owner to route to its provider component.
#[derive(PartialEq, Eq, Debug)]
pub enum Lower {
    /// A completion requested by a run; ends with Completed, Failed or Cancelled.
    Complete {
        owner: Token,
        grant: smith::GrantName,
        prompt: smith::llm::Prompt,
        timeout: Duration,
        max_completion_bytes: u64,
        max_completion_blocks: u32,
        max_failure_bytes: u32,
        decoded_call_bytes: u64,
    },
    /// Cancel a completion; its existing terminal remains owed.
    Cancel { owner: Token },
}

/// A parent command or lower terminal sent by the owning host.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "the parent face moves values without an extra allocation")]
pub enum Input {
    /// A command in the face shared by both agent kinds.
    Parent(parent::Event),
    /// One terminal for a lower request, scoped to the agent that asked.
    Below { agent: Token, terminal: Below },
}
