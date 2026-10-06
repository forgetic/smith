//! The local host's typed terminal, store, credential and agent boundary
//! (domain/host.md, sections 2 and 7–9). The store owns encoding; this domain
//! keeps only records. Every `Load`, `SaveState`, `SaveTurn` and `Credential` request
//! receives one terminal, including after a cancellation.

use alloc::boxed::Box;
use skein_lib::Token;
use smith_domain::{self as agent, run, tools};

/// Chat metadata kept beside durable turns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChatState {
    /// Last activation saved before it started.
    pub activation: u64,
    /// Last person message name saved before it was sent.
    pub next_message: u64,
    /// Last person message read in a told turn.
    pub read: Option<Token>,
}

/// Why the store could not keep or load this chat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreFailure {
    /// The record could not be read.
    Read,
    /// The record could not be made durable.
    Write,
}

/// Why the configured account has no usable grant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CredentialFailure {
    /// The account has not been signed in.
    Missing,
    /// The account could not be refreshed.
    Refresh,
}

/// How this invocation ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExitStatus {
    /// The person left an idle chat.
    Success,
    /// A required store or account operation failed.
    Failed,
}

/// IO and LLM terminals for requests forwarded from the agent unchanged.
#[derive(Debug)]
pub enum AgentIo {
    /// Terminal for an LLM completion.
    Completed { owner: Token, completion: agent::llm::Completion },
    /// Failed LLM completion, including provider evidence.
    Failed { owner: Token, failure: agent::llm::Failure, evidence: agent::llm::Evidence, detail: Box<[u8]> },
    /// Cancelled LLM completion.
    Cancelled { owner: Token },
    /// Terminal for an agent tool operation.
    Done { owner: Token, done: tools::Done },
    /// Terminal for a guide read.
    Read { owner: Token, read: run::Read },
    /// Terminal for check discovery.
    Probed { owner: Token, executable: bool },
    /// Terminal for a check process.
    Checked { owner: Token, ran: run::Ran },
    /// Terminal for an aborted check.
    Aborted { owner: Token },
}

impl AgentIo {
    /// Restore the child's typed terminal without interpreting it.
    pub(crate) fn into_event(self) -> agent::Event {
        match self {
            AgentIo::Completed { owner, completion } => agent::Event::Completed { owner, completion },
            AgentIo::Failed { owner, failure, evidence, detail } => {
                agent::Event::Failed { owner, failure, evidence, detail }
            }
            AgentIo::Cancelled { owner } => agent::Event::Cancelled { owner },
            AgentIo::Done { owner, done } => agent::Event::Done { owner, done },
            AgentIo::Read { owner, read } => agent::Event::Read { owner, read },
            AgentIo::Probed { owner, executable } => agent::Event::Probed { owner, executable },
            AgentIo::Checked { owner, ran } => agent::Event::Checked { owner, ran },
            AgentIo::Aborted { owner } => agent::Event::Aborted { owner },
        }
    }
}

/// One event from the person or a typed lower boundary.
#[derive(Debug)]
pub enum Event {
    /// Person text, bounded by `Limits::line_bytes` before retention.
    Line { text: Box<[u8]> },
    /// The person asks to cancel a live run or leave an idle chat.
    Interrupt,
    /// The terminal closed; finish outstanding work, then leave.
    Closed,
    /// Terminal for Load, including a possibly empty saved chat.
    Loaded { state: Option<ChatState>, transcript: Option<agent::Transcript> },
    /// Terminal for `SaveState`; names and activation are now durable.
    StateSaved,
    /// Terminal for `SaveTurn`; the named turn is now durable.
    TurnSaved { number: u32 },
    /// Terminal for any outstanding store request that failed.
    StoreFailed { reason: StoreFailure },
    /// Terminal for one Credential request.
    Credential { grant: agent::Grant },
    /// Terminal for one Credential request that could not be served.
    NoCredential { account: u32, reason: CredentialFailure },
    /// Terminal for a forwarded agent IO or LLM request.
    Agent(AgentIo),
}

/// One request to the person, store, credential source or agent IO.
#[derive(Debug)]
pub enum Request {
    /// Bounded text shown to the person.
    Show { text: Box<[u8]> },
    /// Load this configured chat's metadata and concrete history.
    Load,
    /// Save the updated activation or person-message counter before using it.
    /// A fresh start replaces incompatible old history in the same store operation.
    SaveState { state: ChatState, fresh: bool },
    /// Append one numbered concrete turn and its read fence atomically; answered by `TurnSaved` or `StoreFailed`.
    SaveTurn { number: u32, read: Option<Token>, turn: agent::Turn },
    /// Fetch or refresh one account; answered by `Credential` or `NoCredential`.
    Credential { account: u32 },
    /// Forward one agent IO or LLM request unchanged.
    Agent(agent::Request),
    /// The caller may terminate this invocation.
    Exit { status: ExitStatus },
}
