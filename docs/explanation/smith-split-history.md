# Smith split history

Smith was extracted from Temper after Temper gained provider-neutral interaction
and workflow-role process protocols.

The split moved concrete `pi_agent_rust` behavior out of Temper:

1. Temper froze interactive responder and workflow-role decision wire fixtures.
2. Temper added hermetic process adapters and conformance tests.
3. Smith was bootstrapped with provider/auth/decision code and live-provider
   tests.
4. Product-manager profile behavior moved to `smith-product-manager-responder`.
5. Workflow-role LLM decisions moved to `smith-workflow-role-decision`.
6. Temper removed `temper-agents`, `pi_agent_rust`, provider/auth CLI flags, and
   real-agent test fixtures.

The stable result is the current ownership model: Temper is the runtime and
source-of-truth workflow engine; Smith is a concrete responder repository. Tests
moved out of Temper have Smith equivalents or Temper-side process-adapter
coverage, summarized in `docs/reference/split-coverage.md`.
