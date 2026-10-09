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
as unavailable. Committed medians join after summaries exist.

The `small` tier is pinned for both providers. `working.codex` awaits
the user's model and effort; `working.anthropic` belongs to the next
comparison pass. A run requiring an unresolved tier refuses setup.
