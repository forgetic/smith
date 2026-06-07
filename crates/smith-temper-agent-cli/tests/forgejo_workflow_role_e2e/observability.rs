use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::Value;
use smith_temper_agent::WORKFLOW_ROLE_DECISION_CAPTURE_DIR_ENV;
use temper_runner::WorkItemIdentity;

pub(crate) struct ObservabilityProbe {
    wrapper: PathBuf,
    stderr_log: PathBuf,
    capture_dir: PathBuf,
}

impl ObservabilityProbe {
    pub(crate) fn new(root: &Path, responder: &str, provider_base_url: &str) -> Self {
        let capture_dir = root.join("smith-captures");
        fs::create_dir_all(&capture_dir).expect("capture dir creates");
        let stderr_log = root.join("smith-observability-stderr.log");
        let wrapper = root.join("smith-observability-wrapper.sh");
        let script = format!(
            "#!/bin/sh\n{}={}\nSMITH_TEST_PROVIDER_BASE_URL={}\nexport {} SMITH_TEST_PROVIDER_BASE_URL\nexec {} \"$@\" 2>>{}\n",
            WORKFLOW_ROLE_DECISION_CAPTURE_DIR_ENV,
            shell_quote_path(capture_dir.as_path()),
            shell_quote(provider_base_url),
            WORKFLOW_ROLE_DECISION_CAPTURE_DIR_ENV,
            shell_quote_path(Path::new(responder)),
            shell_quote_path(stderr_log.as_path()),
        );
        fs::write(&wrapper, script).expect("wrapper writes");
        make_executable(&wrapper);
        Self {
            wrapper,
            stderr_log,
            capture_dir,
        }
    }

    pub(crate) fn wrapper(&self) -> &Path {
        &self.wrapper
    }

    pub(crate) fn assert_smith_logs_and_capture(
        &self,
        identity: &WorkItemIdentity,
        expected_action: &str,
        forbidden_values: &[String],
    ) {
        let stderr = fs::read_to_string(&self.stderr_log).expect("Smith stderr log is readable");
        let events = smith_events(&stderr);
        assert!(!events.is_empty(), "no Smith JSON events in stderr log");

        for event_name in [
            "smith.workflow_role_decision.request",
            "smith.workflow_role_decision.provider_call.start",
            "smith.workflow_role_decision.provider_call.finish",
            "smith.workflow_role_decision.reply",
            "smith.workflow_role_decision.capture.written",
        ] {
            assert_event_identity(event(&events, event_name), identity);
        }

        let request = event(&events, "smith.workflow_role_decision.request");
        assert_json_string_array_contains(&request["allowed_actions"], "no_action");
        assert_json_string_array_contains(&request["allowed_actions"], expected_action);
        assert_json_string_array_contains(&request["available_external_tools"], "coding_workspace");

        let provider_finish = event(&events, "smith.workflow_role_decision.provider_call.finish");
        assert_eq!(provider_finish["outcome"], "ok");
        assert!(
            json_string(provider_finish, "model_action")
                .is_some_and(|action| !action.trim().is_empty())
        );

        let reply = event(&events, "smith.workflow_role_decision.reply");
        assert_eq!(reply["returned_action"], expected_action);
        assert!(json_string(reply, "reason_preview").is_some_and(|reason| !reason.is_empty()));

        let capture_event = event(&events, "smith.workflow_role_decision.capture.written");
        let capture_path = PathBuf::from(
            json_string(capture_event, "capture_path").expect("capture_path is logged"),
        );
        assert!(
            capture_path.starts_with(&self.capture_dir),
            "capture path {capture_path:?} should stay under {:?}",
            self.capture_dir
        );

        let capture_files = capture_files(&self.capture_dir);
        assert_eq!(capture_files.len(), 1, "expected one Smith capture file");
        assert_eq!(capture_files[0], capture_path);
        let capture_json = fs::read_to_string(&capture_path).expect("capture file is readable");
        let capture: Value = serde_json::from_str(&capture_json).expect("capture JSON parses");
        assert_eq!(capture["trace"]["work_item_id"], identity.work_item_id);
        assert_eq!(capture["trace"]["decision_id"], identity.decision_id);
        assert_eq!(capture["trace"]["tick_id"], "smith-forgejo-e2e-tick");
        assert_eq!(capture["final_reply"]["action"], expected_action);
        assert!(
            capture["final_reply"]["reason_preview"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
        assert_json_string_array_contains(&capture["allowed_actions"], "no_action");
        assert_json_string_array_contains(&capture["allowed_actions"], expected_action);
        assert_json_string_array_contains(
            &capture["available_external_tool_ids"],
            "coding_workspace",
        );
        assert_text_capture_is_bounded_preview(&capture["prompt"]);
        assert_text_capture_is_bounded_preview(&capture["context"]);
        assert!(capture.get("system_prompt").is_none());
        assert!(capture.get("user_context").is_none());
        assert!(capture.get("raw_prompt").is_none());
        assert!(capture.get("raw_context").is_none());

        let combined = format!("{stderr}\n{capture_json}");
        assert!(!combined.contains("auth.json"));
        assert!(!combined.contains(".pi/agent"));
        assert!(!combined.contains("access_token"));
        assert!(!combined.contains("refresh_token"));
        for value in forbidden_values {
            assert_absent_if_non_empty(&combined, value);
        }
    }
}

pub(crate) fn forbidden_observability_values(
    engineer_token: &str,
    engineer_password: &str,
    auth_fixture: &Path,
) -> Vec<String> {
    let mut values = vec![
        engineer_token.to_string(),
        engineer_password.to_string(),
        auth_fixture.display().to_string(),
        "jig-dummy".to_string(),
        "eyJhbGciOiAibm9uZSJ9.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOiB7ImNoYXRncHRfYWNjb3VudF9pZCI6ICJhY2N0X2ppZyJ9fQ.".to_string(),
        "acct_jig".to_string(),
    ];
    values.retain(|value| !value.is_empty());
    values
}

fn smith_events(stderr: &str) -> Vec<Value> {
    stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| {
            value
                .get("event")
                .and_then(Value::as_str)
                .is_some_and(|event| event.starts_with("smith.workflow_role_decision."))
        })
        .collect()
}

fn event<'a>(events: &'a [Value], name: &str) -> &'a Value {
    events
        .iter()
        .find(|event| event["event"] == name)
        .unwrap_or_else(|| panic!("missing Smith event {name}; events: {events:?}"))
}

fn assert_event_identity(event: &Value, identity: &WorkItemIdentity) {
    assert_eq!(event["work_item_id"], identity.work_item_id);
    assert_eq!(event["decision_id"], identity.decision_id);
    assert_eq!(event["tick_id"], "smith-forgejo-e2e-tick");
    assert_eq!(event["repository"], identity.repo.to_string());
    assert_eq!(event["role"], identity.role.to_string());
    assert_eq!(event["work_item_role"], identity.role.to_string());
    assert_eq!(event["queue"], identity.queue.to_string());
    assert_eq!(event["artifact_type"], identity.artifact_type.as_str());
    assert_eq!(
        event["artifact_number"],
        identity.artifact_number.get().to_string()
    );
    assert_eq!(event["kind"], identity.artifact_kind.to_string());
}

fn assert_json_string_array_contains(value: &Value, expected: &str) {
    let values = value.as_array().expect("expected JSON array");
    assert!(
        values.iter().any(|value| value.as_str() == Some(expected)),
        "expected {expected:?} in {values:?}"
    );
}

fn assert_text_capture_is_bounded_preview(value: &Value) {
    let chars = value["chars"].as_u64().expect("text capture has chars");
    let preview = value["preview"].as_str().expect("text capture has preview");
    assert!(chars > 0);
    assert!(!preview.is_empty());
    assert!(preview.chars().count() <= 1_001);
    assert!(value.get("text").is_none());
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn capture_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(dir)
        .expect("capture dir is readable")
        .map(|entry| entry.expect("capture dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn assert_absent_if_non_empty(haystack: &str, needle: &str) {
    if !needle.trim().is_empty() {
        assert!(
            !haystack.contains(needle),
            "secret-like value leaked into Smith observability"
        );
    }
}

fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.as_os_str().to_string_lossy())
}

fn shell_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).expect("wrapper metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).expect("wrapper chmod");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
