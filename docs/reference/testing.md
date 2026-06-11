# Testing and coverage

## Default hermetic Smith validation

Run the same provider-free validation shape that CI uses for ordinary pull
requests:

```sh
cargo fmt --all
cargo dev-clippy
cargo dev-check
cargo dev-test
```

`cargo dev-test` is the default hermetic test gate. It compiles ignored tests but
does not run ignored e2e or live/provider checks.

Useful focused areas:

- provider/auth parsing and redaction: `provider`, `oauth`, `anthropic_oauth`;
- product-manager request/reply mapping and Temper fixture compatibility: `product_manager`;
- workflow-role prompt/context/action mapping: `workflow_role_decision`;
- CLI/process option parsing: `smith-temper-agent-cli` tests.

## Hermetic jig e2e CI

Smith also has ignored e2e tests backed by local jig fakes. They exercise the
real Smith binaries and Temper process boundaries, but provider traffic is
pointed at local hermetic jig servers instead of real providers.

CI runs these targeted commands only:

```sh
SMITH_JIG_E2E=1 \
  cargo test -p smith-temper-agent-cli \
  --features test-provider-base-url-override \
  --test coding_agent_e2e \
  -- --ignored --test-threads=1

TEMPER_BASIC_DELIVERY_JIG_E2E=1 \
TEMPER_WORKSPACE_ROOT="$HOME/.local/state/forgejo/runner/data/temper" \
  cargo test -p smith-temper-agent-cli \
  --test basic_delivery_jig_e2e \
  -- --ignored --test-threads=1
```

`SMITH_JIG_E2E=1` opts into the hermetic jig-backed Smith process tests.
`test-provider-base-url-override` enables the test-only provider base URL hook so
the Smith process tests can route model-provider requests to the local jig
server.

`TEMPER_BASIC_DELIVERY_JIG_E2E=1` opts into the provider-free basic-delivery jig,
which now runs in CI as a gate and remains available locally with the same env
flag. It boots real throwaway Forgejo, host-mode `forgejo-runner`, one
`temper-daemon`, and one `smith-worker`; `BASIC_DELIVERY_CODER=greeting` supplies
deterministic local architect/engineer behavior instead of provider calls, so the
test does not require `TEMPER_FORGEJO_AGENTS=1` or provider auth variables. The
test honors `TEMPER_DAEMON_BIN`, `TEMPER_PROVISION_BIN`, `SMITH_WORKER_BIN`,
`TEMPER_FORGEJO_BINARY`, and `TEMPER_FORGEJO_RUNNER_BINARY` for prebuilt
artifacts. Set `TEMPER_WORKSPACE_ROOT` when the Temper checkout is not the
sibling `../temper`; CI points it at the runner's Temper checkout, which also
holds the pinned Forgejo/runner binary cache.

CI does not run a broad ignored-test sweep such as `cargo test -- --ignored`;
ignored live provider checks remain manual-only.

## Manual live/provider gates

Live provider checks can call real model providers or refresh real OAuth
credentials. They are intentionally not CI commands. See
[Run live provider checks](../how-to/run-live-provider-tests.md) for
copy-pasteable manual gates.

## Temper-side coverage that protects Smith integration

Run from `../temper` when changing a protocol boundary:

```sh
cargo test -p temper-interaction process_responder
cargo test -p temper-runner role_decision_process
cargo test -p temper-production interaction
cargo test -p temper-production worker_args worker_role_agent
```

Temper's tests should remain provider-free. Real provider behavior belongs in
Smith's manual live/provider gates.
