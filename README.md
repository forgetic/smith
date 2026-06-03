# Smith

**Concrete LLM responder implementations for Temper.**

Smith is the external home for the `pi_agent_rust` provider/auth/decision code
that Temper calls through process protocols. Temper owns workflow and interaction
contracts; Smith owns the first concrete pi-SDK-backed responders for those
contracts.

[Docs](docs/README.md) · [Provider auth](docs/how-to/configure-provider-auth.md) · [Run responders](docs/how-to/run-temper-responders.md) · [Process boundary](docs/explanation/process-boundary.md)

## What Smith owns

- ChatGPT/OpenAI Codex OAuth, Anthropic OAuth, and DeepSeek API-key provider
  wiring.
- One-turn structured LLM decisions through `pi_agent_rust`.
- The `product-manager` interactive profile responder.
- The manifest-driven workflow-role decision responder.
- Live provider tests and the real Forgejo + real LLM process-boundary proof.

Smith does **not** mutate Forge or workflow state. Temper validates every reply
and performs all transcript, proposal, Forge, and workflow mutations.

## Workspace

- `crates/smith-temper-agent` — library for provider selection, OAuth auth-file
  handling, per-request provider knobs, one-turn structured decisions,
  product-manager profile mapping, and workflow-role decisions.
- `crates/smith-temper-agent-cli` — credential preflight utility plus the
  `smith-product-manager-responder` and `smith-workflow-role-decision` binaries.

Smith may use local path dependencies on the sibling Temper checkout for
protocol/domain crates used by tests and process binaries:

```text
../temper/crates/temper-interaction
../temper/crates/temper-runner
../temper/crates/temper-workflow
```

Temper must not depend on Smith as a Rust crate.

## Quick start

```sh
cargo fmt --all
cargo test --workspace --all-targets
```

Run provider preflight after logging in or configuring a key:

```sh
cargo run -p smith-temper-agent-cli -- preflight --auth chatgpt-oauth
cargo run -p smith-temper-agent-cli -- preflight --auth anthropic-oauth
cargo run -p smith-temper-agent-cli -- preflight --auth deepseek
```

See `docs/how-to/configure-provider-auth.md` for login, model, and auth-file
options.

## Use with Temper

Build the responder binaries and point Temper at them:

```sh
cargo build -p smith-temper-agent-cli --bin smith-workflow-role-decision
cargo build -p smith-temper-agent-cli --bin smith-product-manager-responder
```

```sh
export TEMPER_WORKER_ROLE_DECISION_COMMAND=$PWD/target/debug/smith-workflow-role-decision
export TEMPER_WORKER_ROLE_DECISION_ARGS_JSON='["--auth","chatgpt-oauth"]'

export TEMPER_PRODUCT_CHAT_RESPONDER_COMMAND=$PWD/target/debug/smith-product-manager-responder
export TEMPER_PRODUCT_CHAT_RESPONDER_ARGS_JSON='["--auth","chatgpt-oauth"]'
```

The reference-delivery and dogfood launchers in Temper are configured to use
Smith process responders by default. Keep their Smith workspace setting pointed
at this checkout and pass provider options through the documented Smith args/env
surfaces.

## Dependency note

`pi_agent_rust 0.1.13` pulls `asupersync =0.3.1`, which currently needs the
API-compatible `franken-decision 0.3.1`. Keep `Cargo.lock` pinned with:

```sh
cargo update -p franken-decision --precise 0.3.1
```

Re-check this workaround before bumping `pi_agent_rust`.

## Secrets

Never pass provider secrets on argv and never commit auth files here. Local
credentials are read from environment variables, `.cache/deepseek-api-key`, or
the shared pi auth file at `~/.pi/agent/auth.json`.

## Live checks

Live provider and Forgejo checks are ignored and env-gated:

```sh
TEMPER_CHATGPT_OAUTH=1 \
  cargo test --test chatgpt_oauth_live -- --ignored --nocapture

TEMPER_ANTHROPIC_OAUTH=1 \
  cargo test --test anthropic_oauth_live -- --ignored --nocapture

TEMPER_FORGEJO_E2E=1 TEMPER_FORGEJO_AGENTS=1 \
  cargo test -p smith-temper-agent-cli --test forgejo_workflow_role_e2e -- \
  --ignored --test-threads=1
```

See `docs/how-to/run-live-provider-tests.md` before running gates that can refresh
real OAuth credentials or boot external services.
