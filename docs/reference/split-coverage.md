# Split coverage ledger

Smith is the only repository with concrete pi-SDK-backed provider,
product-manager example, and workflow-role decision behavior. Temper owns
process protocols, validation, runner authority, generic interaction services,
transcripts, proposal acceptance, deterministic fake tests, and production
process wiring.

## Ownership after the split

| Area | Temper coverage | Smith coverage |
| --- | --- | --- |
| Provider/auth/model calls | None; Temper treats responder args/env as opaque and clears child env except allow-listed names. | Provider/OAuth unit tests, manual live provider tests, and request-oracle checks. |
| One-turn structured decisions | Process reply validation in `temper-runner`. | Workflow-role decision tests plus provider live smokes. |
| Product-manager example profile behavior | Generic conversations, compiled profile manifests, transcripts, inert proposals, and filing. | Product-manager prompt/mapping/response tests, Temper fixture compatibility, and `smith-product-manager-responder`. |
| Workflow-role behavior | Manifest authority, authorized action validation, `RoleTools`, external-tool binding, process adapter. | `smith-workflow-role-decision` prompt/context/provider implementation. |
| Hermetic Forgejo + jig e2e | Temper process adapter and Forgejo support are exercised from Smith's targeted ignored e2e. | `coding_agent_e2e` and `forgejo_workflow_role_e2e` run in CI with `SMITH_JIG_E2E=1` and local jig fakes. |
| Live provider proofs | None. | Manual-only OAuth and DeepSeek/OpenAI-compatible request-oracle gates. |

## Removed Temper gates

Do not use these as Temper coverage gates after the split:

- `cargo test -p temper-agents ...`
- `temper-testing-worker --agents real`
- production `temper-worker --auth/--codex-model/--auth-file`
- profile-specific interactive responder auth flags in Temper production binaries

## Active Smith gates

Run from this repository:

| Command | Protects | Where |
| --- | --- | --- |
| `cargo dev-test` | Default hermetic provider, product-manager, workflow-role decision, and CLI coverage. | CI |
| `cargo test --workspace --all-targets product_manager` | Product-manager request mapping, response parsing, draft/proposal validation, prompt export, and Temper fixture compatibility. | Focused local |
| `cargo test --workspace --all-targets workflow_role_decision` | Temper workflow-role fixture compatibility, manifest prompt/context mapping, bound external-tool metadata, authorized/no-action mapping, unauthorized action downgrade, and protocol-version rejection. | Focused local |
| `SMITH_JIG_E2E=1 cargo test -p smith-temper-agent-cli --features test-provider-base-url-override --test coding_agent_e2e -- --ignored --test-threads=1` | Hermetic real `smith-coding-agent` binary proof using a local jig fake LLM and a local git checkout. | CI |
| `SMITH_JIG_E2E=1 cargo test -p smith-temper-agent-cli --features test-provider-base-url-override --test forgejo_workflow_role_e2e -- --ignored --test-threads=1` | Hermetic throwaway Forgejo + jig fake LLM proof through Temper's process adapter, coding workspace, and `RoleTools`. | CI |
| `TEMPER_CHATGPT_OAUTH=1 cargo test --test chatgpt_oauth_live -- --ignored --nocapture` | Live ChatGPT/OpenAI Codex OAuth smoke and refresh/write-back. | Manual only |
| `TEMPER_ANTHROPIC_OAUTH=1 cargo test --test anthropic_oauth_live -- --ignored --nocapture` | Live Anthropic OAuth smoke with Claude Code identity handling. | Manual only |
| `TEMPER_DEEPSEEK_REQUEST_ORACLE=1 TEMPER_DEEPSEEK_API_KEY=... cargo test -p smith-temper-agent --test jig_request_oracle --features test-provider-base-url-override -- --ignored --nocapture` | Live DeepSeek/OpenAI-compatible request-body oracle against jig's authoritative template. | Manual only |
