use super::*;
use temper_runner::WorkflowRoleDecisionRequest;
use temper_workflow::{ExternalToolId, RawWorkflowSpec, RoleId};

fn fixture_request() -> WorkflowRoleDecisionRequest {
    serde_json::from_str(include_str!(
        "../../../../temper/crates/temper-runner/fixtures/workflow-role-decision-request.json"
    ))
    .expect("Temper workflow-role decision fixture parses")
}

fn request_with_compiled_external_tool(bound: bool) -> WorkflowRoleDecisionRequest {
    let json = r#"{
            "name": "generic-agent-test",
            "roles": [{
                "id": "banana",
                "prompt": {"guidance": "Use open_pr only when coding_workspace is available."},
                "external_tools": [{
                    "id": "coding_workspace",
                    "description": "Edit and commit repository code.",
                    "required": false,
                    "constraints": ["Only touch the checked-out repository."],
                    "guidance": "Produce a real product diff."
                }],
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
    let spec: RawWorkflowSpec = serde_json::from_str(json).expect("workflow parses");
    let manifest = spec
        .validate()
        .expect("workflow validates")
        .compile()
        .role(&RoleId::new("banana"))
        .expect("role manifest exists")
        .clone();
    let available = bound
        .then(|| BoundExternalTool {
            id: ExternalToolId::new("coding_workspace"),
            description: "Edit and commit repository code.".to_string(),
            required: false,
            constraints: vec!["Only touch the checked-out repository.".to_string()],
            guidance: Some("Produce a real product diff.".to_string()),
            provider: "workspace-local".to_string(),
        })
        .into_iter()
        .collect();
    WorkflowRoleDecisionRequest::new(
        "generic-agent-test",
        manifest,
        serde_json::json!({"artifact": {"number": 1}, "queue": "todo"}),
        available,
    )
}

#[test]
fn reads_temper_process_fixture_and_builds_generic_context() {
    let request = fixture_request();
    let prompt = workflow_role_system_prompt(&request);
    let context: serde_json::Value =
        serde_json::from_str(&workflow_role_user_context(&request).expect("context serializes"))
            .expect("context is JSON");

    assert_eq!(
        request.protocol_version,
        WORKFLOW_ROLE_DECISION_PROTOCOL_VERSION
    );
    assert!(prompt.contains("Workflow: generic-agent-test"));
    assert!(prompt.contains("Role: banana"));
    assert_eq!(
        context["allowed_actions"],
        serde_json::json!(["no_action", "advance"])
    );
    assert_eq!(context["work_item"]["artifact"]["number"], 1);
    assert_eq!(context["authorized_actions"][0]["action"], "advance");
    assert_eq!(
        context["available_external_tools"][0]["provider"],
        "workspace-local"
    );
}

#[test]
fn runtime_prompt_lists_only_bound_external_tools() {
    let unbound = workflow_role_system_prompt(&request_with_compiled_external_tool(false));
    assert!(unbound.contains("no external tools are bound"));
    assert!(!unbound.contains("coding_workspace via"));

    let bound = workflow_role_system_prompt(&request_with_compiled_external_tool(true));
    assert!(bound.contains("coding_workspace via workspace-local"));
    assert!(bound.contains("implementation PR creation must use"));
    assert!(bound.contains("Produce a real product diff."));
}

#[test]
fn authorized_and_no_action_model_decisions_echo_request_version() {
    let request = fixture_request();
    let action = reply_for_model_decision(
        &request,
        WorkflowRoleModelDecision::action("advance", "ready"),
    );
    assert_eq!(action.protocol_version, request.protocol_version);
    assert_eq!(action.action, "advance");
    assert_eq!(action.reason, "ready");

    let none = reply_for_model_decision(&request, WorkflowRoleModelDecision::no_action("not safe"));
    assert_eq!(none.action, WORKFLOW_ROLE_DECISION_NO_ACTION);
    assert_eq!(none.reason, "not safe");
}

#[test]
fn unauthorized_model_action_is_returned_as_no_action() {
    let request = fixture_request();
    let reply = reply_for_model_decision(
        &request,
        WorkflowRoleModelDecision::action("delete_everything", "bad"),
    );

    assert_eq!(reply.protocol_version, request.protocol_version);
    assert_eq!(reply.action, WORKFLOW_ROLE_DECISION_NO_ACTION);
    assert!(reply.reason.contains("delete_everything"));
}

#[test]
fn unauthorized_model_action_reason_is_redacted() {
    let request = fixture_request();
    let reply = reply_for_model_decision(
        &request,
        WorkflowRoleModelDecision::action("sk-secret-do-not-log", "bad"),
    );

    assert_eq!(reply.action, WORKFLOW_ROLE_DECISION_NO_ACTION);
    assert!(!reply.reason.contains("sk-secret-do-not-log"));
    assert!(reply.reason.contains("<redacted>"));
}

#[test]
fn unauthorized_model_action_records_downgrade_metadata() {
    let request = fixture_request();
    let validated = validated_reply_for_model_decision(
        &request,
        WorkflowRoleModelDecision::action("delete_everything", "bad"),
    );

    assert_eq!(validated.reply.action, WORKFLOW_ROLE_DECISION_NO_ACTION);
    assert_eq!(
        validated.log_metadata.model_action.as_deref(),
        Some("delete_everything")
    );
    assert_eq!(
        validated.log_metadata.unauthorized_model_action.as_deref(),
        Some("delete_everything")
    );
    assert_eq!(
        validated.log_metadata.outcome,
        "unauthorized_action_downgraded"
    );
}

#[test]
fn rejects_unknown_protocol_version_before_model_call() {
    let mut request = fixture_request();
    request.protocol_version = 999;

    let error = validate_request_version(&request).expect_err("version fails");
    assert!(matches!(
        error,
        WorkflowRoleDecisionError::UnsupportedProtocolVersion { actual: 999 }
    ));
}
