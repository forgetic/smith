# ADR 0001: Use Diátaxis for documentation

## Status

Accepted.

## Context

Smith is developed by humans and autonomous agents. Future contributors need
documentation that makes tasks, contracts, rationale, and significant decisions
easy to find.

## Decision

Organize documentation with Diátaxis:

- tutorials for learning;
- how-to guides for tasks;
- reference for exact contracts;
- explanation for rationale.

Architecture decision records live in `docs/adr/` as the durable record of
significant choices.

## Consequences

Contributors place documentation according to reader need rather than expanding
one catch-all guide. Public auth, responder, or provider behavior changes should
update reference and how-to docs in the same session.
