#!/bin/sh
# multi-repo-delivery — runnable proof of coordinated cross-repo co-development
# (ADR 0023). Hermetic: real git + the real worker↔agent process boundary, no
# Forgejo and no LLM. See README.md for how this maps to the production topology.
#
# Usage: ./run.sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SMITH_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)

TEST=worker_runs_a_coordinated_multi_repo_job_and_pushes_each_writable_repo

echo "multi-repo-delivery: one engineer job -> a two-repo workspace -> a branch"
echo "pushed to EACH repo (acme/service + acme/lib), one RepoOutcome per repo."
echo

cd "$SMITH_ROOT"
exec cargo test -p smith-worker --test coding_worker_e2e "$TEST" -- --nocapture --exact
