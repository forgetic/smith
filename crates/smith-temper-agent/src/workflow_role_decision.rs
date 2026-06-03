//! Workflow-role decision responder for Temper's process protocol.
//!
//! Smith owns the concrete LLM call. Temper owns workflow authority: this module
//! reads a [`WorkflowRoleDecisionRequest`], asks the provider for one manifest
//! action, and returns a [`WorkflowRoleDecisionReply`]. Smith does not receive or
//! execute Forge/workflow mutation tools; it only chooses `no_action` or one of
//! the request's authorized action names.

use serde::Deserialize;
use temper_runner::{
    BoundExternalTool, WORKFLOW_ROLE_DECISION_NO_ACTION, WORKFLOW_ROLE_DECISION_PROTOCOL_VERSION,
    WorkflowRoleDecisionReply, WorkflowRoleDecisionRequest,
};
use temper_workflow::RoleManifest;

use crate::decision::{DecisionError, run_decision};
use crate::provider::ProviderConfig;

const EXTERNAL_TOOL_SECTION: &str = "User-declared external tools";

/// Provider-backed workflow-role decision responder.
pub struct WorkflowRoleDecisionResponder {
    provider: ProviderConfig,
}

impl WorkflowRoleDecisionResponder {
    /// Builds a responder using Smith's provider config.
    pub fn new(provider: ProviderConfig) -> Self {
        Self { provider }
    }

    /// Runs one LLM-backed workflow-role decision.
    pub async fn respond(
        &self,
        request: &WorkflowRoleDecisionRequest,
    ) -> Result<WorkflowRoleDecisionReply, WorkflowRoleDecisionError> {
        validate_request_version(request)?;
        let system_prompt = workflow_role_system_prompt(request);
        let user_context = workflow_role_user_context(request)
            .map_err(WorkflowRoleDecisionError::RequestContext)?;
        match run_decision::<WorkflowRoleModelDecision>(
            &self.provider,
            &system_prompt,
            &user_context,
        )
        .await
        {
            Ok(decision) => Ok(reply_for_model_decision(request, decision)),
            Err(DecisionError::Provider(error)) => Err(WorkflowRoleDecisionError::Decision(
                DecisionError::Provider(error),
            )),
            Err(error) => {
                eprintln!(
                    "smith-workflow-role-decision: LLM decision failed for workflow '{}' role '{}', treating as no-action: {error}",
                    request.workflow_id, request.role_manifest.id
                );
                Ok(no_action_for_request(request, "decision failed"))
            }
        }
    }
}

/// Minimal model decision shape. Extra fields are ignored for compatibility with
/// older prompts that returned diagnostics beyond `action` and `reason`.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct WorkflowRoleModelDecision {
    /// One manifest action, or `no_action`.
    pub action: String,
    /// Short rationale for operator/debug logs.
    #[serde(default)]
    pub reason: String,
}

impl WorkflowRoleModelDecision {
    /// Builds a safe no-action model decision, mostly for tests.
    pub fn no_action(reason: impl Into<String>) -> Self {
        Self {
            action: WORKFLOW_ROLE_DECISION_NO_ACTION.to_string(),
            reason: reason.into(),
        }
    }

    /// Builds a model decision choosing an action.
    pub fn action(action: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            reason: reason.into(),
        }
    }
}

/// Workflow-role responder failure.
#[derive(Debug)]
pub enum WorkflowRoleDecisionError {
    /// The request uses a protocol version this Smith binary does not implement.
    UnsupportedProtocolVersion { actual: u32 },
    /// Building the provider or obtaining a model decision failed in a way that
    /// should fail the worker rather than silently no-op.
    Decision(DecisionError),
    /// Smith could not serialize the model context for the provider call.
    RequestContext(serde_json::Error),
}

impl std::fmt::Display for WorkflowRoleDecisionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedProtocolVersion { actual } => write!(
                formatter,
                "unsupported workflow-role decision protocol version {actual}; expected {WORKFLOW_ROLE_DECISION_PROTOCOL_VERSION}"
            ),
            Self::Decision(error) => write!(formatter, "{error}"),
            Self::RequestContext(error) => {
                write!(
                    formatter,
                    "serializing workflow-role decision context failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for WorkflowRoleDecisionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decision(error) => Some(error),
            Self::RequestContext(error) => Some(error),
            Self::UnsupportedProtocolVersion { .. } => None,
        }
    }
}

fn validate_request_version(
    request: &WorkflowRoleDecisionRequest,
) -> Result<(), WorkflowRoleDecisionError> {
    if request.protocol_version == WORKFLOW_ROLE_DECISION_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(WorkflowRoleDecisionError::UnsupportedProtocolVersion {
            actual: request.protocol_version,
        })
    }
}

/// Builds the generated runtime system prompt for a workflow-role request.
pub fn workflow_role_system_prompt(request: &WorkflowRoleDecisionRequest) -> String {
    runtime_system_prompt(&request.role_manifest, &request.available_external_tools)
}

fn runtime_system_prompt(manifest: &RoleManifest, tools: &[BoundExternalTool]) -> String {
    if manifest.external_tools.is_empty() {
        return manifest.prompt.render();
    }
    let mut prompt = manifest.prompt.clone();
    if let Some(section) = prompt.section_mut(EXTERNAL_TOOL_SECTION) {
        section.lines = runtime_external_tool_lines(tools);
    }
    prompt.render()
}

/// Builds the user-context JSON string sent to the provider.
pub fn workflow_role_user_context(
    request: &WorkflowRoleDecisionRequest,
) -> Result<String, serde_json::Error> {
    let allowed_actions = std::iter::once(WORKFLOW_ROLE_DECISION_NO_ACTION.to_string())
        .chain(
            request
                .authorized_actions
                .iter()
                .map(|action| action.action.clone()),
        )
        .collect::<Vec<_>>();
    let context = serde_json::json!({
        "work_item": request.work_item_context,
        "allowed_actions": allowed_actions,
        "authorized_actions": request.authorized_actions,
        "available_external_tools": request.available_external_tools,
    });
    serde_json::to_string_pretty(&context)
}

/// Validates a model decision and turns unauthorized actions into `no_action`.
pub fn reply_for_model_decision(
    request: &WorkflowRoleDecisionRequest,
    decision: WorkflowRoleModelDecision,
) -> WorkflowRoleDecisionReply {
    let action = decision.action.trim();
    if action == WORKFLOW_ROLE_DECISION_NO_ACTION {
        return no_action_for_request(request, decision.reason);
    }
    if request
        .authorized_actions
        .iter()
        .any(|candidate| candidate.action == action)
    {
        return WorkflowRoleDecisionReply {
            protocol_version: request.protocol_version,
            action: action.to_string(),
            reason: decision.reason,
        };
    }

    eprintln!(
        "smith-workflow-role-decision: role '{}' model chose unauthorized action '{}', returning no_action",
        request.role_manifest.id, action
    );
    no_action_for_request(request, format!("unauthorized model action: {action}"))
}

fn no_action_for_request(
    request: &WorkflowRoleDecisionRequest,
    reason: impl Into<String>,
) -> WorkflowRoleDecisionReply {
    WorkflowRoleDecisionReply {
        protocol_version: request.protocol_version,
        action: WORKFLOW_ROLE_DECISION_NO_ACTION.to_string(),
        reason: reason.into(),
    }
}

fn runtime_external_tool_lines(tools: &[BoundExternalTool]) -> Vec<String> {
    let mut lines = vec![
        "Only the external tools listed in this section are bound and available for this run."
            .to_string(),
        "Declared tools not listed here are unavailable; do not claim to use them.".to_string(),
        "External tools do not grant workflow or Forge mutation authority beyond the authorized workflow actions above.".to_string(),
    ];
    if tools.is_empty() {
        lines.push("(no external tools are bound for this run)".to_string());
    } else {
        for tool in tools {
            lines.push(format!(
                "{} via {}: {}",
                tool.id, tool.provider, tool.description
            ));
            if !tool.constraints.is_empty() {
                lines.push(format!(
                    "{} constraints: {}",
                    tool.id,
                    tool.constraints.join("; ")
                ));
            }
            if tool.id.as_str() == temper_runner::CODING_WORKSPACE_TOOL_ID {
                lines.push(format!(
                    "{} rule: implementation PR creation must use this workspace-produced branch/head; do not choose PR-opening actions for code work without it.",
                    tool.id
                ));
            }
            if let Some(guidance) = &tool.guidance {
                lines.push(format!("{} guidance: {guidance}", tool.id));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
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
        let context: serde_json::Value = serde_json::from_str(
            &workflow_role_user_context(&request).expect("context serializes"),
        )
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

        let none =
            reply_for_model_decision(&request, WorkflowRoleModelDecision::no_action("not safe"));
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
    fn rejects_unknown_protocol_version_before_model_call() {
        let mut request = fixture_request();
        request.protocol_version = 999;

        let error = validate_request_version(&request).expect_err("version fails");
        assert!(matches!(
            error,
            WorkflowRoleDecisionError::UnsupportedProtocolVersion { actual: 999 }
        ));
    }
}
