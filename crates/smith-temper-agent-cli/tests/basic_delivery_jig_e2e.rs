//! Provider-free basic-delivery process E2E.
//!
//! This ignored test boots the checked-in `examples/basic-delivery` launcher with
//! deterministic local stand-ins for role decisions and coding workspaces. It is
//! intentionally not part of default CI because it starts real Forgejo,
//! forgejo-runner, Temper workers, and webhook trigger processes.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

const SKIP: &str = "skipping basic-delivery jig e2e: set TEMPER_BASIC_DELIVERY_JIG_E2E=1";
const INTAKE_BODY: &str = include_str!("../../../examples/basic-delivery/config/intake-issue.md");

#[test]
#[ignore = "boots real Forgejo + host-mode forgejo-runner; run with TEMPER_BASIC_DELIVERY_JIG_E2E=1 -- --ignored"]
fn basic_delivery_jig_runs_to_bot_merge() {
    if std::env::var("TEMPER_BASIC_DELIVERY_JIG_E2E")
        .ok()
        .as_deref()
        != Some("1")
    {
        eprintln!("{SKIP}");
        return;
    }

    let smith_root = repo_root();
    let real_example = smith_root.join("examples/basic-delivery");
    assert!(
        real_example.join("run.sh").exists(),
        "missing basic-delivery run.sh"
    );
    assert!(
        real_example.join("tools/greeting-coder.sh").exists(),
        "missing greeting coder"
    );
    assert!(
        real_example
            .join("tools/greeting-role-decision.sh")
            .exists(),
        "missing greeting role decision"
    );

    let temp = TempDir::new("basic-delivery-jig-e2e");
    let example = temp.path.join("basic-delivery");
    copy_dir(&real_example, &example);

    let forgejo_port = unused_loopback_port();
    let trigger_port = unused_loopback_port();
    let base_url = format!("http://127.0.0.1:{forgejo_port}");
    let trigger_bind = format!("127.0.0.1:{trigger_port}");
    let webhook_url = format!("http://127.0.0.1:{trigger_port}/forgejo/webhook");
    let output_path = temp.path.join("run.sh.out");
    let output = fs::File::create(&output_path).expect("create run.sh output capture");
    let output_err = output.try_clone().expect("clone output capture");

    let mut command = Command::new("/bin/sh");
    command
        .arg(example.join("run.sh"))
        .current_dir(&example)
        .env("SMITH_WORKSPACE_ROOT", &smith_root)
        .env("TEMPER_WORKSPACE_ROOT", temper_root(&smith_root))
        .env("BASIC_DELIVERY_ROLE_DECISION", "greeting")
        .env("BASIC_DELIVERY_CODER", "greeting")
        .env("BASE_URL", &base_url)
        .env("TRIGGER_BIND", &trigger_bind)
        .env("WEBHOOK_URL", &webhook_url)
        .env("RUN_SECS", "240")
        .env("POLL_MS", "120000")
        .env("CI_STATUS_POLL_MS", "1000")
        .env("IDLE_POLL_MAX_MS", "2000")
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(output_err));
    preserve_overrides(&mut command);

    let child = command
        .spawn()
        .expect("start examples/basic-delivery/run.sh");
    let mut run = RunGuard {
        child,
        example: example.clone(),
        output_path: output_path.clone(),
    };

    let deadline = Instant::now() + Duration::from_secs(360);
    let token = wait_for_token(&example, deadline, &run);

    wait_for_topology_evidence(&base_url, &token, deadline, &run);
    let issue = wait_for_issue_rewrite(&base_url, &token, deadline, &run);
    assert_eq!(issue["number"], 1);
    assert_architect_issue_state(&issue);

    let (pr_number, pr) = wait_for_pr(&base_url, &token, deadline, &run);
    let pr_labels = labels(&pr);
    assert!(
        pr_labels.contains(&"implementation".to_string()),
        "PR #{pr_number} labels: {pr:#}"
    );

    let post_engineer_issue = wait_for_post_engineer_issue_state(&base_url, &token, deadline, &run);
    assert_eq!(post_engineer_issue["number"], 1);

    wait_for_ci_and_merge(&base_url, &token, pr_number, deadline, &run);
    assert!(
        log_tail(&output_path).contains("sh -n src/banner.sh"),
        "runner log evidence missing shell check; diagnostics: {}",
        run.diagnostics()
    );

    let final_pr_path = format!("/api/v1/repos/acme/service/pulls/{pr_number}");
    let pr = api_json(&base_url, &token, &final_pr_path);
    assert_eq!(pr["state"], "closed", "final PR #{pr_number} state: {pr:#}");
    assert!(
        pr["merged"].as_bool().unwrap_or(false),
        "observed PR #{pr_number} was not merged by bot: {pr:#}"
    );
    let final_file = api_json(
        &base_url,
        &token,
        "/api/v1/repos/acme/service/contents/src/banner.sh?ref=main",
    );
    assert_eq!(
        final_file["name"], "banner.sh",
        "main lacks src/banner.sh: {final_file:#}"
    );
    assert!(
        log_tail(&output_path).contains("land_pr"),
        "mechanical land_pr not logged; diagnostics: {}",
        run.diagnostics()
    );

    run.stop();
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn temper_root(smith: &Path) -> PathBuf {
    std::env::var_os("TEMPER_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| smith.parent().unwrap().join("temper"))
}
fn unused_loopback_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn preserve_overrides(command: &mut Command) {
    for key in [
        "TEMPER_FORGEJO_BINARY",
        "TEMPER_FORGEJO_RUNNER_BINARY",
        "TEMPER_WORKER_BIN",
        "TEMPER_PROVISION_BIN",
        "TEMPER_TRIGGER_BIN",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

fn wait_for_token(example: &Path, deadline: Instant, run: &RunGuard) -> String {
    let roles = example.join("secrets/roles.env");
    poll(
        deadline,
        run,
        || {
            fs::read_to_string(&roles).ok().and_then(|s| {
                s.lines().find_map(|l| {
                    l.strip_prefix("TEMPER_FORGEJO_TOKEN_BOT=")
                        .map(|v| v.trim_matches('\'').to_string())
                })
            })
        },
        "bot token",
    )
}

fn wait_for_topology_evidence(base: &str, token: &str, deadline: Instant, run: &RunGuard) {
    poll(
        deadline,
        run,
        || {
            let labels = api_json(base, token, "/api/v1/repos/acme/service/labels");
            let names: Vec<String> = labels
                .as_array()?
                .iter()
                .filter_map(|l| l["name"].as_str().map(str::to_string))
                .collect();
            let needed = [
                "untriaged",
                "code",
                "ready",
                "in-progress",
                "implementation",
                "landed",
            ];
            (needed.iter().all(|n| names.iter().any(|s| s == n))
                && log_tail(&run.output_path).contains("architect")
                && log_tail(&run.output_path).contains("engineer")
                && log_tail(&run.output_path).contains("mechanical")
                && log_tail(&run.output_path).contains("webhook"))
            .then_some(())
        },
        "basic-delivery topology labels/workers/webhook logs",
    );
}

fn wait_for_issue_rewrite(base: &str, token: &str, deadline: Instant, run: &RunGuard) -> Value {
    poll(
        deadline,
        run,
        || {
            let issues = api_json(base, token, "/api/v1/repos/acme/service/issues?state=all");
            let arr = issues.as_array()?;
            let non_pr: Vec<_> = arr
                .iter()
                .filter(|i| i.get("pull_request").is_none())
                .collect();
            assert!(
                non_pr.len() <= 1,
                "expected at most one intake issue, got {non_pr:#?}"
            );
            let issue = (*non_pr.first()?).clone();
            let body = issue["body"].as_str()?;
            (body != INTAKE_BODY
                && [
                    "BANNER_GREETING",
                    "Hello from the basic-delivery demo",
                    "src/banner.sh",
                    "sh -n src/banner.sh",
                ]
                .iter()
                .all(|s| body.contains(s)))
            .then_some(issue)
        },
        "architect issue rewrite",
    )
}

fn assert_architect_issue_state(issue: &Value) {
    let issue_labels = labels(issue);
    assert!(
        issue_labels.contains(&"code".to_string()),
        "architect triage should add code label; observed labels: {issue_labels:?}; issue: {issue:#}"
    );
    let body = issue["body"].as_str().unwrap_or_default();
    for expected in [
        "BANNER_GREETING",
        "Hello from the basic-delivery demo",
        "src/banner.sh",
        "sh -n src/banner.sh",
    ] {
        assert!(
            body.contains(expected),
            "architect-rewritten issue body missing {expected:?}; body: {body}"
        );
    }
}

fn wait_for_pr(base: &str, token: &str, deadline: Instant, run: &RunGuard) -> (u64, Value) {
    poll(
        deadline,
        run,
        || {
            let pulls = api_json(base, token, "/api/v1/repos/acme/service/pulls?state=all");
            let arr = pulls.as_array()?;
            assert!(
                arr.len() <= 1,
                "expected at most one implementation PR while polling, got {arr:#?}"
            );
            let pr = (arr.len() == 1).then(|| arr[0].clone())?;
            let number = pr["number"]
                .as_u64()
                .expect("observed implementation PR should have a number");
            Some((number, pr))
        },
        "exactly one implementation PR",
    )
}

fn wait_for_post_engineer_issue_state(
    base: &str,
    token: &str,
    deadline: Instant,
    run: &RunGuard,
) -> Value {
    poll(
        deadline,
        run,
        || {
            let issue = api_json(base, token, "/api/v1/repos/acme/service/issues/1");
            let issue_labels = labels(&issue);
            (issue_labels.contains(&"in-progress".to_string())
                && !issue_labels.contains(&"ready".to_string()))
            .then_some(issue)
        },
        "issue #1 post-engineer labels (expected in-progress and not ready after implementation PR opened)",
    )
}

fn wait_for_ci_and_merge(
    base: &str,
    token: &str,
    pr_number: u64,
    deadline: Instant,
    run: &RunGuard,
) {
    poll(
        deadline,
        run,
        || {
            let pr_path = format!("/api/v1/repos/acme/service/pulls/{pr_number}");
            let pr = api_json(base, token, &pr_path);
            let statuses = api_json(
                base,
                token,
                "/api/v1/repos/acme/service/commits/main/statuses",
            );
            let logs = log_tail(&run.output_path);
            (pr["merged"].as_bool().unwrap_or(false)
                && logs.contains("success")
                && statuses.is_array())
            .then_some(())
        },
        &format!("CI success and bot merge for PR #{pr_number}"),
    )
}

fn api_json(base: &str, token: &str, path: &str) -> Value {
    let output = Command::new("curl")
        .args([
            "-fsS",
            "-H",
            &format!("Authorization: token {token}"),
            &format!("{base}{path}"),
        ])
        .output()
        .expect("run curl");
    if !output.status.success() {
        return Value::Null;
    }
    serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
}

fn labels(value: &Value) -> Vec<String> {
    value["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["name"].as_str().map(str::to_string))
        .collect()
}

fn poll<T>(deadline: Instant, run: &RunGuard, mut f: impl FnMut() -> Option<T>, what: &str) -> T {
    while Instant::now() < deadline {
        if let Some(v) = f() {
            return v;
        }
        assert!(
            run.child_still_running(),
            "run.sh exited before {what}; diagnostics: {}",
            run.diagnostics()
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    panic!(
        "timed out waiting for {what}; diagnostics: {}",
        run.diagnostics()
    );
}

struct RunGuard {
    child: Child,
    example: PathBuf,
    output_path: PathBuf,
}
impl RunGuard {
    fn child_still_running(&self) -> bool {
        Command::new("kill")
            .arg("-0")
            .arg(self.child.id().to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    fn diagnostics(&self) -> String {
        format!(
            "run.sh tail:\n{}\nlogs tail:\n{}",
            log_tail(&self.output_path),
            logs_tail(&self.example.join("logs"))
        )
    }
    fn stop(&mut self) {
        let _ = Command::new("/bin/sh")
            .arg(self.example.join("run.sh"))
            .arg("stop")
            .current_dir(&self.example)
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for RunGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

fn log_tail(path: &Path) -> String {
    fs::read_to_string(path)
        .map(|s| tail(&s, 120))
        .unwrap_or_default()
}
fn logs_tail(dir: &Path) -> String {
    let mut out = String::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                out.push_str(&format!("\n== {} ==\n{}", p.display(), log_tail(&p)));
            }
        }
    }
    out
}
fn tail(s: &str, n: usize) -> String {
    let lines: Vec<_> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

struct TempDir {
    path: PathBuf,
}
impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn copy_dir(src: &Path, dst: &Path) {
    let status = Command::new("cp")
        .arg("-a")
        .arg(src)
        .arg(dst)
        .status()
        .expect("copy basic-delivery fixture");
    assert!(
        status.success(),
        "cp -a {} {} failed",
        src.display(),
        dst.display()
    );
}
