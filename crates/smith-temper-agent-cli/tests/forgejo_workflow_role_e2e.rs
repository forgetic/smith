//! Ignored throwaway Forgejo + jig fake LLM proof for Smith's workflow-role
//! decision process. The Smith binary chooses the manifest `open_pr` action,
//! while Temper's process adapter validates the reply, invokes the test coding
//! workspace, and opens the PR through `RoleTools`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use jig_core::{Reply, Script};
use jig_server::FakeLlm;
use temper_forge::{CreateIssue, PullRequestQuery};
use temper_forge_forgejo::{ForgejoConfig, ForgejoForge};
use temper_runner::{
    Agent, BoundExternalTool, CODING_WORKSPACE_TOOL_ID, CodingWorkspace, CodingWorkspaceError,
    CodingWorkspaceOutput, CodingWorkspaceRequest, ExternalToolExecutors, RoleTools, WorkItem,
    WorkflowRoleDecisionProcessAgent, WorkflowRoleDecisionProcessConfig,
};
use temper_testing::forgejo_server::{ForgejoServer, provision};
use temper_workflow::{ArtifactKindId, ArtifactSource, ExternalToolId, QueueId, RoleId};

#[path = "forgejo_workflow_role_e2e/observability.rs"]
mod observability;
use observability::{ObservabilityProbe, forbidden_observability_values};

#[test]
#[ignore = "boots a throwaway Forgejo and jig fake LLM; run with SMITH_JIG_E2E=1 -- --ignored"]
fn smith_process_opens_pr_through_temper_role_tools() {
    if !enabled() {
        return;
    }

    let fake = workflow_role_decision_fake();
    let provider_base_url = fake.base_url();

    let server = start_forgejo_server_from_temper_workspace().expect("forgejo server boots");
    let provisioned = block_on(provision(&server)).expect("provisioning succeeds");
    let engineer = provisioned
        .role(&RoleId::new("engineer"))
        .expect("engineer role is provisioned")
        .clone();
    let forge = ForgejoForge::new(
        ForgejoConfig::new(server.base_url(), &engineer.token)
            .with_default_repo(&provisioned.owner, &provisioned.name)
            .with_web_ui_credentials(&engineer.user, &engineer.password),
    );

    let issue = block_on(forge.create_issue(
        &provisioned.repository,
        CreateIssue {
            title: "Prove Smith workflow role decision".into(),
            body: "A ready code issue for the Smith process responder.".into(),
            labels: vec!["code".into(), "ready".into()],
            assignees: Vec::new(),
        },
    ))
    .expect("code issue creates");

    let temp = TempDir::new("smith-forgejo-workflow-role");
    let checkout = prepare_checkout(
        temp.path(),
        server.base_url(),
        &provisioned.owner,
        &provisioned.name,
        &engineer.user,
        &engineer.password,
    );
    let workspace: Arc<dyn CodingWorkspace> = Arc::new(GitWorkspace { checkout });

    let workflow = temper_testing::workflow();
    let compiled = workflow.compile();
    let role = RoleId::new("engineer");
    let manifest = compiled
        .role(&role)
        .expect("engineer manifest exists")
        .clone();
    let observability = ObservabilityProbe::new(
        temp.path(),
        env!("CARGO_BIN_EXE_smith-workflow-role-decision"),
        &provider_base_url,
    );
    let process = WorkflowRoleDecisionProcessConfig::new(observability.wrapper())
        .with_args(process_args())
        .with_env_allowlist(process_env_allowlist())
        .with_timeout(std::time::Duration::from_secs(180));
    let executors = ExternalToolExecutors::new().with_workspace(
        role.clone(),
        ExternalToolId::new(CODING_WORKSPACE_TOOL_ID),
        workspace,
    );
    let agent = WorkflowRoleDecisionProcessAgent::with_bound_external_tools_and_executors(
        compiled.name(),
        manifest,
        process,
        vec![bound_coding_workspace()],
        executors,
    )
    .expect("process agent builds");
    let item = WorkItem {
        queue: QueueId::new("code_ready"),
        role: role.clone(),
        target: ArtifactSource::Issue {
            number: issue.number,
        },
        kind: ArtifactKindId::new("code"),
    };
    let tools = RoleTools::new(
        &workflow,
        &forge,
        &provisioned.repository,
        role.clone(),
        temper_testing::runner_config().execution_context(&role),
    )
    .with_observability_tick_id("smith-forgejo-e2e-tick");
    let expected_identity = tools.work_item_identity(&item);

    let changed = block_on(agent.service(&item, &tools)).expect("Smith process service succeeds");
    assert!(changed, "Smith should choose and execute open_pr");

    let prs =
        block_on(forge.list_pull_requests(&provisioned.repository, PullRequestQuery::default()))
            .expect("PR list succeeds");
    assert_eq!(prs.len(), 1);
    assert_eq!(
        prs[0].source.branch,
        format!("agent/pr-for-code-{}", issue.number.get())
    );
    assert!(prs[0].labels.iter().any(|label| label == "implementation"));
    assert!(prs[0].body.contains("src/smith-workflow-role-decision.txt"));

    let auth_fixture = jig_auth_fixture();
    observability.assert_smith_logs_and_capture(
        &expected_identity,
        "open_pr",
        &forbidden_observability_values(&engineer.token, &engineer.password, &auth_fixture),
    );
}

fn enabled() -> bool {
    if std::env::var("SMITH_JIG_E2E").ok().as_deref() == Some("1") {
        true
    } else {
        eprintln!("skipping Smith Forgejo workflow-role jig e2e: set SMITH_JIG_E2E=1");
        false
    }
}

fn workflow_role_decision_fake() -> FakeLlm {
    FakeLlm::start(Script::Fixed(Reply::text(
        r#"{"action":"open_pr","reason":"jig selects open_pr for Forgejo workflow-role e2e"}"#,
    )))
    .expect("start fake LLM")
}

fn start_forgejo_server_from_temper_workspace() -> Result<ForgejoServer, String> {
    let temper_root = temper_workspace_root()?;
    let _current_dir = CurrentDirGuard::change_to(&temper_root)?;
    ForgejoServer::start().map_err(|error| {
        format!(
            "forgejo server failed to boot from Temper workspace root {}: {error}",
            temper_root.display()
        )
    })
}

fn temper_workspace_root() -> Result<PathBuf, String> {
    let attempted = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../temper");
    let root = attempted.canonicalize().map_err(|error| {
        format!(
            "failed to resolve Temper workspace root at {}: {error}",
            attempted.display()
        )
    })?;

    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Err(format!(
            "resolved Temper workspace root {} is missing Cargo.toml",
            root.display()
        ));
    }

    let forgejo_fixture = root.join("crates/temper-forgejo-fixture");
    if !forgejo_fixture.is_dir() {
        return Err(format!(
            "resolved Temper workspace root {} is missing crates/temper-forgejo-fixture/",
            root.display()
        ));
    }

    Ok(root)
}

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn change_to(path: &Path) -> Result<Self, String> {
        let original = std::env::current_dir()
            .map_err(|error| format!("failed to read current directory: {error}"))?;
        std::env::set_current_dir(path).map_err(|error| {
            format!(
                "failed to change current directory from {} to Temper workspace root {}: {error}",
                original.display(),
                path.display()
            )
        })?;
        Ok(Self { original })
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        if let Err(error) = std::env::set_current_dir(&self.original) {
            eprintln!(
                "failed to restore current directory to {} after Forgejo fixture startup: {error}",
                self.original.display()
            );
        }
    }
}

fn jig_auth_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../smith-temper-agent/tests/fixtures/jig_auth.json")
}

fn process_args() -> Vec<String> {
    vec![
        "--auth".to_string(),
        "chatgpt-oauth".to_string(),
        "--auth-file".to_string(),
        jig_auth_fixture().display().to_string(),
    ]
}

fn process_env_allowlist() -> Vec<String> {
    vec!["SMITH_TEST_PROVIDER_BASE_URL".to_string()]
}

fn bound_coding_workspace() -> BoundExternalTool {
    BoundExternalTool {
        id: CODING_WORKSPACE_TOOL_ID.to_string(),
        description: "Edit and commit repository code.".to_string(),
        required: true,
        constraints: vec!["Produce a real product diff.".to_string()],
        guidance: Some("Create a small Smith proof file.".to_string()),
        provider: "smith-test-git-workspace".to_string(),
    }
}

struct GitWorkspace {
    checkout: PathBuf,
}

#[async_trait]
impl CodingWorkspace for GitWorkspace {
    async fn produce_head(
        &self,
        request: CodingWorkspaceRequest,
    ) -> Result<CodingWorkspaceOutput, CodingWorkspaceError> {
        let branch = request.branch_hint;
        let base = request.base_branch;
        git(&self.checkout, &["fetch", "origin", &base])?;
        git(
            &self.checkout,
            &["checkout", "-B", &branch, &format!("origin/{base}")],
        )?;
        let path = self.checkout.join("src/smith-workflow-role-decision.txt");
        std::fs::create_dir_all(path.parent().expect("file has parent")).map_err(|error| {
            CodingWorkspaceError::new(format!("creating src dir failed: {error}"))
        })?;
        std::fs::write(
            &path,
            format!("Smith process opened {}\n", request.correlation_key),
        )
        .map_err(|error| {
            CodingWorkspaceError::new(format!("writing proof file failed: {error}"))
        })?;
        git(
            &self.checkout,
            &["add", "src/smith-workflow-role-decision.txt"],
        )?;
        git(
            &self.checkout,
            &[
                "commit",
                "-m",
                &format!("Implement {} [ci-pass]", request.correlation_key),
            ],
        )?;
        git(
            &self.checkout,
            &[
                "push",
                "--force-with-lease",
                "origin",
                &format!("HEAD:refs/heads/{branch}"),
            ],
        )?;
        Ok(CodingWorkspaceOutput::new(
            branch,
            base,
            "updated src/smith-workflow-role-decision.txt",
            vec!["src/smith-workflow-role-decision.txt".to_string()],
            vec![
                "implementation".to_string(),
                "needs-reviewer".to_string(),
                "needs-merge".to_string(),
            ],
        ))
    }
}

fn prepare_checkout(
    root: &Path,
    base_url: &str,
    owner: &str,
    name: &str,
    user: &str,
    password: &str,
) -> PathBuf {
    let checkout = root.join("checkout");
    let remote = format!("{}/{}/{}.git", base_url.trim_end_matches('/'), owner, name);
    git(
        root,
        &[
            "clone",
            &remote,
            checkout.to_str().expect("utf8 checkout path"),
        ],
    )
    .expect("clone succeeds");
    git(
        &checkout,
        &["config", "user.email", "engineer@example.invalid"],
    )
    .expect("git email config");
    git(&checkout, &["config", "user.name", "Smith Engineer"]).expect("git name config");
    let credentials = write_git_credentials(root, base_url, user, password);
    git(
        &checkout,
        &[
            "config",
            "credential.helper",
            &format!("store --file={}", credentials.display()),
        ],
    )
    .expect("git credential helper config");
    checkout
}

fn git(root: &Path, args: &[&str]) -> Result<String, CodingWorkspaceError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| {
            CodingWorkspaceError::new(format!("failed to run git {}: {error}", args.join(" ")))
        })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(CodingWorkspaceError::new(format!(
            "git {} failed with {}; stderr: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn write_git_credentials(root: &Path, base_url: &str, user: &str, password: &str) -> PathBuf {
    let without_scheme = base_url
        .trim_end_matches('/')
        .strip_prefix("http://")
        .expect("throwaway Forgejo uses http");
    let credentials = root.join("git-credentials");
    std::fs::write(
        &credentials,
        format!(
            "http://{}:{}@{}\n",
            percent_encode_userinfo(user),
            percent_encode_userinfo(password),
            without_scheme
        ),
    )
    .expect("credential file writes");
    credentials
}

fn percent_encode_userinfo(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            chrono_like_timestamp()
        ));
        std::fs::create_dir_all(&path).expect("temp dir creates");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn chrono_like_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time is after epoch")
        .as_nanos()
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds")
        .block_on(future)
}
