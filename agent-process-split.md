# Agent / orchestration split — two-plane architecture

> **Decision record + implementation plan.** Authored 2026-06-12 after a design
> discussion with the operator (Free). This supersedes the in-process
> consolidation of the coding agent into `smith-worker` (commits `d5f8d0a`..`bff3f70`)
> for the Smith coding path — *not* by reverting it, but by re-drawing the
> boundary it was built behind. The seam introduced by that work
> (`AgentRunner`, `crates/smith-worker/src/agent_runner.rs`) is what makes this
> cheap.

## TL;DR

We split the system along **two independent planes**, and we make the
agent a **separate process in a separate repository**:

1. **Orchestration plane (forge-based).** `temper-daemon` + `smith-worker` +
   Forgejo/GitHub. Drives the code lifecycle: issues, PRs, comments, labels,
   reviews, merges. Narrow, coarse, durable, human-cadence. This is *all* smith
   and temper know about.

2. **Control / observability plane (out-of-band).** Rich, live, bidirectional:
   stream deltas, steering, abort, introspection, a web UI. Owned by the
   **agent side**. `smith`, `temper`, and the forge know **nothing** about it and
   never need to. (Not built here — this plan only preserves the agent-side API
   it will consume; the system itself is future work.)

The agent runs **out-of-process**, behind a small **versioned protocol** spoken
over a child process. The two planes meet at exactly **one field**: a
**correlation id** minted in the orchestration world and carried into the agent,
which stamps it on everything it emits to plane 2.

## Why

### Why two planes

Orchestration events and control/observability events have opposite profiles:

| | orchestration (plane 1) | control/observability (plane 2) |
|---|---|---|
| frequency | rare | high |
| granularity | coarse (a verdict, a PR) | fine (token deltas, tool calls) |
| durability | durable (system of record) | ephemeral |
| cadence | human | real-time |
| change rate | stable (versioned wire) | fast |
| failure blast radius | must not break job completion | can be down with no effect on jobs |
| security | forge creds | privileged (can *steer* a live agent) |

Forcing both through one channel compromises one of them. Forgejo/GitHub model
**code lifecycle** and nothing else — they have no native "stream me the agent's
reasoning" or "pause this run and inject a hint". You can shoehorn (comment-spam,
encode state in labels) but it is abuse, rate-limited, lossy, and it pollutes the
human-facing artifact. Accepting the forge's narrow scope as a *feature* is the
mature call: the forge does code lifecycle, the control plane does everything
real-time, and they don't know about each other.

### Why out-of-process (the decisive argument: reusability)

Fault isolation is real but secondary. The decisive reason is **reusability /
architecture identity**: a process boundary with a serde protocol makes
`smith` + `temper` a *product with a clean contract* — "bring any agent that
speaks this protocol". Someone can run the orchestration with a deterministic
scripted agent, their own LLM stack, a different SDK, or a non-LLM tool, without
pulling in `pi_agent_rust`, our provider/auth code, or committing to our control
plane. The boundary you most want to keep honest is the one you make
*impossible* to violate; an in-process library boundary erodes by convention
(someone reaches across "just this once"), a process+wire boundary cannot.

This also makes "smith is just orchestration" a **deployment fact**, not a
layering claim to defend: smith ships without any agent.

Fault isolation is the bonus: per the port history, the in-process agent loop
wedged five times during this very effort. A wedged/crashed agent turn must not
be able to take down the worker.

### Why progress reporting is *recovery*, not just observability

This is the key upgrade to plane 1. In a forge-orchestrated system **the forge
is the durable state.** If an agent crashes mid-task and has (a) git-pushed the
work it completed and (b) recorded that progress on the PR/issue, then the
orchestration layer can re-assign and a *fresh* agent resumes from a real
checkpoint instead of from zero.

So plane-1 progress is a **crash-recovery checkpoint / handoff channel**, not
just a human nicety. Its primary consumer is the *next agent*; human-readability
is a happy side effect. The recovery point lives in git + the forge, so it
survives the agent process, the worker process, and a full redeploy.

Properties and limits (state them honestly):

- This gives **resumability**, not idempotency/transactionality. A crash between
  "push" and "progress-update" leaves a small inconsistency window. Tolerable:
  the next agent re-derives reality from the branch diff. Semantics are
  best-effort checkpoint + a fresh agent reconciles — matching how the rest of
  the forge-orchestrated system already behaves.
- The checkpoint must be a *safe resume point*: push at **coherent step
  boundaries** (a completed step), and the progress marker must reflect what was
  actually pushed. This requirement and "keep plane 1 narrow/low-frequency"
  point the same way — good sign.

## The relay decision: (A) agent → worker → forge

When the agent finishes a step it reports progress **to the worker**, and the
**worker** relays it to the forge (tick a todo box, update the PR body/comment).

- The **agent has git credentials** (provided via the prepared workspace) to
  **push commits** — and that is *all* the forge access it has.
- The **worker owns every forge-API interaction** (comments, body updates,
  labels, the eventual verdict apply). The agent never calls the forge API and
  needs no forge token.

Rejected alternative (B) "agent comments/pushes to forge directly": it would
force every plugged-in agent to speak forge and hold forge creds, leaking
orchestration concerns into the agent and weakening the reuse contract. (A)
keeps the agent **forge-agnostic** — it just emits "step N done, here's a commit
sha" — which is what makes "bring any agent" real.

## The protocol (plane 1 wire)

Formalize today's informal env-var/file protocol
(`TEMPER_CODING_WORKSPACE_CONTEXT` / `_RESULT`,
`crates/smith-worker/src/external_command_runner.rs`) into a **versioned spec
crate** shared by the worker and the agent. Shape:

- **Inbound (worker → agent), one-shot:** the `WorkspaceContext` the worker
  assembled (repository, role, branch, verdict vocabulary, work item, …) **plus
  the correlation id**. Delivered as today: a context file named by an env var,
  run in the prepared checkout (cwd).
- **Outbound (agent → worker), two kinds:**
  - **Step progress (stream):** zero or more checkpoint records emitted *during*
    the turn — `{ correlation_id, step, status, pushed_sha?, note? }` — each at a
    coherent boundary, after the corresponding commit is pushed. Carried on a
    line-delimited JSON stream the worker reads live (the agent's stdout, framed
    one JSON object per line). The worker relays each to the forge.
  - **Result (terminal):** the existing `WorkspaceResult`
    (`{ verdict?, summary?, body?, review_body?, labels?, children? }`), written
    to the result file as today.

Versioning: the protocol crate carries a `PROTOCOL_VERSION`; the context and
each message embed it; mismatch is a clean protocol error. This crate is a
first-class member of the "DTO/spec, eventually codegen" family alongside
`temper-worker-protocol` / `temper-process-protocol`.

### Bright-line rule (write it down so it can't erode)

> **Plane-1 progress carries only state a human reading the PR wants as a
> durable artifact** — checklist state, phase, a one-line status, a pushed sha.
> Everything mechanistic or high-frequency (token deltas, individual tool calls,
> reasoning) goes to **plane 2**, full stop. If you are adding a field "so the UI
> can show X", that field is on the wrong plane.

## The correlation id (the only bridge)

One id, minted at job assignment in the orchestration world, flows down the
plane-1 context into the agent, and is stamped by the agent onto everything it
emits to plane 2. That is the **entire** coupling between the planes — an id, not
a schema. Design this flow first, before either channel grows. Everything that
is not the correlation id stays on its own side.

## The new repository

**Name: `anvil`** (`/home/free/src/rust/anvil/`). A smith works at an anvil; it
is where the shaping actually happens. Neutral, memorable, not "smith". The
operator said pick something reasonable — this can be renamed later.

Placed as a sibling of `smith`, `temper`, and `jig` so the existing
`../../../jig/...` and `../../../temper/...` path-dep strings resolve unchanged
after crates move.

### What moves to `anvil`

- `smith-agent` → the sans-IO LLM agent loop + sub-agents (`AgentMachine`,
  shell, `SubAgentTool`). **Keep the sans-IO machine pattern.**
- `smith-temper-agent` → provider/auth/decision/coding-loop, responders. (Name
  likely changes to drop the `smith-`/`temper-` prefixes over time; mechanical
  port first, rename later.)
- `smith-temper-agent-cli` → the responder/preflight bins.
- A **new agent binary** that speaks the plane-1 protocol: read context + correlation
  id, run the coding loop, emit step-progress on stdout, write the result file.
  This is the process `smith-worker` spawns.
- `smith-io-engine` is **shared infrastructure** (the sans-IO driver). Decision:
  it stays usable by both repos. Cleanest is to *also* extract it to a small
  shared crate/repo; acceptable interim is to copy/vendor the minimal driver the
  agent needs (the pattern is small and already duplicated from temper by
  design). Resolve during the port.

### What stays in `smith`

- `smith-worker` (orchestration: poll/dispatch/lease/verdict-apply, the sans-IO
  `worker_machine`, workspace lifecycle, **all forge interaction incl. progress
  relay**). After the split it depends on the agent **only** via the protocol
  crate + a spawned binary — no `pi_agent_rust`, no provider/auth code.
- `smith-io-engine` (unless extracted to shared — see above).

## Sequencing

1. **Design doc** (this file). ✅
2. **Create `anvil`; port the agent crates**; make it build standalone with the
   sans-IO pattern intact and its own tests green.
3. **Define the protocol spec crate** (versioned context + step-progress +
   result + correlation id); **build the agent binary** that speaks it.
4. **Rewire `smith-worker`**: out-of-process runner speaking the protocol as the
   Smith default; consume step-progress and relay to the forge; keep an
   external-command fallback for non-Smith coders; drop pi-SDK deps; update
   config flags.
5. **End-to-end + tests**: existing suites green; port `coding_worker_e2e` to
   drive the out-of-process agent; add coverage for the step-progress protocol,
   the progress→forge relay, and crash-recovery resume (partial push + marker →
   next agent resumes from the checkpoint).

## Non-goals / later

- The plane-2 control/observability system + web UI (separate project; this plan
  only preserves the agent-side steering/stream API it will consume).
- Renaming the ported crates to shed `smith-`/`temper-` prefixes (mechanical
  port first).
- Codegen of the protocol crate from a spec (same trajectory as the temper DTO
  crates).
- Multi-agent / per-job fan-out beyond what exists.

## Build/operational constraints (host)

~15Gi RAM, no swap, tight disk. Build with `-j1` to avoid OOM (parallel pi-sdk
builds risk OOM-kill). Prefer incremental; clean unused target dirs. Never use
`tokio::process` on the asupersync worker — use `spawn_blocking` (a real bug the
e2e caught). The new agent binary spawn from the worker follows the same rule.
