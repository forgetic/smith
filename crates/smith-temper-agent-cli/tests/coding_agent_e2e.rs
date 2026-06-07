//! Ignored jig-backed e2e for the Smith coding-workspace agent binary.
//!
//! Runs the real `smith-coding-agent` binary against a local scripted
//! `jig_server::FakeLlm`, so it is hermetic but still opt-in: run ignored tests
//! with `SMITH_JIG_E2E=1`. It drives the temper coding-workspace protocol
//! (context file in, result file out, product diff in the working tree) and
//! asserts the engineer head path produces a real, non-bookkeeping diff and an
//! empty (verdict-less) result.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jig_core::{Reply, Script, StopReason, Turn};
use jig_server::FakeLlm;

const ENGINEER_CONTEXT: &str = r#"{
  "repository": { "id": "repo-1", "owner": "ai", "name": "demo", "default_branch": "main" },
  "work_item": {
    "role": "engineer",
    "queue": "code_ready",
    "kind": "code",
    "target": "Issue { number: ItemNumber(1) }",
    "context": "{\"artifact\":{\"title\":\"Add a NOTES.md file\",\"body\":\"Create a top-level NOTES.md whose first line is exactly 'project notes'. Keep it tiny.\",\"labels\":[\"code\",\"ready\"]}}"
  },
  "base_branch": "main",
  "branch_hint": "agent/pr-for-code-1",
  "correlation_key": "pr-for-code-1",
  "checkout": "writable",
  "guidance": {
    "role_guidance": "Implement the issue with a real product diff.",
    "tool_guidance": "Use the file tools to create the requested file.",
    "tool_constraints": ["Do not create .temper-only bookkeeping diffs."]
  }
}"#;

#[test]
#[ignore = "jig-backed e2e; run with SMITH_JIG_E2E=1 -- --ignored"]
fn engineer_run_leaves_a_product_diff() {
    if !jig_e2e_enabled() {
        return;
    }

    let fake = coding_agent_fake();
    let temp = TempDir::new("smith-coding-agent-e2e");
    let checkout = init_git_repo(temp.path());

    let context_path = temp.path().join("context.json");
    let result_path = temp.path().join("result.json");
    fs::write(&context_path, ENGINEER_CONTEXT).expect("write context file");

    let auth_fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../smith-temper-agent/tests/fixtures/jig_auth.json");

    let status = Command::new(env!("CARGO_BIN_EXE_smith-coding-agent"))
        .current_dir(&checkout)
        .args([
            "--auth",
            "chatgpt-oauth",
            "--auth-file",
            &auth_fixture.display().to_string(),
        ])
        .env("TEMPER_CODING_WORKSPACE_CONTEXT", &context_path)
        .env("TEMPER_CODING_WORKSPACE_RESULT", &result_path)
        .env("SMITH_TEST_PROVIDER_BASE_URL", fake.base_url())
        .status()
        .expect("smith-coding-agent runs");
    assert!(status.success(), "engineer run should exit 0");

    let notes = fs::read_to_string(checkout.join("NOTES.md")).expect("NOTES.md was written");
    assert_eq!(
        notes.lines().next(),
        Some("project notes"),
        "NOTES.md first line must match the requested product content"
    );

    // The head path leaves a real product diff in the working tree.
    let porcelain = Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(&checkout)
        .output()
        .expect("git status");
    let changes = String::from_utf8_lossy(&porcelain.stdout);
    assert!(
        !changes.trim().is_empty(),
        "engineer run must leave a product diff; git status was empty"
    );
    assert!(
        changes.lines().any(|line| line == "?? NOTES.md"),
        "diff must include the requested product file: {changes}"
    );
    assert!(
        !changes.contains(".temper-pr-prep") && !changes.contains(".temper-ci"),
        "diff must not be bookkeeping-only: {changes}"
    );

    // The result is a head-path result: no verdict.
    let result_raw = fs::read_to_string(&result_path).expect("result file written");
    let result: serde_json::Value =
        serde_json::from_str(&result_raw).expect("result is valid JSON");
    assert!(
        result.get("verdict").is_none() || result["verdict"].is_null(),
        "engineer head path should emit no verdict, got {result_raw}"
    );
    assert!(
        fake.requests().len() > 1,
        "expected a tool loop, got one model turn"
    );
}

fn jig_e2e_enabled() -> bool {
    if std::env::var("SMITH_JIG_E2E").ok().as_deref() == Some("1") {
        true
    } else {
        eprintln!("skipping Smith coding-agent jig e2e: set SMITH_JIG_E2E=1");
        false
    }
}

fn coding_agent_fake() -> FakeLlm {
    FakeLlm::start(Script::rule(|view| {
        if view.prior_tool_results == 0 {
            Reply {
                turns: vec![Turn::ToolCall {
                    id: "call_write_notes".to_string(),
                    name: "write".to_string(),
                    args: serde_json::json!({
                        "path": "NOTES.md",
                        "content": "project notes\n"
                    }),
                }],
                usage: Default::default(),
                stop: StopReason::ToolCalls,
            }
        } else {
            Reply::text(r#"{"summary":"Created NOTES.md with project notes."}"#)
        }
    }))
    .expect("start fake LLM")
}

/// Initializes a minimal git repository so the agent's working-tree diff check
/// (and the test's own `git status`) has a real repo to operate on.
fn init_git_repo(parent: &Path) -> PathBuf {
    let checkout = parent.join("checkout");
    fs::create_dir_all(&checkout).expect("create checkout dir");
    run_git(&checkout, &["init", "--initial-branch=main"]);
    run_git(&checkout, &["config", "user.email", "agent@localhost"]);
    run_git(&checkout, &["config", "user.name", "agent"]);
    fs::write(checkout.join("README.md"), "# demo\n").expect("seed README");
    run_git(&checkout, &["add", "."]);
    run_git(&checkout, &["commit", "-m", "seed"]);
    checkout
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap_or_else(|error| panic!("git {args:?} failed to spawn: {error}"));
    assert!(status.success(), "git {args:?} failed");
}

/// A throwaway directory removed on drop. Local to this test to avoid a shared
/// test-support dependency.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!("{prefix}-{}", std::process::id());
        let path = std::env::temp_dir().join(unique);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
