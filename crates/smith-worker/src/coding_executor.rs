use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use temper_worker_protocol::{Assign, Branch, FailureClass, JobArtifactSnapshot, JobRepository};
use tokio::process::Command;

use crate::executor::{JobExecutor, JobOutcome};
use crate::workspace::{
    RoleGitIdentity, Workspace, WorkspaceConfig, WorkspaceError, forgejo_remote_url,
};

/// Configuration for the real coding-job executor.
#[derive(Clone, Debug)]
pub struct CodingExecutorConfig {
    /// Root for persistent per-(repo, role) workspaces (3b-i layout).
    pub workspace_root: PathBuf,
    /// Forge git base URL, e.g. `http://localhost:3000` (joined with the
    /// repo slug via `forgejo_remote_url`; `file://` URLs work for tests).
    pub git_base_url: String,
    /// Coding-agent command: program followed by fixed arguments, e.g.
    /// `["smith-coding-agent", "--auth", "chatgpt-oauth"]`.
    pub agent_command: Vec<String>,
    /// Role id -> git identity (user, email, push token).
    pub role_identities: BTreeMap<String, RoleGitIdentity>,
}

#[derive(Clone, Debug)]
pub struct CodingExecutor {
    config: CodingExecutorConfig,
}

impl CodingExecutor {
    pub fn new(config: CodingExecutorConfig) -> Self {
        Self { config }
    }
}

impl JobExecutor for CodingExecutor {
    fn execute(&self, assign: Assign) -> impl std::future::Future<Output = JobOutcome> + Send {
        let config = self.config.clone();
        async move { execute(config, assign).await }
    }
}

async fn execute(config: CodingExecutorConfig, assign: Assign) -> JobOutcome {
    let artifact_item = assign.artifact.item.clone();
    let context = match serde_json::from_value::<WireJobContext>(assign.job_payload) {
        Ok(context) => context,
        Err(error) => {
            return failure(
                FailureClass::Protocol,
                format!("invalid enriched job payload: {error}"),
            );
        }
    };

    let WireJobContext {
        role,
        repo,
        queue,
        artifact_kind,
        repository,
        base_branch,
        branch_hint,
        correlation_key,
        artifact,
        action: _action,
        checkout_capability,
        allowed_verdicts,
    } = context;

    let repository = match require_enriched_field(repository, "repository") {
        Ok(repository) => repository,
        Err(outcome) => return outcome,
    };
    let base_branch = match require_enriched_field(base_branch, "base_branch") {
        Ok(base_branch) => base_branch,
        Err(outcome) => return outcome,
    };
    let branch_hint = match require_enriched_field(branch_hint, "branch_hint") {
        Ok(branch_hint) => branch_hint,
        Err(outcome) => return outcome,
    };
    let correlation_key = match require_enriched_field(correlation_key, "correlation_key") {
        Ok(correlation_key) => correlation_key,
        Err(outcome) => return outcome,
    };
    let artifact = match require_enriched_field(artifact, "artifact") {
        Ok(artifact) => artifact,
        Err(outcome) => return outcome,
    };
    let checkout = checkout_capability.unwrap_or_else(|| "writable".to_string());
    let mode = match JobMode::from_checkout(&checkout) {
        Ok(mode) => mode,
        Err(outcome) => return outcome,
    };

    let identity = match config.role_identities.get(&role) {
        Some(identity) => identity.clone(),
        None => {
            return failure(
                FailureClass::Permanent,
                format!("worker has no git identity for role {role}"),
            );
        }
    };
    let token = identity.token.clone();

    let remote_url = match forgejo_remote_url(&config.git_base_url, &repo) {
        Ok(remote_url) => remote_url,
        Err(error) => return workspace_failure("construct git remote URL", error, ""),
    };
    let workspace_config = WorkspaceConfig {
        root: config.workspace_root,
        base_branch: base_branch.clone(),
    };
    let workspace = match Workspace::new(&workspace_config, &repo, &role, identity, remote_url) {
        Ok(workspace) => workspace,
        Err(error) => return workspace_failure("configure workspace", error, &token),
    };
    let prepare_result = match mode {
        JobMode::Writable | JobMode::ReadOnly => workspace.prepare(&branch_hint).await,
        JobMode::PullRequestReadOnly => {
            workspace
                .prepare_pull_request_head(artifact.number, &branch_hint)
                .await
        }
    };
    if let Err(error) = prepare_result {
        return workspace_failure("prepare workspace", error, &token);
    }

    let temp = match tempfile::tempdir() {
        Ok(temp) => temp,
        Err(error) => {
            return failure(
                FailureClass::Transient,
                format!("create coding-agent temp dir: {error}"),
            );
        }
    };
    let context_path = temp.path().join("context.json");
    let result_path = temp.path().join("result.json");
    let workspace_context = match workspace_context_payload(
        &repo,
        &role,
        &queue,
        &artifact_kind,
        &repository,
        &base_branch,
        &branch_hint,
        &correlation_key,
        &artifact,
        assign.artifact.kind.as_str(),
        &checkout,
        &allowed_verdicts,
    ) {
        Ok(payload) => payload,
        Err(error) => {
            return failure(
                FailureClass::Transient,
                format!("serialize coding-agent context: {error}"),
            );
        }
    };
    let context_bytes = match serde_json::to_vec_pretty(&workspace_context) {
        Ok(bytes) => bytes,
        Err(error) => {
            return failure(
                FailureClass::Transient,
                format!("serialize coding-agent context: {error}"),
            );
        }
    };
    if let Err(error) = std::fs::write(&context_path, context_bytes) {
        return failure(
            FailureClass::Transient,
            format!("write coding-agent context file: {error}"),
        );
    }

    let Some((program, args)) = config.agent_command.split_first() else {
        return failure(
            FailureClass::Permanent,
            "worker coding-agent command is empty".to_string(),
        );
    };
    let output = match Command::new(program)
        .args(args)
        .current_dir(workspace.path())
        .env("TEMPER_CODING_WORKSPACE_CONTEXT", &context_path)
        .env("TEMPER_CODING_WORKSPACE_RESULT", &result_path)
        .output()
        .await
    {
        Ok(output) => output,
        Err(error) => {
            return failure(
                FailureClass::Transient,
                redact_secret(
                    format!("spawn coding-agent command `{program}`: {error}"),
                    &token,
                ),
            );
        }
    };
    if !output.status.success() {
        let status = output
            .status
            .code()
            .map_or_else(|| output.status.to_string(), |code| code.to_string());
        let stderr = stderr_tail_redacted(&output.stderr, 2_000, &token);
        return failure(
            FailureClass::Transient,
            format!("coding-agent command exited with status {status}; stderr tail: {stderr}"),
        );
    }

    let result_bytes = match std::fs::read(&result_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return failure(
                FailureClass::Permanent,
                format!("coding-agent did not write a valid result file: {error}"),
            );
        }
    };
    let result = match serde_json::from_slice::<AgentResult>(&result_bytes) {
        Ok(result) => result,
        Err(error) => {
            return failure(
                FailureClass::Permanent,
                format!("coding-agent result file is not valid JSON: {error}"),
            );
        }
    };

    match mode {
        JobMode::Writable => {
            if let Some(verdict) = result.verdict {
                if allowed_verdicts.contains(&verdict) {
                    if let Err(error) = workspace.discard_changes().await {
                        return workspace_failure(
                            "discard verdict workspace changes",
                            error,
                            &token,
                        );
                    }

                    let summary = result
                        .summary
                        .or_else(|| Some(format!("implemented {correlation_key}")));
                    return JobOutcome::Verdict {
                        verdict,
                        body: result.body.or(result.review_body),
                        summary,
                    };
                }

                return failure(
                    FailureClass::Permanent,
                    format!("verdict routing not supported by smith-worker yet: {verdict}"),
                );
            }

            match workspace.has_changes().await {
                Ok(true) => {}
                Ok(false) => return failure(FailureClass::Permanent, "agent produced no diff"),
                Err(error) => return workspace_failure("inspect workspace changes", error, &token),
            }

            if let Err(error) = workspace
                .commit_all(&commit_message(&correlation_key, &artifact_item))
                .await
            {
                return workspace_failure("commit workspace changes", error, &token);
            }
            let head_sha = match workspace.push_branch(&branch_hint).await {
                Ok(head_sha) => head_sha,
                Err(error) => return workspace_failure("push workspace branch", error, &token),
            };

            JobOutcome::Success {
                branch: Branch {
                    name: branch_hint,
                    head_sha,
                },
                summary: result
                    .summary
                    .or_else(|| Some(format!("implemented {correlation_key}"))),
            }
        }
        JobMode::ReadOnly | JobMode::PullRequestReadOnly => {
            verdict_only_outcome(
                &workspace,
                result,
                &allowed_verdicts,
                &correlation_key,
                &token,
            )
            .await
        }
    }
}

async fn verdict_only_outcome(
    workspace: &Workspace,
    result: AgentResult,
    allowed_verdicts: &[String],
    correlation_key: &str,
    token: &str,
) -> JobOutcome {
    let AgentResult {
        verdict,
        summary,
        body,
        review_body,
    } = result;
    let Some(verdict) = verdict else {
        return failure(FailureClass::Permanent, "read-only job returned no verdict");
    };
    if !allowed_verdicts.contains(&verdict) {
        return failure(
            FailureClass::Permanent,
            format!(
                "read-only job returned undeclared verdict `{verdict}`; allowed verdicts: {}",
                allowed_verdicts_display(allowed_verdicts)
            ),
        );
    }

    if let Err(error) = workspace.discard_changes().await {
        return workspace_failure("discard verdict workspace changes", error, token);
    }

    JobOutcome::Verdict {
        verdict,
        body: body.or(review_body),
        summary: summary.or_else(|| Some(format!("implemented {correlation_key}"))),
    }
}

fn require_enriched_field<T>(field: Option<T>, name: &str) -> Result<T, JobOutcome> {
    field.ok_or_else(|| {
        failure(
            FailureClass::Protocol,
            format!("enriched job payload is missing `{name}`"),
        )
    })
}

fn workspace_failure(action: &str, error: WorkspaceError, token: &str) -> JobOutcome {
    failure(
        FailureClass::Transient,
        redact_secret(format!("{action}: {error}"), token),
    )
}

/// Builds the implementation commit message.
///
/// Numeric issue artifacts gain a `Closes #<n>` trailer so the forge's native
/// close-on-merge closes the source issue when the implementation PR lands —
/// the daemon applies no issue transition on success, so this trailer is what
/// retires the source issue (and its queue entry) at merge time.
fn commit_message(correlation_key: &str, artifact_item: &serde_json::Value) -> String {
    match artifact_item.as_u64() {
        Some(number) => format!("Implement {correlation_key}\n\nCloses #{number}"),
        None => format!("Implement {correlation_key}"),
    }
}

fn failure(class: FailureClass, message: impl Into<String>) -> JobOutcome {
    JobOutcome::Failure {
        class,
        message: message.into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn workspace_context_payload(
    repo: &str,
    role: &str,
    queue: &str,
    artifact_kind: &str,
    repository: &JobRepository,
    base_branch: &str,
    branch_hint: &str,
    correlation_key: &str,
    artifact: &JobArtifactSnapshot,
    artifact_wire_kind: &str,
    checkout: &str,
    allowed_verdicts: &[String],
) -> Result<serde_json::Value, serde_json::Error> {
    let (artifact_type, target_kind) = match artifact_wire_kind {
        "pull_request" => ("pull_request", "PullRequest"),
        _ => ("issue", "Issue"),
    };
    let work_item_context = serde_json::json!({
        "repository": repo,
        "role": role,
        "queue": queue,
        "kind": artifact_kind,
        "artifact": {
            "type": artifact_type,
            "number": artifact.number,
            "title": artifact.title.as_str(),
            "body": artifact.body.as_str(),
            "labels": &artifact.labels,
            "state": artifact.state.as_str(),
        }
    });
    let work_item_context = serde_json::to_string_pretty(&work_item_context)?;

    Ok(serde_json::json!({
        "repository": {
            "id": repo,
            "owner": repository.owner.as_str(),
            "name": repository.name.as_str(),
            "default_branch": repository.default_branch.as_str(),
        },
        "work_item": {
            "role": role,
            "queue": queue,
            "kind": artifact_kind,
            "target": format!("{target_kind} {{ number: ItemNumber({}) }}", artifact.number),
            "context": work_item_context,
        },
        "base_branch": base_branch,
        "branch_hint": branch_hint,
        "correlation_key": correlation_key,
        "checkout": checkout,
        "allowed_verdicts": allowed_verdicts,
        "guidance": {
            "role_guidance": null,
            "tool_guidance": null,
            "tool_constraints": [],
        }
    }))
}

fn stderr_tail_redacted(stderr: &[u8], max_len: usize, secret: &str) -> String {
    stderr_tail(
        redact_secret(String::from_utf8_lossy(stderr).into_owned(), secret),
        max_len,
    )
}

fn stderr_tail(stderr: String, max_len: usize) -> String {
    if stderr.len() <= max_len {
        return stderr;
    }

    let mut start = stderr.len() - max_len;
    while !stderr.is_char_boundary(start) {
        start += 1;
    }
    stderr[start..].to_string()
}

fn redact_secret(text: String, secret: &str) -> String {
    if secret.is_empty() {
        text
    } else {
        text.replace(secret, "<redacted>")
    }
}

#[derive(Debug, Deserialize)]
struct WireJobContext {
    role: String,
    repo: String,
    queue: String,
    artifact_kind: String,
    #[serde(default)]
    repository: Option<JobRepository>,
    #[serde(default)]
    base_branch: Option<String>,
    #[serde(default)]
    branch_hint: Option<String>,
    #[serde(default)]
    correlation_key: Option<String>,
    #[serde(default)]
    artifact: Option<JobArtifactSnapshot>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    checkout_capability: Option<String>,
    #[serde(default)]
    allowed_verdicts: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobMode {
    Writable,
    ReadOnly,
    PullRequestReadOnly,
}

impl JobMode {
    fn from_checkout(checkout: &str) -> Result<Self, JobOutcome> {
        match checkout {
            "writable" => Ok(Self::Writable),
            "read_only" => Ok(Self::ReadOnly),
            "pull_request_read_only" => Ok(Self::PullRequestReadOnly),
            other => Err(failure(
                FailureClass::Protocol,
                format!("unsupported checkout capability `{other}`"),
            )),
        }
    }
}

fn allowed_verdicts_display(allowed_verdicts: &[String]) -> String {
    if allowed_verdicts.is_empty() {
        "[]".to_string()
    } else {
        allowed_verdicts.join(", ")
    }
}

#[derive(Debug, Deserialize)]
struct AgentResult {
    #[serde(default)]
    verdict: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    review_body: Option<String>,
}
