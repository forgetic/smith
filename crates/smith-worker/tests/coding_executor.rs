use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::{Value, json};
use smith_temper_agent::WorkspaceContext;
use smith_worker::{
    CodingExecutor, CodingExecutorConfig, JobExecutor, JobOutcome, RoleGitIdentity,
};
use temper_worker_protocol::{Artifact, Assign, FailureClass, WORKER_PROTOCOL_VERSION};
use tempfile::TempDir;

#[tokio::test]
async fn success_path_commits_pushes_and_reports_branch() {
    let fixture = Fixture::new();
    let capture_path = fixture.temp.path().join("captured-context.json");
    let agent = fixture.agent(AgentBehavior::Success { capture_path });
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
        .await;

    let (branch_name, head_sha, summary) = expect_success(outcome);
    assert_eq!(branch_name, "agent/pr-for-code-7");
    assert_is_sha(&head_sha);
    assert_eq!(summary.as_deref(), Some("did the work"));

    assert_eq!(
        git_output([
            "-C",
            path_str(&fixture.origin),
            "rev-parse",
            "refs/heads/agent/pr-for-code-7",
        ]),
        head_sha
    );
    assert_eq!(
        git_output([
            "-C",
            path_str(&fixture.origin),
            "log",
            "-1",
            "--format=%s",
            "refs/heads/agent/pr-for-code-7",
        ]),
        "Implement pr-for-code-7"
    );
    assert_eq!(
        git_output([
            "-C",
            path_str(&fixture.origin),
            "log",
            "-1",
            "--format=%an <%ae>|%cn <%ce>",
            "refs/heads/agent/pr-for-code-7",
        ]),
        "Smith Engineer <smith-engineer@example.test>|Smith Engineer <smith-engineer@example.test>"
    );
    assert_eq!(
        git_output([
            "-C",
            path_str(&fixture.origin),
            "show",
            "refs/heads/agent/pr-for-code-7:agent-output.txt",
        ]),
        "agent diff"
    );
}

#[tokio::test]
async fn context_shape_matches_temper_coding_agent_contract() {
    let fixture = Fixture::new();
    let capture_path = fixture.temp.path().join("captured-context.json");
    let agent = fixture.agent(AgentBehavior::Success {
        capture_path: capture_path.clone(),
    });
    let executor = fixture.executor(&agent, true);

    expect_success(
        executor
            .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
            .await,
    );

    let context: WorkspaceContext = serde_json::from_slice(
        &fs::read(&capture_path).expect("fake agent captured the context file"),
    )
    .expect("captured context parses as smith-temper-agent WorkspaceContext");
    assert_workspace_context(
        &context,
        "engineer",
        "code_ready",
        "code",
        "writable",
        &[],
        "agent/pr-for-code-7",
        "pr-for-code-7",
    );
}

#[tokio::test]
async fn context_shape_passes_through_read_only_capability_and_verdicts() {
    let fixture = Fixture::new();
    let capture_path = fixture.temp.path().join("captured-read-only-context.json");
    let agent = fixture.agent(AgentBehavior::ReadOnlyVerdict {
        capture_path: Some(capture_path.clone()),
    });
    let executor = fixture.executor(&agent, true);

    expect_verdict(
        executor
            .execute(assign_with_context(
                "triage-7",
                read_only_job_context("agent/triage-7", "triage-7"),
            ))
            .await,
    );

    let context: WorkspaceContext = serde_json::from_slice(
        &fs::read(&capture_path).expect("fake agent captured the context file"),
    )
    .expect("captured context parses as smith-temper-agent WorkspaceContext");
    assert_workspace_context(
        &context,
        "architect",
        "design_review",
        "triage",
        "read_only",
        &["ready_code", "needs_design"],
        "agent/triage-7",
        "triage-7",
    );
}

#[allow(clippy::too_many_arguments)]
fn assert_workspace_context(
    context: &WorkspaceContext,
    role: &str,
    queue: &str,
    kind: &str,
    checkout: &str,
    allowed_verdicts: &[&str],
    branch_hint: &str,
    correlation_key: &str,
) {
    assert_eq!(context.repository.id, "acme/service");
    assert_eq!(context.repository.owner, "acme");
    assert_eq!(context.repository.name, "service");
    assert_eq!(context.repository.default_branch, "main");
    assert_eq!(context.work_item.role, role);
    assert_eq!(context.work_item.queue, queue);
    assert_eq!(context.work_item.kind, kind);
    assert_eq!(context.work_item.target, "Issue { number: ItemNumber(7) }");
    assert_eq!(context.base_branch, "main");
    assert_eq!(context.branch_hint, branch_hint);
    assert_eq!(context.correlation_key, correlation_key);
    assert_eq!(context.checkout.as_deref(), Some(checkout));
    assert_eq!(
        context.allowed_verdicts,
        allowed_verdicts
            .iter()
            .map(|verdict| (*verdict).to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(context.guidance.role_guidance, None);
    assert_eq!(context.guidance.tool_guidance, None);
    assert!(context.guidance.tool_constraints.is_empty());

    let inner: Value =
        serde_json::from_str(&context.work_item.context).expect("inner work item JSON parses");
    assert_eq!(inner["repository"], "acme/service");
    assert_eq!(inner["role"], role);
    assert_eq!(inner["queue"], queue);
    assert_eq!(inner["kind"], kind);
    assert_eq!(inner["artifact"]["type"], "issue");
    assert_eq!(inner["artifact"]["number"], 7);
    assert_eq!(inner["artifact"]["title"], "Implement the thing");
    assert_eq!(inner["artifact"]["body"], "Detailed issue body");
    assert_eq!(inner["artifact"]["labels"], json!(["code", "ready"]));
    assert_eq!(inner["artifact"]["state"], "Open");
}

#[tokio::test]
async fn workspace_is_reused_across_successful_jobs_for_same_repo_and_role() {
    let fixture = Fixture::new();
    let first_capture = fixture.temp.path().join("first-context.json");
    let first_agent = fixture.agent(AgentBehavior::Success {
        capture_path: first_capture,
    });
    let executor = fixture.executor(&first_agent, true);

    expect_success(
        executor
            .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
            .await,
    );
    let workspace_path = fixture.workspace_root.join("acme__service/engineer");
    assert!(workspace_path.exists());
    let sentinel = workspace_path.join(".git/smith-sentinel");
    fs::write(&sentinel, "keep object cache").expect("write sentinel");

    let (branch_name, head_sha, _) = expect_success(
        executor
            .execute(assign("agent/pr-for-code-8", "pr-for-code-8"))
            .await,
    );

    assert_eq!(branch_name, "agent/pr-for-code-8");
    assert!(
        sentinel.exists(),
        "prepare must reuse the existing checkout"
    );
    assert_eq!(
        git_output([
            "-C",
            path_str(&fixture.origin),
            "rev-parse",
            "refs/heads/agent/pr-for-code-8",
        ]),
        head_sha
    );
}

#[tokio::test]
async fn malformed_payload_maps_to_protocol_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoDiff);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(Assign {
            job_payload: json!({"nope": true}),
            ..assign("agent/pr-for-code-7", "pr-for-code-7")
        })
        .await;

    expect_failure_class(outcome, FailureClass::Protocol);
}

#[tokio::test]
async fn missing_enriched_artifact_maps_to_protocol_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoDiff);
    let executor = fixture.executor(&agent, true);
    let mut context = job_context("agent/pr-for-code-7", "pr-for-code-7");
    context.artifact = None;

    let outcome = executor
        .execute(Assign {
            job_payload: serde_json::to_value(context).expect("JobContext serializes"),
            ..assign("agent/pr-for-code-7", "pr-for-code-7")
        })
        .await;

    let message = expect_failure_class(outcome, FailureClass::Protocol);
    assert!(
        message.contains("artifact"),
        "message should name missing field: {message}"
    );
}

#[tokio::test]
async fn missing_role_identity_maps_to_permanent_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoDiff);
    let executor = fixture.executor(&agent, false);

    let outcome = executor
        .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("worker has no git identity for role engineer"),
        "unexpected message: {message}"
    );
}

#[tokio::test]
async fn nonzero_agent_exit_maps_to_transient_failure_with_stderr() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::Exit3);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Transient);
    assert!(
        message.contains("status 3"),
        "unexpected message: {message}"
    );
    assert!(
        message.contains("fake agent failed"),
        "stderr tail missing from message: {message}"
    );
}

#[tokio::test]
async fn missing_result_file_after_zero_exit_maps_to_permanent_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoResultFile);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
        .await;

    expect_failure_class(outcome, FailureClass::Permanent);
}

#[tokio::test]
async fn zero_diff_maps_to_permanent_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoDiff);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("agent produced no diff"),
        "unexpected message: {message}"
    );
}

#[tokio::test]
async fn verdict_result_maps_to_permanent_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::Verdict);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign("agent/pr-for-code-7", "pr-for-code-7"))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("needs_design"),
        "message should name the unsupported verdict: {message}"
    );
}

#[tokio::test]
async fn read_only_job_returns_verdict_and_body() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReadOnlyVerdict { capture_path: None });
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary) = expect_verdict(
        executor
            .execute(assign_with_context(
                "triage-7",
                read_only_job_context("agent/triage-7", "triage-7"),
            ))
            .await,
    );

    assert_eq!(verdict, "ready_code");
    assert_eq!(body.as_deref(), Some("rewritten"));
    assert_eq!(summary.as_deref(), Some("did triage"));
    assert_no_origin_branch(&fixture, "agent/triage-7");
}

#[tokio::test]
async fn read_only_job_with_diff_still_returns_verdict_without_push() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReadOnlyVerdictWithDiff);
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary) = expect_verdict(
        executor
            .execute(assign_with_context(
                "triage-with-diff-7",
                read_only_job_context("agent/triage-with-diff-7", "triage-with-diff-7"),
            ))
            .await,
    );

    assert_eq!(verdict, "ready_code");
    assert_eq!(body.as_deref(), Some("rewritten"));
    assert_eq!(summary.as_deref(), Some("did triage"));
    assert_no_origin_branch(&fixture, "agent/triage-with-diff-7");
    assert_workspace_clean(&fixture, "architect");
}

#[tokio::test]
async fn read_only_job_without_verdict_is_permanent() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoDiff);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign_with_context(
            "triage-no-verdict-7",
            read_only_job_context("agent/triage-no-verdict-7", "triage-no-verdict-7"),
        ))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("read-only job returned no verdict"),
        "unexpected message: {message}"
    );
    assert_no_origin_branch(&fixture, "agent/triage-no-verdict-7");
}

#[tokio::test]
async fn read_only_job_with_undeclared_verdict_is_permanent() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::UndeclaredVerdict);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign_with_context(
            "triage-undeclared-7",
            read_only_job_context("agent/triage-undeclared-7", "triage-undeclared-7"),
        ))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("needs_breakdown"),
        "message should name the emitted verdict: {message}"
    );
    assert!(
        message.contains("ready_code") && message.contains("needs_design"),
        "message should name the allowed vocabulary: {message}"
    );
    assert_no_origin_branch(&fixture, "agent/triage-undeclared-7");
}

#[tokio::test]
async fn writable_job_with_allowed_escalation_verdict_returns_verdict() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::WritableVerdict);
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary) = expect_verdict(
        executor
            .execute(assign_with_context(
                "pr-for-code-7",
                writable_job_context_with_allowed_verdicts(
                    "agent/pr-for-code-7",
                    "pr-for-code-7",
                    &["needs_architect"],
                ),
            ))
            .await,
    );

    assert_eq!(verdict, "needs_architect");
    assert_eq!(body.as_deref(), Some("blocked"));
    assert_eq!(summary.as_deref(), Some("cannot proceed"));
    assert_no_origin_branch(&fixture, "agent/pr-for-code-7");
    assert_workspace_clean(&fixture, "engineer");
}

#[tokio::test]
async fn pull_request_read_only_is_permanent() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::NoDiff);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(assign_with_context(
            "review-7",
            pull_request_read_only_job_context("agent/review-7", "review-7"),
        ))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("pull-request read-only jobs are not supported yet"),
        "unexpected message: {message}"
    );
    assert_no_origin_branch(&fixture, "agent/review-7");
}

struct Fixture {
    temp: TempDir,
    origin: PathBuf,
    workspace_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temp dir");
        let git_root = temp.path().join("git");
        fs::create_dir_all(git_root.join("acme")).expect("create git root");
        let origin = git_root.join("acme/service.git");
        git(["init", "--bare", path_str(&origin)]);
        seed_origin(&origin, temp.path());

        Self {
            workspace_root: temp.path().join("workspaces"),
            temp,
            origin,
        }
    }

    fn executor(&self, agent: &Path, include_identity: bool) -> CodingExecutor {
        let mut role_identities = BTreeMap::new();
        if include_identity {
            role_identities.insert(
                "engineer".to_string(),
                RoleGitIdentity {
                    user: "Smith Engineer".to_string(),
                    email: "smith-engineer@example.test".to_string(),
                    token: "test-token".to_string(),
                },
            );
            role_identities.insert(
                "architect".to_string(),
                RoleGitIdentity {
                    user: "Smith Architect".to_string(),
                    email: "smith-architect@example.test".to_string(),
                    token: "test-token".to_string(),
                },
            );
        }

        CodingExecutor::new(CodingExecutorConfig {
            workspace_root: self.workspace_root.clone(),
            git_base_url: format!("file://{}/git", path_str(self.temp.path())),
            agent_command: vec![path_str(agent).to_string()],
            role_identities,
        })
    }

    fn agent(&self, behavior: AgentBehavior) -> PathBuf {
        let path = self
            .temp
            .path()
            .join(format!("agent-{}.sh", behavior.name()));
        fs::write(&path, behavior.script()).expect("write fake agent script");
        let mut permissions = fs::metadata(&path)
            .expect("fake agent metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("make fake agent executable");
        path
    }
}

enum AgentBehavior {
    Success { capture_path: PathBuf },
    Exit3,
    NoResultFile,
    NoDiff,
    Verdict,
    ReadOnlyVerdict { capture_path: Option<PathBuf> },
    ReadOnlyVerdictWithDiff,
    UndeclaredVerdict,
    WritableVerdict,
}

impl AgentBehavior {
    fn name(&self) -> &'static str {
        match self {
            AgentBehavior::Success { .. } => "success",
            AgentBehavior::Exit3 => "exit-3",
            AgentBehavior::NoResultFile => "no-result",
            AgentBehavior::NoDiff => "no-diff",
            AgentBehavior::Verdict => "verdict",
            AgentBehavior::ReadOnlyVerdict { .. } => "read-only-verdict",
            AgentBehavior::ReadOnlyVerdictWithDiff => "read-only-verdict-with-diff",
            AgentBehavior::UndeclaredVerdict => "undeclared-verdict",
            AgentBehavior::WritableVerdict => "writable-verdict",
        }
    }

    fn script(&self) -> String {
        match self {
            AgentBehavior::Success { capture_path } => format!(
                "#!/bin/sh\nset -eu\ncp \"$TEMPER_CODING_WORKSPACE_CONTEXT\" {}\nprintf 'agent diff\\n' > agent-output.txt\nprintf '{{\"summary\":\"did the work\"}}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n",
                shell_quote(path_str(capture_path))
            ),
            AgentBehavior::Exit3 => {
                "#!/bin/sh\necho 'fake agent failed' >&2\nexit 3\n".to_string()
            }
            AgentBehavior::NoResultFile => "#!/bin/sh\nexit 0\n".to_string(),
            AgentBehavior::NoDiff => {
                "#!/bin/sh\nprintf '{\"summary\":\"nothing changed\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::Verdict => {
                "#!/bin/sh\nprintf '{\"verdict\":\"needs_design\",\"summary\":\"cannot proceed\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::ReadOnlyVerdict { capture_path } => {
                read_only_verdict_script(capture_path.as_deref(), false)
            }
            AgentBehavior::ReadOnlyVerdictWithDiff => read_only_verdict_script(None, true),
            AgentBehavior::UndeclaredVerdict => {
                "#!/bin/sh\nprintf '{\"verdict\":\"needs_breakdown\",\"summary\":\"needs splitting\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::WritableVerdict => {
                "#!/bin/sh\nset -eu\nprintf 'discard me\\n' > agent-output.txt\nprintf '{\"verdict\":\"needs_architect\",\"body\":\"blocked\",\"summary\":\"cannot proceed\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
        }
    }
}

fn read_only_verdict_script(capture_path: Option<&Path>, writes_diff: bool) -> String {
    let mut script = "#!/bin/sh\nset -eu\n".to_string();
    if let Some(capture_path) = capture_path {
        script.push_str(&format!(
            "cp \"$TEMPER_CODING_WORKSPACE_CONTEXT\" {}\n",
            shell_quote(path_str(capture_path))
        ));
    }
    script.push_str(
        "grep '\"checkout\": \"read_only\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n",
    );
    script.push_str("grep '\"ready_code\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n");
    script.push_str("grep '\"needs_design\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n");
    if writes_diff {
        script.push_str("printf 'discard me\\n' > agent-output.txt\n");
    }
    script.push_str("printf '{\"verdict\":\"ready_code\",\"body\":\"rewritten\",\"summary\":\"did triage\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n");
    script
}

fn assign(branch_hint: &str, correlation_key: &str) -> Assign {
    Assign {
        protocol_version: WORKER_PROTOCOL_VERSION,
        job_id: format!("acme/service/issue-7/engineer/{correlation_key}"),
        role: "engineer".to_string(),
        repo: "acme/service".to_string(),
        artifact: Artifact {
            item: json!(7),
            kind: "issue".to_string(),
        },
        job_payload: serde_json::to_value(job_context(branch_hint, correlation_key))
            .expect("JobContext serializes"),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct TestJobContext {
    role: String,
    repo: String,
    queue: String,
    artifact_kind: String,
    repository: Option<TestJobRepository>,
    base_branch: Option<String>,
    branch_hint: Option<String>,
    correlation_key: Option<String>,
    artifact: Option<TestJobArtifactSnapshot>,
    action: Option<String>,
    checkout_capability: Option<String>,
    allowed_verdicts: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct TestJobRepository {
    owner: String,
    name: String,
    default_branch: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct TestJobArtifactSnapshot {
    number: u64,
    title: String,
    body: String,
    labels: Vec<String>,
    state: String,
}

fn job_context(branch_hint: &str, correlation_key: &str) -> TestJobContext {
    TestJobContext {
        role: "engineer".to_string(),
        repo: "acme/service".to_string(),
        queue: "code_ready".to_string(),
        artifact_kind: "code".to_string(),
        repository: Some(TestJobRepository {
            owner: "acme".to_string(),
            name: "service".to_string(),
            default_branch: "main".to_string(),
        }),
        base_branch: Some("main".to_string()),
        branch_hint: Some(branch_hint.to_string()),
        correlation_key: Some(correlation_key.to_string()),
        artifact: Some(TestJobArtifactSnapshot {
            number: 7,
            title: "Implement the thing".to_string(),
            body: "Detailed issue body".to_string(),
            labels: vec!["code".to_string(), "ready".to_string()],
            state: "Open".to_string(),
        }),
        action: None,
        checkout_capability: None,
        allowed_verdicts: vec![],
    }
}

fn read_only_job_context(branch_hint: &str, correlation_key: &str) -> TestJobContext {
    let mut context = job_context(branch_hint, correlation_key);
    context.role = "architect".to_string();
    context.queue = "design_review".to_string();
    context.artifact_kind = "triage".to_string();
    context.action = Some("triage_to_code".to_string());
    context.checkout_capability = Some("read_only".to_string());
    context.allowed_verdicts = vec!["ready_code".to_string(), "needs_design".to_string()];
    context
}

fn writable_job_context_with_allowed_verdicts(
    branch_hint: &str,
    correlation_key: &str,
    allowed_verdicts: &[&str],
) -> TestJobContext {
    let mut context = job_context(branch_hint, correlation_key);
    context.action = Some("open_pr".to_string());
    context.checkout_capability = Some("writable".to_string());
    context.allowed_verdicts = allowed_verdicts
        .iter()
        .map(|verdict| (*verdict).to_string())
        .collect();
    context
}

fn pull_request_read_only_job_context(branch_hint: &str, correlation_key: &str) -> TestJobContext {
    let mut context = job_context(branch_hint, correlation_key);
    context.role = "reviewer".to_string();
    context.queue = "review_ready".to_string();
    context.artifact_kind = "review".to_string();
    context.action = Some("review_pr".to_string());
    context.checkout_capability = Some("pull_request_read_only".to_string());
    context.allowed_verdicts = vec!["approve".to_string(), "changes".to_string()];
    context
}

fn assign_with_context(correlation_key: &str, context: TestJobContext) -> Assign {
    let role = context.role.clone();
    Assign {
        protocol_version: WORKER_PROTOCOL_VERSION,
        job_id: format!("acme/service/issue-7/{role}/{correlation_key}"),
        role,
        repo: "acme/service".to_string(),
        artifact: Artifact {
            item: json!(7),
            kind: "issue".to_string(),
        },
        job_payload: serde_json::to_value(context).expect("JobContext serializes"),
    }
}

fn seed_origin(origin: &Path, temp: &Path) {
    let seed = temp.join("seed");
    git(["init", "-b", "main", path_str(&seed)]);
    fs::write(seed.join("README.md"), "# seed\n").expect("write seed file");
    git([
        "-C",
        path_str(&seed),
        "-c",
        "user.name=Seed User",
        "-c",
        "user.email=seed@example.test",
        "add",
        "README.md",
    ]);
    git([
        "-C",
        path_str(&seed),
        "-c",
        "user.name=Seed User",
        "-c",
        "user.email=seed@example.test",
        "commit",
        "-m",
        "initial commit",
    ]);
    git([
        "-C",
        path_str(&seed),
        "remote",
        "add",
        "origin",
        path_str(origin),
    ]);
    git(["-C", path_str(&seed), "push", "origin", "main"]);
}

fn expect_success(outcome: JobOutcome) -> (String, String, Option<String>) {
    match outcome {
        JobOutcome::Success { branch, summary } => (branch.name, branch.head_sha, summary),
        JobOutcome::Verdict {
            verdict,
            body,
            summary,
        } => {
            panic!("expected success, got verdict {verdict:?} {body:?} {summary:?}")
        }
        JobOutcome::Failure { class, message } => {
            panic!("expected success, got {class:?}: {message}")
        }
    }
}

fn expect_verdict(outcome: JobOutcome) -> (String, Option<String>, Option<String>) {
    match outcome {
        JobOutcome::Verdict {
            verdict,
            body,
            summary,
        } => (verdict, body, summary),
        JobOutcome::Success { branch, summary } => {
            panic!("expected verdict, got success {branch:?} {summary:?}")
        }
        JobOutcome::Failure { class, message } => {
            panic!("expected verdict, got {class:?}: {message}")
        }
    }
}

fn expect_failure_class(outcome: JobOutcome, expected: FailureClass) -> String {
    match outcome {
        JobOutcome::Failure { class, message } => {
            assert_eq!(class, expected, "unexpected failure message: {message}");
            message
        }
        JobOutcome::Success { branch, summary } => {
            panic!("expected {expected:?} failure, got success {branch:?} {summary:?}")
        }
        JobOutcome::Verdict {
            verdict,
            body,
            summary,
        } => {
            panic!("expected {expected:?} failure, got verdict {verdict:?} {body:?} {summary:?}")
        }
    }
}

fn assert_no_origin_branch(fixture: &Fixture, branch_name: &str) {
    let output = Command::new("git")
        .args([
            "-C",
            path_str(&fixture.origin),
            "show-ref",
            "--verify",
            &format!("refs/heads/{branch_name}"),
        ])
        .output()
        .expect("run git show-ref");
    assert!(
        !output.status.success(),
        "origin unexpectedly has branch {branch_name}: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn assert_workspace_clean(fixture: &Fixture, role: &str) {
    assert_eq!(
        git_output([
            "-C",
            path_str(&fixture.workspace_root.join("acme__service").join(role)),
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
        ]),
        ""
    );
}

fn git<const N: usize>(args: [&str; N]) {
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git command failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output<const N: usize>(args: [&str; N]) -> String {
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git command failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .expect("git stdout is utf-8")
        .trim_end_matches('\n')
        .to_string()
}

fn path_str(path: &Path) -> &str {
    path.as_os_str()
        .to_str()
        .unwrap_or_else(|| panic!("non-utf8 path: {:?}", path.as_os_str()))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn assert_is_sha(value: &str) {
    assert_eq!(value.len(), 40, "not a full SHA: {value}");
    assert!(
        value.chars().all(|ch| ch.is_ascii_hexdigit()),
        "not hex: {value}"
    );
}
