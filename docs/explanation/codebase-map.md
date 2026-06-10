# Codebase map

| Path | Look here for |
| --- | --- |
| `crates/smith-temper-agent/` | Provider/auth/decision core, product-manager profile mapping, and workflow-role decision logic. |
| `crates/smith-temper-agent/src/provider/` | OAuth auth-file parsing/refresh, Anthropic request identity, and provider-specific knobs. |
| `crates/smith-temper-agent-cli/` | Preflight CLI and process-protocol binaries for Temper. |
| `crates/smith-worker/` | Worker-process library and `smith-worker` binary for the Temper worker/daemon wire protocol loop, plus the persistent per-`(repo, role)` git workspace plane for clone/fetch, branch preparation, role-authored commits, and role-token branch pushes. |
| `crates/smith-temper-agent-cli/tests/` | Version, coding-agent, and basic-delivery jig tests; the legacy cross-process Forgejo workflow-role e2e was removed after `smith-worker` gained hermetic daemon coverage. |
| `examples/` | Smith-owned Temper launchers that bind Smith responders, including dogfood/product-chat and the Smith-backed reference-delivery demo. |
| `deploy/` | Production-like local deployment assets for the basic-delivery dogfood: systemd user unit templates, `~/.config/smith` config templates, and the idempotent install script. |
| `docs/` | Diátaxis docs, ADRs, and Smith-owned agent lessons. |

## Temper dependency posture

After daemon/worker consolidation Phase 4a, Smith keeps only serialization-only
Temper protocol dependencies in production: `temper-worker-protocol` for the
`smith-worker`↔daemon wire protocol, and `temper-process-protocol` for the ADR
0002 stdio JSON process boundary used by responder and coding-agent binaries.
`temper-process-protocol` is retained deliberately; removing it would change the
process-boundary architecture rather than sever legacy runner/Forge coupling.

`temper-interaction` remains a dev-only dependency of `smith-temper-agent` for
one product-manager contract-parity test. Legacy direct code dependencies and
imports from `temper-forge*`, `temper-runner`, `temper-testing`, and
`temper-workflow` are gone from Smith's manifests and code. Smith does not call
the Forge API; `smith-worker`'s git-plane workspaces are the intentional
exception for role-credentialed clone/fetch/commit/push operations.

