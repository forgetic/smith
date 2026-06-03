# Agent lessons register

This register captures recurring mistakes, failed assumptions, and human
steering that future Smith agents should learn from.

Use it as compact memory, not as a replacement for current docs. If a lesson
becomes a durable rule, also update the relevant README, how-to guide, reference
page, explanation, or ADR.

## When to read

At session start, read this index after `docs/README.md`. Then open entries
whose tags match the task. For provider/auth work, read all active lessons.

## When to add or update a lesson

Add or update a lesson when:

- a human corrects an agent's assumption or design direction;
- an agent makes a mistake that could recur;
- validation fails because docs or workflow guidance was missing or misleading;
- a workaround or steering rule would save future context.

Use `template.md`. Keep each entry short and specific.

## Active lessons

| ID | Title | Tags |
| --- | --- | --- |
| [0001](0001-pin-pi-sdk-transitive-deps.md) | Pin `pi_agent_rust`'s transitive deps when the SDK won't compile | tooling, rust, agents, dependencies, pi-sdk |
| [0002](0002-chatgpt-oauth-shared-auth-dual-schema.md) | Read the shared pi auth.json tolerantly | agents, pi-sdk, oauth, auth |
| [0003](0003-anthropic-oauth-requires-claude-code-system-block.md) | Anthropic OAuth needs the Claude Code identity as the first system block | agents, pi-sdk, oauth, auth, anthropic |
