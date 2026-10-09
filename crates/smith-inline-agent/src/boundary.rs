//! Lower routing for an inline run (domain/host.md, sections 9.1 and 9.4).
//! These stateless records retain owned requests and exact callback owners;
//! no operation is answered here and each provider request owes one terminal.

use alloc::boxed::Box;
use skein_lib::Token;
use smith_domain as smith;
use smith_host_domain::parent;

/// One provider terminal sent by the root to its generation-safe agent handle.
#[derive(PartialEq, Eq, Debug)]
pub enum Completion {
    /// A provider supplied its bounded completion; ends one Complete.
    Completed { owner: Token, completion: smith::llm::Completion },
    /// A provider failed with actual transport evidence; ends one Complete.
    Failed { owner: Token, failure: smith::llm::Failure, evidence: smith::llm::Evidence, detail: Box<[u8]> },
    /// A provider confirmed cancellation; ends one Complete.
    Cancelled { owner: Token },
}

/// Ordered requests sent to the parent or its lower protocol components.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// A notice in the same parent vocabulary as the spawned capability.
    Parent(parent::Request),
    /// A root request scoped by its agent handle; its original owner is unchanged.
    Lower { agent: Token, request: smith::Request },
}
