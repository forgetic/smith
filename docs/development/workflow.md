# Development workflow

How work reaches main. Work happens on local branches; main moves only by
merging a branch that passed every check below, or that changes only
documentation.

For the executable's local mode, see [local-host settings and commands](local-host.md).

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
