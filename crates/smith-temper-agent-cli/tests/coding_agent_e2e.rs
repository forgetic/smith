//! Ignored real-LLM proof for the Smith coding-workspace agent binary.
//!
//! Mirrors the gating of `forgejo_workflow_role_e2e.rs`: it makes real LLM
//! calls, so it is `#[ignore]`d and only runs when `TEMPER_FORGEJO_AGENTS=1`.
//! Unlike the workflow-role e2e it needs no Forgejo server — it drives the
//! `smith-coding-agent` binary directly against a throwaway local git checkout
//! using the temper coding-workspace protocol (context file in, result file
//! out, product diff in the working tree) and asserts the engineer head path
//! produces a real, non-bookkeeping diff and an empty (verdict-less) result.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
#[ignore = "makes real LLM calls; run with TEMPER_FORGEJO_AGENTS=1 -- --ignored"]
fn engineer_run_leaves_a_product_diff() {
    if !agents_enabled() {
        return;
    }

    let temp = TempDir::new("smith-coding-agent-e2e");
    let checkout = init_git_repo(temp.path());

    let context_path = temp.path().join("context.json");
    let result_path = temp.path().join("result.json");
    fs::write(&context_path, ENGINEER_CONTEXT).expect("write context file");

    let status = Command::new(env!("CARGO_BIN_EXE_smith-coding-agent"))
        .current_dir(&checkout)
        .args(process_args())
        .env("TEMPER_CODING_WORKSPACE_CONTEXT", &context_path)
        .env("TEMPER_CODING_WORKSPACE_RESULT", &result_path)
        .status()
        .expect("smith-coding-agent runs");
    assert!(status.success(), "engineer run should exit 0");

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
}

fn agents_enabled() -> bool {
    if std::env::var("TEMPER_FORGEJO_AGENTS").ok().as_deref() == Some("1") {
        true
    } else {
        eprintln!("skipping Smith coding-agent e2e: set TEMPER_FORGEJO_AGENTS=1");
        false
    }
}

fn auth_flag() -> &'static str {
    match std::env::var("TEMPER_AGENTS_AUTH").ok().as_deref() {
        Some("deepseek") => "deepseek",
        Some("anthropic-oauth") => "anthropic-oauth",
        _ => "chatgpt-oauth",
    }
}

fn process_args() -> Vec<String> {
    let mut args = vec!["--auth".to_string(), auth_flag().to_string()];
    if let Ok(model) = std::env::var("TEMPER_AGENTS_CODEX_MODEL") {
        if !model.trim().is_empty() {
            args.extend(["--codex-model".to_string(), model]);
        }
    }
    if let Some(path) = std::env::var_os("TEMPER_AGENTS_AUTH_FILE") {
        args.extend([
            "--auth-file".to_string(),
            PathBuf::from(path).display().to_string(),
        ]);
    }
    args
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
