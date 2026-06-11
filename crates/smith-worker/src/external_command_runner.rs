//! External-command agent runner (the generic subprocess path).
//!
//! Spawns an external program that speaks the file-based coding-workspace
//! protocol: the worker writes the work-item context to a temp file named by
//! `TEMPER_CODING_WORKSPACE_CONTEXT`, runs the program in the prepared checkout
//! (cwd), and reads the [`WorkspaceResult`] back from the file named by
//! `TEMPER_CODING_WORKSPACE_RESULT`.
//!
//! This is the path the Smith coding agent used to take as its own
//! `smith-coding-agent` subprocess; that role is now served in-process by
//! [`PiAgentRunner`](crate::pi_agent_runner::PiAgentRunner). The external runner
//! remains for non-Smith coders — notably the examples' deterministic
//! `greeting` stand-in script, and any operator-provided external coder — so the
//! generic mechanism is not lost when the Smith agent goes in-process.

use std::path::Path;

use smith_temper_agent::{WorkspaceContext, WorkspaceResult};
use tokio::process::Command;

use crate::agent_runner::{AgentRunError, AgentRunner};

/// Env var naming the file the worker wrote the work-item context JSON to.
const CONTEXT_ENV: &str = "TEMPER_CODING_WORKSPACE_CONTEXT";
/// Env var naming the file the command must write its result JSON to.
const RESULT_ENV: &str = "TEMPER_CODING_WORKSPACE_RESULT";

/// Spawns an external command speaking the context/result file protocol.
#[derive(Clone, Debug)]
pub struct ExternalCommandRunner {
    /// Program followed by fixed arguments, e.g.
    /// `["/path/to/greeting-coder.sh"]`.
    command: Vec<String>,
}

impl ExternalCommandRunner {
    /// Builds a runner for the given command (program first, then args).
    pub fn new(command: Vec<String>) -> Self {
        Self { command }
    }
}

impl AgentRunner for ExternalCommandRunner {
    async fn run(
        &self,
        context: &WorkspaceContext,
        cwd: &Path,
    ) -> Result<WorkspaceResult, AgentRunError> {
        let Some((program, args)) = self.command.split_first() else {
            return Err(AgentRunError::permanent("external agent command is empty"));
        };

        let temp = tempfile::tempdir()
            .map_err(|error| AgentRunError::transient(format!("create agent temp dir: {error}")))?;
        let context_path = temp.path().join("context.json");
        let result_path = temp.path().join("result.json");

        let context_bytes = serde_json::to_vec_pretty(context).map_err(|error| {
            AgentRunError::transient(format!("serialize agent context: {error}"))
        })?;
        std::fs::write(&context_path, context_bytes).map_err(|error| {
            AgentRunError::transient(format!("write agent context file: {error}"))
        })?;

        let output = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .env(CONTEXT_ENV, &context_path)
            .env(RESULT_ENV, &result_path)
            .output()
            .await
            .map_err(|error| {
                AgentRunError::transient(format!("spawn agent command `{program}`: {error}"))
            })?;

        if !output.status.success() {
            let status = output
                .status
                .code()
                .map_or_else(|| output.status.to_string(), |code| code.to_string());
            let stderr = stderr_tail(&output.stderr, 2_000);
            return Err(AgentRunError::transient(format!(
                "agent command exited with status {status}; stderr tail: {stderr}"
            )));
        }

        let result_bytes = std::fs::read(&result_path).map_err(|error| {
            AgentRunError::permanent(format!("agent did not write a valid result file: {error}"))
        })?;
        serde_json::from_slice::<WorkspaceResult>(&result_bytes).map_err(|error| {
            AgentRunError::permanent(format!("agent result file is not valid JSON: {error}"))
        })
    }
}

/// Last `max_len` bytes of captured stderr, on a char boundary, for error
/// messages. (Secret redaction is applied by the executor, which holds the
/// push token.)
fn stderr_tail(stderr: &[u8], max_len: usize) -> String {
    let text = String::from_utf8_lossy(stderr).into_owned();
    if text.len() <= max_len {
        return text;
    }
    let mut start = text.len() - max_len;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_tail_keeps_short_input_and_truncates_long_on_boundary() {
        assert_eq!(stderr_tail(b"short", 100), "short");
        let long = "x".repeat(5_000);
        let tail = stderr_tail(long.as_bytes(), 2_000);
        assert_eq!(tail.len(), 2_000);
    }
}
