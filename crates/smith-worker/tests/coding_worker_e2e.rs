//! Hermetic full-worker end-to-end test: a real smith-worker runs a real coding
//! job through the NATIVE agent loop against a jig fake LLM, with NO external
//! Forgejo or runner.
//!
//! Topology, all in one process:
//! - a **jig `FakeLlm`** scripts the engineer agent to write a file, then finish;
//! - a **fake daemon** (tokio axum) speaks the worker/daemon wire protocol:
//!   accepts register, assigns one real coding job (a full `WireJobContext`
//!   payload), then accepts the result;
//! - a **real `smith-worker`** runs on its own asupersync runtime thread with
//!   `ExecutorSelection::Coding` + `PiAgentRunner` pointed at the jig base URL;
//! - git remotes are local `file://` bare repos seeded with an initial commit.
//!
//! This drives the entire production path: worker register → poll → assign →
//! `CodingExecutor` prepares the checkout → `run_coding_agent_native`
//! (AgentMachine + asupersync shell + pi tools) edits the working tree → the
//! executor commits and pushes the branch to the `file://` origin → reports
//! Success. The assertions verify the branch landed on origin with the agent's
//! file, and the worker reported Success.
//!
//! Hermetic and fast; runs by default.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use jig_core::{Reply, Script, StopReason, Turn};
use jig_server::FakeLlm;
use serde_json::json;
use smith_temper_agent::ProviderConfig;
use smith_worker::{
    CodingExecutor, CodingExecutorConfig, ExecutorSelection, PiAgentRunner, RoleGitIdentity,
    WorkerConfig, run_worker,
};
use smith_worker::config::CapabilitySpec;
use temper_worker_protocol::{
    Artifact, Assign, ProtocolError, Release, ReleaseDisposition, ResultStatus,
    WORKER_PROTOCOL_VERSION, WorkerProtocolMessage,
};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn worker_runs_a_real_coding_job_through_the_native_loop() {
    // --- Git fixture: a bare origin seeded with an initial commit on main. ---
    let fixture = GitFixture::new();

    // --- Jig fake LLM: the engineer writes a product file, then finishes (head
    //     path, no verdict). ---
    let fake = FakeLlm::start(Script::rule(|view| {
        if view.prior_tool_results == 0 {
            Reply {
                turns: vec![Turn::ToolCall {
                    id: "call_write".to_string(),
                    name: "write".to_string(),
                    args: json!({
                        "path": "GREETING.md",
                        "content": "hello from the native loop\n"
                    }),
                }],
                usage: Default::default(),
                stop: StopReason::ToolCalls,
            }
        } else {
            Reply::text(r#"{"summary":"Created GREETING.md"}"#)
        }
    }))
    .expect("start fake LLM");

    // --- Worker config: coding executor pointed at the jig provider. ---
    let config = worker_config(&fixture);

    // --- Fake daemon assigns one coding job, then accepts the result. ---
    let (daemon_url, observed) = spawn_fake_daemon(&fixture).await;
    let config = WorkerConfig {
        daemon_url,
        ..config
    };

    // --- Build the real coding executor with a jig-backed PiAgentRunner. ---
    let provider = ProviderConfig::new(
        "jig-openai-compatible",
        "jig-coding-worker-e2e",
        "https://example.invalid/unused",
        "sk-jig-test",
    )
    .with_base_url_override(fake.base_url());
    let runner = Arc::new(PiAgentRunner::new(provider, 6, None));
    let executor_config = CodingExecutorConfig {
        workspace_root: fixture.workspace_root.clone(),
        git_base_url: fixture.git_base_url(),
        role_identities: role_identities(),
    };
    let executor = Arc::new(CodingExecutor::new(executor_config, runner));

    // --- Run the real worker on its own asupersync runtime thread. ---
    spawn_worker_thread(config, executor);

    // --- Await the daemon observing the result. ---
    let observed = tokio::time::timeout(std::time::Duration::from_secs(30), observed)
        .await
        .expect("daemon observes a result within the timeout")
        .expect("daemon sends the observed run");

    // --- Assert: the worker reported Success with a branch. ---
    assert_eq!(observed.result.status, ResultStatus::Success, "result: {:?}", observed.result);
    let branch = observed.result.branch.expect("success carries a branch");
    assert_eq!(branch.name, "agent/pr-for-code-7");
    assert_eq!(branch.head_sha.len(), 40, "head sha looks like a real sha");

    // --- Assert: the branch landed on origin with the agent's file. ---
    let pushed_sha = fixture.origin_rev("refs/heads/agent/pr-for-code-7");
    assert_eq!(pushed_sha, branch.head_sha, "the reported sha is what was pushed");
    let greeting = fixture.origin_show("refs/heads/agent/pr-for-code-7:GREETING.md");
    assert_eq!(
        greeting, "hello from the native loop",
        "the agent's product file was committed and pushed"
    );
    // The commit message carries the correlation key + Closes trailer.
    let subject = fixture.origin_log_format("refs/heads/agent/pr-for-code-7", "%s");
    assert_eq!(subject, "Implement pr-for-code-7");
    let body = fixture.origin_log_format("refs/heads/agent/pr-for-code-7", "%b");
    assert_eq!(body, "Closes #7");

    // The fake LLM was actually exercised (a tool loop, not a single turn).
    assert!(fake.requests().len() > 1, "expected the native loop to do a tool round");
}

// ---------------------------------------------------------------------------
// Worker config + identities.
// ---------------------------------------------------------------------------

fn worker_config(_fixture: &GitFixture) -> WorkerConfig {
    WorkerConfig {
        daemon_url: "http://placeholder".to_string(),
        worker_id: "coding-worker-e2e".to_string(),
        capabilities: vec![CapabilitySpec {
            repo: "acme/service".to_string(),
            role: "engineer".to_string(),
        }],
        max_concurrent_jobs: 1,
        poll_wait: std::time::Duration::from_millis(50),
        heartbeat_interval: std::time::Duration::from_millis(50),
        // `run_worker` takes the executor we construct directly, so the config's
        // `executor` field is unused here (it only matters to the binary's arg
        // parsing); leave it as the stub shape.
        executor: ExecutorSelection::Stub,
    }
}

fn role_identities() -> BTreeMap<String, RoleGitIdentity> {
    let mut m = BTreeMap::new();
    m.insert(
        "engineer".to_string(),
        RoleGitIdentity {
            user: "Smith Engineer".to_string(),
            email: "engineer@example.test".to_string(),
            token: "test-token".to_string(),
        },
    );
    m
}

fn spawn_worker_thread<E>(config: WorkerConfig, executor: Arc<E>)
where
    E: smith_worker::JobExecutor + Send + Sync + 'static,
{
    std::thread::spawn(move || {
        let _ = smith_io_engine::block_on(async move { run_worker(config, executor).await });
    });
}

// ---------------------------------------------------------------------------
// Fake daemon: assign one coding job, accept the result.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    requests: mpsc::Sender<DaemonRequest>,
}

struct DaemonRequest {
    message: WorkerProtocolMessage,
    reply: oneshot::Sender<DaemonReply>,
}

enum DaemonReply {
    NoContent,
    Message(Box<WorkerProtocolMessage>),
}

struct ObservedRun {
    result: temper_worker_protocol::JobResult,
}

async fn message_handler(
    State(state): State<AppState>,
    Json(message): Json<WorkerProtocolMessage>,
) -> Response {
    let (reply_tx, reply_rx) = oneshot::channel();
    if state
        .requests
        .send(DaemonRequest { message, reply: reply_tx })
        .await
        .is_err()
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, "fake daemon stopped").into_response();
    }
    match reply_rx.await {
        Ok(DaemonReply::NoContent) => StatusCode::NO_CONTENT.into_response(),
        Ok(DaemonReply::Message(reply)) => Json(*reply).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "fake daemon dropped reply").into_response(),
    }
}

async fn spawn_fake_daemon(fixture: &GitFixture) -> (String, oneshot::Receiver<ObservedRun>) {
    let (request_tx, request_rx) = mpsc::channel(16);
    let (observed_tx, observed_rx) = oneshot::channel();
    let assign = coding_assign(fixture);
    tokio::spawn(fake_daemon_controller(request_rx, observed_tx, assign));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind fake daemon");
    let addr = listener.local_addr().expect("fake daemon addr");
    let app = Router::new()
        .route("/v1/message", post(message_handler))
        .with_state(AppState { requests: request_tx });
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (format!("http://{addr}"), observed_rx)
}

async fn fake_daemon_controller(
    mut requests: mpsc::Receiver<DaemonRequest>,
    observed: oneshot::Sender<ObservedRun>,
    assign: Assign,
) {
    let mut assigned = false;
    let mut observed = Some(observed);
    while let Some(request) = requests.recv().await {
        match request.message {
            WorkerProtocolMessage::Register(_) => {
                let _ = request.reply.send(DaemonReply::NoContent);
            }
            WorkerProtocolMessage::Poll(_) if !assigned => {
                assigned = true;
                let _ = request.reply.send(DaemonReply::Message(Box::new(
                    WorkerProtocolMessage::Assign(assign.clone()),
                )));
            }
            WorkerProtocolMessage::Poll(_) | WorkerProtocolMessage::Heartbeat(_) => {
                let _ = request.reply.send(DaemonReply::Message(Box::new(poll_timeout())));
            }
            WorkerProtocolMessage::Result(result) => {
                let release = WorkerProtocolMessage::Release(Release {
                    protocol_version: WORKER_PROTOCOL_VERSION,
                    worker_id: result.worker_id.clone(),
                    job_id: result.job_id.clone(),
                    disposition: ReleaseDisposition::Accepted,
                    message: None,
                });
                let _ = request.reply.send(DaemonReply::Message(Box::new(release)));
                if let Some(observed) = observed.take() {
                    let _ = observed.send(ObservedRun { result });
                }
            }
            other => {
                let _ = request.reply.send(DaemonReply::Message(Box::new(
                    WorkerProtocolMessage::Error(ProtocolError {
                        protocol_version: WORKER_PROTOCOL_VERSION,
                        code: temper_worker_protocol::ErrorCode::MalformedMessage,
                        message: format!("unexpected: {other:?}"),
                        retry_after_ms: None,
                        job_id: None,
                    }),
                )));
            }
        }
    }
}

fn poll_timeout() -> WorkerProtocolMessage {
    WorkerProtocolMessage::Error(ProtocolError {
        protocol_version: WORKER_PROTOCOL_VERSION,
        code: temper_worker_protocol::ErrorCode::PollTimeout,
        message: "no work".to_string(),
        retry_after_ms: None,
        job_id: None,
    })
}

/// A full coding-job Assign payload (the enriched WireJobContext the executor
/// requires).
fn coding_assign(_fixture: &GitFixture) -> Assign {
    let job_context = json!({
        "role": "engineer",
        "repo": "acme/service",
        "queue": "code_ready",
        "artifact_kind": "code",
        "repository": {
            "owner": "acme",
            "name": "service",
            "default_branch": "main"
        },
        "base_branch": "main",
        "branch_hint": "agent/pr-for-code-7",
        "correlation_key": "pr-for-code-7",
        "artifact": {
            "number": 7,
            "title": "Add a greeting file",
            "body": "Create GREETING.md.",
            "labels": ["code", "ready"],
            "state": "Open"
        },
        "action": "open_pr",
        "checkout_capability": "writable",
        "allowed_verdicts": []
    });
    Assign {
        protocol_version: WORKER_PROTOCOL_VERSION,
        job_id: "acme/service/issue-7/engineer/pr-for-code-7".to_string(),
        role: "engineer".to_string(),
        repo: "acme/service".to_string(),
        artifact: Artifact {
            item: json!(7),
            kind: "issue".to_string(),
        },
        job_payload: job_context,
    }
}

// ---------------------------------------------------------------------------
// Git fixture (bare origin seeded with main, file:// remotes).
// ---------------------------------------------------------------------------

struct GitFixture {
    temp: tempfile::TempDir,
    origin: PathBuf,
    workspace_root: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let git_root = temp.path().join("git");
        fs::create_dir_all(git_root.join("acme")).expect("git root");
        let origin = git_root.join("acme/service.git");
        git(["init", "--bare", path_str(&origin)]);
        seed_origin(&origin, temp.path());
        let workspace_root = temp.path().join("workspaces");
        Self { temp, origin, workspace_root }
    }

    fn git_base_url(&self) -> String {
        format!("file://{}/git", path_str(self.temp.path()))
    }

    fn origin_rev(&self, refname: &str) -> String {
        git_output(["-C", path_str(&self.origin), "rev-parse", refname])
    }

    fn origin_show(&self, spec: &str) -> String {
        git_output(["-C", path_str(&self.origin), "show", spec])
    }

    fn origin_log_format(&self, refname: &str, fmt: &str) -> String {
        git_output([
            "-C",
            path_str(&self.origin),
            "log",
            "-1",
            &format!("--format={fmt}"),
            refname,
        ])
    }
}

fn seed_origin(origin: &Path, temp: &Path) {
    let seed = temp.join("seed");
    git(["init", "-b", "main", path_str(&seed)]);
    fs::write(seed.join("README.md"), "# seed\n").expect("seed file");
    git(["-C", path_str(&seed), "-c", "user.name=Seed", "-c", "user.email=seed@example.test", "add", "README.md"]);
    git(["-C", path_str(&seed), "-c", "user.name=Seed", "-c", "user.email=seed@example.test", "commit", "-m", "initial"]);
    git(["-C", path_str(&seed), "remote", "add", "origin", path_str(origin)]);
    git(["-C", path_str(&seed), "push", "origin", "main"]);
}

fn git<const N: usize>(args: [&str; N]) {
    let output = Command::new("git").args(args).output().expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output<const N: usize>(args: [&str; N]) -> String {
    let output = Command::new("git").args(args).output().expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8").trim_end_matches('\n').to_string()
}

fn path_str(p: &Path) -> &str {
    p.as_os_str().to_str().expect("utf8 path")
}
