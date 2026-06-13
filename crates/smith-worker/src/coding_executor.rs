use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use smith_agent_protocol::{
    WorkspaceContext, WorkspaceGuidance, WorkspaceRepository, WorkspaceResult, WorkspaceWorkItem,
};
use temper_worker_protocol::{
    Assign, Branch, FailureClass, JobArtifactSnapshot, JobChild, JobContext, RepoAccess,
    RepoOutcome, WorkspaceManifest,
};

use crate::agent_runner::{AgentRunError, AgentRunner, ProgressSink};
use crate::executor::{JobExecutor, JobOutcome};
use crate::workspace::{
    RoleGitIdentity, Workspace, WorkspaceError, forgejo_remote_url,
};

/// Configuration for the real coding-job executor.
///
/// The agent turn itself is produced by an [`AgentRunner`] passed alongside this
/// config (in-process `pi`-SDK by default; an external command or a test fake in
/// other contexts), so this struct only carries the workspace/git/identity
/// surface the executor owns.
#[derive(Clone, Debug)]
pub struct CodingExecutorConfig {
    /// Root under which coordinated workspaces are assembled. Each job gets a
    /// `<workspace_root>/<coordination_key>/<role>` directory holding the
    /// manifest's repos as siblings (ADR 0023).
    pub workspace_root: PathBuf,
    /// Forge git base URL, e.g. `http://localhost:3000` (joined with the
    /// repo slug via `forgejo_remote_url`; `file://` URLs work for tests).
    pub git_base_url: String,
    /// Role id -> git identity (user, email, push token).
    pub role_identities: BTreeMap<String, RoleGitIdentity>,
}

/// Runs coding/triage/review jobs by assembling the manifest's repos into one
/// workspace, driving one agent turn over the combined root through its
/// [`AgentRunner`], and mapping the result to a [`JobOutcome`] (one
/// commit/push/PR per changed writable repo on the head path, verdict routing
/// otherwise).
#[derive(Clone)]
pub struct CodingExecutor<R: AgentRunner> {
    config: CodingExecutorConfig,
    runner: Arc<R>,
    /// Where agent step-progress checkpoints are relayed (logging by default;
    /// the worker→daemon→forge relay plugs in here later).
    progress: Arc<dyn ProgressSink>,
}

impl<R: AgentRunner> CodingExecutor<R> {
    pub fn new(config: CodingExecutorConfig, runner: Arc<R>) -> Self {
        Self {
            config,
            runner,
            progress: Arc::new(crate::agent_runner::LoggingProgressSink),
        }
    }

    /// Overrides the step-progress sink (e.g. a daemon-relay sink, or a test
    /// recorder).
    pub fn with_progress_sink(mut self, progress: Arc<dyn ProgressSink>) -> Self {
        self.progress = progress;
        self
    }
}

impl<R: AgentRunner + 'static> JobExecutor for CodingExecutor<R> {
    fn execute(&self, assign: Assign) -> impl std::future::Future<Output = JobOutcome> + Send {
        let config = self.config.clone();
        let runner = Arc::clone(&self.runner);
        let progress = Arc::clone(&self.progress);
        async move { execute(config, runner, progress, assign).await }
    }
}

/// One prepared sibling checkout plus its manifest entry.
struct PreparedRepo {
    repo: String,
    writable: bool,
    branch_hint: Option<String>,
    workspace: Workspace,
}

async fn execute<R: AgentRunner>(
    config: CodingExecutorConfig,
    runner: Arc<R>,
    progress: Arc<dyn ProgressSink>,
    assign: Assign,
) -> JobOutcome {
    let artifact_item = assign.artifact.item.clone();
    let artifact_wire_kind = assign.artifact.kind.clone();
    let context: JobContext = match serde_json::from_value(assign.job_payload) {
        Ok(context) => context,
        Err(error) => {
            return failure(
                FailureClass::Protocol,
                format!("invalid enriched job payload: {error}"),
            );
        }
    };

    let JobContext {
        role,
        repo: _primary_repo,
        queue,
        artifact_kind,
        artifact,
        workspace,
        action: _action,
        checkout_capability,
        allowed_verdicts,
    } = context;

    let artifact = match require_enriched_field(artifact, "artifact") {
        Ok(artifact) => artifact,
        Err(outcome) => return outcome,
    };
    let manifest = match require_enriched_field(workspace, "workspace") {
        Ok(manifest) => manifest,
        Err(outcome) => return outcome,
    };
    if manifest.repos.is_empty() {
        return failure(
            FailureClass::Protocol,
            "workspace manifest declared no repositories",
        );
    }
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

    let coordination_key = manifest.coordination_key.clone();
    // Coordinated workspace root: the manifest repos are checked out as flat
    // siblings under here (one dir each) so their inter-repo path dependencies
    // resolve. Keyed by role so the per-repo checkouts (and their git object
    // caches) persist across this role's jobs; with single-job-per-worker
    // capacity, coordinated jobs never share the root concurrently.
    let workspace_root = config.workspace_root.join(&role);

    // Prepare each manifest repo into its sibling dir. The PR-review path only
    // needs the primary repo (at its PR head); coding/triage prepare them all.
    let mut prepared: Vec<PreparedRepo> = Vec::new();
    for repo_spec in &manifest.repos {
        let remote_url = match forgejo_remote_url(&config.git_base_url, &repo_spec.repo) {
            Ok(remote_url) => remote_url,
            Err(error) => return workspace_failure("construct git remote URL", error, ""),
        };
        let base_branch = if repo_spec.base_branch.trim().is_empty() {
            "main".to_string()
        } else {
            repo_spec.base_branch.clone()
        };
        let checkout_path = workspace_root.join(&repo_spec.dir);
        let workspace =
            Workspace::at(checkout_path, base_branch, identity.clone(), remote_url);

        let writable = repo_spec.is_writable();
        let prepare_result = match mode {
            JobMode::PullRequestReadOnly => {
                let branch_hint = repo_spec
                    .branch_hint
                    .clone()
                    .unwrap_or_else(|| format!("agent/{coordination_key}"));
                workspace
                    .prepare_pull_request_head(artifact.number, &branch_hint)
                    .await
            }
            JobMode::Writable if writable => {
                let Some(branch_hint) = repo_spec.branch_hint.clone() else {
                    return failure(
                        FailureClass::Protocol,
                        format!(
                            "writable workspace repo {} is missing a branch hint",
                            repo_spec.repo
                        ),
                    );
                };
                workspace.prepare(&branch_hint).await
            }
            // Read-only sibling in a writable job, or any repo in a read-only
            // (triage) job: materialize the base branch, never push.
            JobMode::Writable | JobMode::ReadOnly => workspace.prepare_read_only().await,
        };
        if let Err(error) = prepare_result {
            return workspace_failure("prepare workspace", error, &token);
        }

        prepared.push(PreparedRepo {
            repo: repo_spec.repo.clone(),
            writable,
            branch_hint: repo_spec.branch_hint.clone(),
            workspace,
        });

        // Review acts on the single PR head; don't assemble siblings for it.
        if mode == JobMode::PullRequestReadOnly {
            break;
        }
    }

    let workspace_context = build_workspace_context(
        &role,
        &queue,
        &artifact_kind,
        &manifest,
        &artifact,
        &artifact_wire_kind,
        &checkout,
        &allowed_verdicts,
    );

    // Run one agent turn out-of-process with the cwd set to the workspace root
    // (not a single repo), so the agent can read and build every sibling. The
    // runner owns the agent mechanism and streams step-progress to the sink.
    let result = match runner
        .run(&workspace_context, &workspace_root, progress.as_ref())
        .await
    {
        Ok(result) => result,
        Err(AgentRunError { class, message }) => {
            return failure(class, redact_secret(message, &token));
        }
    };

    match mode {
        JobMode::Writable => {
            // A mid-work escalation verdict (e.g. needs_architect) discards all
            // edits and routes like a read-only verdict.
            if let Some(verdict) = result.verdict {
                if allowed_verdicts.contains(&verdict) {
                    for prepared in &prepared {
                        if let Err(error) = prepared.workspace.discard_changes().await {
                            return workspace_failure(
                                "discard verdict workspace changes",
                                error,
                                &token,
                            );
                        }
                    }
                    let summary = result
                        .summary
                        .or_else(|| Some(format!("implemented {coordination_key}")));
                    return JobOutcome::Verdict {
                        verdict,
                        body: result.body.or(result.review_body),
                        summary,
                        children: Vec::new(),
                    };
                }
                return failure(
                    FailureClass::Permanent,
                    format!("verdict routing not supported by smith-worker yet: {verdict}"),
                );
            }

            // One commit/push per writable repo that produced a diff → one PR.
            let mut outcomes = Vec::new();
            for (index, prepared) in prepared.iter().enumerate() {
                if !prepared.writable {
                    continue;
                }
                let branch = prepared
                    .branch_hint
                    .clone()
                    .expect("writable repo carries a branch hint (checked at prepare)");

                let has_tree_changes = match prepared.workspace.has_changes().await {
                    Ok(changed) => changed,
                    Err(error) => {
                        return workspace_failure("inspect workspace changes", error, &token);
                    }
                };
                let produced_diff = if has_tree_changes {
                    true
                } else {
                    match prepared.workspace.commits_ahead_of_base().await {
                        Ok(ahead) => ahead,
                        Err(error) => {
                            return workspace_failure("inspect workspace commits", error, &token);
                        }
                    }
                };
                if !produced_diff {
                    continue;
                }

                if has_tree_changes {
                    let message = commit_message(&coordination_key, &artifact_item, index == 0);
                    if let Err(error) = prepared.workspace.commit_all(&message).await {
                        return workspace_failure("commit workspace changes", error, &token);
                    }
                }
                let head_sha = match prepared.workspace.push_branch(&branch).await {
                    Ok(head_sha) => head_sha,
                    Err(error) => return workspace_failure("push workspace branch", error, &token),
                };
                outcomes.push(RepoOutcome {
                    repo: prepared.repo.clone(),
                    branch: Branch {
                        name: branch,
                        head_sha,
                    },
                });
            }

            if outcomes.is_empty() {
                return failure(
                    FailureClass::Permanent,
                    "agent produced no diff in any writable repo",
                );
            }

            JobOutcome::Success {
                repos: outcomes,
                summary: result
                    .summary
                    .or_else(|| Some(format!("implemented {coordination_key}"))),
            }
        }
        JobMode::ReadOnly | JobMode::PullRequestReadOnly => {
            verdict_only_outcome(
                &prepared[0].workspace,
                result,
                &allowed_verdicts,
                &coordination_key,
                &token,
            )
            .await
        }
    }
}

async fn verdict_only_outcome(
    workspace: &Workspace,
    result: WorkspaceResult,
    allowed_verdicts: &[String],
    correlation_key: &str,
    token: &str,
) -> JobOutcome {
    let WorkspaceResult {
        verdict,
        summary,
        body,
        review_body,
        children,
        // `labels` is a head-path PR-label override; read-only verdict routing
        // does not consume it.
        labels: _labels,
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

    let children = children
        .into_iter()
        .map(|child| JobChild {
            slug: child.slug,
            title: child.title,
            body: child.body,
            labels: child.labels,
            depends_on: child.depends_on,
            target_repo: child.target_repo,
        })
        .collect();

    JobOutcome::Verdict {
        verdict,
        body: body.or(review_body),
        summary: summary.or_else(|| Some(format!("implemented {correlation_key}"))),
        children,
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
/// The primary repo's commit gains a `Closes #<n>` trailer so the forge's
/// native close-on-merge retires the coordinating issue when that PR lands.
/// Secondary repos cannot close a cross-repo issue, so they omit the trailer.
fn commit_message(
    coordination_key: &str,
    artifact_item: &serde_json::Value,
    is_primary: bool,
) -> String {
    match (is_primary, artifact_item.as_u64()) {
        (true, Some(number)) => format!("Implement {coordination_key}\n\nCloses #{number}"),
        _ => format!("Implement {coordination_key}"),
    }
}

fn failure(class: FailureClass, message: impl Into<String>) -> JobOutcome {
    JobOutcome::Failure {
        class,
        message: message.into(),
    }
}

/// Assembles the typed [`WorkspaceContext`] the agent turn receives, listing
/// every manifest repo with its sibling dir and access. The
/// `OutOfProcessRunner` serializes this to the JSON document the agent reads
/// from `$TEMPER_CODING_WORKSPACE_CONTEXT`.
#[allow(clippy::too_many_arguments)]
fn build_workspace_context(
    role: &str,
    queue: &str,
    artifact_kind: &str,
    manifest: &WorkspaceManifest,
    artifact: &JobArtifactSnapshot,
    artifact_wire_kind: &str,
    checkout: &str,
    allowed_verdicts: &[String],
) -> WorkspaceContext {
    let (artifact_type, target_kind) = match artifact_wire_kind {
        "pull_request" => ("pull_request", "PullRequest"),
        _ => ("issue", "Issue"),
    };
    let primary_repo = manifest
        .repos
        .first()
        .map(|repo| repo.repo.clone())
        .unwrap_or_default();
    let work_item_context = serde_json::json!({
        "repository": primary_repo,
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
    let work_item_context = serde_json::to_string_pretty(&work_item_context)
        .unwrap_or_else(|_| work_item_context.to_string());

    let repos = manifest
        .repos
        .iter()
        .map(|repo| {
            let (owner, name) = repo.owner_name().unwrap_or(("", ""));
            WorkspaceRepository {
                id: repo.repo.clone(),
                owner: owner.to_string(),
                name: name.to_string(),
                default_branch: repo.default_branch.clone(),
                dir: repo.dir.clone(),
                access: match repo.access {
                    RepoAccess::Writable => "writable",
                    RepoAccess::ReadOnly => "read_only",
                }
                .to_string(),
                base_branch: repo.base_branch.clone(),
                branch_hint: repo.branch_hint.clone(),
            }
        })
        .collect();

    WorkspaceContext {
        repos,
        work_item: WorkspaceWorkItem {
            role: role.to_string(),
            queue: queue.to_string(),
            kind: artifact_kind.to_string(),
            target: format!(
                "{target_kind} {{ number: ItemNumber({}) }}",
                artifact.number
            ),
            context: work_item_context,
        },
        correlation_key: manifest.coordination_key.clone(),
        checkout: Some(checkout.to_string()),
        allowed_verdicts: allowed_verdicts.to_vec(),
        guidance: WorkspaceGuidance::default(),
    }
}

fn redact_secret(text: String, secret: &str) -> String {
    if secret.is_empty() {
        text
    } else {
        text.replace(secret, "<redacted>")
    }
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
