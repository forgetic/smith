# multi-repo-delivery — coordinated cross-repo co-development (ADR 0023)

A proof that **one engineer job can change several repositories at once and open
one pull request per repository**, for a broad issue whose fix is genuinely
cross-cutting (e.g. a worker-protocol change spanning `temper` *and* `smith`).

This is distinct from the existing *decomposition* path (architect fans an
intake issue out into independent child issues). Here a **single coordinating
issue** is serviced by **one worker** that assembles all the relevant repos into
**one workspace** — laid out as flat siblings so their inter-repo `path`
dependencies resolve with no `[patch]` rewriting — edits any subset in one agent
turn, and pushes a branch to each changed repo. The daemon then opens one PR per
repo, every PR linked back to the coordinating issue (repo-qualified for the
cross-repo ones) and stamped with a shared coordination key.

See `temper/docs/adr/0023-multi-repo-co-development-jobs.md` for the design.

## What carries the multi-repo intent

The coordinating issue declares its workspace in a `temper:workspace` metadata
block in the issue body; the daemon reads it during enrichment and builds the
job's `WorkspaceManifest`:

```html
<!-- temper:workspace
{"repos":[
  {"repo":"ai/temper","access":"writable"},
  {"repo":"ai/smith","access":"writable"},
  {"repo":"ai/skein","access":"read_only"}
]}
-->
```

- `writable` repos are eligible for a commit/push/PR; a repo that ends up with no
  diff gets no PR.
- `read_only` repos (here `skein`) are checked out at their base ref so the
  combined build resolves, but are never pushed.
- The worker assigned the job must hold a `(role, repo)` capability for **every**
  repo in the manifest.

## The runnable proof

The end-to-end mechanic — worker assembles the sibling workspace, drives a real
out-of-process agent over the combined root, and pushes a branch to each writable
repo — is exercised by an automated, hermetic test that uses real `git` and the
real worker↔agent process boundary (no LLM, no Forgejo):

```sh
./run.sh
```

It runs `smith-worker`'s `worker_runs_a_coordinated_multi_repo_job_and_pushes_each_writable_repo`
e2e: a two-repo writable manifest (`acme/service` + `acme/lib`) driven through a
fake daemon and the `smith-fake-agent` process, asserting that:

- the worker reports **one `RepoOutcome` per writable repo** (the daemon turns
  each into a PR);
- the shared `agent/coord-for-code-7` branch **landed on each origin** at exactly
  the reported sha, carrying the agent's product file;
- only the **primary** repo's commit carries the `Closes #7` trailer (cross-repo
  close-on-merge does not exist).

## How this maps to the production topology

In a full deployment (cf. the `basic-delivery` example, which boots Forgejo + a
forgejo-runner + `temper-daemon` + `smith-worker`):

1. A coordinating code-ready issue carries the `temper:workspace` block above.
2. `temper-daemon` enrichment reads it, resolves each repo from the Forge, and
   builds the `WorkspaceManifest` (primary first; one shared coordination
   branch) — falling back to a degenerate single-repo manifest when no block is
   present, so ordinary single-repo jobs are unchanged.
3. Dispatch routes the job to a worker capable of every manifest repo.
4. `smith-worker` assembles the siblings, runs one agent turn, and pushes each
   changed writable repo, returning `Vec<RepoOutcome>`.
5. The daemon opens one PR per outcome, cross-linked by the coordination key.

Coordinated landing / cross-repo CI ordering is intentionally out of scope for
this phase (the PRs open independently); see the ADR's "Out of scope".
