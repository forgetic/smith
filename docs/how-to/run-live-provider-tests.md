# Run live provider checks

Default Smith validation and CI are hermetic. Do not use the commands on this
page in CI: they can call real model providers, refresh real OAuth credentials,
or require real API keys.

For CI-safe jig e2e commands that use `SMITH_JIG_E2E=1`, see
[Testing and coverage](../reference/testing.md). `SMITH_JIG_E2E=1` is a hermetic
jig gate, not a live/provider gate.

## ChatGPT/OpenAI Codex OAuth

Run `pi /login openai-codex` first, then:

```sh
TEMPER_CHATGPT_OAUTH=1 \
  cargo test --test chatgpt_oauth_live -- --ignored --nocapture
```

The refresh check may rotate the refresh token and write the refreshed credential
back to the real auth file.

The request-oracle leg for ChatGPT/OpenAI Codex OAuth is also manual-only:

```sh
TEMPER_CHATGPT_OAUTH=1 \
  cargo test -p smith-temper-agent \
  --test jig_request_oracle \
  --features test-provider-base-url-override \
  -- --ignored --nocapture
```

## Anthropic OAuth

Run `pi /login anthropic` first, then:

```sh
TEMPER_ANTHROPIC_OAUTH=1 \
  cargo test --test anthropic_oauth_live -- --ignored --nocapture
```

The request-oracle leg for Anthropic OAuth is also manual-only:

```sh
TEMPER_ANTHROPIC_OAUTH=1 \
  cargo test -p smith-temper-agent \
  --test jig_request_oracle \
  --features test-provider-base-url-override \
  -- --ignored --nocapture
```

## DeepSeek/OpenAI-compatible request oracle

This manual request-oracle leg requires a real DeepSeek/OpenAI-compatible API key
and makes real provider calls. Provide the key inline or via
`TEMPER_DEEPSEEK_API_KEY_PATH`:

```sh
TEMPER_DEEPSEEK_REQUEST_ORACLE=1 \
TEMPER_DEEPSEEK_API_KEY=... \
  cargo test -p smith-temper-agent \
  --test jig_request_oracle \
  --features test-provider-base-url-override \
  -- --ignored --nocapture
```

The same test target also contains the OAuth request-oracle legs above; each leg
runs only when its own provider gate is set.

## Real Forgejo + real agent checks

The Smith-owned Forgejo workflow-role e2e path is currently hermetic: it boots a
throwaway Forgejo and uses a jig fake LLM behind `SMITH_JIG_E2E=1`, as documented
in the testing reference. There is no separate Smith real Forgejo + real agent CI
gate on this checkout. If a future test uses real provider-backed agents with
Forgejo, keep it ignored and document its live provider gate here rather than in
CI.
