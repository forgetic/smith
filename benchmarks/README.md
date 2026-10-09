# smith-bench

Real-provider benchmarks show whether a change has its intended effect.
Offline checks run in the workspace's focused suite, without agents,
network access or credentials. See `docs/design/benchmarks.md` for the
contract.

Commands available so far:

```sh
cargo run -p smith-bench -- check [TASKS_DIRECTORY]
cargo nextest run -p smith-bench
```

`check` validates every `task.toml` below the directory (by default,
`benchmarks/tasks`). A refusal names its file and key path. Tasks carry
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
