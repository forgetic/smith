//! The local host's typed terminal, store, credential and agent boundary
//! (domain/host.md, sections 2 and 7–9). The store owns encoding; this domain
//! keeps only records. Every `Load`, `SaveState`, `SaveTurn` and `Credential` request
//! receives one terminal, including after a cancellation.

use alloc::boxed::Box;
use skein_lib::Token;
use smith_domain::{self as agent, run, tools};

/// One writable directory recorded before a delivery can commit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IntentDirectory {
    /// Position in workspace order.
    pub directory: u32,
    /// Whether the checked tree differs from its starting state.
    pub changed: bool,
    /// Git head before delivery; absent for a plain directory.
    pub head: Option<Box<[u8]>>,
}

/// The durable pre-effect record for a delivery.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeliveryIntent {
    /// All writable directories in workspace order.
    pub directories: Box<[IntentDirectory]>,
}

/// An intent awaiting reconciliation, or its final answer.
#[derive(Clone, PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "the sealed delivery diagnostic is held inline in the durable record")]
pub enum DeliveryState {
    /// The effect may have begun, and must be found before the next run.
    Intent(DeliveryIntent),
    /// The answer to replay for this call name.
    Answer(run::Delivery),
}

/// One durable intent or decision for an agent delivery call name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeliveryRecord {
    /// Transcript-derived name decided once by this host.
    pub name: run::CallName,
    /// Pre-effect intent or terminal to replay when the agent asks again.
    pub state: DeliveryState,
    /// Effects found or observed before the saved answer, in workspace order.
    pub landed: Box<[run::Receipt]>,
    /// Latest turn told before this decision; a later durable turn contains its answer.
    pub after_turn: u32,
    /// Whether a later turn carrying the answer became durable.
    pub told: bool,
}

impl DeliveryRecord {
    /// Return the first saved terminal only for its original call name.
    #[must_use]
    pub fn answer(&self, name: run::CallName) -> Option<run::Delivery> {
        if self.name != name {
            return None;
        }
        match &self.state {
            DeliveryState::Intent(_) => None,
            DeliveryState::Answer(delivery) => Some(delivery.clone()),
        }
    }
}

/// A typed operation on one configured git working tree.
#[derive(Debug)]
pub enum GitOp {
    /// Discover changes and merge-conflicted paths.
    Status,
    /// Inspect the current head and whether its commit carries this delivery's name.
    Inspect { name: run::CallName },
    /// Check original conflict paths for markers still present.
    Markers { paths: Box<[Box<[u8]>]> },
    /// Commit the checked tree with the configured result message.
    Commit { message: Box<[u8]> },
    /// Conditionally push the new commit to the configured branch.
    Push { remote: Box<[u8]>, branch: Box<[u8]> },
}

/// One terminal for a typed git operation.
#[derive(Debug)]
pub enum GitResult {
    /// Current working-tree state and any original merge conflicts.
    Status { changed: bool, merging: Option<Box<[Box<[u8]>]>>, head: Box<[u8]> },
    /// Current head and whether its commit message has the call-name trailer.
    Inspected { head: Box<[u8]>, named: bool },
    /// First original conflict file still holding a marker, if any.
    Markers { first: Option<Box<[u8]>> },
    /// The new commit identity to show the agent and person.
    Committed { receipt: Box<[u8]> },
    /// The configured branch now names the commit.
    Pushed,
    /// The remote branch moved since this checkout began.
    Stale,
    /// Git operation failed with a bounded diagnostic tail.
    Failed { reason: run::DeliveryReason, diagnostic: Box<run::Diagnostic> },
}

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
    Loaded { state: Option<ChatState>, transcript: Option<agent::Transcript>, delivery: Option<Box<DeliveryRecord>> },
    /// Terminal for `SaveState`; names and activation are now durable.
    StateSaved,
    /// Terminal for `SaveTurn`; the named turn is now durable.
    TurnSaved { number: u32 },
    /// Terminal for a saved delivery decision.
    DeliverySaved { name: run::CallName },
    /// Terminal for one workspace git operation.
    Git { owner: Token, result: GitResult },
    /// Terminal for a plain directory's changed-state query.
    PlainStatus { owner: Token, changed: bool },
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
    /// Save a pre-effect intent or terminal; the child hears only the terminal.
    SaveDelivery { record: Box<DeliveryRecord> },
    /// Operate on one git directory within the delivery deadline.
    Git { owner: Token, directory: u32, op: GitOp, deadline: skein_lib::Time },
    /// Compare a plain directory with its start snapshot.
    PlainStatus { owner: Token, directory: u32, deadline: skein_lib::Time },
    /// Fetch or refresh one account; answered by `Credential` or `NoCredential`.
    Credential { account: u32 },
    /// Forward one agent IO or LLM request unchanged.
    Agent(agent::Request),
    /// The caller may terminate this invocation.
    Exit { status: ExitStatus },
}
