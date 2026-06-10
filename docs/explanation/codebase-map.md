# Codebase map

| Path | Look here for |
| --- | --- |
| `crates/smith-temper-agent/` | Provider/auth/decision core, product-manager profile mapping, and workflow-role decision logic. |
| `crates/smith-temper-agent/src/provider/` | OAuth auth-file parsing/refresh, Anthropic request identity, and provider-specific knobs. |
| `crates/smith-temper-agent-cli/` | Preflight CLI and process-protocol binaries for Temper. |
| `crates/smith-worker/` | Worker-process library and `smith-worker` binary for the Temper worker/daemon wire protocol loop, plus the persistent per-`(repo, role)` git workspace plane for clone/fetch, branch preparation, role-authored commits, and role-token branch pushes. |
| `crates/smith-temper-agent-cli/tests/` | Ignored real Forgejo + real LLM proof through Temper's process adapter. |
| `examples/` | Smith-owned Temper launchers that bind Smith responders, including dogfood/product-chat and the Smith-backed reference-delivery demo.
| `deploy/` | Production-like local deployment assets for the basic-delivery dogfood: systemd user unit templates, `~/.config/smith` config templates, and the idempotent install script. |
| `docs/` | Diátaxis docs, ADRs, and Smith-owned agent lessons. |

