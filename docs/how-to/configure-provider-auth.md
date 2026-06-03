# Configure provider auth

Smith responder binaries accept the same provider flags:

```sh
--auth deepseek|chatgpt-oauth|anthropic-oauth
--codex-model MODEL
--auth-file PATH
```

The binaries default to `chatgpt-oauth`. Run preflight before wiring a provider
into Temper:

```sh
cargo run -p smith-temper-agent-cli -- preflight --auth chatgpt-oauth
```

## ChatGPT/OpenAI Codex OAuth

1. Log in once with pi:

   ```sh
   pi /login openai-codex
   ```

2. Smith reads the `openai-codex` entry from `~/.pi/agent/auth.json`, accepting
   both nodejs and Rust pi schemas and preserving the schema on refresh.
3. Optional overrides:

   ```sh
   TEMPER_AGENTS_AUTH_FILE=/path/to/auth.json
   TEMPER_AGENTS_CODEX_MODEL=gpt-5.5
   ```

The Codex access token is short-lived. Smith resolves and refreshes it per
request; live refresh may rotate the refresh token and write back to the auth
file.

## Anthropic OAuth

1. Log in once with pi:

   ```sh
   pi /login anthropic
   ```

   Select the Anthropic OAuth/subscription credential when prompted.

2. Optional overrides:

   ```sh
   TEMPER_AGENTS_AUTH_FILE=/path/to/auth.json
   TEMPER_AGENTS_ANTHROPIC_MODEL=claude-opus-4-8
   ```

Smith injects the Claude Code-compatible request identity required by Anthropic
subscription OAuth.

## DeepSeek API key

Provide a key by env var or a gitignored local file:

```sh
export TEMPER_DEEPSEEK_API_KEY=...
# or
mkdir -p .cache
printf '%s' '...' > .cache/deepseek-api-key
```

`TEMPER_DEEPSEEK_API_KEY_PATH` overrides the file path.

## Secrets discipline

- Never pass provider secrets on argv.
- Never commit auth files, copied `auth.json`, tokens, or `.env` files.
- When Temper launches Smith, allow-list only provider env vars Smith must read;
  never allow-list Forge tokens or workflow credentials.
