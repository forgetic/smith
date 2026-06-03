# Testing and coverage

## Hermetic Smith validation

```sh
cargo fmt --all
cargo test --workspace --all-targets
cargo test --workspace --all-targets product_manager
cargo test --workspace --all-targets workflow_role_decision
```

Useful focused areas:

- provider/auth parsing and redaction: `provider`, `oauth`, `anthropic_oauth`;
- product-manager request/reply mapping: `product_manager`;
- workflow-role prompt/context/action mapping: `workflow_role_decision`;
- CLI/process option parsing: `smith-temper-agent-cli` tests.

## Ignored live Smith gates

```sh
TEMPER_CHATGPT_OAUTH=1 \
  cargo test --test chatgpt_oauth_live -- --ignored --nocapture

TEMPER_ANTHROPIC_OAUTH=1 \
  cargo test --test anthropic_oauth_live -- --ignored --nocapture

TEMPER_FORGEJO_E2E=1 TEMPER_FORGEJO_AGENTS=1 \
  cargo test -p smith-temper-agent-cli --test forgejo_workflow_role_e2e -- \
  --ignored --test-threads=1
```

The Forgejo e2e also asserts Smith decision logs/captures correlate with
Temper's work-item and decision ids without exposing obvious auth/secret values.

## Temper-side coverage that protects Smith integration

Run from `../temper` when changing a protocol boundary:

```sh
cargo test -p temper-interaction process_responder
cargo test -p temper-runner role_decision_process
cargo test -p temper-production product_chat
cargo test -p temper-production worker_args worker_role_agent
```

Temper's tests should remain provider-free. Real provider behavior belongs in
Smith gates.
