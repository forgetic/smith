#!/bin/sh
# checkpoint-live — a REAL end-to-end demonstration of milestone checkpointing.
#
# It runs the actual `anvil-agent` against a REAL LLM (Anthropic via your local
# OAuth login) over a file:// git workspace, then shows the daemon turning the
# agent's progress into a ticked checklist on the coordinating issue. No Forgejo
# and no daemon process are needed — the LLM call is the only network I/O.
#
# Proves, with a real model:
#   1. the model picks up the checkpoint guidance and calls the `checkpoint`
#      tool at each sub-milestone; the host commits + pushes each one;
#   2. those progress markers tick the issue's checklist (the daemon's real
#      apply_progress, against an in-memory forge).
#
# Credentials (no token is ever printed or committed; the synthesized auth file
# lives under the git-ignored work/ dir):
#   - $TEMPER_AGENTS_AUTH_FILE if set; else ~/.pi/agent/auth.json (pi login);
#   - else synthesized from ~/.claude/.credentials.json (Claude Code login).
# Model: $TEMPER_AGENTS_ANTHROPIC_MODEL (default claude-opus-4-8).
#
# Usage: ./run.sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SMITH=$(CDPATH= cd -- "$HERE/../.." && pwd)
ANVIL=$(CDPATH= cd -- "$SMITH/../anvil" && pwd)
TEMPER=$(CDPATH= cd -- "$SMITH/../temper" && pwd)
WORK="$HERE/work"
MODEL=${TEMPER_AGENTS_ANTHROPIC_MODEL:-claude-opus-4-8}

rm -rf "$WORK"; mkdir -p "$WORK"

# --- 0. resolve LLM credentials into an anvil auth file -----------------------
# Prefer an explicit override, then the live Claude Code login (kept fresh),
# then a pi login. (Claude Code creds first because a pi `anthropic` entry can
# be stale.)
if [ -n "${TEMPER_AGENTS_AUTH_FILE:-}" ] && [ -f "${TEMPER_AGENTS_AUTH_FILE:-}" ]; then
    AUTH_FILE=$TEMPER_AGENTS_AUTH_FILE
    echo "auth: using \$TEMPER_AGENTS_AUTH_FILE"
elif [ -f "$HOME/.claude/.credentials.json" ]; then
    AUTH_FILE="$WORK/auth.json"
    python3 - "$HOME/.claude/.credentials.json" "$AUTH_FILE" <<'PY'
import json, os, sys
cc = json.load(open(sys.argv[1]))["claudeAiOauth"]
out = {"anthropic": {"type": "oauth", "access": cc["accessToken"],
                     "refresh": cc["refreshToken"], "expires": cc["expiresAt"]}}
open(sys.argv[2], "w").write(json.dumps(out)); os.chmod(sys.argv[2], 0o600)
PY
    echo "auth: synthesized from ~/.claude/.credentials.json (token redacted)"
elif [ -f "$HOME/.pi/agent/auth.json" ]; then
    AUTH_FILE="$HOME/.pi/agent/auth.json"
    echo "auth: using ~/.pi/agent/auth.json (pi login)"
else
    echo "ERROR: no credentials. Set TEMPER_AGENTS_AUTH_FILE, log in with Claude Code" >&2
    echo "       (~/.claude/.credentials.json), or run 'pi /login anthropic'." >&2
    exit 1
fi

# --- 1. build the real agent binary ------------------------------------------
echo "building anvil-agent…"
( cd "$ANVIL" && cargo build -q --bin anvil-agent )
AGENT="$ANVIL/target/debug/anvil-agent"

# --- 2. a file:// git workspace: bare origin + seeded main + work-branch checkout
ORIGIN="$WORK/origin/demo.git"
git init -q --bare "$ORIGIN"
SEED="$WORK/seed"
git init -q -b main "$SEED"
printf '# demo\n' > "$SEED/README.md"
git -C "$SEED" -c user.name=Seed -c user.email=seed@x.test add -A
git -C "$SEED" -c user.name=Seed -c user.email=seed@x.test commit -q -m seed
git -C "$SEED" push -q "$ORIGIN" main
WS="$WORK/workspace"; mkdir -p "$WS"
git clone -q "$ORIGIN" "$WS/demo"
git -C "$WS/demo" checkout -q -B agent/live-checkpoint origin/main

# --- 3. the WorkspaceContext the worker would hand the agent ------------------
python3 - "$WORK/context.json" <<'PY'
import json, sys
artifact = {"type": "issue", "number": 1, "title": "Add greeting and farewell files",
            "body": ("Add two files to the repository, in the demo/ directory:\n"
                     "1. GREETING.md containing exactly the line: hello\n"
                     "2. FAREWELL.md containing exactly the line: bye\n"
                     "Each file is a separate, coherent sub-milestone."),
            "labels": ["code", "ready"], "state": "Open"}
inner = {"repository": "ai/demo", "role": "engineer", "queue": "code_ready",
         "kind": "code", "artifact": artifact}
ctx = {"repos": [{"id": "ai/demo", "owner": "ai", "name": "demo",
                  "default_branch": "main", "dir": "demo", "access": "writable",
                  "base_branch": "main", "branch_hint": "agent/live-checkpoint"}],
       "work_item": {"role": "engineer", "queue": "code_ready", "kind": "code",
                     "target": "Issue { number: ItemNumber(1) }",
                     "context": json.dumps(inner, indent=2)},
       "correlation_key": "live-checkpoint", "checkout": "writable",
       "allowed_verdicts": []}
open(sys.argv[1], "w").write(json.dumps(ctx, indent=2))
PY

# --- 4. run the REAL agent (cwd = workspace root, exactly as the worker does) --
echo
echo "=== [1/2] running the real agent ($MODEL) — watch it call the checkpoint tool ==="
( cd "$WS" && \
  TEMPER_CODING_WORKSPACE_CONTEXT="$WORK/context.json" \
  TEMPER_CODING_WORKSPACE_RESULT="$WORK/result.json" \
  TEMPER_AGENTS_AUTH_FILE="$AUTH_FILE" \
  TEMPER_AGENTS_ANTHROPIC_MODEL="$MODEL" \
  TEMPER_FORGEJO_USER_ENGINEER="Anvil Engineer" \
  TEMPER_FORGEJO_EMAIL_ENGINEER="engineer@x.test" \
    "$AGENT" --auth anthropic-oauth --max-iterations 10 ) \
  > "$WORK/stdout.jsonl" 2> "$WORK/stderr.log" \
  || { echo "agent failed; stderr tail:"; tail -20 "$WORK/stderr.log"; exit 1; }

echo "--- step-progress the agent emitted (its stdout protocol stream) ---"
cat "$WORK/stdout.jsonl"
echo "--- checkpoint commits pushed to the branch ---"
git -C "$ORIGIN" log --format='%h  %s' agent/live-checkpoint

# proof for half 1: at least one checkpoint marker carried a pushed sha, and a
# checkpoint commit reached the branch.
grep -q '"pushed_sha"' "$WORK/stdout.jsonl" \
    || { echo "FAIL: agent emitted no pushed checkpoint marker"; exit 1; }
git -C "$ORIGIN" log --format='%s' agent/live-checkpoint | grep -q '^checkpoint(step' \
    || { echo "FAIL: no checkpoint commit on the branch"; exit 1; }

# --- 5. show the issue checklist the daemon would tick from those markers -----
echo
echo "=== [2/2] feeding those markers through the daemon -> issue checklist ==="
( cd "$TEMPER" && cargo run -q --example checkpoint_progress_to_issue -- "$WORK/stdout.jsonl" 2> "$WORK/temper.log" ) \
  || { echo "temper example failed; log:"; cat "$WORK/temper.log"; exit 1; }

echo
echo "checkpoint-live: PASS — a real model checkpointed at its milestones, and"
echo "the daemon ticked the issue checklist from the agent's progress markers."
