# End a development session cleanly

Use this checklist before handing work to a human or another agent.

## 1. Inspect the change set

```sh
git status --short
git diff --stat
```

Use `git diff` for every file you edited. Confirm there are no generated files,
secrets, copied auth files, or unrelated Temper changes.

## 2. Run validation

For docs-only changes, formatting is usually enough. For code changes, run:

```sh
cargo fmt --all
cargo dev-clippy
cargo dev-check
```

Run focused tests for behavior changes. Use `cargo dev-test` for the full
hermetic workspace when practical.

## 3. Review documentation from the top

Start with the files a fresh agent will read first:

1. `README.md`
2. `AGENTS.md`
3. `docs/README.md`

Then review task-relevant how-to, reference, explanation, and ADR files.

## 4. Capture lessons learned

If the session involved a human correction, failed assumption, repeated mistake,
or missing guidance, add or update an entry in
`docs/reference/agent-lessons/`. Use `record-agent-lesson.md`.

## 5. Check file size budgets

Keep hand-written docs focused and Rust source/test files under about 600 lines.
Useful checks:

```sh
wc -l README.md AGENTS.md docs/**/*.md
find crates -type f -name '*.rs' -print0 | xargs -0 wc -l | sort -n
```

## 6. Leave an explicit handoff

Include what changed, validation run, limitations, and any follow-up work.
