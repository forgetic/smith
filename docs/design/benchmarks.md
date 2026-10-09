# Benchmarks

Provisional, 2026-10-09. How smith shows that a change has the effect it
claims and keeps it, and how smith compares with other agents. Tests are
testing.md's; the events smith is observed through are
protocol/events.md's; its commands, settings and exit codes are
shell.md's. What is still open is listed in section 15.

## 1. In one page

- **Tests specify; benchmarks show.** A behaviour is tested in the lowest
  tier that shows it. A benchmark runs the product against real
  providers to show that a change has its effect in the world, and keeps
  it. Benchmarks are never the merge gate.
- **One notion of measurement.** A task is a seed, a prompt and checks,
  run by an agent configuration some number of times. **Probes** force
  one behaviour and pass or fail; **fixtures** are small coding tasks,
  for trends and comparisons; **repository tasks** are real repositories
  at a frozen commit, run rarely.
- **Cheap by default.** Tasks name a model tier, not a model. Probes run
  on the small tier at low effort, cheapest first; comparisons and
  repository tasks run by choice.
- **A fix names its probe,** and its commit body records the probe
  failing before it and passing after it. A change to the runtime,
  limits, providers or host records a smoke run there.
- **Agents are adapters.** `Smith`, `Codex` and `ClaudeCode` are a closed
  set, each run with a pinned configuration and the user's existing login,
  never a copy of a refresh token, under a hard deadline on its whole
  process tree. smith is observed only through its event stream.
- **Unknown is never zero.** A measurement is observed, a lower bound, or
  unavailable with its reason. Tokens count once per response.
- **Statistics fit the question.** A probe reruns once on failure. A
  comparison interleaves its arms in one session and reports a median
  ratio with a bootstrap interval against the task's minimum detectable
  effect. Baselines are drift alarms. Raw results stay outside the tree.

## 2. What benchmarks are for

- **To show a change is effective,** and that it stays so while
  providers, models and other agents change under smith: a probe sees a
  provider's wire, routing and caching as they are.
- **To compare agents** on the same tasks, prompts and graders, so that a
  difference in outcome, time, tokens or cost is the agent's, not the
  harness's.
- **To find what the tests missed:** passive checks on every attempt hold
  smith's evidence to its contracts (section 11.2).

They are not tests (section 12) and not the merge gate: no check of
`docs/development/workflow.md` runs an agent. Nor are they a ranking: a
comparison states an effect with its interval, or that none was
detected.

## 3. What is measured

### 3.1 Tasks and kinds

A task (section 5.2) holds a prompt, byte-identical for every agent, a
seed, a model tier, budgets, checks and the design sections it guards.
An attempt is one run of a task by one agent configuration in a fresh
workspace, ending in one result (section 8). The kinds are a closed set:

| Kind | Judged by | Default design | Cost per attempt (estimate) |
|---|---|---|---|
| probe | every check passes | one attempt, rerun once on failure | seconds to a minute |
| fixture | the grader's pass rate, and metrics against another arm | interleaved arms, at least five attempts each | minutes |
| repository | the grader's pass, and large effects only | three attempts per agent | tens of minutes |

- **A probe forces one behaviour.** Its prompt leaves the model no choice
  ("with one `write` call", "delegate with `max_turns` 3"); its seed is
  tiny and its outcome exact. One that cannot be calibrated on the small
  tier at low effort names a higher effort and says why. Its outcome
  checks hold for any agent; its event checks hold smith to its
  contracts.
- **A probe is calibrated** before it joins a suite: ten passes of ten on
  the binary it guards, and a failure on the binary before the fix or,
  for a behaviour already in place, on a control arm. A probe that cannot
  fail guards nothing.
- **A fixture** has a hidden grader, run after the agent has exited.
  Every healthy agent passes it, so it tells agents apart by time, tokens
  and cost. **A repository task** is frozen with every dependency
  vendored, so that it builds offline for good and its grader may rely on
  that commit's interfaces; it shows long conversations, delegation,
  compaction and budgets at scale. Every grader fails on the seed and
  passes on the task's reference solution.

### 3.2 Checks and arms

- **Outcome checks** are agent-neutral, read once the agent's tree has
  ended: a file's content, digest, absence or pattern; protected files;
  writes outside the workspace and the attempt's private directories;
  the end (section 8.3) and, for smith, its exit code and the words on
  its standard error (shell.md, section 4); the grader's commands.
- **Event checks** read smith's event stream (protocol/events.md, section
  3): a closed set of named, typed checks with parameters, such as "one
  `write` call with `input_bytes` of at least N, `written`", each with an
  offline test.
- **Passive checks** run on every attempt (section 11.2); a task that
  expects a failure names those it waives. A failed check carries its
  evidence: the records or files it read.
- **An arm** is a binary, a configuration override or an agent. Two arms
  make an experiment: a **fix** (before and after), a **control** (the
  mechanism switched off), a **decision** left to measurement (section
  11.3), or a **comparison** (another agent, or main against a
  candidate). An arm built from a commit is built in the harness's own
  clone, outside every developer checkout, and named by the commit and
  the binary's SHA-256.

## 4. Tiers

### 4.1 When each runs

| Tier | Contents | When | Agents | Cost (estimate) |
|---|---|---|---|---|
| offline | parsers on recorded outputs, manifests, statistics | in the focused suite | none | under a second |
| smoke | a few probes and the passive checks, one attempt each | each change to the runtime, limits, providers or host, and each fix | smith on `codex`, small tier; `anthropic` too for a change to its path | a few minutes, under 100k tokens |
| probes | every probe and variant, both providers; outcome halves on other agents | weekly | smith; Codex; later Claude Code | about an hour, a few million tokens |
| compare | fixtures, interleaved | on demand | smith and the agents compared, working tier | one to two hours |
| repository | repository tasks | at milestones | smith and Codex, working tier | several hours |

Only the offline tier is a test (section 14). The others are runs of
`smith-bench`, by choice. Each tier is a suite file (section 5.3).

### 4.2 A fix and its probe

- **Every fix names the probe that guards it** in its commit body, with
  two runs the harness prints ready for it: **before,** on the fix's
  parent, failing; **after,** on the fix, passing.
- **A fix with no probe adds one,** calibrated, in the same change. A
  defect no probe can force, such as a race or a rare provider stall, is
  guarded by a passive check or an event check on the closest probe, and
  the commit says which.
- **A change to the runtime, limits, providers or host** records a smoke
  run in its commit body, with the binary it ran.
- **The probe stays** in its suites with its `guards`, so a later failure
  names the design sections at stake.
- **Choosing what to run.** The harness lists the tasks guarding the
  sections a change cites, cheapest first by their committed medians or
  estimates, within the suite's wall and token budgets, and reports what
  did not fit. A fix's probes run at least once before it and once after;
  repetition beyond that is the statistics' (section 9).

## 5. Layout and formats

### 5.1 The crate

```
benchmarks/
  Cargo.toml               smith-bench, a member of the workspace
  clippy.toml              the harness's own lints
  README.md                how to run it, and sign each agent in once
  src/                     the harness
  tests/                   the offline tier and its recorded outputs
  tasks/probes/<id>/       task.toml, seed/
  tasks/fixtures/<id>/     task.toml, seed/, grader/, reference/
  tasks/repository/<id>/   task.toml, seed/ (a manifest), grader/, reference/
  suites/<name>.toml       what a run runs
  agents/<agent>/          the pinned configurations
  agents/models.toml       the model tiers
  baselines/               per suite, agent configuration and tier
  summaries/               one per committed run
```

- **Ordinary Rust** under the test-harness rules (programming-model.md,
  section 10.2): its orders, samples and bootstrap draw from a seed it
  records, and it measures time on one monotonic clock.
- **Its own `clippy.toml`.** The root file's shell bans include
  `std::process` (shell.md, section 3.2), and the harness starts
  processes. Like `tests/clippy.toml`, its file keeps the bans that make
  a run reproduce from its seed and read one clock.
- **Modules by concern,** one per agent and smith's legacy face in its
  own (section 6.2). smith's events are read through `smith-events`, so a
  changed record fails the build or the offline tier, never a run.
- **Commands** run a suite or a task with its arms, calibrate a probe,
  record a fix's before and after, list the tasks guarding a section,
  summarise, write a baseline, sign an agent's home in, and check the
  tree offline. Their spelling is the README's.

### 5.2 Tasks

```toml
id = "large-write"
version = 1
kind = "probe"
guards = ["domain/tools.md, section 3", "protocol/limits.md, section 6"]
tier = "small"
deadline_seconds = 120
prompt = """
Create numbers.txt containing the integers 1 to 6000, one per line, \
with a single write call. Then finish.
"""

[[outcome]]
check = "file-digest"
path = "numbers.txt"
sha256 = "..."

[[event]]
check = "tool-call"
tool = "write"
input_bytes_at_least = 16384
verdict = "written"
```

| Field | What it holds |
|---|---|
| `id`, `version`, `kind`, `title` | `version` changes with the prompt, seed, grader or checks; results of different versions are never compared |
| `guards`, `behaviours` | design sections, cited as the design cites them; the design's names for what is guarded |
| `tier`, `effort` | the model tier; an effort only where calibration needs one |
| `prompt` | delivered whole, through a file or standard input |
| `deadline_seconds`, `estimate` | the hard deadline on the attempt's process tree; seconds and tokens, until measured |
| `[budget]` | turns, time and spend, given to each agent as far as it takes them (section 6.1) |
| `[smith]`, `[environment]` | settings overrides, passed as `-c` settings, never through the seed; variables the agent's commands see |
| `setup`, `protected` | commands that warm caches outside the timer, with the agent's own build profile; files that must not change |
| `[[outcome]]`, `[[event]]`, `[[grade]]`, `waives` | the checks, and the passive checks the task expects to fail |
| `repetitions` | where the kind's default does not fit, as for a probe judged on a median |
| `[[variant]]` | a named change to the prompt, overrides or checks, reported as a task of its own (`large-write/oversized`) |

- **`seed/`** is the workspace as the agent first sees it, committed in a
  fresh repository the agent's configuration trusts, its digest in the
  manifest. It holds no `.smith/` settings unless the task is about
  them, never `benchmarks/`, and no file of any grader or reference. A
  repository seed is the bundle's manifest (source commit, vendored pins,
  digest); the bundle lives in the harness's data directory.
- **`grader/`** holds hidden tests and their data. Each `[[grade]]`
  command runs after the agent's tree has ended, in a copy of the final
  workspace with the grader beside it, under a deadline, in its own
  process group, with bounded logs; its exit is its verdict.
  **`reference/`** is a solution, applied to the seed to validate it.

### 5.3 Suites

```toml
name = "smoke"
tier = "small"
design = "single"              # or "interleaved"
repetitions = 1
rerun_failed = 1
max_wall_seconds = 600
max_tokens = 100000
tasks = ["probes/first-request", "probes/budget/wrap-up"]

[[agents]]
agent = "smith"
provider = "codex"
config = "standard"
```

A suite names its tasks (`<kind>/<id>[/<variant>]`, or by kind and
behaviour), its agent configurations, a tier overriding the tasks' own,
its design, repetitions and budgets, and its arms where it compares. A
new schedule is a new file.

### 5.4 Agent configurations and results

- **`agents/<agent>/`** holds pinned configurations, each for a stated
  version of the agent, changed only by commit. "Vanilla" is the agent's
  defaults plus this file. A suite's agent configuration is an agent, a
  provider, a pinned configuration and a tier, and its digest is in
  every result's identity.
- **Raw results stay outside the tree,** under
  `$XDG_STATE_HOME/smith-bench/runs/<run>/<attempt>/`: the result, the
  agent's output and records, standard error's tail, the grader's logs,
  and the diff against the seed's commit, the agent's commits included.
  Workspaces are deleted once archived; anything raw is scanned for
  credentials before it leaves the machine.
- **Committed:** `summaries/`, one per run of the probes, compare and
  repository suites (per task and arm, counts by end, medians, ranges,
  comparisons, drift notices and audits), and `baselines/` (section 9).
  Smoke runs, and a fix's before and after, go in commit bodies.

## 6. Agents as adapters

`enum Agent { Smith, Codex, ClaudeCode }` is matched exhaustively, one
module per variant, in five steps. **Prepare** the attempt's private
directories and the agent's dedicated home, touching no one else's.
**Command:** an exact environment from an allow-list, nothing inherited,
and the prompt on standard input. **Observe** the output as it arrives,
as neutral observations: a response and its usage, a tool call, a child,
the answer, which ends task wall time and starts teardown. **Collect**,
once the tree has ended, what the agent persisted, as metrics with their
scope and convention. **Classify** one end (section 8.3). Claude Code is
built after Codex, and designed now so that it is one module and its
files.

| Step | Smith | Codex | Claude Code |
|---|---|---|---|
| prepare | `XDG_CONFIG_HOME` holds the pinned settings as the user's layer, capturing `calls` so the scope gate sees each call's input (`everything` where a task needs prompts); `XDG_STATE_HOME` the state; the account borrows the user's `~/.codex` login, read-only and access-token-only (shell.md, section 7). Before an arm's first attempt, `smith check --json` records the effective limits, schema versions and credentials; a nonzero exit refuses the arm as setup | the user's own `CODEX_HOME`, so its login and its refresh stay where they are; every key that shapes behaviour pinned by `-c` overrides: no MCP servers, the attempt root trusted, workspace edits allowed, no approvals, the model and effort; the user configuration's digest recorded with the attempt; its background service disabled where it can be, or ended with the tree | the user's own Claude login, the same way as Codex's; pinned settings and an empty MCP configuration passed on the command line, edits and commands allowed without asking |
| command | `smith exec --json -C <workspace> -m <endpoint>/<model> --effort <effort> -`, overrides as `-c KEY=VALUE` (shell.md, section 4.2); interactive, `smith --trace <file>` at a pseudo-terminal | `codex exec --json -m <model> -c model_reasoning_effort=<effort> -C <workspace> -o <final> -` | `claude -p --output-format stream-json --verbose --model <model> --effort <effort> --max-budget-usd <ceiling> --settings <pinned> --mcp-config <empty>`; `--input-format stream-json` for later messages |
| observe | the event stream, through `smith-events`; standard error is never parsed | JSON lines: thread, turns, items, and each turn's usage, of the root thread only, cached input within input | the opening record, assistant messages with usage, sub-agent messages with their parent's call, the result with durations, turns, cost and per-model usage, in the pinned version's recorded shapes |
| collect | nothing beyond the stream | the rollouts of the thread named: per-response usage and child threads; where a child is not linked to its parent, child scopes are unavailable and totals lower bounds | nothing; transcripts are archived |
| classify | the exit code (shell.md, section 4.3), held against `run.completed` | the exit, the last turn's completion or failure, the deadline | the result's subtype and error flag, the exit, the deadline |

- **One runner.** Each attempt runs on skein's world harness, end to end
  (skein's examples.md, section 6): the agent starts as it ships; a
  scripted person types where a probe is interactive; the attempt's
  referee reads the output as it arrives and keeps the hard deadline.
  The live tier runs smith the same way.
- **An adapter fails loudly.** A known record in an unknown shape ends the
  attempt as a harness error naming it. Unknown record types are skipped
  (protocol/events.md, section 6).

### 6.1 What each agent sees

- **Instructions.** A seed's are in `AGENTS.md`, and every agent gets the
  same text through the file it reads: Claude Code's `CLAUDE.md` is made
  at preparation and left out of the diff. smith's pinned settings add
  nothing. No home or directory above the attempt's root holds
  instructions.
- **The environment.** `HOME`, `TMPDIR` and the build caches are the
  attempt's; `PATH` and the toolchain's home are the machine's,
  read-only; then the locale and the agent's home. A task's variables
  reach smith's commands through its settings, which own their
  environment whole, and the others' through their own process.
- **Budgets.** smith takes turns, time and spend; Claude Code a spend
  ceiling; Codex none. In a comparison every agent's own limits sit above
  the task's hard deadline, the bound they share. Each result records the
  effective limits, and a budget end is labelled as one.

### 6.2 smith's legacy face

- **The first baseline is taken before events exist.** Until they land,
  the Smith adapter has a second face, in one module: it drives the
  binary before events as that binary takes its settings and task, reads
  its trace as that binary renders it, and maps it to the same results
  under the same deadline and settlement.
- **What that trace lacks is unavailable,** never estimated. A result
  names its face, and faces compare only on what both observe.
- **The module is deleted** in the change that takes the first baseline
  on events. Nothing else depends on it. Results from before this
  harness are never a baseline.

## 7. Models as tiers

| Tier | `codex` | `anthropic` | Used by |
|---|---|---|---|
| `small` | `gpt-6-luna`, low effort | `claude-haiku-5-5`, low effort | probes, smoke, the live tier |
| `working` | the user's working model and effort | the same | fixtures, repository tasks |

- **`agents/models.toml`** holds this mapping, pinned: the working tier
  changes by commit when the user's working model does, so that a
  comparison never mixes models. A new tier is an entry; a new model for
  a tier starts new baselines.
- **Providers are a closed set,** named as smith's presets (shell.md,
  section 6.2). smith runs on both, Codex on `codex`, Claude Code on
  `anthropic`. Each result records the model and effort asked for and
  those the agent reports; a mismatch is flagged.

## 8. The result schema

### 8.1 One result per attempt

A JSON document. Like every format it is pre-release: one version, any
change a reader would notice is a new one, and a reader refuses others.
Committed summaries and baselines of an older version are regenerated or
dropped, never read across versions.

| Part | What it holds |
|---|---|
| identity | run, attempt and arm; the task's id, version, seed and prompt digests; the agent, its version, its binary's SHA-256 and, for smith, its face; the configuration's digest; the provider; model and effort, requested and resolved; the harness's commit; the host and CPUs used; the start, in wall time |
| timing | on the harness's one clock: process wall, to the tree's end; task wall, to the answer observed; teardown, from the answer to the tree's end; grading wall. The agent's own times beside them: smith's `first_byte_ms`, `largest_gap_ms` and longest completion, and its `t_ms`, from its own start |
| tokens | per scope (section 8.2) |
| counts | responses, provider attempts and retries; completions; tool calls by name, not comparable across agents; tool failures by verdict; children; conversations and compactions; messages by terminal |
| resources | CPU time and peak memory, each with its scope: the tree's accounting, the agent's process alone, or unavailable |
| spend | the agent's own figure, such as smith's `spent` or Claude Code's cost, with its basis: notional, priced or reported |
| outcome | the end; the exit, a code or a signal; whether the harness forced it (no, terminated, killed); each check's verdict and evidence; protected files changed; writes outside the workspace |
| health | smith's loss per sink; headroom against each limit, as the fraction of its bound used; the passive checks |
| artifacts | each raw file's path and digest |

Every measurement is a `Measure`: observed, a lower bound, or unavailable
with its reason, never written as zero and never filled from another
scope.

### 8.2 Tokens

One normalised record per scope: **fresh** (input not read from a cache,
cache writes included), **cache read**, **cache write**, **output**
(reasoning included) and **reasoning**, each where reported. Each
convention is a tag on the record, with a fixed mapping:

| Convention | Fresh is | Not reported apart |
|---|---|---|
| Codex | input less cached input | reasoning, within output; cache writes |
| Claude Code | input plus cache writes | thinking, within output |
| smith's events (protocol/events.md, section 3.10) | `input_tokens` plus `cache_write_tokens` | whatever is null |

- **Once per response.** A response's usage counts once however many
  records repeat it: smith's by its `response`, the others' by message
  id. A retried attempt that was billed is a response of its own.
- **Scopes:** the root conversation; each child, under its parent;
  compactions; helper models; the total. A total is observed only when
  every scope is; otherwise it is a lower bound naming what is missing,
  and unavailable when the root is.

### 8.3 Ends

| End | Meaning | smith's exit (shell.md, section 4.3) |
|---|---|---|
| `Completed` | answered and exited by itself | 0 |
| `Failed(reason)` | ended without an answer: the provider, the model, a tool, input needed, or another typed reason | 1; 5 for input needed |
| `Budget(which)` | stopped on its own limit: turns, time or spend | 4 |
| `Refused(setup)` | never reached inference: the seed, the warm-up, the configuration or the credentials | 2; 3 for credentials |
| `Timeout` | the harness's hard deadline ended the tree | 130 after the harness's signal, or killed |
| `HarnessError(what)` | the harness failed: an unknown persisted shape, a grader that crashed | |

A budget end is not a failure, and a setup refusal is not the agent's:
rates leave both out and report them beside. The grade is apart: a
completed attempt can fail its grader. A forced exit after an accepted
answer is a warning on a completed attempt.

## 9. Statistics

- **Probes.** A probe passes when every check passes, and reruns once on
  failure. Two failures of two are a regression; calibration makes one
  unlikely to be chance. One of two quarantines the probe: reported, and
  gating nothing until it passes calibration again.
- **Comparisons interleave in one session.** Each block holds one attempt
  of each arm, in an order drawn from the run's seed, so that drift,
  warming caches and shared rate limits fall on every arm alike. At
  least five attempts per arm: three can never reach significance.
- **A difference is reported** per task and metric when the 95%
  bootstrap interval of the ratio of the arms' medians, resampled with
  the run's seed, excludes 1, and the effect exceeds the task's minimum
  detectable effect: about 2.8 · CV · √(2/n) for n attempts per arm, from
  its committed variation. Otherwise the summary says none was detected,
  not that the arms are equal.
- **Every attempt counts.** Pass rates are counts with exact intervals; a
  median over successes alone is never reported without the rest. Only
  outcome, wall time, tokens by convention and spend on one basis are
  compared across agents.
- **Baselines** are committed summaries of a suite for one agent
  configuration and tier: each task's medians, each metric's coefficient
  of variation, n and minimum detectable effects, with the binary and the
  date. A run outside one by more than a task's minimum detectable effect
  raises a drift notice, which asks for a comparison in one session and
  is never a verdict. A baseline is retaken by commit after an intended
  change, a new model, a new task version or a new result version.
- **Audits are sampled.** A summary draws a few attempts with its seed,
  plus every attempt whose checks disagree with its grader, for a person
  or a model to audit: did the agent do the task asked, without reading
  the grader or changing what it must not. The scope gate is automatic:
  protected files unchanged, nothing written outside the workspace, no
  read of the benchmark tree in what the agent shows of its calls.

## 10. Isolation

- **Credentials are the user's existing logins,** never a copy of a
  refresh token: issuers rotate them, and a copy logs the other holder
  out. smith borrows `~/.codex` read-only and takes its access token only
  (domain/host.md, section 7). Codex runs in the user's own `CODEX_HOME`,
  so its login and refresh stay in one place, and Claude Code later
  likewise with its own. A login that has lapsed is a setup refusal
  naming the CLI to run once. Nothing in the harness writes to a user's
  tool directories.
- **Configuration is pinned, not inherited.** An agent sharing the user's
  home gets every key that shapes its behaviour from the command line
  (MCP servers, trust, approvals, model, effort), and the digest of the
  user's own configuration is recorded with each attempt, so a change
  there is visible. smith's settings come only from the harness's layer
  (shell.md, section 5).
- **Out of sight, nothing shared.** Attempts run under the harness's
  scratch root, outside every repository and the benchmark tree, each
  with its own temporary directory, build caches and target directory,
  warmed outside the timer and copied by reflink where the filesystem
  allows. A grader reaches the workspace's copy only after the tree has
  ended. Until contained trees exist nothing keeps an agent from the
  user's files; the scope gate looks for it.
- **Process trees.** An attempt ends when its whole tree has ended. At
  the hard deadline the tree is terminated, then killed after a grace;
  after an observed answer, the deadline is the agent's teardown bound
  plus that grace. Accounting of a whole tree (a cgroup scope, its CPU
  and peak memory) is skein's to add first; until then each attempt runs
  in its own process group, and its resources carry the narrower scope.
- **Setup is not the agent's.** A seed digest that differs, a failed
  warm-up, a refused configuration or missing credentials end the attempt
  as `Refused(setup)`, and mark the suite incomplete.
- **Load and disk.** A grader sensitive to load runs serially on an idle
  machine. Raw output is bounded per attempt, and a suite checks free
  space against its estimate before it starts.

## 11. The initial catalogue

Costs are estimates on the small tier until measured.

### 11.1 Probes

Every probe's outcome is its file, or the answer it asks for. The outcome
halves of `smoke-tools` and `large-write` also run on Codex and Claude
Code.

| Probe | Guards | Forces | Checks | Cost |
|---|---|---|---|---|
| `first-request` | a start's messages, the opening, awaiting (domain/run.md, sections 3 and 6) | an empty brief and a one-line task that writes a file; headless with the task in the start, and interactive typed after startup; with and without a workspace | under `everything`, the first request's new messages hold the task; no response or `wait` before the task's `message.received`; at most two completions; time to the first `response.started` | seconds |
| `smoke-tools` | the commands' environment; one path namespace, the directory as `.`; a complete stream; teardown; exit evidence (domain/tools.md, sections 2 and 5; protocol/events.md, section 5) | list `.`, read the README, `printenv` a nonce variable and `command -v cargo` into a file through the shell, answer with a README token. `child`: one child first, so two pooled connections are open at the answer | `.` listed; one shell call, exit 0; tool records paired by `call`; the responses' `spent` and `usage` sum to `run.completed`'s; nothing lost; `teardown_ms` within bound, not forced | ten seconds, ten thousand tokens |
| `follow-up` | message terminals, the fence, the unread report, a finish crossing a message, deferred `wait` (domain/run.md, section 6); the local host's unread lines (domain/host.md, section 8) | interactive: a greeting, then, once `finish` shows, a file asked for through the shell. `eof`: two lines, then end of input. `chat`: three exchanges with wait authority | exit 0; each `message.received` has one terminal; the request handled once, in the run it crossed or the next one's opening; no waiting prompt before the first task; in `chat`, one completion per exchange | thirty seconds |
| `large-write` | the declared tool payload, per-call outcomes, each model's `output`, the truncated Anthropic `tool_use` (domain/tools.md, section 3; protocol/limits.md, section 6) | one `write` of tens of kilobytes within the payload. `oversized`: above it | one `write` of at least the size asked, `written`; no `max_tokens` stop. `oversized`: one `oversized` call, then the file in pieces, accepted | under a minute, ten thousand output tokens |
| `history` | the window, compaction, derived limits, selective decoding (domain/session.md, section 8; protocol/limits.md, section 3) | sum values held in many small files, read one call at a time. `small-window`: a window several times smaller than the reading | `request_bytes` and prompt tokens per request recorded. `small-window`: a `conversation.opened` of kind `compaction`; no prompt above the window; cache reuse around it recorded | two minutes, up to a million input tokens; weekly |
| `cache-reuse` | affinity, the stable prefix, Anthropic breakpoints, usage (protocol/llm.md, section 2) | pointer chasing over a dozen files, one read each, a fixed number of requests. `children`: two children in turn. `crossing`: a message part way, at medium effort, so reasoning crosses a user turn. `idle-gap`: a gap past the cache's lifetime before a message | reuse R, the cache read over the previous request's prompt, summed where the predecessor can be cached: median of three at least 0.85, and full misses (under half) at most one per ten requests, until calibration sets them. On `anthropic`, cache writes about each turn's addition. A control arm without affinity falls clearly below | a minute each; `idle-gap` weekly |
| `budget` | the reserve, wind-down, the final turn, budget text, the clamped share, real spend (domain/run.md, sections 5.3 and 9) | `starvation`: a tight budget and a delegated survey. `wrap-up`: a child given `max_turns` 3. `child-quota`: a child given `max_turns` 2. `visibility`: a report of what is left. `spend`: notional prices, a ceiling about twice a calibrated spend | `starvation`: accepted, main's last completion after the child's return, turns within budget. `wrap-up`: under `everything`, `stop: budget` and text, the last request at `tool_choice` `none`. `child-quota`: the child answers within two turns, its `share` in `conversation.opened`. `visibility`: matches the last `budget` record; no child opens into the reserve. `spend`: never past the ceiling | under a minute each |
| `parallel-read` | calls per response (protocol/limits.md, section 2.2) | twenty files read in one step | the reads in one response's calls; no `limit` failure; no failed send | thirty seconds |
| `parallel-children` | concurrent conversations, a pool at capacity waits, the write lease (domain/run.md, section 5.3; protocol/limits.md, section 2.2) | four read-only children in parallel, one directory each. `reviews`: three independent reviews | two children overlapping in time; no call refused by the pool | a minute |
| `failures` | typed failures in words, headless exit codes, limit failures and HTTP status, clipped provider text (domain/host.md, section 8; shell.md, section 4.3) | `unknown-model`; `derived-limit`, by a test profile; `one-turn`, one turn for a two-turn task; `bad-setting`, a malformed setting | the exit code of the class; standard error naming the class and the clipped message, the limit and bound, or the setting; `run.completed` agreeing; `bad-setting` refused before any request | seconds |
| `paths` | one path namespace (domain/tools.md, section 2) | `app` writable and `spec` read-only: read both, answer in `app`. `single`: one directory, its name never needed | no `missing` verdict or path refused as outside; at most two completions before `finish` | seconds |
| `tool-output` | output budgets, read windows and timeouts as the model chooses them; UTF-8 passed through (domain/tools.md, sections 3 and 5) | `test-output`: which assertion fails, in over ten kilobytes of output. `quote`: a sentence deep in a long document. `timeout`: a command past its timeout, and one asking past the ceiling. `utf-8`: replace an en dash | one shell call; at most two reads; `timed_out` naming its bound, `clamped` set, no hang; one edit, matching first time | seconds each |
| `process-tree` | a command in its own process group, signalled whole (domain/tools.md, section 5) | `sh -c 'sleep 600 & wait'` with a short timeout | no `sleep` left, by tree accounting; answered within timeout and grace; accepted | seconds |
| `credentials` | the `borrow` source (domain/host.md, section 7) | an account borrowing the user's Codex login | accepted; smith never writes the login file (its digest unchanged by smith's run) | seconds |
| `slow-completion` | deadlines that measure progress; an oversized reasoning item as a typed failure (protocol/limits.md, section 7) | long reasoning at high effort | no timeout while events arrive; no reasoning-size failure; `first_byte_ms` and `largest_gap_ms`, per provider and model, for shell.md's `head` and `idle` | minutes; occasional |

Later, as their behaviour is built: **`mid-work`**, a message offered
between completions of a long job, and threshold notes the same way;
**`build-environment`**, a seed with an ambient build wrapper that builds
and tests, once trees declare build profiles.

### 11.2 Passive checks

On every smith attempt:

- one `session.started`, one `run.completed` per run, one
  `session.ended`, and the exit agreeing with the answer;
- no authoritative record lost, each sink's `loss` counted, none under
  `complete`;
- `teardown_ms` within the agent's bound and not forced; a forced exit
  after an accepted answer is a warning;
- every `response.started` and `tool.started` completed, a tool's under
  its `call`; every `conversation.opened` closed; every
  `message.received` with exactly one terminal;
- the responses' `spent` summing to the run's; no failure of class
  `limit` the task does not expect;
- headroom, with a warning at half: `request_bytes`; prompt tokens
  against the window; `first_byte_ms` against `head`; `largest_gap_ms`
  against `idle`; turns; spend; `peak_rss_bytes` against `memory`.

On every attempt of any agent: the tree ended; protected files
unchanged; nothing written outside the workspace and the private
directories; no read of the benchmark tree seen.

### 11.3 Experiments

| Experiment | Over | Arms |
|---|---|---|
| `affinity` | `cache-reuse` | the cache key and session header; plus the thread header; plus private routing state and reasoning context, adopted only on a material gain |
| `cache-lifetime` | `cache-reuse/idle-gap` | Anthropic's default lifetime, and a longer one |
| `compaction` | `history/small-window` | the threshold, and what the new window holds |
| `edit-format` | `03-planner`: "add a doc comment to each public function and run the tests" | each dialect's formats, by completions before the first edit and edits that fail |

### 11.4 Fixtures and the repository task

| Task | Asks for | Grader | Runs |
|---|---|---|---|
| `01-backoff` | overflow-safe delay and deadline arithmetic in a small crate | hidden edge-case and regression tests; formatting | by default |
| `02-line-decoder` | an incremental line decoder with a bounded buffer | hidden tests; fixed-seed comparisons with an oracle | by default |
| `03-planner` | a closure planner with ordered topological output and error priority, across files | hidden tests; an oracle over generated graphs | occasionally |
| `shell-cwd` | a working-directory option for the shell tool, through protocol, domain, machine and documentation, on smith at a frozen commit with its skein pin vendored | an oracle crate over that commit's interfaces; formatting, lints, both suites within budget, serially on an idle machine | at milestones |

A fixture takes minutes per attempt at the working tier; `shell-cwd`
tens of minutes and millions of tokens (estimates). Where a correct
solution could fail a grader, or a wrong one pass it, the grader is
strengthened first. Per change, `budget` and `history` stand in for the
repository task's delegation and length.

### 11.5 Suites

| Suite | Tasks | Agents | Repetitions |
|---|---|---|---|
| `smoke` | `first-request`, `smoke-tools`, `large-write`, `budget/child-quota`, `budget/wrap-up`, `failures/one-turn` | smith on `codex`, small | one, rerun on failure |
| `probes` | every probe and variant | smith on both providers; Codex, later Claude Code, on outcome halves | one; three where judged on a median |
| `compare` | `01-backoff`, `02-line-decoder`; `03-planner` when asked | smith and Codex, later Claude Code, working | at least five per arm, interleaved |
| `repository` | `shell-cwd` | smith and Codex, working | three, outcomes and large effects only |
| `calibrate` | one probe | smith, the probe's tier | ten on the fix; then the binary before, or a control arm |

## 12. How it relates

- **To smith's tests.** Benchmarks find and show; tests specify and gate.
  A benchmark does not replay and its outcome varies, so what it finds is
  reproduced in the lowest tier that shows it (testing-strategy.md,
  section 9) and fixed there with that test; the probe stays as evidence
  of the effect. Benchmarks never stand in for a world's story.
- **To the live tier** (testing.md, section 2.3). The live tier checks
  once, by outcome, that each wire, issuer refresh and push is right.
  Benchmarks repeat, measure and compare. Both read smith through
  `smith-events`, take the user's existing logins under one credential
  guard, and default to
  the small tier.
- **To skein's test kit.** The harness is not a world: no schedule,
  replay, counting allocator or referee plumbing of its own. What it
  needs that is generic to processes, such as a tree's settlement and
  resource accounting, lands in skein first, smith-bench its first user.
- **To the product.** It runs smith as a person would (shell.md, section
  4); a gap it meets there is the product's to close, never the
  harness's to work around.

## 13. Adding to it

| To add | Where |
|---|---|
| an agent | a variant of `Agent` and its module; its pinned configurations; its recorded outputs and their offline tests |
| a provider | a variant; its token convention; an entry per tier in `agents/models.toml` |
| a task | a directory under its kind, calibrated if a probe, validated if graded |
| a task kind | a variant with its judgement and defaults, and a directory under `tasks/` |
| a check | a variant of the outcome or event checks, with its offline test |
| a metric | a `Measure` in the result, with its scope, and a new result version |
| a tier or a schedule | an entry in `agents/models.toml`; a suite file |

## 14. The world

The offline tier runs in the focused suite, with no agent and no network:

- **Parsers on recorded outputs,** per agent and version: smith's are the
  event codec's goldens (protocol/events.md, section 8); Codex's are JSON
  lines and rollouts with root-only usage, an unlinked child and repeated
  usage; Claude Code's are streams with repeated message ids, sub-agent
  messages and helper models. Unknown records are skipped, a known record
  in a new shape is a harness error, usage counts once per response, and
  unavailable is never zero.
- **Classification:** one recorded end of each class per agent, and each
  of smith's exit codes; the legacy reader on a recorded trace, until its
  module goes.
- **Manifests:** every task, suite and configuration parses; seed digests
  match; prompts are whole; every check is known; every guarded section
  exists in the design; no seed holds `benchmarks/` or a grader's or
  reference's file; suites name existing tasks and agents.
- **Statistics:** the bootstrap, the interleaving and the audit sample
  reproduce from their seeds; the rerun rule; the minimum detectable
  effect.
- **An attempt against a replaying agent:** a small program that prints a
  recorded stream, leaves a grandchild behind, or never ends, observed,
  ended at the deadline and classified.
- **Fuzzy:** parsers fed truncated and corrupted lines never panic and
  never count a response twice.

Each runs in milliseconds, within the focused and fuzzy budgets.

## 15. Open questions

- **Codex's rollouts:** whether they link child threads to their
  parents, and the configuration key that trusts the attempt root.
- **Claude Code:** the settings that allow edits and commands headless;
  whether `CLAUDE_CONFIG_DIR` keeps every user file out;
  `claude-haiku-5-5` at low effort on the subscription route.
- **Tree accounting:** cgroup v2 delegated to the user on the development
  machine, as containment also needs (testing.md, section 8).
- **Instruction parity:** whether smith reads a seed's `AGENTS.md` as
  Codex does, or the harness gives it the same text another way.
- **The shared credential guard:** which crate the live tier and
  smith-bench both take it from.
- **Repository bundles:** where a bundle is kept durably, so that another
  machine can run the task.
- **Concurrency:** attempts run serially; parallel ones would need
  disjoint CPUs.
- **Task kinds beyond code,** such as chat, research or review, with
  graders of their own; and **audits:** how many attempts a run samples,
  and who audits them.
- **Recording smoke runs:** whether a commit hook checks that a runtime
  change's body carries one.
