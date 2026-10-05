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
