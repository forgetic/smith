# Process responders

Smith provides the first concrete LLM implementations for Temper's process
protocols. Protocol structs and validation are Temper-owned.

## Workflow-role decision

Binary:

```text
smith-workflow-role-decision [--auth ...] [--codex-model MODEL] [--auth-file PATH]
```

Input: one `temper_runner::WorkflowRoleDecisionRequest` JSON value on stdin.
Output: one `temper_runner::WorkflowRoleDecisionReply` JSON value on stdout.
Errors/logs go to stderr.

Smith reads the role manifest, work-item context, authorized action list, and
bound external-tool metadata. It returns `no_action` or one authorized action
name. Unauthorized model actions are downgraded to `no_action`; unsupported
protocol versions fail before a model call.

## Product-manager interactive profile

Binary:

```text
smith-product-manager-responder [--auth ...] [--codex-model MODEL] [--auth-file PATH]
```

Input: one `temper_interaction::ConversationRequest` JSON value on stdin.
Output: one `temper_interaction::ConversationReply` JSON value on stdout.
Errors/logs go to stderr.

Smith serves only the `product-manager` profile. Replies contain display text and
inert issue proposals. Temper owns transcript storage, proposal validation,
explicit acceptance, and issue filing.

## Authority boundary

Responder processes receive no Forge credentials or mutation tools. Temper
clears the child environment except for configured allow-listed names, validates
reply shape/action/proposals, applies timeouts, and executes all state changes
through Temper-owned code.
