# Codebase map

| Path | Look here for |
| --- | --- |
| `crates/smith-temper-agent/` | Provider/auth/decision core, product-manager profile mapping, and workflow-role decision logic. |
| `crates/smith-temper-agent/src/provider/` | OAuth auth-file parsing/refresh, Anthropic request identity, and provider-specific knobs. |
| `crates/smith-temper-agent-cli/` | Preflight CLI and process-protocol binaries for Temper. |
| `crates/smith-temper-agent-cli/tests/` | Ignored real Forgejo + real LLM proof through Temper's process adapter. |
| `examples/` | Smith-owned Temper launchers that bind Smith responders, including dogfood/product-chat and the Smith-backed reference-delivery demo.
| `docs/` | Diátaxis docs, ADRs, and Smith-owned agent lessons. |

