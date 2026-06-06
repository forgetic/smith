# Reference-delivery observability guide

Use this page while `./run.sh start` is still running. `./run.sh stop` removes
the throwaway Forgejo data, so Forge-state validation must happen before
teardown. Logs stay under `logs/` for later inspection.

## Where to look

- **Worker startup and capabilities** — each worker log starts with
  `worker_capabilities` JSON plus a `temper-worker: resolved repositories ...`
  line. Check `worker_kind`, `worker`, `role`, `repositories`,
  `authorized_actions`, and `available_external_tools`.
- **Scan and work-item selection** — role worker logs emit `scan_summary` and
  `work_item_selected` when a scan finds work. The important join fields are
  `tick_id`, `work_item_id`, `decision_id`, `repo`, `role`, `queue`,
  `artifact_type`, `artifact_number`, and `artifact_kind`.
- **Smith decision correlation** — Temper emits `role_decision_request` before
  invoking Smith and `role_decision_reply` after Smith returns. The same
  authority-neutral observability fields are nested in
  `work_item_context.observability`, so Smith logs or captures can be joined by
  `decision_id`/`work_item_id`. Smith receives metadata only: no Forge token,
  Forge handle, or mutation tool. Smith-owned provider details are documented in
  `~/src/rust/smith/plans/observability/README.md`.
- **Workspace verdicts** — the architect/engineer/reviewer roles each declare a
  workspace external tool (`triage_workspace`, `coding_workspace`,
  `review_workspace`). One bound command (the Smith pi-SDK agent by default)
  runs per role and returns a work product plus an optional verdict. The engine
  routes the action's declared `outcomes` on that verdict: the architect's
  `ready_code`/`needs_design`/`needs_breakdown` route triage (and `set_body` /
  `create_issues` effects), the engineer's absent verdict produces the PR head
  while `needs_architect` escalates, and the reviewer's `approve`/`changes`/
  `escalate` route landing, a native review (`attach_review`), or escalation.
- **Action and transition outcomes** — `action_dispatch` records the selected
  manifest action, transition id, and external-executor availability.
  `transition_execution` records `outcome`, `stale_work`, compact effect
  summaries, failure class, diagnostic classes, and postcondition outcome.
- **Mechanical landing** — `mechanical.log` starts with a
  `temper-worker: mechanical ... ci_reader=bot ...` line: the mechanical worker
  runs as the provisioned `bot` user and owns landing in this workflow. It reads
  Forgejo Actions status with the bot's web-UI credentials (ADR 0019) and runs
  `land_pr` (merge) once a `landing`-labelled PR's review and CI gates pass, or
  `route_merge_conflict` when the merge conflicts. `./run.sh validate-webhooks`
  fails loudly if the bot credentials or the `ci_reader=bot` startup line are
  missing, or if the worker reports a CI-read fallback error.
- **Mechanical reconciliation** — the same worker also emits
  `mechanical_reconciliation` for controller-plane findings/actions (lease
  expiry, partial-transition repair, dependency unblock). In the optional
  cross-repo mode a blocked code issue with no dependency relations is named as
  `diagnostic=blocked_artifact_without_dependencies` with `dependency_count=0`;
  this explains why dependency-gated unblocking intentionally does not proceed.
- **Validator diagnostics** — `./run.sh validate-multi-repo` checks logs and live
  Forge state. The single-repo default converges and needs no fan-out, so this is
  only meaningful in the optional cross-repo mode, where a bare parent the LLM
  architect cannot fan out surfaces as:

  ```text
  missing: cross-repo parent acme/service#1 expected 2 child dependencies, found 0
  diagnosis: architect blocked the parent but no fan-out side effects were recorded
  missing: blocked parent acme/service#1 has zero dependencies
  diagnosis: dependency-gated unblocking intentionally cannot proceed without at least one recorded dependency
  ```

## Minimal movement trail

For one moving item, the expected per-decision trail is:

```text
worker_capabilities -> scan_summary -> work_item_selected
role_decision_request -> role_decision_reply -> action_dispatch
transition_execution -> completed tick ... tick_id=...
```

The converging single-repo default walks these transitions in order across the
role workers, each architect/engineer/reviewer step routed by the verdict its
bound workspace returns:

- `mark_untriaged` (mechanical bot automation on `raw_intake`, adds `untriaged`)
- `triage_intake` (architect, `triage_workspace` -> `ready_code` verdict) routes
  to `triage_intake_to_code` (`set_body` rewrite + `code`/`ready` labels)
- `open_pr` (engineer, `coding_workspace` leaves a product diff and the engine
  opens the PR; a `needs_architect` verdict instead routes
  `request_code_architect_input`)
- `review_pr` (reviewer, `review_workspace` -> `approve` verdict) routes to
  `approve_review` (adds `landing`); `changes` routes
  `request_changes_with_review` and `escalate` routes `request_architect_input`
- `land_pr` (mechanical bot automation on `landing`, merges the PR; a merge
  conflict routes `route_merge_conflict`)
- `reconcile_landed` (architect)

For a stuck cross-repo parent (optional mode only), add:

```text
mechanical_reconciliation diagnostic=blocked_artifact_without_dependencies
validate-multi-repo missing: ... expected N child dependencies, found 0
```

All event payloads are bounded and omit full bodies, comments, provider args,
auth paths, and secrets.
