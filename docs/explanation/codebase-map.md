# Codebase map

| Path | Look here for |
| --- | --- |
| `crates/smith-temper-agent/` | Provider/auth/decision core, product-manager profile mapping, and workflow-role decision logic. |
| `crates/smith-temper-agent/src/provider/` | OAuth auth-file parsing/refresh, Anthropic request identity, and provider-specific knobs. |
| `crates/smith-temper-agent-cli/` | Preflight CLI and process-protocol binaries for Temper. |
| `crates/smith-worker/` | Worker-process library and `smith-worker` binary for the Temper worker/daemon wire protocol loop, plus the persistent per-`(repo, role)` git workspace plane for clone/fetch, branch preparation, role-authored commits, and role-token branch pushes. |
| `crates/smith-temper-agent-cli/tests/` | Version, coding-agent, and basic-delivery jig tests; the legacy cross-process Forgejo workflow-role e2e was removed after `smith-worker` gained hermetic daemon coverage. |
| `examples/` | Smith-owned operator demos (`basic-delivery` and `reference-delivery`), both booting the two-tier Temper daemon / Smith worker topology. |
| `deploy/` | The Smith worker tier of the two-tier topology: the `smith-worker.service` unit template, the `smith-worker-launcher` ExecStart shim, `~/.config/smith` config templates, and the idempotent install script. The Temper daemon tier deploys from the sibling `temper/deploy/`. |
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

Deployment matches this split: the Temper daemon (from `temper/deploy/`) is the
sole Forge API writer and holds the per-role API tokens, while `deploy/` here
ships only the protocol-speaking `smith-worker` tier, which holds per-role git
credentials and talks to the daemon over `temper-worker-protocol`.

