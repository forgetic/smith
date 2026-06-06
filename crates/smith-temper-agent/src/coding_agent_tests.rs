//! Hermetic unit tests for the coding-workspace agent: context parsing,
//! prompt construction, role→capability/tool selection, and result
//! serialization. No network or live provider is touched here; the live agent
//! loop is exercised by the gated e2e in the CLI crate.

use super::*;

const CONTEXT_FIXTURE: &str = r#"{
  "repository": {
    "id": "repo-1",
    "owner": "acme",
    "name": "service",
    "default_branch": "main"
  },
  "work_item": {
    "role": "engineer",
    "queue": "code_ready",
    "kind": "code",
    "target": "Issue { number: ItemNumber(7) }",
    "context": "{\"artifact\":{\"title\":\"Implement docs\"}}"
  },
  "base_branch": "main",
  "branch_hint": "agent/pr-for-code-7",
  "correlation_key": "pr-for-code-7",
  "checkout": "writable",
  "allowed_verdicts": ["needs_architect"],
  "guidance": {
    "role_guidance": "Make a real product change.",
    "tool_guidance": "Use docs/product-change.md for this fixture.",
    "tool_constraints": ["No .temper-only diffs."]
  }
}"#;

fn parsed_fixture() -> WorkspaceContext {
    serde_json::from_str(CONTEXT_FIXTURE).expect("context fixture parses")
}

#[test]
fn parses_full_context_fixture() {
    let context = parsed_fixture();
    assert_eq!(context.repository.id, "repo-1");
    assert_eq!(context.repository.owner, "acme");
    assert_eq!(context.repository.name, "service");
    assert_eq!(context.repository.default_branch, "main");
    assert_eq!(context.work_item.role, "engineer");
    assert_eq!(context.work_item.queue, "code_ready");
    assert_eq!(context.work_item.kind, "code");
    assert_eq!(context.work_item.target, "Issue { number: ItemNumber(7) }");
    assert_eq!(
        context.work_item.context,
        r#"{"artifact":{"title":"Implement docs"}}"#
    );
    assert_eq!(context.base_branch, "main");
    assert_eq!(context.branch_hint, "agent/pr-for-code-7");
    assert_eq!(context.correlation_key, "pr-for-code-7");
    assert_eq!(context.checkout.as_deref(), Some("writable"));
    assert_eq!(
        context.allowed_verdicts,
        vec!["needs_architect".to_string()]
    );
    assert_eq!(
        context.guidance.role_guidance.as_deref(),
        Some("Make a real product change.")
    );
    assert_eq!(
        context.guidance.tool_guidance.as_deref(),
        Some("Use docs/product-change.md for this fixture.")
    );
    assert_eq!(
        context.guidance.tool_constraints,
        vec!["No .temper-only diffs.".to_string()]
    );
}

#[test]
fn parses_context_without_optional_guidance_and_checkout() {
    let minimal = r#"{
      "repository": { "id": "r", "owner": "o", "name": "n", "default_branch": "main" },
      "work_item": { "role": "architect", "queue": "triage", "kind": "code", "target": "Issue { number: ItemNumber(1) }", "context": "{}" },
      "base_branch": "main",
      "branch_hint": "agent/x",
      "correlation_key": "x",
      "guidance": {}
    }"#;
    let context: WorkspaceContext = serde_json::from_str(minimal).expect("minimal context parses");
    assert_eq!(context.checkout, None);
    // A context without `allowed_verdicts` defaults to empty (back-compat with an
    // older temper that does not surface the vocabulary).
    assert!(context.allowed_verdicts.is_empty());
    assert_eq!(context.guidance, WorkspaceGuidance::default());
}

#[test]
fn role_maps_to_capability() {
    assert_eq!(
        Capability::for_role("engineer"),
        Capability::CodingWorkspace
    );
    assert_eq!(
        Capability::for_role("reviewer"),
        Capability::ReviewWorkspace
    );
    assert_eq!(
        Capability::for_role("architect"),
        Capability::TriageWorkspace
    );
    // Unknown roles fall back to read-only triage; they must never be writable.
    assert_eq!(Capability::for_role("mystery"), Capability::TriageWorkspace);
    assert!(!Capability::for_role("mystery").is_writable());
}

#[test]
fn only_engineer_is_writable() {
    assert!(Capability::CodingWorkspace.is_writable());
    assert!(!Capability::TriageWorkspace.is_writable());
    assert!(!Capability::ReviewWorkspace.is_writable());
}

#[test]
fn system_prompt_is_role_specific() {
    let engineer = system_prompt(Capability::CodingWorkspace, &[]);
    assert!(engineer.contains("ROLE: engineer"));
    assert!(engineer.contains("product diff"));
    assert!(engineer.contains("Do NOT run git commit"));
    assert!(engineer.contains("needs_architect"));

    let architect = system_prompt(Capability::TriageWorkspace, &[]);
    assert!(architect.contains("ROLE: architect"));
    assert!(architect.contains("ready_code"));
    assert!(architect.contains("needs_design"));
    assert!(architect.contains("needs_breakdown"));

    let reviewer = system_prompt(Capability::ReviewWorkspace, &[]);
    assert!(reviewer.contains("ROLE: reviewer"));
    assert!(reviewer.contains("approve"));
    assert!(reviewer.contains("review_body"));
    assert!(reviewer.contains("escalate"));

    // Every role must be told to emit a single final JSON object.
    for prompt in [engineer, architect, reviewer] {
        assert!(prompt.contains("single JSON object"));
        assert!(prompt.contains("children"));
    }
}

#[test]
fn system_prompt_without_allowed_verdicts_has_no_constraint_block() {
    // Back-compat: an empty vocabulary leaves the built-in per-role menu and adds
    // no constraint section.
    let architect = system_prompt(Capability::TriageWorkspace, &[]);
    assert!(!architect.contains("VERDICT CONSTRAINT"));
}

#[test]
fn system_prompt_constrains_to_allowed_verdicts() {
    // A multi-outcome triage: the constraint names exactly the declared set.
    let allowed = vec!["ready_code".to_string(), "needs_design".to_string()];
    let architect = system_prompt(Capability::TriageWorkspace, &allowed);
    assert!(architect.contains("VERDICT CONSTRAINT"));
    assert!(architect.contains("`ready_code`"));
    assert!(architect.contains("`needs_design`"));
    // It must not suggest the single-outcome collapse for a 2-element set.
    assert!(!architect.contains("SINGLE declared outcome"));
}

#[test]
fn system_prompt_single_outcome_collapses_to_one_choice() {
    // The basic-delivery architect: a single declared outcome ⇒ exactly one
    // choice. This is the deterministic single-outcome triage the example relies
    // on.
    let allowed = vec!["ready_code".to_string()];
    let architect = system_prompt(Capability::TriageWorkspace, &allowed);
    assert!(architect.contains("VERDICT CONSTRAINT"));
    assert!(architect.contains("SINGLE declared outcome"));
    assert!(architect.contains("verdict `ready_code`"));
}

#[test]
fn system_prompt_engineer_keeps_head_path_under_constraint() {
    // Even with a declared verdict (needs_architect), the engineer may still take
    // the no-verdict head path.
    let allowed = vec!["needs_architect".to_string()];
    let engineer = system_prompt(Capability::CodingWorkspace, &allowed);
    assert!(engineer.contains("VERDICT CONSTRAINT"));
    assert!(engineer.contains("head path"));
    // The single-outcome collapse line is engineer-inapplicable.
    assert!(!engineer.contains("SINGLE declared outcome"));
}

#[test]
fn user_context_includes_work_item_and_guidance() {
    let context = parsed_fixture();
    let rendered = user_context(&context);
    assert!(rendered.contains("Repository: acme/service"));
    assert!(rendered.contains("Role: engineer"));
    assert!(rendered.contains("Target: Issue { number: ItemNumber(7) }"));
    assert!(rendered.contains("Base branch: main"));
    assert!(rendered.contains("Branch hint: agent/pr-for-code-7"));
    assert!(rendered.contains("Correlation key: pr-for-code-7"));
    assert!(rendered.contains("Checkout mode: writable"));
    assert!(rendered.contains("Make a real product change."));
    assert!(rendered.contains("Use docs/product-change.md"));
    assert!(rendered.contains("No .temper-only diffs."));
    assert!(rendered.contains(r#"{"artifact":{"title":"Implement docs"}}"#));
}

#[test]
fn head_path_result_serializes_without_verdict_or_empty_fields() {
    let result = WorkspaceResult {
        summary: Some("Implemented the banner greeting.".to_string()),
        ..WorkspaceResult::default()
    };
    let json = serde_json::to_string(&result).expect("serializes");
    assert_eq!(json, r#"{"summary":"Implemented the banner greeting."}"#);
    // Round-trips through temper's shape (deny_unknown_fields would reject extras).
    let back: WorkspaceResult = serde_json::from_str(&json).expect("round trips");
    assert_eq!(back, result);
}

#[test]
fn breakdown_result_serializes_children() {
    let result = WorkspaceResult {
        verdict: Some("needs_breakdown".to_string()),
        children: vec![
            WorkspaceResultChild {
                slug: "api".to_string(),
                title: "Add the API".to_string(),
                body: "api body".to_string(),
                labels: vec!["code".to_string()],
                depends_on: Vec::new(),
            },
            WorkspaceResultChild {
                slug: "ui".to_string(),
                title: "Add the UI".to_string(),
                body: "ui body".to_string(),
                labels: Vec::new(),
                depends_on: vec!["api".to_string()],
            },
        ],
        ..WorkspaceResult::default()
    };
    let value = serde_json::to_value(&result).expect("serializes");
    assert_eq!(value["verdict"], "needs_breakdown");
    assert_eq!(value["children"][0]["slug"], "api");
    assert_eq!(value["children"][1]["depends_on"][0], "api");
    // No spurious head-path fields.
    assert!(value.get("summary").is_none());
    assert!(value.get("body").is_none());
}

#[test]
fn parse_result_extracts_bare_json() {
    let result = parse_result(r#"{"verdict":"approve","summary":"ok"}"#).expect("parses");
    assert_eq!(result.verdict.as_deref(), Some("approve"));
    assert_eq!(result.summary.as_deref(), Some("ok"));
}

#[test]
fn parse_result_tolerates_code_fence_and_prose() {
    let text =
        "Here is the result:\n```json\n{\"verdict\": \"ready_code\", \"body\": \"spec\"}\n```\n";
    let result = parse_result(text).expect("parses");
    assert_eq!(result.verdict.as_deref(), Some("ready_code"));
    assert_eq!(result.body.as_deref(), Some("spec"));
}

#[test]
fn parse_result_empty_reply_is_empty_head_path() {
    let result = parse_result("   \n").expect("empty reply parses as default");
    assert_eq!(result, WorkspaceResult::default());
}

#[test]
fn parse_result_rejects_unparseable_prose() {
    let error = parse_result("I could not finish the task.").expect_err("no JSON object");
    assert!(matches!(error, CodingAgentError::Parse { .. }));
}

#[test]
fn validate_contract_engineer_requires_diff_or_verdict() {
    // No diff, no verdict ⇒ NoProduct. Use a temp dir that is not a git repo so
    // `git status` fails and `working_tree_has_changes` returns false.
    let temp = std::env::temp_dir().join(format!("smith-coding-agent-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("temp dir");
    let empty = WorkspaceResult::default();
    let error =
        validate_contract(Capability::CodingWorkspace, &empty, &temp).expect_err("no product");
    assert!(matches!(error, CodingAgentError::NoProduct));

    // A verdict (needs_architect) satisfies the contract even with no diff.
    let with_verdict = WorkspaceResult {
        verdict: Some("needs_architect".to_string()),
        ..WorkspaceResult::default()
    };
    validate_contract(Capability::CodingWorkspace, &with_verdict, &temp)
        .expect("verdict satisfies engineer contract");
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn validate_contract_readonly_requires_verdict() {
    let cwd = std::env::temp_dir();
    let no_verdict = WorkspaceResult {
        summary: Some("looked around".to_string()),
        ..WorkspaceResult::default()
    };
    assert!(matches!(
        validate_contract(Capability::TriageWorkspace, &no_verdict, &cwd),
        Err(CodingAgentError::AgentStopped(_))
    ));
    assert!(matches!(
        validate_contract(Capability::ReviewWorkspace, &no_verdict, &cwd),
        Err(CodingAgentError::AgentStopped(_))
    ));

    let approved = WorkspaceResult {
        verdict: Some("approve".to_string()),
        ..WorkspaceResult::default()
    };
    validate_contract(Capability::ReviewWorkspace, &approved, &cwd)
        .expect("verdict satisfies reviewer contract");
}

#[test]
fn validate_verdict_vocabulary_accepts_declared_verdict() {
    let allowed = vec!["ready_code".to_string()];
    let result = WorkspaceResult {
        verdict: Some("ready_code".to_string()),
        body: Some("spec".to_string()),
        ..WorkspaceResult::default()
    };
    validate_verdict_vocabulary(&result, &allowed).expect("declared verdict passes");
}

#[test]
fn validate_verdict_vocabulary_rejects_undeclared_verdict() {
    // The single-outcome basic-delivery triage: a `needs_design` from the model
    // is rejected before temper would fail the tick.
    let allowed = vec!["ready_code".to_string()];
    let result = WorkspaceResult {
        verdict: Some("needs_design".to_string()),
        ..WorkspaceResult::default()
    };
    let error =
        validate_verdict_vocabulary(&result, &allowed).expect_err("undeclared verdict rejected");
    match error {
        CodingAgentError::UndeclaredVerdict { emitted, allowed } => {
            assert_eq!(emitted, "needs_design");
            assert_eq!(allowed, vec!["ready_code".to_string()]);
        }
        other => panic!("expected UndeclaredVerdict, got {other:?}"),
    }
}

#[test]
fn validate_verdict_vocabulary_allows_head_path_and_empty_vocabulary() {
    let allowed = vec!["ready_code".to_string()];
    // No verdict (head path) passes even when a vocabulary is declared.
    let head = WorkspaceResult {
        summary: Some("left a diff".to_string()),
        ..WorkspaceResult::default()
    };
    validate_verdict_vocabulary(&head, &allowed).expect("head path passes");

    // An empty vocabulary (older temper / no declared outcomes) skips the check.
    let any_verdict = WorkspaceResult {
        verdict: Some("anything".to_string()),
        ..WorkspaceResult::default()
    };
    validate_verdict_vocabulary(&any_verdict, &[]).expect("empty vocabulary skips the check");
}

#[test]
fn tool_registry_writability_matches_capability() {
    // Constructing the registries must not panic and must be scoped to cwd.
    // We can't easily introspect tool names, but we assert the writable mapping
    // is what selects the edit/write tools.
    let cwd = std::env::temp_dir();
    let _writable = tool_registry(Capability::CodingWorkspace, &cwd);
    let _readonly = tool_registry(Capability::TriageWorkspace, &cwd);
    assert!(Capability::CodingWorkspace.is_writable());
    assert!(!Capability::TriageWorkspace.is_writable());
}
