//! The agent-turn seam.
//!
//! [`CodingExecutor`](crate::coding_executor::CodingExecutor) owns the workspace
//! lifecycle — prepare the checkout, run one agent turn, map the result to a
//! [`JobOutcome`](crate::executor::JobOutcome), commit/push or discard. The
//! *agent turn itself* is abstracted behind [`AgentRunner`] so the orchestration
//! is independent of how the turn is produced:
//!
//! - [`PiAgentRunner`] runs the `pi`-SDK coding loop **in-process** on the
//!   worker's asupersync runtime (the consolidated path — no subprocess).
//! - [`ExternalCommandRunner`] spawns an external program speaking the
//!   `TEMPER_CODING_WORKSPACE_CONTEXT` / `_RESULT` file protocol (the generic
//!   path; still used by the examples' deterministic `greeting` stand-in and any
//!   non-Smith coder).
//! - test fakes return scripted results without any LLM or subprocess.
//!
//! This is the same boundary the future sans-IO sub-agent work plugs into: a
//! sub-agent is "one LLM call with custom context and an optional workspace",
//! i.e. another `AgentRunner`.

use std::path::Path;

use smith_temper_agent::WorkspaceContext;
use temper_worker_protocol::FailureClass;

pub use smith_temper_agent::WorkspaceResult;

/// Why an agent turn could not produce a [`WorkspaceResult`].
///
/// Carries the [`FailureClass`] the executor reports to the daemon so the
/// classification (transient vs permanent vs protocol) lives with the runner
/// that knows the failure's nature, rather than being re-derived downstream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRunError {
    pub class: FailureClass,
    pub message: String,
}

impl AgentRunError {
    pub fn new(class: FailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }

    /// A transient failure: retrying later may succeed (provider/transport
    /// hiccup, subprocess spawn failure, ...).
    pub fn transient(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Transient, message)
    }

    /// A permanent failure: the same input will fail the same way (the agent
    /// produced no usable product, an undeclared verdict, ...).
    pub fn permanent(message: impl Into<String>) -> Self {
        Self::new(FailureClass::Permanent, message)
    }
}

impl std::fmt::Display for AgentRunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AgentRunError {}

/// Runs one role-aware coding/triage/review turn in a prepared checkout.
///
/// `context` is the work-item context the worker assembled (repository, role,
/// branch, verdict vocabulary, ...). `cwd` is the prepared checkout the turn
/// operates on: a writable role leaves a product diff in it; a read-only role
/// inspects it and returns a verdict. The runner must not commit, push, or
/// otherwise mutate Forge state — the executor owns that.
pub trait AgentRunner: Send + Sync {
    fn run(
        &self,
        context: &WorkspaceContext,
        cwd: &Path,
    ) -> impl std::future::Future<Output = Result<WorkspaceResult, AgentRunError>> + Send;
}
