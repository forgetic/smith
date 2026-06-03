# Run Smith responders from Temper

Smith implements Temper's provider-neutral process protocols. Temper still owns
validation, transcript/proposal storage, workflow authority, and all Forge
mutation.

## Workflow-role decision responder

Build Smith's role responder:

```sh
cargo build -p smith-temper-agent-cli --bin smith-workflow-role-decision
```

Point Temper role workers at it:

```sh
export TEMPER_WORKER_ROLE_DECISION_COMMAND=$PWD/target/debug/smith-workflow-role-decision
export TEMPER_WORKER_ROLE_DECISION_ARGS_JSON='["--auth","chatgpt-oauth"]'
```

The binary reads one `WorkflowRoleDecisionRequest` JSON value on stdin and writes
one `WorkflowRoleDecisionReply` JSON value on stdout. It receives no Forge
handle, Forge token, SDK bash/file tools, or workflow mutation tools.

## Product-manager responder

```sh
cargo build -p smith-temper-agent-cli --bin smith-product-manager-responder
```

```sh
export TEMPER_PRODUCT_CHAT_RESPONDER_COMMAND=$PWD/target/debug/smith-product-manager-responder
export TEMPER_PRODUCT_CHAT_RESPONDER_ARGS_JSON='["--auth","chatgpt-oauth"]'
```

The binary reads one Temper `ConversationRequest` JSON value on stdin and writes
one `ConversationReply` JSON value on stdout. It returns reply text plus inert
proposals only. External frontends should call Temper's product-chat service, not
this responder directly.

## Provider args and env

Pass Smith provider options through the `*_ARGS_JSON` variables or repeated
Temper CLI responder-arg flags. Use env allow-lists only for provider names Smith
documents, such as `TEMPER_DEEPSEEK_API_KEY` or
`TEMPER_AGENTS_ANTHROPIC_MODEL`.

Protocol details live in Temper's `docs/reference/` directory; Smith's exact
responder contract summary is `docs/reference/process-responders.md`.
