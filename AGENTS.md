# Agent entry point

This file is the first stop for coding agents working in Smith. Read
`README.md` first, then use this map to choose task-relevant docs. Keep this
file as orientation only; stable rules and status belong in the linked docs.

## Codebase map

| Path | Look here for |
| --- | --- |
| `crates/smith-temper-agent/` | Provider/auth/decision core, product-manager profile mapping, and workflow-role decision logic. |
| `crates/smith-temper-agent/src/provider/` | OAuth auth-file parsing/refresh, Anthropic request identity, and provider-specific knobs. |
| `crates/smith-temper-agent-cli/` | Preflight CLI and process-protocol binaries for Temper. |
| `crates/smith-temper-agent-cli/tests/` | Ignored real Forgejo + real LLM proof through Temper's process adapter. |
| `../temper/` | Temper-owned process protocols, workflow/interaction runtime, Forge backends, and fixtures. Do not make Temper depend on Smith. |
| `docs/` | Diátaxis docs, ADRs, and Smith-owned agent lessons. |

## Documentation map

- Start here for process: `docs/how-to/start-a-development-session.md`.
- Development rules and validation: `docs/reference/development-conventions.md`,
  `docs/how-to/fast-local-iteration.md`, and
  `docs/how-to/end-a-development-session.md`.
- Provider/auth details: `docs/how-to/configure-provider-auth.md` and
  `docs/reference/provider-auth.md`.
- Temper integration: `docs/how-to/run-temper-responders.md`,
  `docs/reference/process-responders.md`,
  `docs/reference/workflow-role-observability.md`, and
  `docs/explanation/process-boundary.md`.
- Testing: `docs/reference/testing.md` and
  `docs/how-to/run-live-provider-tests.md`.
- Significant decisions: `docs/adr/README.md`.
- Recurring gotchas: `docs/reference/agent-lessons/README.md`.
