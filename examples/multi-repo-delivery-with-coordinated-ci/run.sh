#!/bin/sh
# multi-repo-delivery-with-coordinated-ci — runnable proof that a coordinated
# PR set lands in dependency order (ADR 0023, acyclic). Extends
# multi-repo-delivery with the landing half. Hermetic: real git, the real anvil
# agent loop (jig), no Forgejo, no live model.
#
# Usage: ./run.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SMITH_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
ANVIL_ROOT=$(CDPATH= cd -- "$SMITH_ROOT/../anvil" && pwd)
TEMPER_ROOT=$(CDPATH= cd -- "$SMITH_ROOT/../temper" && pwd)

echo "== 1/3: the REAL anvil agent edits two sibling repos in one workspace =="
( cd "$ANVIL_ROOT" && cargo test -p anvil-temper-agent --test jig_coding_agent \
    jig_coding_agent_native_edits_two_sibling_repos_in_one_workspace \
    -- --nocapture --exact )

echo
echo "== 2/3: the smith worker assembles the workspace and pushes a branch per repo =="
( cd "$SMITH_ROOT" && cargo test -p smith-worker --test coding_worker_e2e \
    worker_runs_a_coordinated_multi_repo_job_and_pushes_each_writable_repo \
    -- --nocapture --exact )

echo
echo "== 3/3: the daemon lands the coordinated PRs in dependency order (serial) =="
( cd "$TEMPER_ROOT" && cargo test -p temper-runner --test coordinated_serial_landing \
    dependent_pr_lands_only_after_its_cross_repo_prerequisite_merges \
    -- --nocapture --exact )

echo
echo "multi-repo-delivery-with-coordinated-ci: OK — real anvil agent edited"
echo "multiple repos, the worker opened a PR per repo, and the daemon landed them"
echo "in dependency order (the dependent waited for its prerequisite to merge)."
