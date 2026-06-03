# Observability story — Smith implementation plan

This plan is Smith's half of the Temper observability story. The motivating
incident is recorded in the sibling Temper checkout at
`../temper/plans/observability/evidence.md`: a reference-delivery parent became
`code,blocked`, no fan-out children or dependencies were created, and the Smith
role-decision path did not make the selected action or rationale obvious enough
for operators.

Hand the prompt files to agents one phase at a time, in order. Each phase should
land green and update this README's status.

## Goal

For every workflow-role decision Smith handles, an operator should be able to
correlate Smith's provider/model behavior with Temper's work item and answer:

- Which Temper work item, role, queue, repo, artifact, and decision id was this?
- Which provider/model/auth mode handled it, without exposing credentials?
- Which allowed actions and available external tools did Smith see?
- What did the model choose, what did Smith return after validation, and why?
- How long did prompt construction, provider execution, and parsing take?
- Did Smith downgrade an unauthorized model action, fail to parse, hit a provider
  error, or deliberately return `no_action`?

## Ownership boundary

- Temper owns workflow authority, Forge state, process protocol structs,
  validation, transition execution, and Forge mutations.
- Smith owns concrete LLM/provider observability: prompt/context construction,
  provider/model identity, model decision, final reply, latency, failures, and
  optional redacted captures.
- Smith receives no Forge handles, Forge tokens, workflow mutation tools, or
  broad environment. It may log/capture authority-neutral observability fields
  that Temper includes in `work_item_context`.
- Secrets and auth files never appear in logs or captures. Bodies, prompts, and
  reasons are bounded and redacted when persisted.

## Shared event fields

Smith should read these fields when Temper provides them, and otherwise degrade
gracefully:

- `work_item_context.observability.run_id`
- `work_item_context.observability.tick_id`
- `work_item_context.observability.work_item_id`
- `work_item_context.observability.decision_id`
- `work_item_context.repository`, `role`, `queue`, `kind`
- `work_item_context.artifact.type`, `number`

Smith-owned event fields include:

- `event`, `workflow_id`, `role`, `provider`, `model`, `auth_mode`
- `allowed_actions`, `available_external_tools`
- `model_action`, `returned_action`, `reason_preview`, `outcome`
- `latency_ms`, `prompt_chars`, `context_chars`, `capture_path`

## Phases

Status legend: ☐ pending · ☑ done · ⚠ blocked

1. ☑ **Phase 1 — Workflow-role decision structured logs.**
   `prompts/phase-1-workflow-role-decision-structured-logs.md`

   Add safe structured logs around request intake, prompt/context construction,
   provider call, model decision, reply validation/downgrade, and final output.
   Correlate with Temper's `work_item_context.observability` fields.

2. ☑ **Phase 2 — Redacted decision capture files.**
   `prompts/phase-2-redacted-decision-captures.md`

   Add an env-gated capture directory for debugging one-turn decisions. Captures
   should include redacted request summary, prompt/context previews, parsed model
   decision, final reply, provider/model identity, and timing.

3. ☑ **Phase 3 — Live/e2e observability proof and docs.**
   `prompts/phase-3-live-e2e-observability-proof-and-docs.md`

   Updated Smith docs and the ignored Forgejo + real LLM process-boundary test so
   a Temper work item with trace ids proves Smith logs/captures correlate with
   Temper worker events.

## Whole-plan acceptance criteria

- Smith decision logs can be joined to Temper logs by work item / decision ids.
- Operators can distinguish `no_action`, selected authorized action,
  unauthorized-action downgrade, parse failure, provider failure, timeout, and
  unsupported protocol version.
- Provider/model/auth identity is visible without leaking credentials or auth
  file paths.
- Optional captures are explicit, bounded, redacted, and disabled by default.
- Smith docs explain what Smith observes and what remains Temper-owned.

Status (2026-06-03): complete. Phase 3 validation passed with
`cargo fmt --all`, `cargo test --workspace --all-targets`, `cargo dev-clippy`,
and `cargo dev-check`. Live provider/Forgejo gates remain ignored and env-gated.

## Relevant starting points

- `README.md`
- `docs/explanation/process-boundary.md`
- `docs/reference/process-responders.md`
- `docs/reference/provider-auth.md`
- `crates/smith-temper-agent/src/{workflow_role_decision,decision,provider}.rs`
- `crates/smith-temper-agent-cli/src/bin/smith-workflow-role-decision.rs`
- `crates/smith-temper-agent-cli/tests/forgejo_workflow_role_e2e.rs`
- Temper sibling plan: `../temper/plans/observability/README.md`
- Temper protocol doc: `../temper/docs/reference/workflow-role-decision-process-protocol.md`
