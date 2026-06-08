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
    sync::{Arc, Mutex},
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
    let read_token = wait_for_read_token(
        &base_url,
        &example.join("secrets/roles.env"),
        deadline,
        &run,
    );
    let issue = wait_for_issue_rewrite(&base_url, &read_token, deadline, &run);
    assert_eq!(issue["number"], 1);
    assert_architect_issue_state(&issue);

    let (pr_number, pr) = wait_for_pr(&base_url, &read_token, deadline, &run);
    let pr_labels = labels(&pr);
    assert!(
        pr_labels.contains(&"implementation".to_string()),
        "PR #{pr_number} labels: {pr:#}"
    );

    let post_engineer_issue =
        wait_for_post_engineer_issue_state(&base_url, &read_token, deadline, &run);
    assert_eq!(post_engineer_issue["number"], 1);

    wait_for_ci_and_merge(&base_url, &read_token, pr_number, deadline, &run);

    let final_pr_path = format!("/api/v1/repos/acme/service/pulls/{pr_number}");
    let pr = api_json(&base_url, &read_token, &final_pr_path, "final PR state");
    assert_eq!(pr["state"], "closed", "final PR #{pr_number} state: {pr:#}");
    assert!(
        pr["merged"].as_bool().unwrap_or(false),
        "observed PR #{pr_number} was not merged by bot: {pr:#}"
    );
    let final_file = api_json(
        &base_url,
        &read_token,
        "/api/v1/repos/acme/service/contents/src/banner.sh?ref=main",
        "final src/banner.sh lookup",
    );
    assert_eq!(
        final_file["name"], "banner.sh",
        "main lacks src/banner.sh: {final_file:#}"
    );
    assert_mechanical_merge_evidence(&run, pr_number);

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
            fs::read_to_string(&roles)
                .ok()
                .and_then(|s| parse_bot_token(&s))
        },
        "bot token",
    )
}

fn parse_roles_env(roles_env: &str) -> Vec<(String, String)> {
    roles_env
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let value = value.trim();
            let value = if (value.starts_with('\'') && value.ends_with('\''))
                || (value.starts_with('"') && value.ends_with('"'))
            {
                &value[1..value.len().saturating_sub(1)]
            } else {
                value
            };
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn parse_bot_token(roles_env: &str) -> Option<String> {
    parse_roles_env(roles_env)
        .into_iter()
        .find(|(name, _)| name == "TEMPER_FORGEJO_BOT_TOKEN")
        .or_else(|| {
            parse_roles_env(roles_env)
                .into_iter()
                .find(|(name, _)| name == "TEMPER_FORGEJO_TOKEN_BOT")
        })
        .map(|(_, token)| token)
}

#[test]
fn parses_current_bot_token_before_legacy_fallback() {
    let roles_env = "\
TEMPER_FORGEJO_TOKEN_BOT='legacy-token'\n\
TEMPER_FORGEJO_BOT_TOKEN='current-token'\n";

    assert_eq!(parse_bot_token(roles_env).as_deref(), Some("current-token"));
}

#[test]
fn parses_legacy_bot_token_fallback() {
    assert_eq!(
        parse_bot_token("TEMPER_FORGEJO_TOKEN_BOT='legacy-token'\n").as_deref(),
        Some("legacy-token")
    );
}

#[test]
fn parses_unquoted_current_bot_token() {
    assert_eq!(
        parse_bot_token("TEMPER_FORGEJO_BOT_TOKEN=current-token\n").as_deref(),
        Some("current-token")
    );
}

#[test]
fn parses_double_quoted_role_tokens() {
    assert_eq!(
        parse_roles_env("TEMPER_FORGEJO_TOKEN_ENGINEER=\"engineer-token\"\n"),
        vec![(
            "TEMPER_FORGEJO_TOKEN_ENGINEER".to_string(),
            "engineer-token".to_string()
        )]
    );
}

#[test]
fn label_token_candidates_include_bot_architect_engineer() {
    let roles_env = "\
TEMPER_FORGEJO_BOT_TOKEN='bot-token'\n\
TEMPER_FORGEJO_TOKEN_ARCHITECT=architect-token\n\
TEMPER_FORGEJO_TOKEN_ENGINEER=\"engineer-token\"\n";

    let names: Vec<_> = label_token_candidates(roles_env)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, vec!["bot", "architect", "engineer"]);
}

fn wait_for_topology_evidence(base: &str, _token: &str, deadline: Instant, run: &RunGuard) {
    let labels_path = "/api/v1/repos/acme/service/labels?page=1&limit=50";
    let mut diagnostics = LabelPollDiagnostics::default();
    while Instant::now() < deadline {
        diagnostics = poll_labels(base, labels_path, &run.example.join("secrets/roles.env"));
        let needed = [
            "untriaged",
            "code",
            "ready",
            "in-progress",
            "implementation",
            "landed",
        ];
        let logs = logs_tail(&run.example.join("logs"));
        if needed
            .iter()
            .all(|n| diagnostics.observed_labels.iter().any(|s| s == n))
            && has_worker_evidence(&logs, "architect")
            && has_worker_evidence(&logs, "engineer")
            && has_mechanical_evidence(&logs)
            && has_trigger_evidence(&logs)
        {
            return;
        }
        assert!(
            run.child_still_running(),
            "run.sh exited before basic-delivery topology labels/workers/webhook logs; {}; diagnostics: {}",
            diagnostics.summary(),
            run.diagnostics()
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    panic!(
        "timed out waiting for basic-delivery topology labels/workers/webhook logs; {}; diagnostics: {}",
        diagnostics.summary(),
        run.diagnostics()
    );
}

#[derive(Default)]
struct LabelPollDiagnostics {
    observed_labels: Vec<String>,
    attempts: Vec<LabelPollAttempt>,
}

struct LabelPollAttempt {
    token_name: String,
    status: Option<String>,
    json_ok: bool,
    failure: Option<String>,
}

impl LabelPollDiagnostics {
    fn summary(&self) -> String {
        let attempts = self
            .attempts
            .iter()
            .map(|a| {
                format!(
                    "{} status={} json_ok={}{}",
                    a.token_name,
                    a.status.as_deref().unwrap_or("transport-error"),
                    a.json_ok,
                    a.failure
                        .as_ref()
                        .map(|f| format!(" failure={f}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "observed labels: {:?}; label API attempts: [{}]",
            self.observed_labels, attempts
        )
    }
}

fn poll_labels(base: &str, path: &str, roles_path: &Path) -> LabelPollDiagnostics {
    let mut diagnostics = LabelPollDiagnostics::default();
    let roles_env = match fs::read_to_string(roles_path) {
        Ok(s) => s,
        Err(e) => {
            diagnostics.attempts.push(LabelPollAttempt {
                token_name: "roles.env".to_string(),
                status: None,
                json_ok: false,
                failure: Some(format!("read failed: {e}")),
            });
            return diagnostics;
        }
    };
    for (name, token) in label_token_candidates(&roles_env) {
        let (attempt, labels) = fetch_labels(base, path, &name, &token);
        if let Some(labels) = labels {
            diagnostics.observed_labels = labels;
        }
        diagnostics.attempts.push(attempt);
    }
    diagnostics
}

fn label_token_candidates(roles_env: &str) -> Vec<(String, String)> {
    let parsed = parse_roles_env(roles_env);
    [
        ("bot", "TEMPER_FORGEJO_BOT_TOKEN"),
        ("bot", "TEMPER_FORGEJO_TOKEN_BOT"),
        ("architect", "TEMPER_FORGEJO_TOKEN_ARCHITECT"),
        ("engineer", "TEMPER_FORGEJO_TOKEN_ENGINEER"),
    ]
    .into_iter()
    .filter_map(|(label, wanted)| {
        parsed
            .iter()
            .find(|(name, token)| name == wanted && !token.is_empty())
            .map(|(_, token)| (label.to_string(), token.clone()))
    })
    .collect()
}

fn fetch_labels(
    base: &str,
    path: &str,
    token_name: &str,
    token: &str,
) -> (LabelPollAttempt, Option<Vec<String>>) {
    let output = Command::new("curl")
        .args([
            "-sS",
            "-w",
            "\n%{http_code}",
            "-H",
            &format!("Authorization: token {token}"),
            &format!("{base}{path}"),
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(e) => {
            return (
                LabelPollAttempt {
                    token_name: token_name.to_string(),
                    status: None,
                    json_ok: false,
                    failure: Some(format!("transport error: {e}")),
                },
                None,
            );
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (body, status) = stdout
        .rsplit_once('\n')
        .unwrap_or((stdout.as_ref(), "unknown"));
    let mut attempt = LabelPollAttempt {
        token_name: token_name.to_string(),
        status: Some(status.to_string()),
        json_ok: false,
        failure: None,
    };
    let status_code = status.parse::<u16>().ok();
    if !output.status.success() || !matches!(status_code, Some(200..=299)) {
        attempt.failure = Some(format!("HTTP status {status}"));
        return (attempt, None);
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Array(arr)) => {
            attempt.json_ok = true;
            let labels = arr
                .iter()
                .filter_map(|l| l["name"].as_str().map(str::to_string))
                .collect::<Vec<_>>();
            (attempt, Some(labels))
        }
        Ok(other) => {
            attempt.json_ok = true;
            attempt.failure = Some(format!("non-list JSON response: {other:#}"));
            (attempt, None)
        }
        Err(e) => {
            attempt.failure = Some(format!("JSON parse failed: {e}"));
            (attempt, None)
        }
    }
}

fn has_worker_evidence(logs: &str, worker: &str) -> bool {
    logs.contains(&format!("role:{worker}")) || logs.contains(&format!("worker={worker}"))
}

fn has_mechanical_evidence(logs: &str) -> bool {
    logs.to_ascii_lowercase().contains("mechanical")
}

fn has_trigger_evidence(logs: &str) -> bool {
    logs.contains("listening on") || logs.contains("webhook accepted")
}

fn wait_for_issue_rewrite(
    base: &str,
    token: &ReadToken,
    deadline: Instant,
    run: &RunGuard,
) -> Value {
    poll(
        deadline,
        run,
        || {
            let issues = api_json(
                base,
                token,
                "/api/v1/repos/acme/service/issues?state=all",
                "architect issue rewrite polling",
            );
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

fn wait_for_pr(base: &str, token: &ReadToken, deadline: Instant, run: &RunGuard) -> (u64, Value) {
    poll(
        deadline,
        run,
        || {
            let pulls = api_json(
                base,
                token,
                "/api/v1/repos/acme/service/pulls?state=all",
                "implementation PR discovery",
            );
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
    token: &ReadToken,
    deadline: Instant,
    run: &RunGuard,
) -> Value {
    poll(
        deadline,
        run,
        || {
            let issue = api_json(
                base,
                token,
                "/api/v1/repos/acme/service/issues/1",
                "post-engineer issue state checks",
            );
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
    token: &ReadToken,
    pr_number: u64,
    deadline: Instant,
    run: &RunGuard,
) {
    poll(
        deadline,
        run,
        || {
            let pr_path = format!("/api/v1/repos/acme/service/pulls/{pr_number}");
            let pr = api_json(base, token, &pr_path, "PR merged-state polling");
            let statuses = api_json(
                base,
                token,
                "/api/v1/repos/acme/service/commits/main/statuses",
                "commit status polling",
            );
            let runner_logs = log_tail(&run.example.join("logs/runner.log"));
            let ci_success = runner_logs.contains("Success")
                || runner_logs.contains("Job succeeded")
                || (runner_logs.contains("sh -n src/banner.sh")
                    && pr["merged"].as_bool().unwrap_or(false));
            (pr["merged"].as_bool().unwrap_or(false) && ci_success && statuses.is_array())
                .then_some(())
        },
        &format!("CI success and bot merge for PR #{pr_number}"),
    )
}

fn assert_mechanical_merge_evidence(run: &RunGuard, pr_number: u64) {
    let mechanical_path = run.example.join("logs/mechanical.log");
    let mechanical = fs::read_to_string(&mechanical_path).unwrap_or_default();
    assert!(
        has_merge_evidence_for_pr(&mechanical, pr_number),
        "mechanical merge evidence for PR #{pr_number} not logged; mechanical.log tail:\n{}\ndiagnostics: {}",
        tail(&mechanical, 200),
        run.diagnostics()
    );
}

fn has_merge_evidence_for_pr(logs: &str, pr_number: u64) -> bool {
    let pr_markers = [
        format!("PR #{pr_number}"),
        format!("pr #{pr_number}"),
        format!("pull/{pr_number}"),
        format!("pulls/{pr_number}"),
        format!("pr_number={pr_number}"),
        format!("\"pr_number\":{pr_number}"),
        format!("number={pr_number}"),
        format!("#{pr_number}"),
    ];
    (logs.contains("land_pr") || logs.to_ascii_lowercase().contains("merge"))
        && pr_markers.iter().any(|marker| logs.contains(marker))
}

#[derive(Clone, Debug)]
struct ReadToken {
    name: String,
    value: String,
}

fn wait_for_read_token(
    base: &str,
    roles_path: &Path,
    deadline: Instant,
    run: &RunGuard,
) -> ReadToken {
    let last = Arc::new(Mutex::new(String::new()));
    let write_last = Arc::clone(&last);
    poll_with_diagnostics(
        deadline,
        run,
        &mut || {
            let roles_env = match fs::read_to_string(roles_path) {
                Ok(s) => s,
                Err(e) => {
                    *write_last.lock().unwrap() = format!("read roles.env failed: {e}");
                    return None;
                }
            };
            let candidates = label_token_candidates(&roles_env);
            let names = candidates
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            for (name, value) in candidates {
                match try_api_json(base, &name, &value, "/api/v1/repos/acme/service/issues/1") {
                    Ok(_) => return Some(ReadToken { name, value }),
                    Err(e) => {
                        *write_last.lock().unwrap() = format!("candidates [{names}]; latest: {e}")
                    }
                }
            }
            None
        },
        "read-capable Forgejo API token",
        || last.lock().unwrap().clone(),
    )
}

fn api_json(base: &str, token: &ReadToken, path: &str, what: &str) -> Value {
    try_api_json(base, &token.name, &token.value, path)
        .unwrap_or_else(|e| panic!("{what} failed: {e}"))
}

fn try_api_json(base: &str, token_name: &str, token: &str, path: &str) -> Result<Value, String> {
    let output = Command::new("curl")
        .args([
            "-sS",
            "-w",
            "\n%{http_code}",
            "-H",
            &format!("Authorization: token {token}"),
            &format!("{base}{path}"),
        ])
        .output()
        .map_err(|e| format!("GET {path} with {token_name} => transport error: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (body, status) = stdout
        .rsplit_once('\n')
        .unwrap_or((stdout.as_ref(), "unknown"));
    let status_code = status.parse::<u16>().ok();
    if !output.status.success() || !matches!(status_code, Some(200..=299)) {
        return Err(format!(
            "GET {path} with {token_name} => {status}; body: {}",
            small_summary(body)
        ));
    }
    serde_json::from_str::<Value>(body).map_err(|e| {
        format!(
            "GET {path} with {token_name} => {status} JSON parse failed: {e}; body: {}",
            small_summary(body)
        )
    })
}

fn small_summary(s: &str) -> String {
    let compact = s.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(160).collect()
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
    poll_with_diagnostics(deadline, run, &mut f, what, || String::new())
}

fn poll_with_diagnostics<T>(
    deadline: Instant,
    run: &RunGuard,
    f: &mut impl FnMut() -> Option<T>,
    what: &str,
    diagnostics: impl Fn() -> String,
) -> T {
    while Instant::now() < deadline {
        if let Some(v) = f() {
            return v;
        }
        assert!(
            run.child_still_running(),
            "run.sh exited before {what}; attempts: {}; diagnostics: {}",
            diagnostics(),
            run.diagnostics()
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    panic!(
        "timed out waiting for {what}; attempts: {}; diagnostics: {}",
        diagnostics(),
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
