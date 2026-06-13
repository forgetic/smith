#!/bin/sh
# multi-repo-delivery — runnable proof of coordinated cross-repo co-development
# (ADR 0023). Hermetic: real git, the real anvil agent loop (jig fake LLM), and
# the real worker↔agent process boundary — no Forgejo, no live model. See
# README.md for how this maps to the production topology.
#
# It runs the two halves of the path, each with the REAL component for its layer:
#   1. the real anvil agent editing TWO sibling repos in one workspace turn
#      (../anvil jig e2e);
#   2. the smith worker assembling that workspace and pushing a branch to EACH
#      writable repo — one RepoOutcome → one PR per repo.
#
# Usage: ./run.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SMITH_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
ANVIL_ROOT=$(CDPATH= cd -- "$SMITH_ROOT/../anvil" && pwd)

echo "== 1/2: the REAL anvil agent edits two sibling repos in one workspace =="
( cd "$ANVIL_ROOT" && cargo test -p anvil-temper-agent --test jig_coding_agent \
    jig_coding_agent_native_edits_two_sibling_repos_in_one_workspace \
    -- --nocapture --exact )

echo
echo "== 2/2: the smith worker assembles the workspace and pushes a branch per repo =="
( cd "$SMITH_ROOT" && cargo test -p smith-worker --test coding_worker_e2e \
    worker_runs_a_coordinated_multi_repo_job_and_pushes_each_writable_repo \
    -- --nocapture --exact )

echo
echo "multi-repo-delivery: OK — real anvil agent edited multiple repos, and the"
echo "worker produced one pushed branch (→ one PR) per writable repo."
