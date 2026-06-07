use std::path::PathBuf;

use jig_core::{Reply, Script};
use jig_server::FakeLlm;
use serde::Deserialize;
use smith_temper_agent::{ProviderConfig, run_decision};
use temper_workflow::{RawWorkflowSpec, RoleId, RoleManifest};

#[derive(Debug, Deserialize)]
struct RoleDecision {
    action: String,
    #[allow(dead_code)]
    #[serde(default)]
    reason: String,
}

#[test]
fn deepseek_openai_compatible_decision_against_jig() {
    let runtime = runtime();
    let fake = fixed_decision_fake();
    let provider = ProviderConfig::new("deepseek", "deepseek-chat", fake.base_url(), "test-key");

    let decision = run_fixture_decision(&runtime, &provider);

    assert_eq!(decision.action, "advance");
}

#[test]
fn anthropic_oauth_decision_against_jig() {
    let runtime = runtime();
    let fake = fixed_decision_fake();
    let provider = ProviderConfig::anthropic_oauth(Some(jig_auth_fixture()))
        .with_base_url_override(fake.base_url());

    let decision = run_fixture_decision(&runtime, &provider);

    assert_eq!(decision.action, "advance");
}

#[test]
fn chatgpt_oauth_decision_against_jig() {
    let runtime = runtime();
    let fake = fixed_decision_fake();
    let provider = ProviderConfig::chatgpt_oauth(None, Some(jig_auth_fixture()))
        .with_base_url_override(fake.base_url());

    let decision = run_fixture_decision(&runtime, &provider);

    assert_eq!(decision.action, "advance");
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
}

fn fixed_decision_fake() -> FakeLlm {
    FakeLlm::start(Script::Fixed(Reply::text(r#"{"action":"advance"}"#))).expect("start fake LLM")
}

fn run_fixture_decision(
    runtime: &tokio::runtime::Runtime,
    provider: &ProviderConfig,
) -> RoleDecision {
    let role = fixture_role_manifest();
    let context = fixture_role_context(&role);
    runtime
        .block_on(run_decision::<RoleDecision>(
            provider,
            &role.prompt.render(),
            &context,
        ))
        .expect("jig-backed workflow-role decision succeeds and parses")
}

fn jig_auth_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jig_auth.json")
}

fn fixture_role_manifest() -> RoleManifest {
    let json = r#"{
        "name": "jig-generic-role-smoke",
        "roles": [{
            "id": "banana",
            "prompt": {
                "guidance": "When the work item is a task in the todo queue with the todo label, choose the advance action."
            },
            "queues": ["todo"]
        }],
        "labels": [{"id": "task"}, {"id": "todo"}, {"id": "done"}],
        "artifact_kinds": [{
            "id": "task",
            "target": "issue",
            "identifying_labels": ["task"]
        }],
        "queues": [{"id": "todo", "artifact": "task", "labels": ["todo"]}],
        "transitions": [{
            "id": "advance",
            "artifact": "task",
            "roles": ["banana"],
            "effects": [
                {"kind": "remove_label", "label": "todo"},
                {"kind": "add_label", "label": "done"}
            ]
        }]
    }"#;
    let spec: RawWorkflowSpec = serde_json::from_str(json).expect("workflow json parses");
    spec.validate()
        .expect("workflow validates")
        .compile()
        .role(&RoleId::new("banana"))
        .expect("banana role manifest exists")
        .clone()
}

fn fixture_role_context(role: &RoleManifest) -> String {
    let context = serde_json::json!({
        "work_item": {
            "repository": "forgejo:acme/service",
            "queue": "todo",
            "role": role.id.as_str(),
            "kind": "task",
            "artifact": {
                "type": "issue",
                "number": 1,
                "title": "Advance a generic task",
                "body": "This synthetic task is ready for the generic action.",
                "labels": ["task", "todo"],
                "state": "Open"
            }
        },
        "allowed_actions": ["no_action", "advance"],
        "authorized_actions": [{
            "action": "advance",
            "transition": "advance",
            "artifact": "task",
            "requires_gates": []
        }]
    });
    serde_json::to_string_pretty(&context).expect("context serializes")
}
