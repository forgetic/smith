# Basic-delivery example

A **deliberately minimal**, no-human-in-the-loop operator demo: it drives a
single issue **from submission to a merged PR with nobody in the loop**. It is
the "happy path, nothing fancy" counterpart to
[`examples/reference-delivery/`](../reference-delivery/): **one** repo, **three**
roles (`architect`, `engineer`, and the `bot` mechanical automation authority)
plus CI, webhooks on from the start, and landing gated on **CI alone** — no
reviewer, no owner, no human.

Like reference-delivery it boots the full Temper production topology from
development-profile binaries (a throwaway Forgejo server, a host-mode
`forgejo-runner` producing real CI, the production provision/worker binaries) and
binds the **Smith pi-SDK coding agent** (`smith-coding-agent`) for the LLM roles.
The launcher defaults to the sibling Temper checkout at `../temper`; set
`TEMPER_WORKSPACE_ROOT=/path/to/temper` if your checkout layout differs.

## What it demonstrates

The seeded intake flows end to end with only three workers running:

1. `run.sh` boots a throwaway Forgejo + runner and creates exactly **one org +
   repo** (`acme/service` by default).
2. `run.sh` files **one unlabeled intake issue** with a dead-simple coding task,
   authored by the **site admin** (mimicking an external filer). This works
   because the bundled `config/workflow.json` declares
   `intake_author: { "kind": "site_admin" }` (Temper W2), so provisioning seeds
   the issue as the admin identity and the issue lands **unlabeled**.
3. The workflow declares exactly **three roles**: `architect`, `engineer`, and
   `mechanical` (serviced by the `bot`), plus CI.
4. The **bot** (`mechanical` worker) transitions the issue unlabeled →
   `untriaged` (the `raw_intake` queue's `mark_untriaged` automation).
5. The **architect** (real LLM) triages. Its *only* option is to **rewrite the
   body into a crisp code spec and mark it `code` + `ready`** — there is no
   `needs_design` / `needs_breakdown` branch (`triage_intake` has a single
   `ready_code → triage_intake_to_code` outcome).
6. The **engineer** (real LLM) claims the ready code issue, implements it,
   leaving a real product diff, and opens an `implementation` PR.
7. The **`forgejo-runner`** runs real CI on the PR head, and it goes green.
8. The **bot** sees the green PR and **auto-merges** it (the `landing` queue's
   `land_pr` automation, gated on `ci_gate` only), then marks it `landed`.

No reviewer approves; no owner or human acts. The bot is the **sole landing
authority** and lands purely on CI.

## Temper prerequisite (must be in place)

This example loads its own 3-role spec at runtime and seeds intake as the site
admin, so it depends on the Temper-side changes tracked in
[`../temper/basic-delivery.md`](../../../temper/basic-delivery.md):

- **W1 — runtime workflow selection.** `temper-worker` and
  `temper-provision-forgejo` accept `--workflow <path>` (and `TEMPER_WORKFLOW_FILE`),
  defaulting to the bundled reference fixture when unset. `run.sh` passes
  `--workflow config/workflow.json` to **both** the provision and the worker
  invocations. Without W1 the file here would be read by nobody.
- **W2 — `intake_author` workflow knob.** The spec declares who files intake;
  this example sets `{ "kind": "site_admin" }` and has **no** `human` role.
  Without W2, provisioning hard-codes the seed to the `human` role and errors.
- **W3 — allowed verdicts surfaced to the workspace.** Temper writes the action's
  declared verdict vocabulary into the coding-workspace context as
  `allowed_verdicts`. See "Single-outcome triage" below.

All three landed on Temper `main` (children of parent #52: #53/W1, #54/W2,
#55/W3, #56 fixture+tests). If your Temper checkout predates them, rebuild it:
`run.sh` refreshes `target/debug` via `cargo build -p temper` on start (unless
`TEMPER_SKIP_BUILD=1`) and refuses to run against a provision/worker binary that
does not advertise `--workflow`.

## Single-outcome triage (how the architect is constrained)

The architect's `triage_intake` declares a **single** outcome (`ready_code`). For
the run to converge deterministically the architect must emit exactly that
verdict — never `needs_design` / `needs_breakdown`.

This example uses the **W3 path**: Temper surfaces the action's
`allowed_verdicts` in the workspace context, and the **Smith coding agent reads
it** (`crates/smith-temper-agent/src/coding_agent.rs`). When `allowed_verdicts`
is non-empty the agent constrains the role's system prompt to exactly that option
set — for a single-outcome triage this collapses to one choice — and rejects any
out-of-vocabulary verdict before Temper would fail the tick with an "undeclared
verdict" error. An empty `allowed_verdicts` (the engineer head path, or an older
Temper) falls back to the agent's built-in per-role verdict menu, so
reference-delivery is unaffected.

## Layout

```text
examples/basic-delivery/
├── README.md            # this file
├── .gitignore           # ignores runtime run/, logs/, *.pid, *.log
├── config/
│   ├── temper.env       # operator-editable knobs (no secrets)
│   ├── workflow.json    # the 3-role basic-delivery spec (tracks the canonical
│   │                    #   fixture crates/temper-workflow/fixtures/
│   │                    #   basic-delivery.json — keep the two in sync; see below)
│   └── ci.yml           # the host-mode CI run.sh applies over the provisioned
│                        #   marker CI (real coder heads must pass it)
├── tools/
│   └── greeting-coder.sh # deterministic engineer-head stand-in (BASIC_DELIVERY_CODER=greeting)
├── secrets/             # gitignored except the templates + .gitignore
│   └── .env.example
└── run.sh               # launcher / teardown
```

The workflow **roles**, labels, role guidance, prompt extensions, and
external-tool declarations are derived from `config/workflow.json`. `config/`
otherwise carries what an operator may edit (the org/repo, endpoint, cadence,
Smith responder args, and the coding workspace binding).

> **Keeping the spec in sync.** `config/workflow.json` is the canonical
> basic-delivery spec. A copy lives as the Temper test fixture
> `crates/temper-workflow/fixtures/basic-delivery.json` (validation/route tests
> in `crates/temper-workflow/tests/basic_delivery.rs`). The two must stay in
> sync. **Note:** at the time of writing the Temper fixture omits the
> `intake_author` field that this example requires (its W2-era note said the
> field would be added when W2 landed, but the fixture was not updated). This
> example sets `intake_author: { "kind": "site_admin" }` because provisioning
> needs it; a follow-up should add the same field to the Temper fixture so the
> two match byte-for-byte.

## Prerequisites

- The operator-facing Temper binaries built: `cargo build -p temper` (provides
  `temper-worker`, `temper-provision-forgejo`, `temper-trigger-forgejo`). `run.sh`
  refreshes them under `target/debug` before start unless `TEMPER_SKIP_BUILD=1`.
  Override paths with `TEMPER_WORKER_BIN` / `TEMPER_PROVISION_BIN` /
  `TEMPER_TRIGGER_BIN`.
- The pinned binaries: Forgejo `7.0.12` and `forgejo-runner` `3.5.1`. Pre-stage
  them under `.cache/forgejo/` with
  `cargo test -p temper-forgejo-fixture --test cache -- --ignored`, or set
  `TEMPER_FORGEJO_BINARY` / `TEMPER_FORGEJO_RUNNER_BINARY` in `config/temper.env`.
- A host that permits **host-mode** CI jobs (spawning child processes, binding a
  loopback port) — the runner executes steps directly on the host, no containers.
- Smith provider/auth for the LLM roles. By default the launcher builds this
  Smith checkout's `smith-workflow-role-decision` and `smith-coding-agent` and
  runs them with `--auth chatgpt-oauth`. Smith owns provider/auth setup and
  preflight; Temper only passes opaque responder args. See `secrets/.env.example`.

## Quick start

From this directory:

```sh
./run.sh                # boot everything; webhooks wake workers; Ctrl-C tears down
./run.sh validate-webhooks   # summarize webhook registration/delivery/wake logs
./run.sh stop                # tear down a previous run via the saved PIDs
./run.sh help                # usage
```

Progress is printed without secrets (server URL, the seeded issue URL, where logs
live); per-process logs land under `logs/`. The checked-in default
`POLL_MS=120000` is intentional: polling is only the liveness backstop, while the
trigger's webhook wakes make the demo visibly progress before the two-minute
deadline. Edit `config/temper.env` for the org/repo, endpoint, cadence, and Smith
responder args; any of those may also be overridden by exporting the matching env
var before invoking the script (env wins over the file).

### Offline / no-LLM smoke

The full unattended run needs a real LLM for the architect (single-outcome
triage) **and** the engineer (implementation). For an offline smoke of just the
**engineer head path** — boot, provision, the deterministic stand-in coder leaves
a fixed `src/banner.sh` diff that passes the bundled CI — set:

```sh
BASIC_DELIVERY_CODER=greeting ./run.sh
```

The greeting stand-in does not emit the architect's `ready_code` verdict, so in
greeting mode the engineer step converges only for an already-`code`+`ready`
issue; the full triage → code → PR → merge run needs the default
`BASIC_DELIVERY_CODER=smith`.

## Coding workspace binding

`config/workflow.json` declares **two** workspace external tools: the architect's
`triage_workspace` and the engineer's `coding_workspace` (there is **no**
`review_workspace` — no reviewer role). One bound command backs both: Temper
invokes it per role with the right checkout capability (read-only for the
architect, writable for the engineer) and the work-item context.

In the default the launcher **auto-binds `smith-coding-agent`** (built from this
checkout): it clones the configured repo into `run/coding-workspace/`, applies the
bundled `config/ci.yml`, and points `TEMPER_CODING_WORKSPACE_ROOT`/`COMMAND` at
the agent. `BASIC_DELIVERY_CODER` selects the auto-bound command (`smith` default,
`greeting` stand-in). The engineer's PR enters the `landing` queue **directly**
because `run.sh` sets `TEMPER_CODING_WORKSPACE_PR_LABELS=implementation` — there
is no reviewer to add a `landing` label.

To validate a different implementation path, bind your own coder and `run.sh`
respects it verbatim (and does not auto-bind):

```sh
export TEMPER_CODING_WORKSPACE_ROOT=/path/to/checkout
export TEMPER_CODING_WORKSPACE_COMMAND='your-coder --context "$TEMPER_CODING_WORKSPACE_CONTEXT"'
./run.sh
```

## CI

`config/ci.yml` is a host-mode workflow. `run.sh` commits it over the provisioned
commit-message-marker CI (which only the deterministic temper-testing fake worker
satisfies) before the workers start, so a real coder's ordinary-message PR head
clears the landing CI gate. It checks out the PR head and parse-checks every shell
script in the tree (`sh -n`); when the engineer leaves a non-shell diff there is
nothing to validate and the job passes through. A real project replaces this file
with its real CI (build, test, lint) and pairs the engineer with a coder whose
diffs pass it.

## Acceptance / validation

A converged run (default `BASIC_DELIVERY_CODER=smith`) ends with the seeded issue
`code`+`ready`, an implementation PR open, CI green, and the PR merged + `landed`
by the bot — with only `architect` + `engineer` + `mechanical` workers running and
no human action. `./run.sh validate-webhooks` confirms webhooks were registered,
accepted, delivered, consumed, and acted on. `./run.sh stop` / Ctrl-C tears
everything down cleanly; re-runs start fresh.

## Point it at your own Forgejo

Set `BASE_URL` to your instance and provide tokens, then drop the bundled
server/runner + provisioning steps — the same "swap to real" story as
reference-delivery. The workflow spec, the CI-only landing gate, and the
single-outcome triage are unchanged.
