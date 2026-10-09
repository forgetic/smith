# smith-bench

Attempt results are version-one JSON documents (`src/result.rs`), with strict
fields and explicit observed, lower-bound or unavailable measurements. The
token ledger retains one record per conversation scope and convention,
deduplicates response ids, and names missing scopes in partial totals. A
missing root keeps the total unavailable. Completed answers remain completed
when the harness forces their teardown, with a warning; budget and setup ends
are reported beside the eligible rate counts.

Real-provider benchmarks show whether a change has its intended effect.
Offline checks run in the workspace's focused suite, without agents,
network access or credentials. See `docs/design/benchmarks.md` for the
contract.

Commands available so far:

```sh
cargo run -p smith-bench -- check [TASKS_DIRECTORY]
cargo run -p smith-bench -- guards --suite benchmarks/tests/recorded/suites/sample.toml --tasks benchmarks/tests/recorded/tasks/valid 'benchmarks.md, section 5.2'
cargo run -p smith-bench -- summarise RESULTS_DIRECTORY SUITE TIER single 123
cargo run -p smith-bench -- baseline benchmarks/summaries/RUN.json NAME
cargo nextest run -p smith-bench
```

`check` validates tasks, suites, model tiers and agent pins in this tree.
With a directory argument, it checks only tasks below that directory.
A refusal names its file and key path. Tasks carry
the complete prompt, under 4,088 UTF-8 bytes, as one paragraph ending in
a newline. `seed_sha256` freezes their `seed/` directory.

The seed digest is SHA-256 over files sorted by relative UTF-8 path.
Each file contributes the path length as a big-endian u64, path bytes
(with `/` separators), one byte indicating any executable permission,
the byte length as a big-endian u64, and the file bytes. Empty directories
contribute nothing. Symlinks, special files and `.git` directories are
refused. No seed may contain `benchmarks`, `grader`, `grading`,
`reference` or `reference-solutions` directories.

Runtime data will live under `$XDG_STATE_HOME/smith-bench/`, outside
repositories: raw `runs/`, agent `homes/`, build `clone/` and `arms/`,
and attempt `scratch/`. Summaries and baselines are committed here.
Agents use the user's existing login: Codex runs in `~/.codex`, and
smith borrows it read-only. No one signs in for a benchmark or copies a
refresh token.

Suite task names are `<kind>/<id>[/<variant>]`; `[[select]]` can instead
name a `kind` and `behaviours`, matching all those behaviours. An agent
configuration names `agents/<agent>/<config>.pin.toml`: its agent,
provider, version and relative configuration file. Its digest includes
the exact metadata and configuration bytes, each framed with its byte
length as a big-endian u64. `[[arm]]` has `source = "binary"`,
`"override"` or `"agent"`, with the corresponding commit, smith settings
or agent pin. Comparison schedules arrive with their tasks.

`guards` uses a suite's wall and token budgets to list guarded tasks
cheapest first and explain omissions. Estimates reserve the configured
repetitions, agents, arms and failure rerun. Missing costs are reported
as unavailable. The latest committed medians in `summaries/` replace estimates
only when their task version, digests, tier, model and current agent pins match,
and every configured agent has complete wall and token observations. Variant
costs are separate; missing variant costs fall back to the base estimate.

`summarise` reads `result.json` files recursively or one explicit result file.
It writes `summaries/<run>.json`, recording the seed, suite, tier and design
(`single` or `interleaved`). An optional baseline path is its final argument.
Every end and every missing metric is counted; medians use all observed ends,
with observed and total counts beside them. Only `interleaved` runs compare
arms, with five complete observations each and 10,000 seeded resamples.
Comparisons preserve token conventions and spend bases; cross-agent resource
figures are kept apart. Audit samples include every check/grader disagreement.

`baseline` freezes one configuration, binary and tier in `baselines/<name>.json`.
Both commands refuse to overwrite an existing record. Drift notices ask for a
comparison in one session. They never declare a regression. No grader is
represented by an absent grade; an expected but missing grade has an explicit
unavailable reason.

The `small` tier is pinned for both providers. `working.codex` awaits
the user's model and effort; `working.anthropic` belongs to the next
comparison pass. A run requiring an unresolved tier refuses setup.

Codex's JSON-lines observer is pinned to codex-cli 0.160.0. It retains typed
thread, turn and item records, skips unknown kinds, and reports known shape
changes with their field path. The final turn and settled exit decide the end;
a forced teardown after its answer is a warning. Turn usage is root-only and
repeated terminals count once. Cache writes remain within input, and reasoning
within output; omitted fields are unavailable. JSON lines have no provider
response ids or complete child scopes, so response counts stay unavailable
and totals name the missing non-root evidence. Rollout collection supplies
those scopes later.

The recordings' manifest under `tests/recorded/codex/` pins archive sources,
retained lines and SHA-256 hashes. Completed and children are archived;
failed and repeated are explicitly synthetic derivatives awaiting the first
live recordings. The fuzzy parser sweep is `cargo nextest run -p smith-bench
--profile fuzzy`. These tests read no login and run no agent.

The shared `smith_bench::guard` checks every runtime write destination after
resolving existing ancestors and links. It protects the user's Codex and Claude
homes, `.claude.json`, and smith's XDG configuration and state directories,
including custom tool homes. Relative paths, parent traversal, dangling links
and aliases into those locations are refused before writes. The live tier uses
the same guard and reads its default models from `agents/models.toml`.
Credential conversion refuses refresh-token fields; a pre-borrow binary can
receive only the guard's explicit unusable sentinel, alongside the access token
and account id. The guard reads no login and never reports credential values.
