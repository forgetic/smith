# multi-repo-delivery-with-coordinated-ci — serial landing of a coordinated set

Extends [`multi-repo-delivery`](../multi-repo-delivery/) with the **landing**
half: once a coordinated change has opened one PR per repo, the PRs land in
**dependency order** — a dependent repo's PR merges only after its prerequisite
PR (in another repo) has merged, *even when the dependent's own CI is already
green*.

This is the acyclic, sequential case (ADR 0023's "Out of scope" item, now
implemented for acyclic sets): the architect/engineer declares the landing order
in the coordinating issue's `temper:workspace` block via `depends_on`, the daemon
turns it into cross-repo dependency links between the opened PRs, and the
mechanical landing backstop holds each PR closed behind its `dependency_gate`
until its prerequisites merge.

```html
<!-- temper:workspace
{"repos":[
  {"repo":"ai/temper","access":"writable"},
  {"repo":"ai/smith","access":"writable","depends_on":["ai/temper"]}
]}
-->
```

Here `ai/smith` consumes `ai/temper`'s worker-protocol crate by path, so smith's
PR must land **after** temper's. The daemon opens temper's PR first (no deps) and
smith's PR with a cross-repo dependency link to temper's; smith's PR then waits.

## The runnable proof

```sh
./run.sh
```

`run.sh` runs three hermetic halves (real `git`, real anvil agent loop via jig,
no Forgejo, no live model), each exercising the real component for its layer:

1. **Real anvil agent edits multiple repos in one workspace** — `../anvil`'s
   `jig_coding_agent_native_edits_two_sibling_repos_in_one_workspace`.
2. **The worker pushes a branch (→ one PR) per repo** — `smith-worker`'s
   `worker_runs_a_coordinated_multi_repo_job_and_pushes_each_writable_repo`.
3. **The daemon lands the PRs in dependency order** — `../temper`'s
   `coordinated_serial_landing::dependent_pr_lands_only_after_its_cross_repo_prerequisite_merges`:
   two coordinated PRs (B depends on A) with green CI on both; B does **not**
   land while A is open, A lands first, then B lands once A has merged.

## How serial landing works (and what it deliberately does not do)

- The coordinated PRs carry cross-repo dependency links (`WorkspaceRepo.depends_on`
  → PR metadata `dependencies: [{repository_id, number}]`), written by the daemon
  in topological order so a dependent's link points at its already-opened
  prerequisite.
- `land_pr` requires the `dependency_gate` (`dependencies_resolved`), which the
  workflow engine already resolves across repos
  (`dependency_state::target_landed` reads the target PR in the other repo and
  checks `Merged`). A PR with no dependency links (an ordinary single-repo PR)
  lands exactly as before.
- **Acyclic only.** The set must form a DAG; a dependent waits for its
  prerequisites, full stop. Mutual/cyclic dependencies (where two repos each need
  the other before either can go green) are not handled by sequential landing —
  that needs combined-checkout CI, which remains out of scope.
