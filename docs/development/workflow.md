# Development workflow

How work reaches main. Work happens on local branches; main moves only by
merging a branch that passed every check below, or that changes only
documentation.

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
`skein_world::domain` directly. The `smith-world-tests` package checks that
dependency's public boundary without copying its machinery.
05s2 adds the copied agent domains and provider/OAuth codecs, their
component worlds and a composed typed scripted host world. Its named-source
mapping and preserved coverage are recorded in
[migration-05s2.md](migration-05s2.md). Protocol integration and binaries
remain later increments; their stories and limits belong to their design
documents.

Before adding a world, record its focused and fuzzy serial measurements
here, on an idle machine, and keep the workspace suites within their
existing budgets. Do not raise a timeout to accommodate a new world.
The foundation was measured serially on 2026-10-05: two focused tests in
0.007 seconds and one fuzzy test (64 seeds, each replayed) in 0.006 seconds.
These are consumer tests of the shared kit; later worlds record their own shares.

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
| Shared harness consumer | 2 / 0.006 | 1 / 0.006 |

The copied production crate tests contribute the remaining 217 focused
tests. These measurements retain the copied seed sweeps and memory
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
