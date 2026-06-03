# Run live provider and Forgejo checks

Default Smith validation is hermetic:

```sh
cargo fmt --all
cargo test --workspace --all-targets
```

Live checks are ignored because they can call model providers, refresh real OAuth
credentials, or boot a throwaway Forgejo.

## ChatGPT/OpenAI Codex OAuth

Run `pi /login openai-codex` first, then:

```sh
TEMPER_CHATGPT_OAUTH=1 \
  cargo test --test chatgpt_oauth_live -- --ignored --nocapture
```

The refresh check may rotate the refresh token and write the refreshed credential
back to the real auth file.

## Anthropic OAuth

Run `pi /login anthropic` first, then:

```sh
TEMPER_ANTHROPIC_OAUTH=1 \
  cargo test --test anthropic_oauth_live -- --ignored --nocapture
```

## Real Forgejo + real LLM process-boundary proof

This Smith-owned proof boots a throwaway Forgejo, runs the Smith workflow-role
process through Temper's `WorkflowRoleDecisionProcessAgent`, invokes a test
coding workspace, and opens a PR through `RoleTools`:

```sh
TEMPER_FORGEJO_E2E=1 TEMPER_FORGEJO_AGENTS=1 \
  cargo test -p smith-temper-agent-cli --test forgejo_workflow_role_e2e -- \
  --ignored --test-threads=1
```

`TEMPER_AGENTS_AUTH=deepseek|chatgpt-oauth|anthropic-oauth` selects the auth mode
for this e2e; it defaults to ChatGPT OAuth.
