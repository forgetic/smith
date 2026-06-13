# checkpoint-live — milestone checkpointing, proven with a real LLM

A real end-to-end demonstration that the coding agent **checkpoints at coherent
sub-milestones** and that those checkpoints **tick a checklist on the
coordinating issue/PR**.

```sh
./run.sh
```

The only network call is to the LLM. There is no Forgejo and no daemon process;
git uses local `file://` remotes, and the issue update is produced by the
daemon's real progress applier against an in-memory forge.

## What it does

1. **Real agent, real model (`[1/2]`).** Builds and runs the actual
   `anvil-agent` (the same binary the worker spawns) with cwd set to a workspace
   root holding a `demo/` repo. It's given a two-part task (create `GREETING.md`,
   then `FAREWELL.md`). The agent, following the checkpoint prompt guidance,
   calls the `checkpoint` tool after each file; the host commits + pushes each
   one to the work branch and emits a `StepProgress` marker. You'll see, e.g.:

   ```
   {"step":2,"status":"add GREETING.md with hello","state":"done","pushed_sha":"819ee0b…"}
   {"step":3,"status":"add FAREWELL.md with bye","state":"done","pushed_sha":"5439615…"}
   …
   checkpoint(step 3): add FAREWELL.md with bye
   checkpoint(step 2): add GREETING.md with hello
   ```

2. **Issue checklist (`[2/2]`).** Those exact markers are replayed through the
   daemon's real `apply_progress` (the `temper` example
   `checkpoint_progress_to_issue`) against an in-memory coordinating issue,
   producing the ticked checklist the daemon would post in production:

   ```
   - [ ] step 1: start engineer run (engineer)
   - [x] step 2: add GREETING.md with hello (engineer, pushed 819ee0b5f48f)
   - [x] step 3: add FAREWELL.md with bye (engineer, pushed 54396150343f)
   - [x] step 4: finish engineer run (engineer)
   ```

`run.sh` fails (non-zero) unless a checkpoint marker carried a pushed sha, a
`checkpoint(step …)` commit reached the branch, and the issue checklist was
ticked — so a green run is a proof, not just a demo.

## Credentials

No token is printed or committed; any synthesized auth file lives under the
git-ignored `work/`. `run.sh` resolves credentials in order:

1. `$TEMPER_AGENTS_AUTH_FILE` (an anvil auth file you point at);
2. `~/.claude/.credentials.json` (a Claude Code login) — synthesized into an
   anvil `anthropic` OAuth auth file;
3. `~/.pi/agent/auth.json` (a `pi /login anthropic` login).

Model defaults to `claude-opus-4-8`; override with `TEMPER_AGENTS_ANTHROPIC_MODEL`.

## Layout

Sibling repos are assumed (`../anvil`, `../temper` next to this `smith`
checkout). The agent + checkpoint tool live in `anvil`; the progress→checklist
applier lives in `temper`; this launcher lives in `smith`.
