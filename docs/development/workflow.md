# Development workflow

How work reaches main. Work happens on local branches; main moves only by
merging a branch that passed every check below, or that changes only
documentation.

For the executable's local mode, see [local-host settings and commands](local-host.md).
The root package builds `src/main.rs`; its end-to-end and live targets live
under `tests/end-to-end/`. Bare cargo commands include every workspace member.
Run the local command with `cargo run -- local SETTINGS STATE [WS]`, or install
the binary with `cargo install --path .`.

## 1. Before merging to main

On the branch's tip, rebased on main, so that what is checked is what main
becomes:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo nextest run --workspace --profile fuzzy
```

Then, once all four pass:

```sh
git checkout main
git merge --ff-only <branch>
```

A branch that changes only Markdown files (the documents under `docs/`,
`AGENTS.md`) skips the four checks, since nothing it touches is built or
tested, and is merged the same way. Doc comments in `.rs` files are not
documentation in this sense: they are code, and clippy checks them.

## 2. The two test suites

The suites are those of skein's `docs/foundation/testing-strategy.md`
(section 8). smith's domains and their world stories are specified in
`docs/design/domain/`.

- **The default suite** is every crate's unit tests and each world's
  focused tests (`tests/<world>/tests/*.rs`): focused tests of
  expected behaviour (scenarios, referee tests, replay, facts changing
  nothing, memory against the worst case), and a cheap random world as a
  smoke test where one helps. It is what `cargo nextest run --workspace`
  runs, and it takes at most **15 seconds**.
- **The fuzzy suite** is the worlds' `tests/fuzzy_*.rs`: randomized tests,
  such as sweeps of random worlds and domains driven at random, over many
  seeds. It is not run by default, only with `--profile fuzzy`, as the
  gate before merging to main, and it takes at most **1 minute**.

The budgets are enforced, not advisory: `.config/nextest.toml` gives each
profile a `global-timeout`, so a suite that runs past its budget fails,
naming the tests it stopped, and a slow period past which a test is
flagged SLOW as it runs. A change that breaks a budget is fixed by making
tests cheaper, or by moving randomized ones to a world's fuzzy tests.
Raising a budget is a decision to take explicitly, not a fix. To see what
each test costs on its own, the `measure` profile has no budget:

```sh
cargo nextest run --workspace --profile measure -j 1
cargo nextest run --workspace --profile measure -j 1 --ignore-default-filter -E 'binary(/^fuzzy_/)'
```

The budgets are wall time on the development machine (4 cores, 8 threads,
which nextest uses all of), with nothing else building. Run the checks on
an idle machine.

## 3. When a fuzzy seed fails

A failure names its seed, and a seed replays to the same run. Fix the bug,
then keep its seed: as a scenario among its world's focused tests if it
shows behaviour worth naming, or among the sweep's pinned seeds. A finding
that cannot be fixed yet joins its world's `FINDINGS`, which an ignored
test replays until it is fixed.

## 4. Workspace foundation

05s1 establishes the workspace conventions and reuses
`skein_world::domain` directly; smith's worlds consume the kit, and its
generic boundary is tested in skein.
05s2 adds the copied agent domains and provider/OAuth codecs, their
component worlds and a composed typed scripted host world. Its named-source
mapping and preserved coverage are recorded in
[migration-05s2.md](migration-05s2.md). Protocol integration and binaries
remain later increments; their stories and limits belong to their design
documents.

Before adding a world, record its focused and fuzzy serial measurements
here, on an idle machine, and keep the workspace suites within their
existing budgets. Do not raise a timeout to accommodate a new world.

05s2 was measured serially on 2026-10-05 at source `762c858`, with the
canonical skein lock at `5e52dd9`. The commands in section 2 ran the whole
workspace with `-j 1`: 351 focused tests passed in 4.446 seconds, and eight
fuzzy tests passed in 6.278 seconds, with no skips. The world shares below
sum nextest's reported individual test durations, rounded to milliseconds;
the suite summaries also include test process and runner overhead.

| World | Focused tests / serial seconds | Fuzzy tests / serial seconds |
| --- | ---: | ---: |
| Agent on scripted host | 22 / 0.213 | 2 / 2.608 |
| Run | 28 / 0.523 | 1 / 0.311 |
| Session | 44 / 2.259 | 2 / 0.664 |
| Tools | 34 / 0.667 | 2 / 2.688 |
| Fake LLM byte peer | 4 / 0.014 | 0 / 0.000 |

The copied production crates contributed 217 focused tests to those
measurements. These measurements retain the copied seed sweeps and memory
drivers; the parallel default and fuzzy gate still enforce the original
15-second and 60-second workspace ceilings.

05s4 RESULTS was measured serially on 2026-10-05 at source `e4c8d0e`,
using the same commands on an idle machine and an isolated build directory.
All 368 focused tests passed in 4.359 seconds and all eight fuzzy tests
passed in 6.150 seconds, with no skips. The affected agent world has
26 focused tests / 0.267 seconds and two fuzzy tests / 2.635 seconds;
the run world has 30 focused tests / 0.537 seconds and one fuzzy test /
0.273 seconds. These world shares sum the rounded individual PASS
durations, rather than the suite elapsed time. Existing seed sweeps and
memory drivers remain; no timeout or budget was increased.

05s4 DELIVERY was measured serially on 2026-10-05, from `6dfff6a` plus
independently reviewed terminal, fixture and documentation corrections.
The commands in section 2 passed 394 focused tests in 4.429 seconds and
nine fuzzy tests in 6.229 seconds, with no skips. The affected agent world
has 40 focused tests / 0.269 seconds and three fuzzy tests / 2.634 seconds;
the run world has 32 focused tests / 0.549 seconds and one fuzzy test /
0.264 seconds. Shares sum rounded individual PASS durations. The additional
mid-delivery schedule sweep retains all five actual host outcomes; the
existing memory and replay sweeps remain. Workspace budgets are unchanged.

05s4 HOST TOOLS was measured serially on 2026-10-05 from `2a621a5`
plus the independently reviewed host-tools increment. The commands in section 2
passed 411 focused tests in 4.764 seconds and nine fuzzy tests in 15.212
seconds, with no skips. The affected agent world has 49 focused tests /
0.324 seconds and three fuzzy tests / 6.508 seconds; the run world has
33 focused tests / 0.566 seconds and one fuzzy test / 0.314 seconds.
Shares sum rounded individual PASS durations. Full opaque inputs and replies,
settled recovery, actual shutdown and the existing memory/replay sweeps remain.
Workspace budgets are unchanged. Source `ab15cae` passed the exact-tip four
gates before merging: 411 focused / 1.786 seconds and nine fuzzy / 3.111
seconds, without skips; formatting and all-target clippy also passed.

05s6 HOST was measured serially on 2026-10-05 from integration base
`9b9b7c6` plus the independently reviewed typed host extraction. Section 2's
commands passed 459 focused tests in 5.001 seconds and ten fuzzy tests in
6.569 seconds, with no skips. The new host world contributes 45 focused
tests / 0.145 seconds and one fuzzy test / 0.018 seconds; the host domain
contributes three focused tests / 0.010 seconds. Shares sum rounded individual
PASS durations. Four host memory drivers retain maximum starts, full queued
messages/replies, caller-owned Start coexistence and sealed proof replacement.
The 240-seed V2 sweep asserts actual ending classes and settlement, preserving
mapped source behavior and new typed ownership controls. Budgets are unchanged.
Source `eb46ecc` passed the exact-tip four gates before merging: 459 focused /
1.606 seconds and ten fuzzy / 2.593 seconds, without skips; formatting and
workspace all-target clippy also passed.

05s4 SESSION CONTRACTION was measured serially on 2026-10-06 from temporary
parent `9930191` plus this increment's frozen source. Section 2's commands
passed 567 focused tests in 9.064 seconds and eleven fuzzy tests in 8.153
seconds, with no skips. The affected Session world contributes 66 focused
tests / 3.308 seconds and two fuzzy tests / 0.769 seconds; Session's unit
crate contributes 59 focused tests / 0.240 seconds. Shares sum rounded PASS
durations. Original scheduled seeds and fault classes, 3 x 4 payload-pressure
memory cases and new exact/full/one-short receiving controls remain. The
parallel gate passed 567 focused / 3.138 seconds and eleven fuzzy / 3.917
seconds; formatting and all-target Clippy passed. This is temporary-checkout
validation; original-main acceptance still requires reconciliation and gates.

05s6 FINAL ACCOUNTING was measured serially on 2026-10-06 from temporary
parent `2f324b0` plus the frozen increment. Section 2's commands passed
572 focused tests in 9.474 seconds and eleven fuzzy tests in 8.150 seconds,
with no skips. The affected host world contributes 52 focused tests /
0.170 seconds and one fuzzy test / 0.018 seconds; the host domain contributes
four focused tests / 0.027 seconds. The agent world contributes 95 focused
tests / 3.204 seconds and four fuzzy tests / 4.548 seconds. Shares sum rounded
PASS durations before the suite summary. Existing payload maxima, four host
memory drivers and the 240-seed host sweep remain. The parallel gate passed
572 focused / 3.866 seconds and eleven fuzzy / 4.064 seconds; formatting and
all-target Clippy passed. This validates the temporary checkout; original-main
acceptance and shared SDK gates remain open.

05s6 MESSAGE REFUSALS was measured serially on 2026-10-06 from temporary
parent `1b62e06` plus the frozen increment. Section 2's commands passed
589 focused tests in 9.217 seconds and eleven fuzzy tests in 8.163 seconds,
with no skips. The host world contributes 66 focused tests / 0.207 seconds
and one fuzzy test / 0.019 seconds; the host domain contributes four focused
tests / 0.017 seconds. The agent world contributes 98 focused tests /
3.218 seconds and four fuzzy tests / 4.559 seconds. Shares sum rounded PASS
durations before the first suite summary. Original 240 host seeds, all eleven
endings, four host memory drivers, payload maxima and existing agent stories
remain; seventeen focused controls supplement them. The parallel gate passed
589 focused / 3.262 seconds and eleven fuzzy / 3.804 seconds; formatting and
all-target Clippy passed. This validates the temporary checkout; original-main
acceptance and shared SDK gates remain open.

05s4 CALL ACTIVATION was measured serially on 2026-10-06 with the `measure`
commands in section 2 on an idle machine: 591 focused tests passed in
9.488 seconds and eleven fuzzy tests passed in 9.102 seconds, with no skips.
The run world adds the answered-host-call crash story, and the host world adds
the wrong-activation rule control. The default 15-second and fuzzy 60-second
budgets are unchanged.

The domain/protocol world separation was measured serially on 2026-10-06
with the `measure` commands in section 2: 594 focused tests passed in
9.395 seconds and eleven fuzzy tests in 8.543 seconds, with no skips. The
new protocol LLM world contributes 28 focused tests in 2.051 seconds and
no fuzzy tests. It owns the moved wire fixture and tests; the agent world
uses only the typed fake LLM, including three domain feature controls.
Both suite budgets are unchanged.

The shared fake LLM cleanup was measured serially on 2026-10-06 with the
section 2 `measure` commands. Before removal, 583 focused tests passed in
8.786 seconds and eleven fuzzy tests passed in 8.458 seconds. After removal,
538 focused tests passed in 8.862 seconds and eleven fuzzy tests passed in
8.557 seconds, with no skips. The 45 removed tests took 0.201 seconds in
the before run; suite wall times varied upward by 0.076 and 0.099 seconds.
Skein owns the provider and fake LLM tests and recorded exchanges; Temper
retains the OAuth codec tests until the shared OAuth client is built.
The workspace budgets are unchanged.

The local host's first scripted chat was measured serially on 2026-10-06
with the `measure` profile and `-j 1`: the new local world has one focused
story, passing in 0.004 seconds, and no fuzzy test yet. It runs the local
domain and in-process agent against Skein's fake provider. The default and
fuzzy workspace budgets are unchanged.

Local transcript and crash-cut stories were measured serially on 2026-10-06
with `measure -j 1`: six focused local-world tests passed in 0.037 seconds,
including resumed invocation, activation and turn crash cuts, and refusal
of incompatible saved history. There are still no local fuzzy tests. The
workspace timeouts remain unchanged.

Local credential, cancellation and store-pressure stories were measured
serially on 2026-10-06 with `measure -j 1`: ten focused local-world tests
passed in 0.044 seconds. There are no local fuzzy tests yet. The parallel
workspace gate passed 557 focused tests in 3.526 seconds and eleven fuzzy
tests in 4.638 seconds. The suite budgets are unchanged.

The local referee, replay, fact-independence, memory and randomized schedules
were measured serially on 2026-10-06 with `measure -j 1`: 21 focused
local-world tests passed in 0.087 seconds, and the 64-seed fuzzy sweep passed
in 0.033 seconds. Store failures at load, metadata save and turn save are
covered. The world remains within its one-second focused and five-second
fuzzy shares; workspace timeouts are unchanged.

The local workspace and delivery stories were measured serially on
2026-10-06 with `measure -j 1`: 34 focused local-world tests passed in
0.288 seconds. The eight delivery stories cover git commits, plain files,
no changes, markers, a later directory failure, repeated names, a crash
after recording a commit, and cancellation during delivery. The world's
64-seed fuzzy sweep remains its one fuzzy test. The one-second focused and
five-second fuzzy world shares and workspace budgets are unchanged.

Configured local pushes were measured serially on 2026-10-06 with
`measure -j 1`: 36 focused local-world tests passed in 0.166 seconds.
The two added stories cover a push that lands and a remote branch that
moved before the push. The existing one-test, 64-seed local fuzzy sweep
remains. The workspace and local-world budgets are unchanged.

The protocol codec world was measured serially on 2026-10-07 with
`measure -j 1`: eleven focused tests passed in 0.112 seconds and one fuzzy
decoder test in 0.061 seconds. The focused tests cover generated goldens,
drift, version and kind tables, and bounded charter and turn decoding. The
fuzzy test drives all 20 top-level records with arbitrary and mutated bytes.
The world's one-second focused and five-second fuzzy shares, and workspace
budgets, are unchanged.

The protocol machine world was measured serially on 2026-10-07 with
`cargo nextest run -p smith-machine-world --profile measure -j 1` on an idle
machine. Its six focused stories passed in 0.025 seconds, including the file
version conflict, git and symbolic-link refusals, bounded output flood,
child deadline, three check outcomes, and write-authority referee. There is
no fuzzy test until increment 04.4. Its one-second focused share and the
workspace budgets remain unchanged.

After increment 04.4, the protocol machine world was measured serially on
2026-10-07 with the `measure -j 1` profile on an idle machine. Seven focused
tests, including its saturated memory bound, passed in 0.029 seconds. Its
128-seed fuzzy replay sweep passed in 0.037 seconds. The world's one-second
focused and five-second fuzzy shares, and the workspace budgets, remain
unchanged.

The channel opening world was measured serially on 2026-10-07 with
`cargo nextest run -p smith-channel-world --profile measure -j 1`. Its three
focused opening stories passed in 0.012 seconds. It has no fuzzy test until
the channel referee and random stream cuts land. The one-second focused
share and workspace budgets remain unchanged.

The completed protocol channel world was measured serially on 2026-10-07
with the `measure -j 1` profile on an idle machine. Its 38 focused tests,
including a run routed through both real domains and both channel halves,
restart numbering, and an answer lost before a named call's retry,
passed in 0.160 seconds. Its one fuzzy test, sweeping seeded cuts on both
stream shapes, passed in 0.205 seconds. The one-second focused and five-second
fuzzy world shares, and workspace budgets, remain unchanged.

The completed protocol LLM component was measured serially on 2026-10-07
with `measure -j 1`: 57 focused protocol LLM world tests passed in 2.271
seconds. The twelve new component, identity and fake-peer stories contributed
0.060 seconds of reported individual test durations, including both dialects,
typed calls, grants, phase deadlines and a full pool memory measurement. This
world has no fuzzy binary. The workspace budgets are unchanged.

The protocol agent process world was measured serially on 2026-10-08 with
`measure -j 1`: three focused tests passed in 0.027 seconds, and its 64-seed
fuzzy sweep passed in 0.224 seconds. Its focused tests cover the actual Start
to Answer exchange through a TLS fake LLM peer, signal cancellation, and
service construction against the checked memory bound. The binary's startup
integration tests cover a configuration that cannot be read and the final
standard-error line for an unanswered run. The one-second focused and
five-second fuzzy world shares remain intact.

The host process world was measured serially on 2026-10-08 with
`cargo nextest run -p smith-hosts-world --profile measure -j 1` before its
first merge. Five focused stories passed in 0.039 seconds: a hosted agent
service answering through a fake LLM, refused spawn, an error tail, an opening
deadline, and cancel followed by terminate and kill. Its fuzzy sweep and
memory test arrive in the next increment. The one-second focused share and
workspace budgets are unchanged.

### Fact emission metadata validation, 2026-10-09

The eight new emission metadata stories passed serially through the heavy
queue in 0.091 seconds. They delay drains across steps, retain workspace
and delegated call identity through both terminals, record child parentage
and resumed admission, retain completion numbering across retries, and
check the accepted charge in the host's unit. The channel story checks
elapsed emission time relative to activation. The legacy observer accepts
both recorded baseline shapes and the new admission and opening fields,
while rejecting malformed or unknown fields. Existing suite budgets are
unchanged.

The completed host process world was measured serially on 2026-10-08 with
`measure -j 1`: nine focused stories, referee controls and the process-memory
test passed in 0.163 seconds. The seeded crash sweep, including every step of
one complete hosted run, passed in 0.777 seconds. The world's one-second
focused and five-second fuzzy shares, and the workspace budgets, are unchanged.

The local uncertain-delivery stories were measured serially on 2026-10-08
with the `measure -j 1` profile: 57 focused local-world tests passed in
0.279 seconds, and its one fuzzy sweep passed in 0.038 seconds. A commit
whose deadline races landing and a commit whose receipt head cannot be read
are found by their delivery trailer. An unreadable inspection keeps the
intent across later turns and a restart; an absent trailer with an unchanged
head proves no effect. The world's one-second focused and five-second fuzzy
shares, and the workspace budgets, remain unchanged.

07 LOCAL PROCESS was measured serially on 2026-10-08 from `c89db88` plus
this increment. The new world calls the binary's shared shell, with the agent
spawned or colocated. Its ten focused tests passed in 0.833 seconds (0.833
seconds summed rounded PASS durations), and its fuzzy test passed in 2.723
seconds (2.722 seconds PASS duration). The 16-seed sweep runs both placements
and replays each report, cancellation or change with short IO, delayed
completions and cancellation races. Seed 115 is retained as the late final
acknowledgement regression. The referee sees terminal bytes, decoded peer
requests, service facts and a narrow head/message/files checkout interface.
The memory test reuses Skein's allocator and separately prices the finite
world's trace and fixture. TLS signing is intentionally nondeterministic;
replay compares application observations through Skein's trace kit.
The final-tip parallel gate passed 1,409 focused tests in 4.221 seconds and
18 fuzzy tests in 6.132 seconds, with no skips; formatting and workspace
all-target Clippy passed. Existing workspace budgets are unchanged.

07 LOCAL PROCESS AUTHENTICATION extends that world with sign-in, refresh,
refused refresh and a timed refresh before a lent grant expires. Both modes
use the actual private token store, fake issuer and browser Hosts. The referee
includes their observations and uses opaque head bytes for its repository
interface, so real git object names require no referee rewrite. Its final
serial measurements are recorded below. The same 16-seed, two-placement,
twice-replayed sweep now includes all four authentication states as well as
reports, changes and cancellation. The memory story exercises sign-in in the
spawned placement and a change in the colocated placement.
On the final source, fourteen focused tests passed serially in 0.911 seconds
and one fuzzy test in 2.715 seconds (2.714 seconds PASS duration). The final-tip
parallel gate passed 1,414 focused / 4.396 seconds and 18 fuzzy / 6.249
seconds, with no skips; formatting and workspace all-target Clippy passed
(17.04 seconds). Budgets remain unchanged.

The shared agent shell's entry-point stories were measured serially on
2026-10-08 with `cargo nextest run -p smith --profile measure -j 1` on an
idle machine. The three startup, terminal-diagnostic and successful
hosted-trace stories contributed 0.024 seconds of reported test durations;
the whole shell package passed 36 focused tests in 0.214 seconds. The
stories use the binary's `agent_shell::Agent` startup and `Host` pass with
caller-supplied descriptors and deterministic seeds. Existing focused,
fuzzy and per-world budgets remain unchanged.

Testing pass 02.3 moves the agent-process world onto Skein's hosted harness,
sharing its agent adapter with the host world. Idle `measure -j 1` runs on
2026-10-08 measured five focused agent-process tests / 0.116 seconds and one
fuzzy replay sweep / 0.822 seconds; eleven focused host tests / 0.066 seconds
and one fuzzy crash sweep / 2.085 seconds; fourteen focused local-process
tests / 0.613 seconds and one fuzzy sweep / 2.081 seconds. Shares sum rounded
PASS durations. The agent memory driver attains the 65,536-byte admitted
run context, its maximum section array, all sixteen provider output parts
and the exact 8,192-byte client answer aggregate including observed replay
token ownership. Every hosted iteration and drop is metered. Its replay
and fact-discard controls compare complete kernel traces. Checks used
unoptimized profiles with debug information disabled to reduce build-cache
storage; debug assertions and existing timing budgets remain enabled.

Testing pass 02.4 moves the local-process world onto Skein's hosted harness.
Idle serial `measure -j 1` runs on 2026-10-08 measured fifteen focused
local-process tests / 0.882 seconds and its one sixteen-seed, two-placement
replay sweep / 2.767 seconds. The shared agent adapter's affected siblings
measured five focused agent-process tests / 0.115 seconds and one fuzzy /
0.841 seconds; eleven focused host tests / 0.071 seconds and one fuzzy /
2.114 seconds. Shares sum rounded PASS durations. Every local, agent,
terminal, browser, issuer, provider and fake git process is individually
metered through construction, iteration and drop. The TLS sign-in control
uses the same process enum and referee, with transport configuration only;
all seeded worlds remain plaintext. The default fifteen-second, fuzzy
sixty-second and per-world budgets are unchanged. Unoptimized builds retain
debug assertions with debug information disabled as in 02.3.

Testing pass 02.5 moves the machine component world onto Skein's harness.
Idle `measure -j 1` runs on 2026-10-08 measured ten focused tests / 0.045
seconds summed rounded PASS durations (0.047 seconds suite elapsed), and
one 128-seed fuzzy replay sweep / 0.061 seconds. Every request closes its
independently owned root and settles the full IO stack; the fake checkout
and absolute scenario clock persist between runs. Shared replay and checked
construction, iteration and drop controls supplement the retained saturated
component memory test. The bounded flood remains explicitly a component-seam
fixture. Workspace and per-world budgets remain unchanged; unoptimized
builds retain debug assertions with debug information disabled as in 02.3.

Testing pass 03.1 adds the real local-host adapter over Skein's shared ring.
Idle serial `measure -j 1` on 2026-10-08 measured the spawned and in-process
TLS report paths together / 0.219 seconds, and the TLS issuer/browser sign-in
control / 0.173 seconds (0.392 seconds suite elapsed). Both reuse the shared
local-process enum and referee. TLS peers use unprivileged loopback ports;
nextest serializes these stories. The shared invocation deadline starts at
the first supplied clock value, and fixture connection reuse expires after
50 ms. Per-process real-loop metering still needs Skein's shared harness
support before testing pass 03.3 can finish. Budgets remain unchanged.

Testing pass 03.2 repins Skein to e8bc018 and adds the four named real-loop
stories. Idle serial `measure -j 1` on 2026-10-08 measured four real stories /
0.607 seconds summed PASS durations: checked local commit 0.329, first sign-in
0.080, durable second invocation 0.104 and equivalent colocated report 0.094.
They subsume the two original adapter controls. Shared tool-domain and
machine-adapter ceilings now agree; a lower-tier admission regression checks
read/search/shell/check requests at their configured bounds. Fixture reuse
expires after 5 ms. The affected local-process world's sixteen focused tests
measured 0.873 seconds and its fuzzy replay sweep 2.700 seconds. The real
commit contains the edited file, command result and actual check marker, and
its message follows the result fields. All existing budgets remain unchanged.

Testing pass 03.3 measured the real world serially on the idle machine on
2026-10-08, consuming Skein's checked accounting at `e8bc018`. Its four
focused stories passed in 0.723 seconds (sum of individual reports 0.722):
commit/tool/check story 0.392, sign-in 0.094, durable second invocation 0.122,
and equivalent hosted/colocated invocation 0.114 seconds. These include
per-process construction/iteration/drop checks, complete expected scratch
changes and outside child-reaping observations. This world has no fuzzy
suite: kernel schedules are not seeded replay. Existing suite budgets and
serial measurement policy remain unchanged.

Testing pass 04 runs the shipped local binary and the agent it launches against
shared TLS peers, with the shared scripted person on a controlling terminal.
Idle `measure -j 1` on 2026-10-08 measured four end-to-end tests / 0.322 seconds:
first sign-in plus a real tool/check/git commit, a second binary invocation
resuming durable history, a basic terminal report, and startup refusals of both
commands. The cases formerly in `tests/end-to-end/startup.rs` are now in
`end_to_end.rs`. The same scratch setup, narrow checkout observations and
outside scenario policy are used as in the real-loop tier. Every invocation
checks its expected durable outputs, a fresh agent trace, successful process
settlement and reaping. Refusal controls instead require failure and no channel
or chat. Missing io_uring, git, rg or sh fails with a prerequisite diagnostic.

The affected shared local-process tier measured sixteen focused tests / 0.943
seconds and one fuzzy sweep / 3.505 seconds; the real tier measured four tests /
0.727 seconds. The final merge gate passed 1436 focused tests / 7.439 seconds
and seventeen fuzzy tests / 11.001 seconds, with formatting and workspace
all-target Clippy passing. Existing suite and per-world budgets are unchanged.
Run the binary tier alone with:

```sh
cargo nextest run -p smith --test end_to_end --profile measure -j 1
```

After a machine freeze, these gates used one build job, a shared repository
`target`, debug information disabled without disabling assertions, and two
nextest workers. Heavy commands ran sequentially in a user systemd scope with
`MemoryHigh=6G`, `MemoryMax=8G` and `MemorySwapMax=0`, after checking at least
6 GiB available RAM. No memory-pressure or scope-limit failure occurred.

Benchmark increment 00.1 adds the strict task-manifest offline tier. On
2026-10-09, an idle `heavy cargo nextest run -p smith-bench --profile measure
-j 1` run passed its 22 unit and offline tests in 0.214 seconds. The harness
has no fuzzy parser sweep yet. It uses ordinary Rust, no agent or network,
and remains within its one-second focused share. Existing suite budgets are
unchanged; builds used one job with debug information disabled and assertions
retained.

Benchmark increment 00.2 extends that tier with suites, configuration pins,
model tiers and guarded-task selection. Its idle serial `heavy` measurement
on 2026-10-09 passed 29 unit/offline tests in 0.314 seconds. Missing cost or
working-model choices are reported without guessed values. No fuzzy parser
sweep exists yet, and the one-second focused share and suite budgets remain
unchanged.

Benchmark increment 00.3 adds version-one attempt results and scoped token
accounting. Its idle serial `heavy` measurement on 2026-10-09 passed 39
unit/offline tests in 0.429 seconds. Missing scopes and null usage retain
their reasons; no agent or network runs in this tier. There is no fuzzy
parser sweep yet. The one-second focused share and suite budgets remain
unchanged; builds retain assertions with debug information disabled.

Benchmark increment 00.4 adds seeded statistics, summaries, baselines and
committed-median guard costs. Its idle serial `heavy` measurement on
2026-10-09 passed 49 unit/offline tests in 0.702 seconds. It counts every
end, keeps unavailable grades and metrics explicit, and compares only
complete interleaved arms. There is no fuzzy parser sweep yet. The
one-second focused share and suite budgets remain unchanged.

Benchmark increment 00.5 adds Codex's typed JSON-lines observer and its
credential-scanned archived and explicitly synthetic fixtures. Its idle serial
`heavy` measurement on 2026-10-09 passed 52 focused unit/offline tests in
0.645 seconds and one seeded fuzzy parser sweep in 0.129 seconds. Root-only
turn aggregates never stand in for non-root usage or provider response counts.
The harness remains within its one-second focused and five-second fuzzy shares;
no agent, network or login is used by these tests.

Benchmark increment 00.9 merges before 00.6 so every persistent arm/build
write can use the shared credential guard. Its idle serial `heavy` measurement
on 2026-10-09 passed 54 focused unit/offline tests in 0.702 seconds and one
fuzzy parser sweep in 0.156 seconds. The moved guard cases use a synthetic
home and cover resolved tool aliases, XDG directories and refresh-token
refusals without reading a login. The live tier shares the guard and its
small-model choices; no live provider runs at the gate. Both harness shares
and the workspace suite budgets remain unchanged.

Benchmark increment 00.6 adds frozen commit builds after the shared guard.
Its idle serial `heavy` measurement on 2026-10-09 passed 56 focused unit/offline
tests in 0.788 seconds and one fuzzy parser sweep in 0.157 seconds. The build
of `30259d8` and its repeated archive verification run separately through
`heavy`; unit tests build no binary and use only a synthetic git source. The
one-second focused and five-second fuzzy harness shares remain unchanged.

Benchmark increment 00.7 adds the legacy Debug reader and original
`30259d8` traces recorded against fake peers. Its idle serial `heavy`
measurement on 2026-10-09 passed 61 focused unit/offline tests in 0.869
seconds and two fuzzy parser sweeps in 0.239 seconds. Main's acceptance
uses run-issued call identities; usage under calls capture and trace loss
remain unavailable. The fake-provider recording commands run separately;
the offline tier starts no agent and reads no login. The one-second focused
and five-second fuzzy harness shares and workspace budgets remain unchanged.

## 5. Opt-in live suite

The `live` profile runs the shipped binary against real backends, serially,
without retries or fail-fast, with a ten-minute suite timeout. It is excluded
from the default and fuzzy profiles; the merge gate never runs it. Its
credentials and durable test-only token directory are supplied by the caller.

```sh
cargo nextest run --workspace --profile live
```


### Live environment and one-time sign-in

Set `SMITH_TEST_LIVE_TOKEN_DIR` to an **absolute, private directory reserved
for this suite**, and `SMITH_TEST_LIVE_PROVIDERS` to `codex`, `anthropic`, or
`codex,anthropic`. Each provider keeps its account-zero record in its own
subdirectory. The suite requires its bootstrap marker and an existing signed-in
record. It never defaults to the user's token directory. Never copy a refresh
token from the user's tool login: issuers rotate refresh tokens, and doing so
can log those tools out. Dedicated test records are kept durably after each
refresh.

For each selected provider, `SMITH_TEST_LIVE_CODEX_ACCOUNT_FILE` or
`SMITH_TEST_LIVE_ANTHROPIC_ACCOUNT_FILE` names a JSON file containing only public
registration data: `account_id` and `oauth` in the shape described in
[local-host settings](local-host.md#accounts-and-sign-in). Use the account and
issuer's registered public client, authorization/token URLs, callback, scope,
address, TLS server name and form/JSON choice. Do not put tokens or client
secrets in this file. Unknown fields are refused. Machine trust roots verify
both the provider and issuer; no fixture certificates are used. Provider
identity headers come from Skein; Anthropic opts into its Claude Code profile
and only the basic OAuth betas used by Skein's live reference.

`SMITH_TEST_LIVE_CODEX_MODEL` defaults to `gpt-5.5` and
`SMITH_TEST_LIVE_ANTHROPIC_MODEL` to `claude-haiku-4-5`, matching Skein's live
reference. The design moves these defaults to the benchmarks' small tier
(`docs/design/benchmarks.md`, section 7) once it is implemented.
`SMITH_TEST_LIVE_GIT_REMOTE` is optional. It authorizes the configured
push story to that caller-selected test remote, including removal of the run's
branch. Use a disposable test repository with already configured authentication;
no remote is invented by the suite. Model prices are zero in this outcome-only
test fixture; token and turn ceilings still bound each invocation.

For example, bootstrap Codex once (replace the public registration path):

```sh
export SMITH_TEST_LIVE_TOKEN_DIR="$HOME/.local/state/smith-live-tokens"
export SMITH_TEST_LIVE_PROVIDERS=codex
export SMITH_TEST_LIVE_CODEX_ACCOUNT_FILE="$HOME/smith-live-codex-account.json"
export SMITH_TEST_LIVE_BOOTSTRAP_DIR="$HOME/.local/state/smith-live-bootstrap"
cargo nextest run -p smith --test live --profile live -E 'test(=bootstrap_settings)'
cargo run -p smith -- local "$SMITH_TEST_LIVE_BOOTSTRAP_DIR/codex/settings.json" "$SMITH_TEST_LIVE_BOOTSTRAP_DIR/codex"
```

Type a short message at the terminal, open the displayed sign-in URL by hand,
and complete the browser redirect. After the response, close input with Ctrl-D.
For Anthropic, export its account-file variable, select `anthropic`, regenerate
bootstrap settings, and use the `anthropic/settings.json` and `anthropic` state
paths. This sign-in creates the suite's private saved-token record without
reading or writing another tool's login. Remove the bootstrap selector before
running the stories:

```sh
unset SMITH_TEST_LIVE_BOOTSTRAP_DIR
cargo nextest run --workspace --profile live
```

Missing required variables or signed-in records fail with a named prerequisite;
no story silently substitutes a fake or falls back to an ordinary token store.


### Live stories and observations

The four named stories run every selected provider: issuer refresh, checked
in-place commit, durable second invocation, and optional configured push. The
refresh story expires only the dedicated test record, then requires a higher
persisted generation and a usable new grant. The commit story checks exact
committed contents and runs the prescribed check independently. Resume asks the
first binary to remember a unique code and requires a second binary to deliver
it in a file without receiving the code again; earlier turn files must remain
unchanged. Each binary drains and is reaped, with a fresh finish call and an
accepted or delivered finish terminal in its outside trace. No assertion depends on the model's reply
wording. Credential-bearing stderr and command diagnostics are withheld.

When `SMITH_TEST_LIVE_GIT_REMOTE` is set, the optional story uses a unique
`smith-live-<provider>-<pid>-<clock>` branch, verifies its remote head equals the
delivered commit, and removes that branch after success or a caught story
failure. Cleanup failure fails the test. The host's push environment explicitly
carries `HOME`, `PATH` and `SSH_AUTH_SOCK` when set, for existing git/SSH
configuration; no credential is put in a prompt or trace. A run without the
remote reports that push story as not run.

Run only one path while setting up a provider, for example:

```sh
cargo nextest run -p smith --test live --profile live -E 'test(=a_run_refreshes_its_grant_at_the_issuer)'
cargo nextest run -p smith --test live --profile live -E 'test(=a_chat_ends_with_a_commit_in_place)'
cargo nextest run -p smith --test live --profile live -E 'test(=a_second_run_resumes_the_chat_from_its_files)'
```

## 6. Benchmarks

Benchmarks run real agents against real providers, and are designed in
`docs/design/benchmarks.md`. They are never part of the merge gate: no check
in section 1 runs an agent, and no benchmark result blocks or permits a merge.

- **A change to the runtime, limits, providers or host** records a smoke run in
  its commit body, with the binary it ran.
- **Every fix names the probe that guards it,** and its commit body records that
  probe failing on the fix's parent and passing on the fix. A fix without a
  probe adds one in the same change (`docs/design/benchmarks.md`, section 4.2).

Reliability limits increment 4.1 was measured through the serialized heavy
queue with `measure -j 1`: the protocol LLM packages passed 62 focused tests
in 2.279 seconds. The new cut-result byte-peer story took 0.017 seconds;
random rendering-cap properties took 0.662 seconds. The result is cut and
marked without a larger allocation, and both provider dialects accept its
request. Workspace budgets are unchanged.

### Event codec validation, 2026-10-09

The version-one event codec was measured through the serial heavy queue on
2026-10-09, with the canonical Skein lock at `e8bc018`. The affected codec
world and crate passed 21 focused tests in 0.598 seconds; its new corruption
and truncation sweep passed in 0.467 seconds. Every event kind has an owned
record, committed deterministic goldens cover listed values and capture
policies, and the shared counting allocator checks each kind at its largest
configured payload. Line and chunked readers refuse unsupported versions,
retain unknown listed values, and skip unknown records and fields. Existing
workspace budgets are unchanged.

Reliability limits increment 4.2 was measured through the serial heavy queue
on 2026-10-09 with `measure -j 1`: the affected startup, domain-run, machine,
process-world, local-settings and binary packages passed 217 focused tests
in 4.887 seconds, with no skips. New configuration refusals and declaration
overrides use deterministic fixtures; the checked render-to-machine property
retains 256 deterministic cases. All six shipped-binary stories passed.
The workspace focused and fuzzy budgets remain unchanged.
