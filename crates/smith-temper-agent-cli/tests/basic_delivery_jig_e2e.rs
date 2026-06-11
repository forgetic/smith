//! Provider-free basic-delivery process E2E.
//!
//! This ignored test boots the checked-in `examples/basic-delivery` launcher with
//! the deterministic local greeting coding-agent stand-in. It is intentionally
//! not part of default CI because it starts real Forgejo, forgejo-runner, one
//! Temper daemon, and one Smith worker process.

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

    let temp = TempDir::new("basic-delivery-jig-e2e");
    let example = temp.path.join("basic-delivery");
    copy_dir(&real_example, &example);

    let forgejo_port = unused_loopback_port();
    let daemon_port = unused_loopback_port();
    let base_url = format!("http://127.0.0.1:{forgejo_port}");
    let daemon_bind = format!("127.0.0.1:{daemon_port}");
    let output_path = temp.path.join("run.sh.out");
    let output = fs::File::create(&output_path).expect("create run.sh output capture");
    let output_err = output.try_clone().expect("clone output capture");

    let mut command = Command::new("/bin/sh");
    command
        .arg(example.join("run.sh"))
        .current_dir(&example)
        .env("SMITH_WORKSPACE_ROOT", &smith_root)
        .env("TEMPER_WORKSPACE_ROOT", temper_root(&smith_root))
        .env("BASIC_DELIVERY_CODER", "greeting")
        .env("BASE_URL", &base_url)
        .env("DAEMON_BIND", &daemon_bind)
        .env("RUN_SECS", "240")
        .env("DAEMON_POLL_CADENCE_SECS", "120")
        .env("DAEMON_MECHANICAL_CADENCE_SECS", "1")
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

    let (pr_number, _pr) = wait_for_pr(&base_url, &read_token, deadline, &run);
    wait_for_implementation_pr_label(&base_url, &read_token, pr_number, deadline, &run);

    wait_for_ci_and_merge(&base_url, &read_token, pr_number, deadline, &run);
    let post_engineer_issue = wait_for_source_issue_closed(&base_url, &read_token, deadline, &run);
    assert_eq!(post_engineer_issue["number"], 1);

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
    wait_for_mechanical_merge_evidence(&run, pr_number, deadline);

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
        "TEMPER_DAEMON_BIN",
        "TEMPER_PROVISION_BIN",
        "SMITH_WORKER_BIN",
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
        let log_dir = run.example.join("logs");
        let daemon_log = log_tail(&log_dir.join("daemon.log"));
        let worker_log = log_tail(&log_dir.join("worker.log"));
        if needed
            .iter()
            .all(|n| diagnostics.observed_labels.iter().any(|s| s == n))
            && has_worker_evidence(&worker_log, "architect")
            && has_worker_evidence(&worker_log, "engineer")
            && has_mechanical_evidence(&daemon_log)
            && has_trigger_evidence(&daemon_log)
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
    logs.contains("smith-worker: registered")
        && logs.contains("smith-worker: assigned job_id=")
        && logs.contains(&format!("role={worker}"))
}

fn has_mechanical_evidence(daemon_log: &str) -> bool {
    daemon_log.contains("mechanical")
        || daemon_log.contains("ci_gate")
        || daemon_log.contains("land_pr")
        || daemon_log.to_ascii_lowercase().contains("merge")
}

fn has_trigger_evidence(daemon_log: &str) -> bool {
    daemon_log.contains("webhook accepted")
}

fn wait_for_issue_rewrite(
    base: &str,
    token: &ReadToken,
    deadline: Instant,
    run: &RunGuard,
) -> Value {
    let last = Arc::new(Mutex::new(String::from(
        "issue #1 has not been requested yet",
    )));
    let write_last = Arc::clone(&last);
    poll_with_diagnostics(
        deadline,
        run,
        &mut || {
            let issue_path = "/api/v1/repos/acme/service/issues/1";
            let issue = match try_api_json(base, &token.name, &token.value, issue_path) {
                Ok(issue) => issue,
                Err(e) => {
                    *write_last.lock().unwrap() =
                        format!("issue #1 could not be read while polling architect rewrite: {e}");
                    return None;
                }
            };

            let body = match issue["body"].as_str() {
                Some(body) => body,
                None => {
                    *write_last.lock().unwrap() = format!(
                        "issue #1 was readable via GET {issue_path} with {}, but the body was missing or not a string; issue: {}",
                        token.name,
                        small_summary(&format!("{issue:#}"))
                    );
                    return None;
                }
            };

            let expected_markers = [
                "BANNER_GREETING",
                "Hello from the basic-delivery demo",
                "src/banner.sh",
                "sh -n src/banner.sh",
            ];
            let missing = expected_markers
                .iter()
                .copied()
                .filter(|marker| !body.contains(marker))
                .collect::<Vec<_>>();
            if body == INTAKE_BODY {
                *write_last.lock().unwrap() = format!(
                    "issue #1 was readable via GET {issue_path} with {}, but still had the original thin intake body",
                    token.name
                );
                return None;
            }
            if !missing.is_empty() {
                *write_last.lock().unwrap() = format!(
                    "issue #1 was readable and rewritten via GET {issue_path} with {}, but missing expected greeting spec markers: {missing:?}; body snippet: {}",
                    token.name,
                    small_summary(body)
                );
                return None;
            }

            *write_last.lock().unwrap() = format!(
                "issue #1 satisfied the architect rewrite condition via GET {issue_path} with {}",
                token.name
            );
            Some(issue)
        },
        "architect issue #1 rewrite with greeting-ready-code markers",
        || last.lock().unwrap().clone(),
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

fn wait_for_implementation_pr_label(
    base: &str,
    token: &ReadToken,
    pr_number: u64,
    deadline: Instant,
    run: &RunGuard,
) {
    let diagnostics = Arc::new(Mutex::new(PrLabelDiagnostics::new(pr_number)));
    let write_diagnostics = Arc::clone(&diagnostics);
    poll_with_diagnostics(
        deadline,
        run,
        &mut || {
            let mut latest = PrLabelDiagnostics::new(pr_number);
            let pull_path = format!("/api/v1/repos/acme/service/pulls/{pr_number}");
            if let Some(labels) =
                poll_pr_label_endpoint(base, token, &pull_path, LabelSource::Pull, &mut latest)
            {
                if labels.iter().any(|label| label == "implementation") {
                    *write_diagnostics.lock().unwrap() = latest;
                    return Some(());
                }
            }

            let issue_path = format!("/api/v1/repos/acme/service/issues/{pr_number}");
            if let Some(labels) =
                poll_pr_label_endpoint(base, token, &issue_path, LabelSource::Issue, &mut latest)
            {
                if labels.iter().any(|label| label == "implementation") {
                    *write_diagnostics.lock().unwrap() = latest;
                    return Some(());
                }
            }

            *write_diagnostics.lock().unwrap() = latest;
            None
        },
        &format!("implementation label visibility for PR #{pr_number}"),
        || diagnostics.lock().unwrap().summary(&token.name),
    )
}

#[derive(Clone, Copy)]
enum LabelSource {
    Pull,
    Issue,
}

#[derive(Clone, Debug)]
struct EndpointLabelDiagnostics {
    status: Option<String>,
    labels: Option<Vec<String>>,
    state: Option<String>,
    error: Option<String>,
}

impl EndpointLabelDiagnostics {
    fn status_summary(&self) -> String {
        self.status
            .as_deref()
            .unwrap_or("transport-error")
            .to_string()
    }

    fn error_summary(&self) -> String {
        self.error.as_deref().unwrap_or("<none>").to_string()
    }
}

#[derive(Clone, Debug)]
struct PrLabelDiagnostics {
    pr_number: u64,
    pull: Option<EndpointLabelDiagnostics>,
    issue: Option<EndpointLabelDiagnostics>,
}

impl PrLabelDiagnostics {
    fn new(pr_number: u64) -> Self {
        Self {
            pr_number,
            pull: None,
            issue: None,
        }
    }

    fn summary(&self, token_name: &str) -> String {
        let pull_status = self
            .pull
            .as_ref()
            .map(EndpointLabelDiagnostics::status_summary)
            .unwrap_or_else(|| "not-requested".to_string());
        let pull_state = self
            .pull
            .as_ref()
            .and_then(|d| d.state.as_deref())
            .unwrap_or("<unknown>");
        let pull_labels = self
            .pull
            .as_ref()
            .and_then(|d| d.labels.clone())
            .unwrap_or_default();
        let issue_status = self
            .issue
            .as_ref()
            .map(EndpointLabelDiagnostics::status_summary)
            .unwrap_or_else(|| "not-requested".to_string());
        let issue_labels = self
            .issue
            .as_ref()
            .and_then(|d| d.labels.clone())
            .unwrap_or_default();
        let pull_error = self
            .pull
            .as_ref()
            .map(EndpointLabelDiagnostics::error_summary)
            .unwrap_or_else(|| "<none>".to_string());
        let issue_error = self
            .issue
            .as_ref()
            .map(EndpointLabelDiagnostics::error_summary)
            .unwrap_or_else(|| "<none>".to_string());
        format!(
            "implementation label did not appear for PR #{} before timeout; latest pull endpoint via {}: status={} state={} labels={:?}; latest issue endpoint via {}: status={} labels={:?}; last errors: pull={}, issue={}",
            self.pr_number,
            token_name,
            pull_status,
            pull_state,
            pull_labels,
            token_name,
            issue_status,
            issue_labels,
            pull_error,
            issue_error
        )
    }
}

fn poll_pr_label_endpoint(
    base: &str,
    token: &ReadToken,
    path: &str,
    source: LabelSource,
    diagnostics: &mut PrLabelDiagnostics,
) -> Option<Vec<String>> {
    let result = try_api_json_with_status(base, &token.name, &token.value, path);
    let endpoint = match result {
        Ok((json, status)) => EndpointLabelDiagnostics {
            status: Some(status),
            labels: Some(labels(&json)),
            state: json["state"].as_str().map(str::to_string),
            error: None,
        },
        Err(e) => EndpointLabelDiagnostics {
            status: e.status,
            labels: None,
            state: None,
            error: Some(e.message),
        },
    };
    let labels = endpoint.labels.clone();
    match source {
        LabelSource::Pull => diagnostics.pull = Some(endpoint),
        LabelSource::Issue => diagnostics.issue = Some(endpoint),
    }
    labels
}

fn wait_for_source_issue_closed(
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
                "source issue closed-state checks",
            );
            (issue["state"].as_str() == Some("closed")).then_some(issue)
        },
        "source issue #1 closed after CI-green bot merge",
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

/// Waits for the daemon's mechanical `land_pr` evidence to reach the log file.
///
/// The daemon's runner-event JSON lines go to a block-buffered stdout pipe, so
/// the merge can be visible through the API before the matching log line is
/// flushed; poll instead of asserting on a single read.
fn wait_for_mechanical_merge_evidence(run: &RunGuard, pr_number: u64, deadline: Instant) {
    let daemon_path = run.example.join("logs/daemon.log");
    poll(
        deadline,
        run,
        || {
            let daemon = fs::read_to_string(&daemon_path).unwrap_or_default();
            has_merge_evidence_for_pr(&daemon, pr_number).then_some(())
        },
        &format!("mechanical merge evidence for PR #{pr_number} in logs/daemon.log"),
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
        // The daemon's mechanical backstop logs the landing as a structured
        // `mechanical_automation_execution` JSON event identifying the PR as
        // `"artifact_number":<n>` with `"transition":"land_pr"`.
        format!("\"artifact_number\":{pr_number}"),
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
        .unwrap_or_else(|e| panic!("{what} failed: {}", e.message))
}

fn try_api_json(
    base: &str,
    token_name: &str,
    token: &str,
    path: &str,
) -> Result<Value, ApiJsonError> {
    try_api_json_with_status(base, token_name, token, path).map(|(json, _)| json)
}

#[derive(Debug)]
struct ApiJsonError {
    status: Option<String>,
    message: String,
}

impl std::fmt::Display for ApiJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiJsonError {}

fn try_api_json_with_status(
    base: &str,
    token_name: &str,
    token: &str,
    path: &str,
) -> Result<(Value, String), ApiJsonError> {
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
        .map_err(|e| ApiJsonError {
            status: None,
            message: format!("GET {path} with {token_name} => transport error: {e}"),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (body, status) = stdout
        .rsplit_once('\n')
        .unwrap_or((stdout.as_ref(), "unknown"));
    let status = status.to_string();
    let status_code = status.parse::<u16>().ok();
    if !output.status.success() || !matches!(status_code, Some(200..=299)) {
        return Err(ApiJsonError {
            status: Some(status.clone()),
            message: format!(
                "GET {path} with {token_name} => {status}; body: {}",
                small_summary(body)
            ),
        });
    }
    serde_json::from_str::<Value>(body)
        .map(|json| (json, status.clone()))
        .map_err(|e| ApiJsonError {
            status: Some(status.clone()),
            message: format!(
                "GET {path} with {token_name} => {status} JSON parse failed: {e}; body: {}",
                small_summary(body)
            ),
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
    poll_with_diagnostics(deadline, run, &mut f, what, String::new)
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
        // The sandbox copy lives under a temp dir, so the launcher cannot
        // derive the workspace roots from its own location; pass them
        // explicitly or `run.sh stop` dies resolving `../temper` under
        // `set -eu` and leaves the boot processes running.
        let _ = Command::new("/bin/sh")
            .arg(self.example.join("run.sh"))
            .arg("stop")
            .current_dir(&self.example)
            .env("SMITH_WORKSPACE_ROOT", repo_root())
            .env("TEMPER_WORKSPACE_ROOT", temper_root(&repo_root()))
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
