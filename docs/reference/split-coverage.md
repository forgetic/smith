# Split coverage ledger

Smith is the only repository with concrete pi-SDK-backed provider, product-manager,
and workflow-role decision behavior. Temper owns process protocols, validation,
runner authority, transcripts, proposal acceptance, deterministic fake tests, and
production process wiring.

## Ownership after the split

| Area | Temper coverage | Smith coverage |
| --- | --- | --- |
| Provider/auth/model calls | None; Temper treats responder args/env as opaque and clears child env except allow-listed names. | Provider/OAuth unit tests and ignored live provider tests. |
| One-turn structured decisions | Process reply validation in `temper-runner`. | Workflow-role decision tests plus provider live smokes. |
| Product-manager profile behavior | Generic conversations, transcripts, inert proposals, and filing. | Product-manager prompt/mapping/response tests and `smith-product-manager-responder`. |
| Workflow-role behavior | Manifest authority, authorized action validation, `RoleTools`, external-tool binding, process adapter. | `smith-workflow-role-decision` prompt/context/provider implementation. |
| Forgejo + real LLM proof | Temper process adapter and Forgejo support are used from Smith's ignored e2e. | `forgejo_workflow_role_e2e` real Forgejo + real LLM gate. |

## Removed Temper gates

Do not use these as Temper coverage gates after the split:

- `cargo test -p temper-agents ...`
- `temper-testing-worker --agents real`
- production `temper-worker --auth/--codex-model/--auth-file`
- product-chat `--auth/--codex-model/--auth-file`

## Active Smith gates

Run from this repository:

| Command | Protects |
| --- | --- |
| `cargo test --workspace --all-targets` | Hermetic provider, product-manager, workflow-role decision, and CLI coverage. |
| `cargo test --workspace --all-targets product_manager` | Product-manager request mapping, response parsing, draft/proposal validation, prompt export, and Temper fixture compatibility. |
| `cargo test --workspace --all-targets workflow_role_decision` | Temper workflow-role fixture compatibility, manifest prompt/context mapping, bound external-tool metadata, authorized/no-action mapping, unauthorized action downgrade, and protocol-version rejection. |
| `TEMPER_CHATGPT_OAUTH=1 cargo test --test chatgpt_oauth_live -- --ignored --nocapture` | Live ChatGPT/OpenAI Codex OAuth smoke and refresh/write-back. |
| `TEMPER_ANTHROPIC_OAUTH=1 cargo test --test anthropic_oauth_live -- --ignored --nocapture` | Live Anthropic OAuth smoke with Claude Code identity handling. |
| `TEMPER_FORGEJO_E2E=1 TEMPER_FORGEJO_AGENTS=1 cargo test -p smith-temper-agent-cli --test forgejo_workflow_role_e2e -- --ignored --test-threads=1` | Real Forgejo + real LLM proof through Temper's process adapter, coding workspace, and `RoleTools`. |
