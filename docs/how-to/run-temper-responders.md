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

## Product-manager example interaction responder

```sh
cargo build -p smith-temper-agent-cli --bin smith-product-manager-responder
```

The binary reads one Temper `ConversationRequest` JSON value on stdin and writes
one `ConversationReply` JSON value on stdout. It serves only the
`product-manager` example profile and returns reply text plus inert issue-draft
proposals. External frontends should call Temper's generic `temper-interaction`
REPL or HTTP service, not this responder directly.

Bind the responder through the interaction deployment binding file that Temper
loads with the user-defined profile spec (fragment shown):

```json
{
  "responders": {
    "product-manager-responder": {
      "command": "/path/to/smith/target/debug/smith-product-manager-responder",
      "args": ["--auth", "chatgpt-oauth"],
      "env_allowlist": [],
      "timeout_secs": 120
    }
  }
}
```

Then launch Temper, for example:

```sh
temper-interaction repl \
  --spec /path/to/product-manager.json \
  --bindings /path/to/interaction-bindings.json \
  --profile product-manager
```

Temper dogfood's `./run.sh product-chat` command generates the generic
interaction binding and points it at this Smith binary by default.

## Provider args and env

Pass Smith provider options through the role responder `*_ARGS_JSON` variables
or the interaction binding file's `responders.<id>.args` array. Use env
allow-lists only for provider names Smith documents, such as
`TEMPER_DEEPSEEK_API_KEY` or `TEMPER_AGENTS_ANTHROPIC_MODEL`.

Protocol details live in Temper's `docs/reference/` directory; Smith's exact
responder contract summary is `docs/reference/process-responders.md`, and
Smith's decision events/captures are documented in
`docs/reference/workflow-role-observability.md`.

## Optional captures for the reference-delivery demo

Captures are off by default. For one debugging run of Smith's
reference-delivery demo through Temper:

```sh
cd ~/src/rust/smith
cargo build -p smith-temper-agent-cli --bin smith-workflow-role-decision

cd examples/reference-delivery
mkdir -p run/smith-captures
export SMITH_WORKFLOW_ROLE_DECISION_CAPTURE_DIR="$PWD/run/smith-captures"
export SMITH_WORKFLOW_ROLE_DECISION_ENV_ALLOWLIST=SMITH_WORKFLOW_ROLE_DECISION_CAPTURE_DIR
POLL_MS=120000 ./run.sh start
./run.sh validate-multi-repo
```

Keep provider credentials on Smith's documented auth surfaces; append only the
provider env names Smith requires (for example DeepSeek's key env) if your auth
mode needs them. Do not add Forge tokens or auth-file paths to the role-decision
allow-list. Inspect Temper worker logs for `role_decision_*` /
`transition_execution`, then join to Smith events or captures by `decision_id` /
`work_item_id`.
