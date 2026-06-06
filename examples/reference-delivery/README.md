# Reference-delivery example

A **Smith-owned operator demo** of Temper's production topology: a
Forgejo server, a `forgejo-runner` producing real CI, and one LLM-backed worker
per workflow role scanning a configured repository set, all coordinating through
the Forge. The launcher defaults to the sibling Temper checkout at `../temper`;
set `TEMPER_WORKSPACE_ROOT=/path/to/temper` if your checkout layout differs.

> **Status:** this example is wired to production-owned binary names
> (`temper-worker`, `temper-provision-forgejo`, and
> `temper-trigger-forgejo`) from Temper's root `temper` package instead of the
> `temper-testing` binaries. After the user-defined-role migration, production
> workers register generic agents from compiled workflow manifests, so reference
> role behavior lives in `config/workflow.json` (which tracks the canonical
> fixture `crates/temper-workflow/fixtures/reference-delivery.json`), not in
> production prompt constants. The checked-in default is a **single-repo happy
> path that converges to a merged PR**: the launcher auto-binds the **Smith
> pi-SDK workspace agent** (`smith-coding-agent`) for the architect, engineer,
> and reviewer workspace tools, so the architect rewrites the intake body, the
> engineer opens a real implementation PR, the reviewer approves from the diff,
> and the **bot** (`mechanical` worker) lands it once the review and CI gates
> pass. Set `REFERENCE_DELIVERY_CODER=greeting` to swap in the deterministic
> stand-in coder (`tools/greeting-coder.sh`) for an offline/CI smoke run. Temper
> runtime details live in the sibling checkout's docs and plans.

## Honest framing — read this first

This is a **demo**, not a turnkey production deployment:

- It uses the production-owned binaries and does not fall back to
  `temper-testing` entry points. If those binaries are absent, `run.sh` stops at
  the build/resolve step.
- It boots its **own throwaway Forgejo + runner** so it runs from binaries
  alone. To target a **real** Forgejo you change `BASE_URL` + tokens and drop the
  bundled server/runner + provisioning — the same "swap to real" story as
  [`docs/how-to/run-forgejo-multiprocess-e2e.md`](../../../temper/docs/how-to/run-forgejo-multiprocess-e2e.md).
- The production provisioner commits a commit-message-marker CI that only the
  deterministic `temper-testing` fake worker satisfies. Because this demo pairs
  the engineer with a **real** coding workspace (whose PR-head commits carry an
  ordinary message), `run.sh` overrides that marker CI with the bundled
  pass-through `config/ci.yml` so a real product diff clears the landing CI gate.
  The PR diff guard still rejects bookkeeping-only heads.
- By default the workspace tools run a **real LLM agent** (`smith-coding-agent`):
  the architect reads the intake and returns a triage verdict, the engineer
  edits the checkout to leave a product diff, and the reviewer reads the real
  diff and CI before returning an approve/changes/escalate verdict. Set
  `REFERENCE_DELIVERY_CODER=greeting` for the deterministic stand-in (a fixed
  `src/banner.sh` diff, no LLM, no verdicts) when you need an offline, reliably
  converging run. Bind your own coder via `TEMPER_CODING_WORKSPACE_*` to validate
  a different implementation path.
- It is the operator-facing, shell-driven version of the same topology covered
  by the ignored Forgejo multi-process tests — not new workflow behavior.
- Cross-repo fan-out (set `REPOS` to several repos) is **optional**. The architect
  can now break an intake down into dependent child issues via the
  `needs_breakdown` verdict and the routed `create_issues` effect, but a
  cross-repo coordination parent still needs a **user-authored plan** naming the
  target repos and slugs; without it a bare cross-repo parent stays `code +
  blocked`. It is planning/aggregation only — no atomic cross-repo merges, shared
  branches, or per-repo workflow definitions.

Keep these caveats in mind; this does not pretend to be more than a faithful
end-to-end rehearsal.

## Prerequisites

- The operator-facing workspace binaries built: `cargo build -p
  temper` (provides `temper-worker`,
  `temper-provision-forgejo`, and `temper-trigger-forgejo`). `run.sh` refreshes
  the development-profile binaries under `target/debug` before start unless
  `TEMPER_SKIP_BUILD=1`, so stale binaries do not break the demo after source
  changes. Override paths with `TEMPER_WORKER_BIN` / `TEMPER_PROVISION_BIN` /
  `TEMPER_TRIGGER_BIN` if needed.
- The two pinned binaries: Forgejo `7.0.12` and `forgejo-runner` `3.5.1`.
  Pre-stage them under `.cache/forgejo/` with
  `cargo test -p temper-forgejo-fixture --test cache -- --ignored`, or set
  `TEMPER_FORGEJO_BINARY` / `TEMPER_FORGEJO_RUNNER_BINARY` in
  `config/temper.env`.
- A host that permits **host-mode** CI jobs (spawning child processes, binding a
  loopback port) — the runner executes steps directly on the host, no containers.
- Smith workflow-role decisions. By default the launcher uses this Smith
  checkout for `smith-workflow-role-decision`; override `SMITH_WORKSPACE_ROOT`
  only for a different checkout. Smith owns provider/auth setup and preflight;
  Temper only passes opaque responder args/env allow-list configuration.

See `secrets/.env.example` for the options in detail.

## Layout

```text
examples/reference-delivery/
├── README.md            # this file
├── observability.md     # operator event/validator guide
├── .gitignore           # ignores runtime run/, logs/, *.pid, *.log
├── config/
│   ├── temper.env      # operator-editable knobs (no secrets)
│   ├── workflow.json    # the bundled workflow spec (tracks the canonical
│   │                    #   fixture crates/temper-workflow/fixtures/
│   │                    #   reference-delivery.json — do not fork its semantics)
│   └── ci.yml           # the host-mode pass-through CI run.sh applies over the
│                        #   provisioned marker CI (real coder heads must pass it)
├── tools/
│   └── greeting-coder.sh # deterministic stand-in coder (REFERENCE_DELIVERY_CODER=greeting)
├── secrets/             # gitignored except the templates + .gitignore
│   └── .env.example
└── run.sh               # launcher/teardown (phase B3)
```

The workflow **roles** (architect, engineer, reviewer, owner, human), labels,
role guidance, prompt extensions, and external-tool declarations are derived from
`config/workflow.json` (which tracks the canonical fixture). Generated prompts
carry mechanics and authority boundaries; `charter`, `prompt.guidance`,
`prompt.tool_guidance`, and `external_tools` carry the reference-delivery demo's
user-authored behavior. `config/` otherwise carries what an operator must edit
(the repository set, endpoint, cadence, Smith responder args, and the coding
workspace binding).

## Quick start

From this directory:

```sh
POLL_MS=120000 ./run.sh       # long-poll mode: webhooks wake workers promptly;
                               #   Ctrl-C tears everything down
./run.sh validate-multi-repo   # repo-specific provisioning/webhook/worker smoke
./run.sh validate-webhooks     # summarize webhook registration/delivery/wake logs
./run.sh stop                  # tear down a previous run via the saved PIDs
./run.sh help                  # usage
```

Each start refreshes the development-profile workspace binaries (`cargo build -p
temper`, usually a no-op when current) under `target/debug` and
expects the pinned Forgejo + `forgejo-runner` binaries under `.cache/forgejo/`
(populate with `cargo test -p temper-forgejo-fixture --test cache -- --ignored`,
or set `TEMPER_FORGEJO_BINARY` / `TEMPER_FORGEJO_RUNNER_BINARY`). `run.sh
start` runs from a private snapshot under `run/`, so editing the launcher while a
demo is running cannot corrupt the eventual teardown path. Edit
`config/temper.env` for the repo set, endpoint, cadence, and Smith responder
args; any of those may also be overridden by exporting the matching env var
before invoking
the script (env wins over the file). The checked-in default is the single-repo
converging happy path: `REPOS="acme/service"` with `CROSS_REPO_INTAKE=0`, so one
intake is triaged to a ready code issue, the engineer opens a real PR, CI passes,
the reviewer approves, and the bot lands the merge. Set `REPOS` to several repos
to scan a repo set (tokens must have Forge access to every listed repo; Forge
permissions, not scan-shard membership, authorize writes); cross-repo fan-out is
optional and needs a bound fan-out tool to converge (see "Honest framing").

Progress is printed without secrets (server URL, seeded issue URLs, where logs
live); per-process logs land under `logs/`. The checked-in default
`POLL_MS=120000` is intentional: polling is only the liveness backstop, while
webhooks should make the demo visibly progress before the two-minute deadline.

## Coding workspace binding

`config/workflow.json` declares three workspace external tools: the architect's
`triage_workspace`, the engineer's `coding_workspace`, and the reviewer's
`review_workspace`. One bound command backs all three — temper invokes it per
role with the right checkout capability (writable for the engineer, read-only for
the architect, PR-read-only for the reviewer) and the work-item context — so the
single binding serves the whole trail.

In the single-repo default the launcher **auto-binds the Smith pi-SDK workspace
agent** (`smith-coding-agent`, built from this checkout) so the example converges
without extra setup: it clones the configured repo into `run/coding-workspace/`,
applies the pass-through `config/ci.yml`, and points
`TEMPER_CODING_WORKSPACE_ROOT`/`COMMAND` at the agent. The agent is
capability/role-aware: the engineer gets edit tools and leaves a product diff;
the architect and reviewer get read-only tools and return a verdict.

`REFERENCE_DELIVERY_CODER` selects the auto-bound command:

- `smith` (default) — the LLM workspace agent above. Provider/auth come from
  `SMITH_CODING_AGENT_ARGS` (default `--auth chatgpt-oauth`, mirroring the
  role-decision responder).
- `greeting` — the deterministic stand-in (`tools/greeting-coder.sh`): a fixed
  `src/banner.sh` diff, no LLM, no verdicts. Use it for an offline/CI smoke run.

To validate a different implementation path, bind your own coder; `run.sh` then
respects your binding and does not auto-bind:

```sh
export TEMPER_CODING_WORKSPACE_ROOT=/path/to/checkout
export TEMPER_CODING_WORKSPACE_COMMAND='your-coder --context "$TEMPER_CODING_WORKSPACE_CONTEXT"'
export TEMPER_CODING_WORKSPACE_REMOTE=origin
export TEMPER_CODING_WORKSPACE_PUSH=1
./run.sh start
```

The command runs with the checkout as its working directory and receives a JSON
context path in `TEMPER_CODING_WORKSPACE_CONTEXT` (carrying the role, the
checkout capability, and the work item) plus a `TEMPER_CODING_WORKSPACE_RESULT`
path it may write a verdict/content result to. On the engineer's writable
checkout it must leave a meaningful non-`.temper*` product diff; the local-git
provider commits and pushes the branch, then the workflow opens the PR. On a
read-only checkout (architect/reviewer) it must return a verdict instead.
Leaving the binding empty in multi-repo mode keeps the workspace roles idle (the
safe `no_action` state). Use these focused checks before a full demo run:

```sh
cargo test -p temper-coding-workspace local_git_workspace_accepts_product_code_or_docs_diff
cargo test -p temper-testing --test forgejo_workspace_pr -- --ignored --test-threads=1
```

## What it does

Boots Forgejo + a host-mode `forgejo-runner`, starts the local webhook trigger,
provisions the configured repo with labels, CI, a webhook, and a `bot`
automation user, then seeds one **plain unlabeled** human intake issue. In the
single-repo default it also clones the repo, applies the pass-through CI, and
binds the Smith pi-SDK workspace agent. It launches exactly one `temper-worker`
per role-with-an-agent plus one mechanical worker. Workers use wall-clock polling
as the liveness backstop; webhooks wake them early.

The intake then flows end to end:

1. the **bot** (`mechanical` worker) marks the seeded raw intake `untriaged`
   (`raw_intake` automation → `mark_untriaged`);
2. **architect** runs the `triage_workspace` and, on a `ready_code` verdict,
   rewrites the issue body into a code spec and routes it to a ready `code`
   issue (`set_body` + `code`/`ready`); `needs_design` and `needs_breakdown`
   verdicts route the design and child-issue branches instead;
3. **engineer** claims the `code` issue and runs the `coding_workspace` to leave
   a real product diff; the engine pushes the branch and opens an
   `implementation` PR labelled `needs-reviewer` (a `needs_architect` verdict
   instead escalates the issue to the architect);
4. the **`forgejo-runner`** runs real CI on the PR head;
5. **reviewer** runs the `review_workspace` against the real diff and CI and, on
   an `approve` verdict, approves the PR, which adds the `landing` label
   (`changes` submits a native review carrying the authored body; `escalate`
   flags the architect);
6. the **bot** lands (merges) the PR once the review and CI gates are green
   (`landing` automation → `land_pr`), then marks it `landed` + `alignment`;
7. **architect** reconciles the `landed` PR.

The **mechanical** worker also runs the controller plane (lease expiry, partial-
transition repair, dependency unblock) and, in this workflow, owns landing: it
reads Forgejo Actions status as the `bot` user (the owner no longer merges). See
[`docs/explanation/forgejo-e2e-topology.md`](../../../temper/docs/explanation/forgejo-e2e-topology.md)
for the durable topology and real-CI design.

## Cross-repo fan-out (optional, advanced)

Setting `REPOS` to several repos with `CROSS_REPO_INTAKE=auto` seeds one parent
intake in the first repo whose body names every repo id and asks the architect
for one child per repo — a **user-authored plan**. The architect's
`triage_workspace` reads that plan and, on a `needs_breakdown` verdict, authors
the dependent child issues through the routed `create_issues` effect. Whether the
parent then unblocks depends on the LLM run faithfully producing one correctly
repo-qualified child per repo from the plan; a bare cross-repo parent with no
such plan stays `code + blocked` (exactly the stall the single-repo default
avoids). Cross-repo mode remains primarily useful to exercise per-repo
provisioning, webhooks, and the fixed-pool scan; it is planning/aggregation only,
with no atomic cross-repo merges.

## Cross-repo production model

- Run **one worker per role**, not one worker pool per repository. The same role
  process scans every configured repo.
- Use **one token identity per role** with Forge access to every involved repo.
  Labels, CI workflow, and webhooks are ensured **per repo** during provisioning
  and worker startup.
- The source parent links to children with repo-qualified artifact references;
  child creation uses global correlation keys so re-scans do not duplicate work.
- Webhooks carry repo-specific hints that wake the shared pool and prioritize the
  hinted repo; polling remains the correctness backstop.
- Children land independently. The parent is an aggregation record, not an atomic
  cross-repo transaction.

## Watching progress and validating webhooks

See [`observability.md`](observability.md) for the event names, correlation
fields, and Forge-state validator diagnostics to inspect when the workflow moves
or stalls.

Open the Forgejo UI at `BASE_URL` (log in as any provisioned role). In the
configured repo, open the seeded intake. Watch the architect triage it to a ready
code issue, the engineer open an implementation PR, CI run, the reviewer approve
(which adds `landing`), and the **bot** land the merge. Worker logs land under
`logs/` (created at run time). When webhooks are enabled, the trigger wakes the
fixed worker pool for events from any repo. Wake payloads carry the repository
hint; configured repos are scanned first and unknown-repo hints are logged by
workers and treated as a broad scan:

- `logs/provision.log` records `repo=owner/name intake_issue_url=...` for the
  seeded intake (in cross-repo mode, `cross_repo_parent_url=...` for the source
  repo and `no_intake_seeded=cross-repo-target` for targets) and
  `repo=owner/name webhook registered url=...` for each repo;
- `logs/trigger.log` reports `listening on`, `webhook accepted` or
  `webhook rejected`, and `wake_delivery outcome=no_sockets|sent|all_failed`
  with target/sent/failed counts;
- worker logs report `consumed authenticated wake` and then
  `completed tick trigger=wake actions=N`. `actions=0` means the worker woke,
  scanned fresh Forge state, and found no queue item.

In another terminal, run:

```sh
./run.sh validate-multi-repo
```

It verifies that every configured repo appears in provisioning, trigger, and
worker startup logs, checks that only the source repo received the parent intake,
then summarizes accepted webhook deliveries, wake batches sent, per-worker wake
consumption, wake-triggered ticks, and whether any wake-triggered tick made
workflow progress (`actions>0`). When cross-repo intake is enabled it also reads
live Forge state through `temper-validate-reference-delivery` and fails loudly
for missing fan-out children, child metadata, or a blocked parent with zero
dependencies. Run it before `./run.sh stop`; teardown removes the throwaway
Forgejo state. For a cheaper generic wake check, use `./run.sh validate-webhooks`.
For a long-poll smoke, start the demo with
`POLL_MS=120000 ./run.sh`, wait until workflow movement appears in Forgejo for
each repo, then run `./run.sh validate-multi-repo`; it should pass before any
two-minute poll backstop is needed.

## Validated smoke paths

The default, hermetic observability proof is:

```sh
cargo test -p temper-testing --test observability_smoke
```

The default, hermetic multi-repo topology smoke is the filesystem process
rehearsal:

```sh
cargo test -p temper-testing --test multi_repo_multiprocess
```

The live Forgejo/webhook smoke is ignored because it boots Forgejo and a
host-mode runner:

```sh
cargo test -p temper-testing --test forgejo_multi_repo_webhook -- --ignored --test-threads=1
```

That live test validates the repo-hinted wake path. The cross-repo fan-out
scenario is covered by the ignored Forgejo multiprocess suite and the default
multi-repo process suite. The shell demo's own validation path is
`./run.sh validate-multi-repo` during a live run; it validates operator logs and
Forge state for the seeded cross-repo parent.

## Troubleshooting long-poll wakeups

- **No `webhook registered` in `logs/provision.log`:** provisioning failed before
  the hook was registered, or `WEBHOOKS=0` was set.
- **Registered but no `webhook accepted` in `logs/trigger.log`:** confirm the
  trigger reached `listening on`, `WEBHOOK_URL` points at `TRIGGER_BIND`, and the
  bundled Forgejo config allows loopback webhooks (`ALLOWED_HOST_LIST` includes
  `127.0.0.1,localhost`).
- **Accepted but `wake_delivery outcome=no_sockets`:** a webhook arrived before
  workers created wake sockets. The launcher starts downstream workers and waits
  for their sockets before launching the architect, so persistent `no_sockets`
  usually means workers failed during startup; inspect the role logs.
- **Wakes sent but a worker lacks `consumed authenticated wake`:** inspect that
  worker's log for auth/backend setup errors. Polling will still recover at the
  next `POLL_MS` deadline, but the accelerator is not working for that worker.
- **`provision binary is stale or incompatible` or `worker binary is stale or
  incompatible`:** rerun without `TEMPER_SKIP_BUILD=1`, or rebuild the
  development binaries manually with `cargo build -p temper`.
  `TEMPER_SKIP_BUILD=1` assumes `target/debug` and any `TEMPER_*_BIN`
  overrides are already current.
- **Wake consumed with `actions=0`:** the wake path worked; that worker simply
  had no active queue item after re-reading Forge state. Workers batch queued
  wake datagrams before a tick, so a webhook burst should show one
  `consumed authenticated wake batch ...` line followed by one
  `completed tick trigger=wake actions=0` follow-up per worker, rather than a
  long train of stale no-op scans.
- **Forgejo remains CPU-heavy after the workflow is done:** first check whether
  worker logs are still processing wake batches. Persistent `actions=0` batches
  mean webhooks are only causing fresh scans; `git cat-file` processes are normal
  Forgejo helpers and are not by themselves proof of active work. Also remember
  that `ps %CPU` is a lifetime average; use `top`, `pidstat`, or two `/proc/PID/stat`
  samples to see whether CPU dropped after stopping workers. The demo caps
  `GOMAXPROCS` for Forgejo/runner; lower `TEMPER_FORGEJO_GOMAXPROCS` if you
  need a tighter local CPU ceiling.
- **`Forgejo already responds` on start:** an orphaned or separately started
  server is still bound to `BASE_URL`. Run `./run.sh stop`; if pid files were
  lost, clean up with the orphan commands below.

## Teardown

`./run.sh stop` (or a `SIGINT`/`SIGTERM` to the running script) kills every spawned
process and removes the throwaway data dirs. If a run is force-killed, clean up
orphans with `pkill -f forgejo` / `pkill -f temper-worker` and remove
the run/data dirs.

## Point it at your own Forgejo

Change `BASE_URL` to your instance, set `REPOS` (or a production `--repo-list`)
to the scan shard, supply one real per-role token with access to every listed
repo that may receive child work, and skip the bundled server/runner +
provisioning steps. Ensure labels, CI, and webhooks per repo before filing the
intake. Replace `config/ci.yml` with your real CI and pair the engineer with a
real coding agent via `TEMPER_CODING_WORKSPACE_*`. The demo includes the ADR 0009
**webhook triggering**
accelerator for the local topology. For a real Forgejo, register a hook on each
repo, expose `WEBHOOK_URL`
over HTTPS to the Forgejo server, and keep the worker wake sockets host-local.
This is the same swap-to-real path documented in
[`docs/how-to/run-forgejo-multiprocess-e2e.md`](../../../temper/docs/how-to/run-forgejo-multiprocess-e2e.md)
and [`docs/explanation/forgejo-e2e-topology.md`](../../../temper/docs/explanation/forgejo-e2e-topology.md).
