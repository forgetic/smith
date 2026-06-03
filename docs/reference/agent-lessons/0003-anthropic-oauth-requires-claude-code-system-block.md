# Lesson 0003: Anthropic OAuth needs the Claude Code identity as the first system block

## Tags

`agents`, `pi-sdk`, `oauth`, `auth`, `anthropic`

## Trigger

Anthropic OAuth live validation failed every decision with
`HTTP 429 {"type":"rate_limit_error","message":"Error"}` even though the
operator's Claude subscription access was valid.

## What went wrong

The 429 was not quota. Anthropic's Claude subscription OAuth path rejects a
`/v1/messages` request unless the first `system` block is exactly:

```text
You are Claude Code, Anthropic's official CLI for Claude.
```

The pinned SDK sends `system` as a single string and cannot send an array of
system blocks. Concatenating that identity with the role prompt in one string does
not satisfy the check.

## Steering for future agents

Do not treat a bare Anthropic OAuth 429 as quota until this identity rule has
been checked. Smith sends the Claude Code identity as the system prompt and folds
the role prompt into the user turn for Anthropic OAuth only. ChatGPT OAuth and
DeepSeek keep the role prompt as `system`.

## Where this is now documented

- `crates/smith-temper-agent/src/provider/anthropic_oauth.rs`.
- `crates/smith-temper-agent/src/provider.rs`.
- `crates/smith-temper-agent/src/decision.rs`.
- `docs/reference/provider-auth.md`.
