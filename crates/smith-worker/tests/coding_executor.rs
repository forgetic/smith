use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::{Value, json};
use smith_temper_agent::{WorkspaceContext, WorkspaceResult, WorkspaceResultChild};
use smith_worker::{
    CodingExecutor, CodingExecutorConfig, JobExecutor, JobOutcome, RoleGitIdentity,
};
use temper_worker_protocol::{Artifact, Assign, FailureClass, JobChild, WORKER_PROTOCOL_VERSION};
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
            "--format=%b",
            "refs/heads/agent/pr-for-code-7",
        ]),
        "Closes #7"
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
        ExpectedWorkspaceContext {
            role: "engineer",
            queue: "code_ready",
            kind: "code",
            checkout: "writable",
            allowed_verdicts: &[],
            branch_hint: "agent/pr-for-code-7",
            correlation_key: "pr-for-code-7",
            target: "Issue { number: ItemNumber(7) }",
            artifact_type: "issue",
        },
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
        ExpectedWorkspaceContext {
            role: "architect",
            queue: "design_review",
            kind: "triage",
            checkout: "read_only",
            allowed_verdicts: &["ready_code", "needs_design"],
            branch_hint: "agent/triage-7",
            correlation_key: "triage-7",
            target: "Issue { number: ItemNumber(7) }",
            artifact_type: "issue",
        },
    );
}

#[tokio::test]
async fn review_context_shape_carries_pull_request_target() {
    let fixture = Fixture::new();
    let capture_path = fixture.temp.path().join("captured-review-context.json");
    let agent = fixture.agent(AgentBehavior::ReviewApprove {
        head_capture_path: fixture.pull_request_head_path(),
        capture_path: Some(capture_path.clone()),
    });
    let executor = fixture.executor(&agent, true);

    expect_verdict(
        executor
            .execute(pr_assign("agent/review-7", "review-7", pr_job_context))
            .await,
    );

    let context: WorkspaceContext = serde_json::from_slice(
        &fs::read(&capture_path).expect("fake agent captured the context file"),
    )
    .expect("captured context parses as smith-temper-agent WorkspaceContext");
    assert_workspace_context(
        &context,
        ExpectedWorkspaceContext {
            role: "reviewer",
            queue: "pr_needs_review",
            kind: "implementation_pr",
            checkout: "pull_request_read_only",
            allowed_verdicts: &["approve", "changes", "escalate"],
            branch_hint: "agent/review-7",
            correlation_key: "review-7",
            target: "PullRequest { number: ItemNumber(7) }",
            artifact_type: "pull_request",
        },
    );
}

struct ExpectedWorkspaceContext<'a> {
    role: &'a str,
    queue: &'a str,
    kind: &'a str,
    checkout: &'a str,
    allowed_verdicts: &'a [&'a str],
    branch_hint: &'a str,
    correlation_key: &'a str,
    target: &'a str,
    artifact_type: &'a str,
}

fn assert_workspace_context(context: &WorkspaceContext, expected: ExpectedWorkspaceContext<'_>) {
    assert_eq!(context.repository.id, "acme/service");
    assert_eq!(context.repository.owner, "acme");
    assert_eq!(context.repository.name, "service");
    assert_eq!(context.repository.default_branch, "main");
    assert_eq!(context.work_item.role, expected.role);
    assert_eq!(context.work_item.queue, expected.queue);
    assert_eq!(context.work_item.kind, expected.kind);
    assert_eq!(context.work_item.target, expected.target);
    assert_eq!(context.base_branch, "main");
    assert_eq!(context.branch_hint, expected.branch_hint);
    assert_eq!(context.correlation_key, expected.correlation_key);
    assert_eq!(context.checkout.as_deref(), Some(expected.checkout));
    assert_eq!(
        context.allowed_verdicts,
        expected
            .allowed_verdicts
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
    assert_eq!(inner["role"], expected.role);
    assert_eq!(inner["queue"], expected.queue);
    assert_eq!(inner["kind"], expected.kind);
    assert_eq!(inner["artifact"]["type"], expected.artifact_type);
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

    let (verdict, body, summary, children) = expect_verdict(
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
    assert!(children.is_empty());
    assert_no_origin_branch(&fixture, "agent/triage-7");
}

#[tokio::test]
async fn read_only_job_with_diff_still_returns_verdict_without_push() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReadOnlyVerdictWithDiff);
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary, children) = expect_verdict(
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
    assert!(children.is_empty());
    assert_no_origin_branch(&fixture, "agent/triage-with-diff-7");
    assert_workspace_clean(&fixture, "architect");
}

#[tokio::test]
async fn read_only_breakdown_verdict_passes_children_through() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReadOnlyBreakdownVerdict);
    let executor = fixture.executor(&agent, true);
    let mut context = read_only_job_context("agent/breakdown-7", "breakdown-7");
    context.allowed_verdicts = vec!["needs_breakdown".to_string()];

    let (verdict, body, summary, children) = expect_verdict(
        executor
            .execute(assign_with_context("breakdown-7", context))
            .await,
    );

    assert_eq!(verdict, "needs_breakdown");
    assert_eq!(body, None);
    assert_eq!(summary.as_deref(), Some("planned breakdown"));
    assert_eq!(
        children,
        vec![
            JobChild {
                slug: "api-schema".to_string(),
                title: "Define the API schema".to_string(),
                body: "Write the shared API schema.".to_string(),
                labels: vec!["code".to_string(), "ready".to_string()],
                depends_on: Vec::new(),
                target_repo: None,
            },
            JobChild {
                slug: "web-client".to_string(),
                title: "Implement the web client".to_string(),
                body: "Build the web client against the API schema.".to_string(),
                labels: Vec::new(),
                depends_on: vec!["api-schema".to_string()],
                target_repo: Some("acme/other".to_string()),
            },
        ]
    );
    assert_no_origin_branch(&fixture, "agent/breakdown-7");
}

#[test]
fn worker_agent_result_parses_temper_agent_children_contract() {
    let result = WorkspaceResult {
        verdict: Some("needs_breakdown".to_string()),
        summary: Some("planned breakdown".to_string()),
        children: vec![
            WorkspaceResultChild {
                slug: "api-schema".to_string(),
                title: "Define the API schema".to_string(),
                body: "Write the shared API schema.".to_string(),
                labels: vec!["code".to_string(), "ready".to_string()],
                depends_on: Vec::new(),
                target_repo: None,
            },
            WorkspaceResultChild {
                slug: "web-client".to_string(),
                title: "Implement the web client".to_string(),
                body: "Build the web client against the API schema.".to_string(),
                labels: Vec::new(),
                depends_on: vec!["api-schema".to_string()],
                target_repo: Some("acme/other".to_string()),
            },
        ],
        ..WorkspaceResult::default()
    };

    let parsed: smith_worker::coding_executor::AgentResult =
        serde_json::from_value(serde_json::to_value(&result).expect("agent result serializes"))
            .expect("worker parses agent result");

    assert_eq!(parsed.verdict.as_deref(), Some("needs_breakdown"));
    assert_eq!(parsed.summary.as_deref(), Some("planned breakdown"));
    assert_eq!(parsed.children.len(), 2);
    assert_eq!(parsed.children[0].slug, "api-schema");
    assert_eq!(parsed.children[0].title, "Define the API schema");
    assert_eq!(parsed.children[0].body, "Write the shared API schema.");
    assert_eq!(
        parsed.children[0].labels,
        vec!["code".to_string(), "ready".to_string()]
    );
    assert!(parsed.children[0].depends_on.is_empty());
    assert_eq!(parsed.children[0].target_repo, None);
    assert_eq!(parsed.children[1].slug, "web-client");
    assert_eq!(parsed.children[1].title, "Implement the web client");
    assert_eq!(
        parsed.children[1].body,
        "Build the web client against the API schema."
    );
    assert!(parsed.children[1].labels.is_empty());
    assert_eq!(
        parsed.children[1].depends_on,
        vec!["api-schema".to_string()]
    );
    assert_eq!(
        parsed.children[1].target_repo.as_deref(),
        Some("acme/other")
    );
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
async fn review_job_checks_out_pr_head_and_returns_approve_verdict() {
    let fixture = Fixture::new();
    let head_capture_path = fixture.pull_request_head_path();
    let agent = fixture.agent(AgentBehavior::ReviewApprove {
        head_capture_path: head_capture_path.clone(),
        capture_path: None,
    });
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary, children) = expect_verdict(
        executor
            .execute(pr_assign("agent/review-7", "review-7", pr_job_context))
            .await,
    );

    assert_eq!(verdict, "approve");
    assert_eq!(body, None);
    assert_eq!(summary.as_deref(), Some("looks good"));
    assert!(children.is_empty());
    assert_eq!(
        fs::read_to_string(&head_capture_path)
            .expect("fake agent captured HEAD")
            .trim(),
        fixture.pull_request_head_sha
    );
    assert_no_origin_branch(&fixture, "agent/review-7");
    assert_no_extra_origin_head_branches(&fixture, &["main"]);
}

#[tokio::test]
async fn review_job_changes_verdict_passes_review_body_through() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReviewChanges);
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary, children) = expect_verdict(
        executor
            .execute(pr_assign(
                "agent/review-changes-7",
                "review-changes-7",
                pr_job_context,
            ))
            .await,
    );

    assert_eq!(verdict, "changes");
    assert_eq!(body.as_deref(), Some("please add error handling"));
    assert_eq!(summary.as_deref(), Some("needs error handling"));
    assert!(children.is_empty());
    assert_no_origin_branch(&fixture, "agent/review-changes-7");
    assert_no_extra_origin_head_branches(&fixture, &["main"]);
    assert_workspace_clean(&fixture, "reviewer");
}

#[tokio::test]
async fn review_job_missing_verdict_is_permanent_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReviewMissingVerdict);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(pr_assign(
            "agent/review-missing-verdict-7",
            "review-missing-verdict-7",
            pr_job_context,
        ))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("read-only job returned no verdict"),
        "unexpected message: {message}"
    );
    assert_no_origin_branch(&fixture, "agent/review-missing-verdict-7");
    assert_no_extra_origin_head_branches(&fixture, &["main"]);
}

#[tokio::test]
async fn review_job_undeclared_verdict_is_permanent_failure() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::ReviewUndeclaredVerdict);
    let executor = fixture.executor(&agent, true);

    let outcome = executor
        .execute(pr_assign(
            "agent/review-undeclared-7",
            "review-undeclared-7",
            pr_job_context,
        ))
        .await;

    let message = expect_failure_class(outcome, FailureClass::Permanent);
    assert!(
        message.contains("merge_now"),
        "message should name the emitted verdict: {message}"
    );
    assert!(
        message.contains("approve") && message.contains("changes") && message.contains("escalate"),
        "message should name the allowed vocabulary: {message}"
    );
    assert_no_origin_branch(&fixture, "agent/review-undeclared-7");
    assert_no_extra_origin_head_branches(&fixture, &["main"]);
}

#[tokio::test]
async fn writable_job_with_allowed_escalation_verdict_returns_verdict() {
    let fixture = Fixture::new();
    let agent = fixture.agent(AgentBehavior::WritableVerdict);
    let executor = fixture.executor(&agent, true);

    let (verdict, body, summary, children) = expect_verdict(
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
    assert!(children.is_empty());
    assert_no_origin_branch(&fixture, "agent/pr-for-code-7");
    assert_workspace_clean(&fixture, "engineer");
}

struct Fixture {
    temp: TempDir,
    origin: PathBuf,
    workspace_root: PathBuf,
    pull_request_head_sha: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temp dir");
        let git_root = temp.path().join("git");
        fs::create_dir_all(git_root.join("acme")).expect("create git root");
        let origin = git_root.join("acme/service.git");
        git(["init", "--bare", path_str(&origin)]);
        let pull_request_head_sha = seed_origin(&origin, temp.path());

        Self {
            workspace_root: temp.path().join("workspaces"),
            temp,
            origin,
            pull_request_head_sha,
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
            role_identities.insert(
                "reviewer".to_string(),
                RoleGitIdentity {
                    user: "Smith Reviewer".to_string(),
                    email: "smith-reviewer@example.test".to_string(),
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

    fn pull_request_head_path(&self) -> PathBuf {
        self.temp.path().join("pull-request-head")
    }
}

enum AgentBehavior {
    Success {
        capture_path: PathBuf,
    },
    Exit3,
    NoResultFile,
    NoDiff,
    Verdict,
    ReadOnlyVerdict {
        capture_path: Option<PathBuf>,
    },
    ReadOnlyBreakdownVerdict,
    ReadOnlyVerdictWithDiff,
    UndeclaredVerdict,
    WritableVerdict,
    ReviewApprove {
        head_capture_path: PathBuf,
        capture_path: Option<PathBuf>,
    },
    ReviewChanges,
    ReviewMissingVerdict,
    ReviewUndeclaredVerdict,
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
            AgentBehavior::ReadOnlyBreakdownVerdict => "read-only-breakdown-verdict",
            AgentBehavior::ReadOnlyVerdictWithDiff => "read-only-verdict-with-diff",
            AgentBehavior::UndeclaredVerdict => "undeclared-verdict",
            AgentBehavior::WritableVerdict => "writable-verdict",
            AgentBehavior::ReviewApprove { .. } => "review-approve",
            AgentBehavior::ReviewChanges => "review-changes",
            AgentBehavior::ReviewMissingVerdict => "review-missing-verdict",
            AgentBehavior::ReviewUndeclaredVerdict => "review-undeclared-verdict",
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
            AgentBehavior::ReadOnlyBreakdownVerdict => read_only_breakdown_verdict_script(),
            AgentBehavior::ReadOnlyVerdictWithDiff => read_only_verdict_script(None, true),
            AgentBehavior::UndeclaredVerdict => {
                "#!/bin/sh\nprintf '{\"verdict\":\"needs_breakdown\",\"summary\":\"needs splitting\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::WritableVerdict => {
                "#!/bin/sh\nset -eu\nprintf 'discard me\\n' > agent-output.txt\nprintf '{\"verdict\":\"needs_architect\",\"body\":\"blocked\",\"summary\":\"cannot proceed\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::ReviewApprove {
                head_capture_path,
                capture_path,
            } => review_approve_script(path_str(head_capture_path), capture_path.as_deref()),
            AgentBehavior::ReviewChanges => {
                "#!/bin/sh\nset -eu\ngrep '\"checkout\": \"pull_request_read_only\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\nprintf 'discard me\\n' > agent-output.txt\nprintf '{\"verdict\":\"changes\",\"review_body\":\"please add error handling\",\"summary\":\"needs error handling\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::ReviewMissingVerdict => {
                "#!/bin/sh\nset -eu\nprintf '{\"summary\":\"no opinion\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
            }
            AgentBehavior::ReviewUndeclaredVerdict => {
                "#!/bin/sh\nset -eu\nprintf '{\"verdict\":\"merge_now\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n".to_string()
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

fn read_only_breakdown_verdict_script() -> String {
    r#"#!/bin/sh
set -eu
grep '"checkout": "read_only"' "$TEMPER_CODING_WORKSPACE_CONTEXT" >/dev/null
grep '"needs_breakdown"' "$TEMPER_CODING_WORKSPACE_CONTEXT" >/dev/null
cat > "$TEMPER_CODING_WORKSPACE_RESULT" <<'JSON'
{"verdict":"needs_breakdown","summary":"planned breakdown","children":[{"slug":"api-schema","title":"Define the API schema","body":"Write the shared API schema.","labels":["code","ready"]},{"slug":"web-client","title":"Implement the web client","body":"Build the web client against the API schema.","depends_on":["api-schema"],"target_repo":"acme/other"}]}
JSON
"#
    .to_string()
}

fn review_approve_script(head_capture_path: &str, capture_path: Option<&Path>) -> String {
    let mut script = "#!/bin/sh\nset -eu\n".to_string();
    if let Some(capture_path) = capture_path {
        script.push_str(&format!(
            "cp \"$TEMPER_CODING_WORKSPACE_CONTEXT\" {}\n",
            shell_quote(path_str(capture_path))
        ));
    }
    script.push_str(
        "grep '\"checkout\": \"pull_request_read_only\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n",
    );
    script.push_str("grep '\"approve\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n");
    script.push_str("grep '\"changes\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n");
    script.push_str("grep '\"escalate\"' \"$TEMPER_CODING_WORKSPACE_CONTEXT\" >/dev/null\n");
    script.push_str(&format!(
        "git rev-parse HEAD > {}\n",
        shell_quote(head_capture_path)
    ));
    script.push_str(
        "printf '{\"verdict\":\"approve\",\"summary\":\"looks good\"}' > \"$TEMPER_CODING_WORKSPACE_RESULT\"\n",
    );
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

fn pr_assign(
    branch_hint: &str,
    correlation_key: &str,
    context_builder: fn(&str, &str) -> TestJobContext,
) -> Assign {
    let context = context_builder(branch_hint, correlation_key);
    Assign {
        protocol_version: WORKER_PROTOCOL_VERSION,
        job_id: format!("acme/service/pull-7/reviewer/{correlation_key}"),
        role: "reviewer".to_string(),
        repo: "acme/service".to_string(),
        artifact: Artifact {
            item: json!(7),
            kind: "pull_request".to_string(),
        },
        job_payload: serde_json::to_value(context).expect("PR JobContext serializes"),
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

fn pr_job_context(branch_hint: &str, correlation_key: &str) -> TestJobContext {
    let mut context = job_context(branch_hint, correlation_key);
    context.role = "reviewer".to_string();
    context.queue = "pr_needs_review".to_string();
    context.artifact_kind = "implementation_pr".to_string();
    context.action = Some("review_pr".to_string());
    context.checkout_capability = Some("pull_request_read_only".to_string());
    context.allowed_verdicts = vec![
        "approve".to_string(),
        "changes".to_string(),
        "escalate".to_string(),
    ];
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

fn seed_origin(origin: &Path, temp: &Path) -> String {
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

    git(["-C", path_str(&seed), "checkout", "-b", "review-head"]);
    fs::write(seed.join("pr-change.txt"), "pull request change\n").expect("write PR file");
    git([
        "-C",
        path_str(&seed),
        "-c",
        "user.name=Seed User",
        "-c",
        "user.email=seed@example.test",
        "add",
        "pr-change.txt",
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
        "pull request change",
    ]);
    let pull_request_head_sha = git_output(["-C", path_str(&seed), "rev-parse", "HEAD"]);
    git([
        "-C",
        path_str(&seed),
        "push",
        "origin",
        "HEAD:refs/temper/seed/pr-7",
    ]);
    git([
        "-C",
        path_str(origin),
        "update-ref",
        "refs/pull/7/head",
        pull_request_head_sha.as_str(),
    ]);
    git([
        "-C",
        path_str(origin),
        "update-ref",
        "-d",
        "refs/temper/seed/pr-7",
    ]);
    pull_request_head_sha
}

fn expect_success(outcome: JobOutcome) -> (String, String, Option<String>) {
    match outcome {
        JobOutcome::Success { branch, summary } => (branch.name, branch.head_sha, summary),
        JobOutcome::Verdict {
            verdict,
            body,
            summary,
            children,
        } => {
            panic!("expected success, got verdict {verdict:?} {body:?} {summary:?} {children:?}")
        }
        JobOutcome::Failure { class, message } => {
            panic!("expected success, got {class:?}: {message}")
        }
    }
}

fn expect_verdict(outcome: JobOutcome) -> (String, Option<String>, Option<String>, Vec<JobChild>) {
    match outcome {
        JobOutcome::Verdict {
            verdict,
            body,
            summary,
            children,
        } => (verdict, body, summary, children),
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
            children,
        } => {
            panic!(
                "expected {expected:?} failure, got verdict {verdict:?} {body:?} {summary:?} {children:?}"
            )
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

fn assert_no_extra_origin_head_branches(fixture: &Fixture, expected: &[&str]) {
    let output = git_output([
        "-C",
        path_str(&fixture.origin),
        "for-each-ref",
        "--format=%(refname:short)",
        "refs/heads",
    ]);
    let mut branches = if output.is_empty() {
        Vec::new()
    } else {
        output.lines().map(str::to_string).collect::<Vec<_>>()
    };
    branches.sort();

    let mut expected = expected
        .iter()
        .map(|branch| (*branch).to_string())
        .collect::<Vec<_>>();
    expected.sort();

    assert_eq!(branches, expected);
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
