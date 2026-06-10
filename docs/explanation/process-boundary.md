# Temper/Smith process boundary

Smith exists so Temper can stay provider-neutral while still using concrete LLM
responders.

## Ownership

Temper owns:

- Forge, workflow, runner, and interaction contracts;
- process adapters and protocol validation;
- workflow authority, `RoleTools`, gates, leases, and transition execution;
- interactive transcripts, proposal validation, explicit acceptance, and Forge
  mutations;
- fake/conformance responders for hermetic tests.

Smith owns:

- pi-SDK provider wiring and auth-file handling;
- ChatGPT/Anthropic/DeepSeek model-specific behavior;
- dogfood/example product-manager interactive responder implementation;
- manifest-driven workflow-role decision implementation;
- live provider tests and real-agent e2e tests.

Temper must not take a Rust dependency on Smith. Smith can depend on Temper
protocol crates to implement the wire contracts.

## Post-Phase-4a dependency posture

Smith's production Temper dependencies are serialization-only protocol contracts:

- `temper-worker-protocol` is the versioned worker↔daemon wire protocol and is
  `smith-worker`'s only Temper dependency.
- `temper-process-protocol` is the spawn-boundary stdio JSON contract spoken by
  the responder and coding-agent binaries. ADR 0002 retains it deliberately;
  removing it would re-architect the process boundary rather than merely finish
  daemon/worker consolidation.

`temper-interaction` remains only as a `smith-temper-agent` dev-dependency for a
single product-manager contract-parity test. The legacy direct code dependencies
and imports from `temper-forge*`, `temper-runner`, `temper-testing`, and
`temper-workflow` are gone from Smith's manifests and code. Smith still never
calls the Forge API; the deliberate exception is the git plane in
`smith-worker`'s role-credentialed workspaces, which uses clone/fetch/commit/push
rather than Forge API crates.

## Why a process boundary

Provider SDKs, auth-file schemas, subscription quirks, and live model behavior
change faster than Temper's workflow contracts. A process protocol lets Temper
send a provider-neutral JSON request and receive one JSON reply without importing
provider SDKs or credential logic.

The authority boundary remains simple:

```text
Temper request + allowed actions/proposals
        │
        ▼
Smith LLM responder process
        │ no Forge token, no mutation tools
        ▼
Temper validation + state mutation
```

Interactive users and external clients enter through Temper's generic
interaction services, not through a Smith process directly. If Smith crashes,
returns malformed JSON, times out, or chooses an unauthorized action, Temper
handles that as a process-adapter failure or no-op according to the Temper-owned
contract.
