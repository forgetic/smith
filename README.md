# Smith

Smith is the external home for concrete agent implementations that Temper calls
through process protocols. It contains the Temper-specific `pi_agent_rust`
provider/auth/decision core and the product-manager interactive responder while
Temper still owns workflow and interaction contracts.

## Workspace

- `crates/smith-temper-agent` — library for provider selection, OAuth auth-file
  handling, per-request provider knobs, one-turn structured decisions, and the
  product-manager interactive profile mapping.
- `crates/smith-temper-agent-cli` — setup utility plus the
  `smith-product-manager-responder` process-protocol binary.

Smith may use local path dependencies on the sibling Temper checkout only for
protocol/domain crates needed by tests or binaries. The current split uses:

```text
../temper/crates/temper-interaction
../temper/crates/temper-workflow
```

Temper must not depend on Smith.

## Auth

Credentials are read from environment variables or the shared pi auth file;
never pass secrets on argv and never commit auth files here.

- ChatGPT/OpenAI Codex OAuth: run `pi /login openai-codex` once. Smith reads the
  `openai-codex` entry from `~/.pi/agent/auth.json`, accepting both nodejs and
  Rust pi schemas and preserving the schema on refresh.
- Anthropic OAuth: run `pi /login anthropic` once and select
  `anthropic-oauth`. Smith injects the Claude Code-compatible request identity
  needed by Anthropic subscription OAuth.
- DeepSeek/OpenAI-compatible API key: set `TEMPER_DEEPSEEK_API_KEY` or write the
  key to `.cache/deepseek-api-key` (gitignored). `TEMPER_DEEPSEEK_API_KEY_PATH`
  overrides that path.

Useful env vars:

```sh
TEMPER_AGENTS_AUTH_FILE=/path/to/auth.json
TEMPER_AGENTS_CODEX_MODEL=gpt-5.5
TEMPER_AGENTS_ANTHROPIC_MODEL=claude-opus-4-8
```

Optional credential preflight:

```sh
cargo run -p smith-temper-agent-cli -- preflight --auth chatgpt-oauth
cargo run -p smith-temper-agent-cli -- preflight --auth anthropic-oauth
```

## Product-manager responder

Build the responder and point Temper's product-chat process adapter at it:

```sh
cargo build -p smith-temper-agent-cli --bin smith-product-manager-responder
export TEMPER_PRODUCT_CHAT_RESPONDER_COMMAND=$PWD/target/debug/smith-product-manager-responder
export TEMPER_PRODUCT_CHAT_RESPONDER_ARGS_JSON='["--auth","chatgpt-oauth"]'
```

The binary reads one Temper `ConversationRequest` JSON value on stdin and writes
one `ConversationReply` JSON value on stdout. It receives no Forge handles,
Forge tokens, or workflow tools; Temper keeps transcript storage, proposal
validation, and explicit proposal acceptance. External frontends should still
call Temper's interaction/product-chat service, not this responder directly.

## Tests

Default validation is hermetic and does not require live credentials:

```sh
cargo fmt --all
cargo test --workspace --all-targets
cargo test --workspace --all-targets product_manager
```

Live provider checks are ignored and env-gated:

```sh
TEMPER_CHATGPT_OAUTH=1 \
  cargo test --test chatgpt_oauth_live -- --ignored --nocapture

TEMPER_ANTHROPIC_OAUTH=1 \
  cargo test --test anthropic_oauth_live -- --ignored --nocapture
```

Run the matching `pi /login ...` command before live tests. The ChatGPT refresh
check may rotate the refresh token and writes the refreshed credential back to
the real auth file, matching Temper's existing safety rule.

## Dependency note

`pi_agent_rust 0.1.13` pulls `asupersync =0.3.1`, which needs the
API-compatible `franken-decision 0.3.1`. Keep `Cargo.lock` pinned with:

```sh
cargo update -p franken-decision --precise 0.3.1
```
