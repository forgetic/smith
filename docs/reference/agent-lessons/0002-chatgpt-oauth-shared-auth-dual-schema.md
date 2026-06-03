# Lesson 0002: Read the shared pi auth.json tolerantly

## Tags

`agents`, `pi-sdk`, `oauth`, `auth`

## Trigger

Wiring ChatGPT/OpenAI Codex OAuth into the provider core assumed the Rust SDK
could load any existing pi login from `~/.pi/agent/auth.json`.

## What went wrong

The nodejs pi and Rust SDK write the same file but use different schemas for the
`openai-codex` entry:

- nodejs: `{ "type": "oauth", "access", "refresh", "accountId", "expires" }`
- Rust SDK: `{ "type": "o_auth", "access_token", "refresh_token", "expires" }`

`AuthStorage::load` does not deserialize the nodejs-written entry as OAuth. The
bearer the Codex route wants is the OAuth access token, and `expires` is unix
milliseconds.

## Steering for future agents

- Accept both field spellings and both type tags.
- Preserve the schema and unknown fields on refresh write-back.
- Supply only the access-token bearer; do not set `chatgpt-account-id` yourself.
- Never log token bytes.
- Delegate login to `pi /login openai-codex`.
- Refresh rotates the refresh token, so live refresh tests must keep the written
  credential, not discard a refreshed copy.

## Where this is now documented

- `crates/smith-temper-agent/src/provider/oauth.rs`.
- `docs/adr/0003-chatgpt-oauth-agent-auth.md`.
- `docs/reference/provider-auth.md`.
- `docs/how-to/configure-provider-auth.md`.
