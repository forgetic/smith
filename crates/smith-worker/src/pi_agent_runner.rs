//! In-process coding-agent runner.
//!
//! Runs the Smith coding loop directly on the worker's asupersync runtime
//! instead of spawning the former `smith-coding-agent` subprocess — and, as of
//! the sub-agent work, on Smith's **native sans-IO agent loop**
//! ([`smith_temper_agent::run_coding_agent_native`] →
//! [`smith_agent::run_sub_agent`]) rather than pi's imperative `Agent::run`.
//! Same role prompts, tools, and contract validation; the loop itself is the
//! deterministic `AgentMachine`.
//!
//! The provider/auth config the subprocess used to parse from `--auth` /
//! `--auth-file` / `--codex-model` flags is now held here and resolved once at
//! worker start, so a missing credential fails the worker's preflight rather
//! than every job.

use std::path::{Path, PathBuf};

use smith_temper_agent::{
    CodingAgentError, ProviderConfig, WorkspaceContext, WorkspaceResult,
    run_coding_agent_native_with_options,
};
use temper_worker_protocol::FailureClass;

use crate::agent_runner::{AgentRunError, AgentRunner};

/// Runs the role-aware `pi`-SDK coding turn in-process.
#[derive(Clone)]
pub struct PiAgentRunner {
    provider: ProviderConfig,
    max_iterations: usize,
    config_dir: Option<PathBuf>,
    enable_subagents: bool,
}

impl PiAgentRunner {
    /// Builds a runner from an already-resolved provider config. Sub-agents are
    /// off by default; use [`PiAgentRunner::with_subagents`] to enable the
    /// `investigate` sub-agent tool in the coding workspace.
    ///
    /// `provider` should be built via [`ProviderConfig::from_auth`] (which
    /// eagerly preflights credentials) at worker start, so the whole fleet
    /// fails fast on a missing login.
    pub fn new(provider: ProviderConfig, max_iterations: usize, config_dir: Option<PathBuf>) -> Self {
        Self {
            provider,
            max_iterations,
            config_dir,
            enable_subagents: false,
        }
    }

    /// Enable (or disable) the in-workspace `investigate` sub-agent tool.
    pub fn with_subagents(mut self, enable: bool) -> Self {
        self.enable_subagents = enable;
        self
    }
}

impl AgentRunner for PiAgentRunner {
    async fn run(
        &self,
        context: &WorkspaceContext,
        cwd: &Path,
    ) -> Result<WorkspaceResult, AgentRunError> {
        run_coding_agent_native_with_options(
            &self.provider,
            context,
            cwd,
            self.max_iterations,
            self.config_dir.as_deref(),
            self.enable_subagents,
        )
        .await
        .map_err(map_coding_agent_error)
    }
}

/// Maps a [`CodingAgentError`] to the executor's [`FailureClass`], preserving
/// the classification the subprocess path produced: provider/run/transport
/// trouble is transient (a re-dispatch may succeed); a contract or parse
/// violation is permanent (the same input fails the same way).
fn map_coding_agent_error(error: CodingAgentError) -> AgentRunError {
    let class = match &error {
        // Transient: credentials, network, provider rejection, abort. The old
        // binary surfaced these as a non-zero exit ⇒ Transient.
        CodingAgentError::Provider(_)
        | CodingAgentError::Run(_)
        | CodingAgentError::AgentStopped(_) => FailureClass::Transient,
        // Permanent: the model's output violated the contract or could not be
        // parsed — re-running with the same input will not help.
        CodingAgentError::Parse { .. }
        | CodingAgentError::NoProduct
        | CodingAgentError::UndeclaredVerdict { .. } => FailureClass::Permanent,
    };
    AgentRunError::new(class, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_and_run_errors_are_transient() {
        assert_eq!(
            map_coding_agent_error(CodingAgentError::Run("boom".to_string())).class,
            FailureClass::Transient
        );
        assert_eq!(
            map_coding_agent_error(CodingAgentError::AgentStopped("stopped".to_string())).class,
            FailureClass::Transient
        );
    }

    #[test]
    fn contract_and_parse_errors_are_permanent() {
        assert_eq!(
            map_coding_agent_error(CodingAgentError::NoProduct).class,
            FailureClass::Permanent
        );
        assert_eq!(
            map_coding_agent_error(CodingAgentError::Parse {
                snippet: "x".to_string(),
                error: "bad".to_string(),
            })
            .class,
            FailureClass::Permanent
        );
        assert_eq!(
            map_coding_agent_error(CodingAgentError::UndeclaredVerdict {
                emitted: "merge_now".to_string(),
                allowed: vec!["approve".to_string()],
            })
            .class,
            FailureClass::Permanent
        );
    }

    #[test]
    fn error_message_is_preserved() {
        let mapped = map_coding_agent_error(CodingAgentError::Run("transport reset".to_string()));
        assert!(mapped.message.contains("transport reset"));
    }
}
