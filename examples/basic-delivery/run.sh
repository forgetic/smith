#!/bin/sh
# basic-delivery example — POSIX launcher / teardown.
#
# The minimal, no-human-in-the-loop counterpart to reference-delivery: ONE repo,
# THREE roles (architect, engineer, mechanical-as-bot) + CI, webhooks on, and
# landing gated on CI alone. It boots EVERY process of the production topology
# from development-profile binaries:
#   1. a throwaway Forgejo server (SQLite, Actions enabled),
#   2. a host-mode forgejo-runner producing real CI,
#   3. admin bootstrap + the production provision binary against the bundled
#      3-role workflow (--workflow), creating the org/users/repo/labels/CI and
#      registering the webhook — but deliberately NOT yet filing the intake
#      issue,
#   4. a fixed temper-worker pool: an `architect` role worker, an `engineer`
#      role worker, and one `mechanical` worker that runs the controller plane
#      AND lands CI-green PRs as the `bot` (the only landing authority),
#   5. ONLY once that pool and the wake trigger are ready, a second seed-only
#      provision pass (--seed-only) files ONE unlabeled intake issue authored by
#      the SITE ADMIN (the workflow's intake_author = site_admin) — so the
#      issue-created webhook is what WAKES the bot (which stamps it untriaged),
#      proving the wake path instead of the bot only noticing on its next poll.
# It binds the Smith pi-SDK coding agent so the architect triages the intake to a
# ready code issue and the engineer opens a real implementation PR; CI runs, goes
# green, and the bot auto-merges — no reviewer, owner, or human. It tears
# everything down cleanly on Ctrl-C / signal / `./run.sh stop`.
#
# This script targets the operator-facing entry points from Temper's root
# package. By default it builds/uses the development-profile binaries under
# target/debug; override TEMPER_*_BIN for prebuilt or release artifacts.
#
# Usage:
#   ./run.sh [start]          boot everything and block until Ctrl-C / stop-file
#   ./run.sh validate-webhooks inspect logs from a running/completed run
#   ./run.sh stop             tear down a previous run via the saved PIDs
#   ./run.sh help             show this usage
#
# Orphan cleanup (lesson 0009) — if a run is force-killed (SIGKILL) the Drop/
# trap guards do not fire; clean up survivors by hand with:
#       pkill -f forgejo
#       pkill -f forgejo-runner
#       pkill -f temper-worker
#       pkill -f temper-trigger-forgejo
#       rm -rf examples/basic-delivery/run
#
# POSIX sh only (no bashisms). Validate with `sh -n run.sh` (and shellcheck).
# Secrets travel by env or the sourced secrets files, NEVER on a command line.

set -eu

# --- Locations ----------------------------------------------------------------
if [ -n "${TEMPER_BASIC_DELIVERY_SCRIPT_DIR:-}" ]; then
    SCRIPT_DIR=$TEMPER_BASIC_DELIVERY_SCRIPT_DIR
else
    SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fi
SMITH_WORKSPACE_ROOT_DEFAULT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
WORKSPACE_ROOT=${TEMPER_WORKSPACE_ROOT:-$(CDPATH= cd -- "$SMITH_WORKSPACE_ROOT_DEFAULT/../temper" && pwd)}
CONFIG_DIR="$SCRIPT_DIR/config"
SECRETS_DIR="$SCRIPT_DIR/secrets"
RUN_DIR="$SCRIPT_DIR/run"
LOG_DIR="$SCRIPT_DIR/logs"

FORGEJO_DATA="$RUN_DIR/forgejo"
APP_INI="$FORGEJO_DATA/custom/conf/app.ini"
RUNNER_DIR="$RUN_DIR/runner"
STOP_FILE="$RUN_DIR/stop"
SERVER_PID_FILE="$RUN_DIR/server.pid"
RUNNER_PID_FILE="$RUN_DIR/runner.pid"
WORKERS_PID_FILE="$RUN_DIR/workers.pids"
TRIGGER_PID_FILE="$RUN_DIR/trigger.pid"
WAKE_DIR="$RUN_DIR/wake"
ROLES_ENV="$SECRETS_DIR/roles.env"
WEBHOOK_SECRET_FILE="$SECRETS_DIR/webhook-secret"
WAKE_SECRET_FILE="$SECRETS_DIR/wake-secret"

# Pinned versions for the bundled throwaway server/runner used by this example.
FORGEJO_VERSION=7.0.12
FORGEJO_RUNNER_VERSION=3.5.1

# Throwaway admin identity. This is also the workflow's intake_author
# (site_admin): the bundled workflow.json declares intake_author = site_admin, so
# the provisioner seeds the intake issue as THIS admin (the "external filer").
# The server is killed + wiped on teardown; this is never a credential that
# reaches anything real, and never echoed.
ADMIN_USER=basicadmin
ADMIN_EMAIL=basicadmin@example.invalid
ADMIN_PASSWORD='Basic-Delivery-Admin-1!'

# Diagnostic strings the production worker emits when the mechanical landing
# worker cannot read Forgejo 7.0.x Actions status over the web UI (ADR 0019).
CI_FALLBACK_MISSING_CREDENTIALS='no web-UI credentials configured for the CI read fallback'
CI_FALLBACK_LOGIN_FAILED='forgejo web-ui login failed'

log() { printf '[run.sh] %s\n' "$*"; }
die() { printf '[run.sh] error: %s\n' "$*" >&2; exit 1; }

sleep_short() {
    sleep 0.2 2>/dev/null || sleep 1
}

DISPLAY_SCRIPT=${TEMPER_BASIC_DELIVERY_ORIGINAL:-$SCRIPT_DIR/run.sh}

# Dash reads long-running scripts lazily. If this file is edited while the demo
# is sleeping in monitor(), the running shell may parse a half-new tail and fail
# during teardown. Run starts from a private snapshot so source edits/rebuilds do
# not affect the already-running launcher.
if [ "${TEMPER_BASIC_DELIVERY_SNAPSHOT:-0}" != "1" ]; then
    case "${1:-start}" in
        start | "")
            mkdir -p "$RUN_DIR"
            _snapshot="$RUN_DIR/run.sh.snapshot.$$"
            cp "$SCRIPT_DIR/run.sh" "$_snapshot"
            chmod 700 "$_snapshot"
            TEMPER_BASIC_DELIVERY_SNAPSHOT=1 \
            TEMPER_BASIC_DELIVERY_SCRIPT_DIR="$SCRIPT_DIR" \
            TEMPER_BASIC_DELIVERY_ORIGINAL="$DISPLAY_SCRIPT" \
                exec /bin/sh "$_snapshot" "$@"
            ;;
    esac
fi

usage() {
    cat <<EOF
usage: $DISPLAY_SCRIPT [start|validate-webhooks|stop|help]

  start (default)      boot Forgejo + runner, provision the single repo against
                       the bundled 3-role workflow, launch the architect +
                       engineer + mechanical(bot) workers, then file one
                       site-admin intake issue (so its webhook wakes them) and
                       block until Ctrl-C or the stop-file.
  validate-webhooks    inspect logs/ and report whether webhook wakes were
                       registered, accepted, delivered, consumed, and acted on.
  stop                 tear down a previous run via run/*.pid.
  help                 show this message.

Configuration is read from config/temper.env (no secrets). Concrete LLM
provider/auth options are passed to Smith as opaque responder arguments; see
Smith's README.
EOF
}

# --- Teardown -----------------------------------------------------------------

# Sends TERM, waits briefly, then KILL. Tolerates a dead/absent pid.
stop_pid() {
    _pid=$1
    [ -n "$_pid" ] || return 0
    kill -0 "$_pid" 2>/dev/null || return 0
    kill -TERM "$_pid" 2>/dev/null || true
    _i=0
    while kill -0 "$_pid" 2>/dev/null && [ "$_i" -lt 20 ]; do
        sleep 0.2 2>/dev/null || sleep 1
        _i=$((_i + 1))
    done
    kill -KILL "$_pid" 2>/dev/null || true
}

# Stops every pid listed (one per line) in a pid file, then removes it.
stop_pid_file() {
    _file=$1
    [ -f "$_file" ] || return 0
    while IFS= read -r _p; do
        [ -n "$_p" ] && stop_pid "$_p"
    done <"$_file"
    rm -f "$_file"
}

# Tears down workers, runner, and server (in that order) and clears run state.
# Idempotent: safe to call from the EXIT trap and from `./run.sh stop`.
cleanup() {
    trap - EXIT INT TERM
    log 'tearing down...'
    # Signal the workers to stop cooperatively first, then hard-stop survivors.
    [ -d "$RUN_DIR" ] && : >"$STOP_FILE" 2>/dev/null || true
    sleep 1
    stop_pid_file "$WORKERS_PID_FILE"
    stop_pid_file "$TRIGGER_PID_FILE"
    stop_pid_file "$RUNNER_PID_FILE"
    stop_pid_file "$SERVER_PID_FILE"
    # Drop the throwaway server/runner data + sentinel so a re-run starts fresh;
    # keep logs/ for inspection.
    rm -rf "$FORGEJO_DATA" "$RUNNER_DIR" "$WAKE_DIR" "$STOP_FILE" \
        "$RUN_DIR/coding-workspace" "$RUN_DIR"/run.sh.snapshot.* \
        2>/dev/null || true
    rmdir "$RUN_DIR" 2>/dev/null || true
    log 'teardown complete'
}

cmd_stop() {
    [ -d "$RUN_DIR" ] || { log 'nothing to stop (no run/ dir)'; return 0; }
    cleanup
}

# --- Config + secrets ---------------------------------------------------------

# Config knobs whose pre-existing environment value should win over the file
# (precedence: CLI/env > config/temper.env > built-in default). The file is the
# operator's edited config; a `VAR=x ./run.sh` still overrides it.
CONFIG_KNOBS="OWNER NAME DEFAULT_BRANCH WORKFLOW_FILE INTAKE_TITLE INTAKE_BODY_FILE BASE_URL POLL_MS CI_STATUS_POLL_MS IDLE_POLL_MAX_MS RUN_SECS WEBHOOKS TRIGGER_BIND WEBHOOK_URL \
TEMPER_FORGEJO_GOMAXPROCS TEMPER_FORGEJO_BINARY \
TEMPER_FORGEJO_RUNNER_BINARY TEMPER_WORKER_BIN TEMPER_PROVISION_BIN \
TEMPER_TRIGGER_BIN TEMPER_BUILD_PACKAGE \
BASIC_DELIVERY_ROLE_DECISION SMITH_WORKSPACE_ROOT SMITH_BUILD_PACKAGE \
SMITH_WORKFLOW_ROLE_DECISION_BIN SMITH_WORKFLOW_ROLE_DECISION_ARGS_JSON \
SMITH_WORKFLOW_ROLE_DECISION_ENV_ALLOWLIST SMITH_WORKFLOW_ROLE_DECISION_CWD \
SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS \
BASIC_DELIVERY_CODER SMITH_CODING_AGENT_BIN SMITH_CODING_AGENT_ARGS \
TEMPER_CODING_WORKSPACE_ROOT TEMPER_CODING_WORKSPACE_COMMAND \
TEMPER_CODING_WORKSPACE_REMOTE TEMPER_CODING_WORKSPACE_PUSH \
TEMPER_CODING_WORKSPACE_PR_LABELS"

repo_owner() { printf '%s\n' "${1%%/*}"; }
repo_name() { printf '%s\n' "${1#*/}"; }

validate_repo_path() {
    _repo=$1
    case "$_repo" in
        */*) ;;
        *) die "repository must be owner/name, got '$_repo'" ;;
    esac
    _owner=$(repo_owner "$_repo")
    _name=$(repo_name "$_repo")
    [ -n "$_owner" ] && [ -n "$_name" ] && [ "$_owner/$_name" = "$_repo" ] \
        || die "repository must be owner/name with non-empty parts, got '$_repo'"
}

load_config() {
    [ -f "$CONFIG_DIR/temper.env" ] || die "missing $CONFIG_DIR/temper.env"
    # Snapshot any pre-existing env values so they survive the file sourcing.
    # CI_STATUS_POLL_MS is special: an intentionally empty value selects "use
    # POLL_MS", so presence matters even when the value is empty.
    _ci_status_poll_was_set=${CI_STATUS_POLL_MS+x}
    _pre_CI_STATUS_POLL_MS_VALUE=${CI_STATUS_POLL_MS-}
    for _k in $CONFIG_KNOBS; do
        eval "_pre_$_k=\${$_k:-}"
    done
    # shellcheck disable=SC1090
    . "$CONFIG_DIR/temper.env"
    # Optional operator secret overrides (gitignored).
    if [ -f "$SECRETS_DIR/.env" ]; then
        # shellcheck disable=SC1090
        . "$SECRETS_DIR/.env"
    fi
    # Re-apply any non-empty pre-existing env value over the file's setting.
    for _k in $CONFIG_KNOBS; do
        eval "_p=\${_pre_$_k}"
        [ -n "$_p" ] && eval "$_k=\$_p"
    done
    if [ -n "$_ci_status_poll_was_set" ]; then
        CI_STATUS_POLL_MS=${_pre_CI_STATUS_POLL_MS_VALUE}
    fi

    OWNER=${OWNER:-acme}
    NAME=${NAME:-service}
    DEFAULT_BRANCH=${DEFAULT_BRANCH:-main}
    WORKFLOW_FILE=${WORKFLOW_FILE:-workflow.json}
    # The seeded intake issue is deliberately THIN: the site admin (external
    # filer) states only the overall intent, with no acceptance criteria, no
    # setting name, and no implementation detail. Turning that into an
    # implementable spec is the architect's job (the triage_intake_to_code
    # set_body rewrite) — that is what this example proves the architect can do.
    INTAKE_TITLE=${INTAKE_TITLE:-Service banner should identify the environment}
    INTAKE_BODY_FILE=${INTAKE_BODY_FILE:-intake-issue.md}
    BASE_URL=${BASE_URL:-http://127.0.0.1:4100}
    POLL_MS=${POLL_MS:-120000}
    # The mechanical landing worker reads CI status on a short poll because
    # Forgejo 7.0.x does not webhook on Actions completion. An explicitly empty
    # CI_STATUS_POLL_MS falls back to POLL_MS; otherwise it defaults to 1000ms.
    if [ "${CI_STATUS_POLL_MS+x}" = "x" ]; then
        [ -n "$CI_STATUS_POLL_MS" ] || CI_STATUS_POLL_MS=$POLL_MS
    else
        CI_STATUS_POLL_MS=1000
    fi
    IDLE_POLL_MAX_MS=${IDLE_POLL_MAX_MS:-8000}
    RUN_SECS=${RUN_SECS:-600}
    WEBHOOKS=${WEBHOOKS:-1}
    TRIGGER_BIND=${TRIGGER_BIND:-127.0.0.1:38100}
    WEBHOOK_URL=${WEBHOOK_URL:-http://127.0.0.1:38100/forgejo/webhook}
    TEMPER_FORGEJO_GOMAXPROCS=${TEMPER_FORGEJO_GOMAXPROCS:-2}
    TEMPER_FORGEJO_BINARY=${TEMPER_FORGEJO_BINARY:-}
    TEMPER_FORGEJO_RUNNER_BINARY=${TEMPER_FORGEJO_RUNNER_BINARY:-}
    TEMPER_WORKER_BIN=${TEMPER_WORKER_BIN:-}
    TEMPER_PROVISION_BIN=${TEMPER_PROVISION_BIN:-}
    TEMPER_TRIGGER_BIN=${TEMPER_TRIGGER_BIN:-}
    TEMPER_BUILD_PACKAGE=${TEMPER_BUILD_PACKAGE:-temper}
    BASIC_DELIVERY_ROLE_DECISION=${BASIC_DELIVERY_ROLE_DECISION:-smith}
    case "$BASIC_DELIVERY_ROLE_DECISION" in
        smith | greeting) ;;
        *) die "BASIC_DELIVERY_ROLE_DECISION must be smith or greeting, got '$BASIC_DELIVERY_ROLE_DECISION'" ;;
    esac
    SMITH_WORKSPACE_ROOT=${SMITH_WORKSPACE_ROOT:-$SMITH_WORKSPACE_ROOT_DEFAULT}
    SMITH_BUILD_PACKAGE=${SMITH_BUILD_PACKAGE:-smith-temper-agent-cli}
    SMITH_WORKFLOW_ROLE_DECISION_BIN=${SMITH_WORKFLOW_ROLE_DECISION_BIN:-}
    SMITH_WORKFLOW_ROLE_DECISION_ARGS_JSON=${SMITH_WORKFLOW_ROLE_DECISION_ARGS_JSON:-}
    if [ -z "$SMITH_WORKFLOW_ROLE_DECISION_ARGS_JSON" ]; then
        SMITH_WORKFLOW_ROLE_DECISION_ARGS_JSON='["--auth","chatgpt-oauth"]'
    fi
    SMITH_WORKFLOW_ROLE_DECISION_ENV_ALLOWLIST=${SMITH_WORKFLOW_ROLE_DECISION_ENV_ALLOWLIST:-}
    SMITH_WORKFLOW_ROLE_DECISION_CWD=${SMITH_WORKFLOW_ROLE_DECISION_CWD:-}
    SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS=${SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS:-}
    # Which coder backs the workspace external tools. `smith` (default) binds the
    # real pi-SDK workspace agent (#3) for triage_workspace/coding_workspace;
    # `greeting` is the deterministic engineer-head stand-in fallback.
    BASIC_DELIVERY_CODER=${BASIC_DELIVERY_CODER:-smith}
    case "$BASIC_DELIVERY_CODER" in
        smith | greeting) ;;
        *) die "BASIC_DELIVERY_CODER must be smith or greeting, got '$BASIC_DELIVERY_CODER'" ;;
    esac
    SMITH_CODING_AGENT_BIN=${SMITH_CODING_AGENT_BIN:-}
    SMITH_CODING_AGENT_ARGS=${SMITH_CODING_AGENT_ARGS:-}
    if [ -z "$SMITH_CODING_AGENT_ARGS" ]; then
        SMITH_CODING_AGENT_ARGS='--auth chatgpt-oauth'
    fi
    TEMPER_CODING_WORKSPACE_ROOT=${TEMPER_CODING_WORKSPACE_ROOT:-}
    TEMPER_CODING_WORKSPACE_COMMAND=${TEMPER_CODING_WORKSPACE_COMMAND:-}
    TEMPER_CODING_WORKSPACE_REMOTE=${TEMPER_CODING_WORKSPACE_REMOTE:-origin}
    TEMPER_CODING_WORKSPACE_PUSH=${TEMPER_CODING_WORKSPACE_PUSH:-1}
    TEMPER_CODING_WORKSPACE_PR_LABELS=${TEMPER_CODING_WORKSPACE_PR_LABELS:-implementation}

    # Single repo only: this example is deliberately one converging happy path.
    REPO="$OWNER/$NAME"
    validate_repo_path "$REPO"
    WORKER_REPO_ARGS="--repo $REPO"

    # Resolve the workflow file. A relative WORKFLOW_FILE is taken relative to
    # config/; an absolute path is used verbatim.
    case "$WORKFLOW_FILE" in
        /*) WORKFLOW_PATH="$WORKFLOW_FILE" ;;
        *)  WORKFLOW_PATH="$CONFIG_DIR/$WORKFLOW_FILE" ;;
    esac
    [ -f "$WORKFLOW_PATH" ] || die "workflow file not found: $WORKFLOW_PATH (set WORKFLOW_FILE in config/temper.env)"

    # Resolve the thin intake body the same way: a relative path is taken
    # relative to config/, an absolute path is used verbatim.
    case "$INTAKE_BODY_FILE" in
        /*) INTAKE_BODY_PATH="$INTAKE_BODY_FILE" ;;
        *)  INTAKE_BODY_PATH="$CONFIG_DIR/$INTAKE_BODY_FILE" ;;
    esac
    [ -f "$INTAKE_BODY_PATH" ] || die "intake body file not found: $INTAKE_BODY_PATH (set INTAKE_BODY_FILE in config/temper.env)"

    # Cap the Go runtime of the spawned forgejo + forgejo-runner (lesson 0009).
    # Exported so both Go processes inherit it; harmless for the Rust workers.
    if [ -n "$TEMPER_FORGEJO_GOMAXPROCS" ]; then
        export GOMAXPROCS="$TEMPER_FORGEJO_GOMAXPROCS"
    fi

    # Derive host/port from BASE_URL (http://host:port).
    _hostport=${BASE_URL#*://}
    _hostport=${_hostport%%/*}
    HOST=${_hostport%%:*}
    case "$_hostport" in
        *:*) PORT=${_hostport##*:} ;;
        *)   PORT=3000 ;;
    esac
}

# Smith owns provider/auth validation for role decision responders.

# --- Binaries -----------------------------------------------------------------

resolve_binaries() {
    WORKER_BIN=${TEMPER_WORKER_BIN:-$WORKSPACE_ROOT/target/debug/temper-worker}
    PROVISION_BIN=${TEMPER_PROVISION_BIN:-$WORKSPACE_ROOT/target/debug/temper-provision-forgejo}
    TRIGGER_BIN=${TEMPER_TRIGGER_BIN:-$WORKSPACE_ROOT/target/debug/temper-trigger-forgejo}

    # Keep the demo entry point self-healing after source changes. Cargo is a
    # cheap no-op when the development binaries are already current; skipping
    # this is an explicit operator choice for prebuilt/current binaries.
    if [ "${TEMPER_SKIP_BUILD:-0}" != "1" ]; then
        log "ensuring development binaries are current (cargo build -p $TEMPER_BUILD_PACKAGE)..."
        ( cd "$WORKSPACE_ROOT" && cargo build -p "$TEMPER_BUILD_PACKAGE" ) \
            || die 'cargo build failed'
    fi

    [ -x "$WORKER_BIN" ] || die "worker binary not found: $WORKER_BIN"
    [ -x "$PROVISION_BIN" ] || die "provision binary not found: $PROVISION_BIN"
    [ -x "$TRIGGER_BIN" ] || die "trigger binary not found: $TRIGGER_BIN"

    # This example REQUIRES the W1 (--workflow) and W2 (intake_author) Temper
    # support: it ships its own 3-role spec and seeds intake as the site admin.
    # Refuse to run against a stale Temper that lacks --workflow.
    _provision_help=$("$PROVISION_BIN" --help 2>&1 || true)
    case "$_provision_help" in
        *--workflow*--seed-intake*--seed-only*) ;;
        *) die "provision binary is stale or incompatible: $PROVISION_BIN does not advertise --workflow/--seed-intake/--seed-only. The basic-delivery example needs Temper's W1 (runtime --workflow) + W2 (intake_author) support, plus the --seed-only entry-issue pass that lets the intake be filed after the workers are up. Re-run without TEMPER_SKIP_BUILD=1 or rebuild the Temper entry-point package with cargo build -p $TEMPER_BUILD_PACKAGE." ;;
    esac
    _worker_help=$("$WORKER_BIN" --help 2>&1 || true)
    case "$_worker_help" in
        *--workflow*) ;;
        *) die "worker binary is stale or incompatible: $WORKER_BIN does not advertise --workflow. The basic-delivery example needs Temper's W1 (runtime --workflow) support. Re-run without TEMPER_SKIP_BUILD=1 or rebuild the Temper entry-point package with cargo build -p $TEMPER_BUILD_PACKAGE." ;;
    esac

    # Pinned Forgejo + runner: env override, else the cached pinned path.
    FORGEJO_BIN=${TEMPER_FORGEJO_BINARY:-$WORKSPACE_ROOT/.cache/forgejo/forgejo-$FORGEJO_VERSION-linux-amd64}
    RUNNER_BIN=${TEMPER_FORGEJO_RUNNER_BINARY:-$WORKSPACE_ROOT/.cache/forgejo/forgejo-runner-$FORGEJO_RUNNER_VERSION-linux-amd64}
    [ -x "$FORGEJO_BIN" ] || die "forgejo binary not found: $FORGEJO_BIN
       Set TEMPER_FORGEJO_BINARY, or pre-stage the pinned binary in .cache/forgejo/
       with: cargo test -p temper-forgejo-fixture --test cache -- --ignored"
    [ -x "$RUNNER_BIN" ] || die "forgejo-runner binary not found: $RUNNER_BIN
       Set TEMPER_FORGEJO_RUNNER_BINARY, or pre-stage the pinned binary in .cache/forgejo/
       with: cargo test -p temper-forgejo-fixture --test cache -- --ignored"

    ROLE_DECISION_ARGS=
    ROLE_DECISION_ARGS_JSON=$SMITH_WORKFLOW_ROLE_DECISION_ARGS_JSON
    ROLE_DECISION_ENV_ALLOWLIST=$SMITH_WORKFLOW_ROLE_DECISION_ENV_ALLOWLIST
    case "$BASIC_DELIVERY_ROLE_DECISION" in
        smith | "") resolve_smith_workflow_role_decision ;;
        greeting) resolve_greeting_workflow_role_decision ;;
        *) die "unknown BASIC_DELIVERY_ROLE_DECISION '$BASIC_DELIVERY_ROLE_DECISION' (expected smith or greeting)" ;;
    esac
}

resolve_greeting_workflow_role_decision() {
    GREETING_ROLE_DECISION_BIN=$SCRIPT_DIR/tools/greeting-role-decision.sh
    [ -f "$GREETING_ROLE_DECISION_BIN" ] || die "greeting role-decision script not found: $GREETING_ROLE_DECISION_BIN"
    [ -x "$GREETING_ROLE_DECISION_BIN" ] || die "greeting role-decision script is not executable: $GREETING_ROLE_DECISION_BIN"

    ROLE_DECISION_ARGS="--role-decision-command $GREETING_ROLE_DECISION_BIN"
    [ -n "$SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS" ] && ROLE_DECISION_ARGS="$ROLE_DECISION_ARGS --role-decision-timeout-secs $SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS"
    ROLE_DECISION_ARGS_JSON='[]'
    ROLE_DECISION_ENV_ALLOWLIST=
    log "role decisions: greeting deterministic stand-in ($GREETING_ROLE_DECISION_BIN)"

    if [ "$BASIC_DELIVERY_CODER" = "greeting" ]; then
        log "coding workspace: greeting stand-in selected (BASIC_DELIVERY_CODER=greeting); skipping Smith coding agent build"
        return 0
    fi
    resolve_smith_coding_agent
}

resolve_smith_workflow_role_decision() {
    SMITH_ROLE_DECISION_BIN=${SMITH_WORKFLOW_ROLE_DECISION_BIN:-$SMITH_WORKSPACE_ROOT/target/debug/smith-workflow-role-decision}
    if [ "${TEMPER_SKIP_BUILD:-0}" != "1" ]; then
        log "ensuring Smith workflow-role decision responder is current (cargo build -p $SMITH_BUILD_PACKAGE --bin smith-workflow-role-decision)..."
        ( cd "$SMITH_WORKSPACE_ROOT" && cargo build -p "$SMITH_BUILD_PACKAGE" --bin smith-workflow-role-decision ) \
            || die 'Smith cargo build failed'
    fi
    [ -x "$SMITH_ROLE_DECISION_BIN" ] || die "Smith workflow-role decision binary not found: $SMITH_ROLE_DECISION_BIN"

    ROLE_DECISION_ARGS="--role-decision-command $SMITH_ROLE_DECISION_BIN"
    [ -n "$SMITH_WORKFLOW_ROLE_DECISION_CWD" ] && ROLE_DECISION_ARGS="$ROLE_DECISION_ARGS --role-decision-cwd $SMITH_WORKFLOW_ROLE_DECISION_CWD"
    [ -n "$SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS" ] && ROLE_DECISION_ARGS="$ROLE_DECISION_ARGS --role-decision-timeout-secs $SMITH_WORKFLOW_ROLE_DECISION_TIMEOUT_SECS"
    log "role decisions: Smith process responder ($SMITH_ROLE_DECISION_BIN)"

    # The workspace external tools (triage_workspace / coding_workspace) run the
    # real pi-SDK agent (#3) unless the operator opted into the deterministic
    # greeting stand-in. Build it here in the same Smith resolve step so a source
    # change is self-healing; greeting mode skips it.
    if [ "$BASIC_DELIVERY_CODER" = "greeting" ]; then
        log "coding workspace: greeting stand-in selected (BASIC_DELIVERY_CODER=greeting); skipping Smith coding agent build"
        return 0
    fi
    resolve_smith_coding_agent
}

resolve_smith_coding_agent() {
    SMITH_CODING_AGENT_BIN=${SMITH_CODING_AGENT_BIN:-$SMITH_WORKSPACE_ROOT/target/debug/smith-coding-agent}
    if [ "${TEMPER_SKIP_BUILD:-0}" != "1" ]; then
        log "ensuring Smith coding-workspace agent is current (cargo build -p $SMITH_BUILD_PACKAGE --bin smith-coding-agent)..."
        ( cd "$SMITH_WORKSPACE_ROOT" && cargo build -p "$SMITH_BUILD_PACKAGE" --bin smith-coding-agent ) \
            || die 'Smith coding-agent cargo build failed'
    fi
    [ -x "$SMITH_CODING_AGENT_BIN" ] || die "Smith coding-workspace agent binary not found: $SMITH_CODING_AGENT_BIN"
    log "coding workspace: Smith pi-SDK agent ($SMITH_CODING_AGENT_BIN $SMITH_CODING_AGENT_ARGS)"
}

# --- Forgejo server -----------------------------------------------------------

write_app_ini() {
    mkdir -p "$FORGEJO_DATA/custom/conf" "$FORGEJO_DATA/data" \
        "$FORGEJO_DATA/log" "$FORGEJO_DATA/repos"
    cat >"$APP_INI" <<EOF
APP_NAME = Basic Delivery Example
RUN_MODE = prod
WORK_PATH = $FORGEJO_DATA

[server]
PROTOCOL = http
HTTP_ADDR = $HOST
HTTP_PORT = $PORT
ROOT_URL = $BASE_URL/
DISABLE_SSH = true
START_SSH_SERVER = false
OFFLINE_MODE = true
APP_DATA_PATH = $FORGEJO_DATA/data

[database]
DB_TYPE = sqlite3
PATH = $FORGEJO_DATA/data/forgejo.db
LOG_SQL = false

[repository]
ROOT = $FORGEJO_DATA/repos

[log]
ROOT_PATH = $FORGEJO_DATA/log
MODE = console
LEVEL = error

[security]
INSTALL_LOCK = true
SECRET_KEY = basic-delivery-example-not-for-production
INTERNAL_TOKEN = basic-delivery-example-internal-not-for-production

[service]
DISABLE_REGISTRATION = true
REQUIRE_SIGNIN_VIEW = false

[mailer]
ENABLED = false

[webhook]
ALLOWED_HOST_LIST = 127.0.0.1,localhost

[actions]
ENABLED = true
EOF
}

# Runs a `forgejo` admin/CLI subcommand against the instance config.
forgejo_cli() {
    GITEA_WORK_DIR="$FORGEJO_DATA" "$FORGEJO_BIN" --config "$APP_INI" "$@"
}

boot_server() {
    log "booting Forgejo at $BASE_URL ..."
    if curl -fsS "$BASE_URL/api/v1/version" >/dev/null 2>&1; then
        die "Forgejo already responds at $BASE_URL before this run started. Stop the existing run first, or clean up orphaned forgejo processes."
    fi
    write_app_ini
    forgejo_cli migrate >"$LOG_DIR/forgejo-migrate.log" 2>&1 \
        || die "forgejo migrate failed (see logs/forgejo-migrate.log)"

    GITEA_WORK_DIR="$FORGEJO_DATA" "$FORGEJO_BIN" --config "$APP_INI" web \
        >"$LOG_DIR/forgejo.log" 2>&1 &
    SERVER_PID=$!
    echo "$SERVER_PID" >"$SERVER_PID_FILE"

    _i=0
    until curl -fsS "$BASE_URL/api/v1/version" >/dev/null 2>&1; do
        kill -0 "$SERVER_PID" 2>/dev/null \
            || die "forgejo exited during startup (see logs/forgejo.log)"
        _i=$((_i + 1))
        [ "$_i" -gt 300 ] && die "forgejo did not become ready (see logs/forgejo.log)"
        sleep 0.2 2>/dev/null || sleep 1
    done
    log "Forgejo ready (pid $SERVER_PID)"
}

ensure_secret_file() {
    _file=$1
    [ -f "$_file" ] && return 0
    umask 077
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -hex 32 >"$_file"
    else
        dd if=/dev/urandom bs=32 count=1 2>/dev/null | od -An -tx1 | tr -d ' \n' >"$_file"
        printf '\n' >>"$_file"
    fi
}

boot_runner() {
    log 'registering host-mode forgejo-runner ...'
    mkdir -p "$RUNNER_DIR"
    _reg_token=$(forgejo_cli actions generate-runner-token | tr -d '[:space:]')
    [ -n "$_reg_token" ] || die 'failed to mint a runner registration token'
    ( cd "$RUNNER_DIR" && "$RUNNER_BIN" register --no-interactive \
        --instance "$BASE_URL" --token "$_reg_token" \
        --name "basic-delivery-$$" --labels host:host ) \
        >"$LOG_DIR/runner-register.log" 2>&1 \
        || die 'forgejo-runner register failed (see logs/runner-register.log)'

    ( cd "$RUNNER_DIR" && "$RUNNER_BIN" daemon ) >"$LOG_DIR/runner.log" 2>&1 &
    RUNNER_PID=$!
    echo "$RUNNER_PID" >"$RUNNER_PID_FILE"
    log "runner daemon running (pid $RUNNER_PID)"
}

wait_for_log_line() {
    _file=$1
    _needle=$2
    _pid=$3
    _label=$4
    _i=0
    while ! grep -q "$_needle" "$_file" 2>/dev/null; do
        kill -0 "$_pid" 2>/dev/null || die "$_label exited before readiness (see $_file)"
        _i=$((_i + 1))
        [ "$_i" -gt 100 ] && die "$_label did not become ready (see $_file)"
        sleep_short
    done
}

wait_for_socket() {
    _socket=$1
    _pid=$2
    _label=$3
    _i=0
    while [ ! -S "$_socket" ]; do
        kill -0 "$_pid" 2>/dev/null || die "$_label exited before creating wake socket $_socket"
        _i=$((_i + 1))
        [ "$_i" -gt 100 ] && die "$_label did not create wake socket $_socket"
        sleep_short
    done
}

# Waits until a worker has finished its FIRST (startup) tick, i.e. has scanned
# the repo once and gone idle. A wake socket existing only means the worker is
# accepting wakes, not that its initial scan is done; filing the intake in that
# window lets a startup tick race the seed and do the work itself. Waiting here
# makes the intake's creation webhook the unambiguous cause of the first actions:
# every worker is past its startup scan (which found no intake), so filing the
# issue is what WAKES the bot into marking it untriaged and the architect into
# triaging it. Non-fatal: a worker that never reports an initial tick only
# forfeits that determinism, it does not abort the demo.
wait_for_worker_idle() {
    _log=$1
    _label=$2
    _i=0
    while ! grep -q 'completed tick trigger=initial' "$_log" 2>/dev/null; do
        _i=$((_i + 1))
        if [ "$_i" -gt 150 ]; then
            log "  note: $_label did not report an initial tick in time; filing intake anyway"
            return 0
        fi
        sleep_short
    done
}

boot_trigger() {
    [ "$WEBHOOKS" = "1" ] || return 0
    log "starting webhook trigger at $TRIGGER_BIND ..."
    ensure_secret_file "$WEBHOOK_SECRET_FILE"
    ensure_secret_file "$WAKE_SECRET_FILE"
    mkdir -p "$WAKE_DIR"
    : >"$LOG_DIR/trigger.log"
    "$TRIGGER_BIN" --bind "$TRIGGER_BIND" \
        --webhook-secret-file "$WEBHOOK_SECRET_FILE" \
        --wake-secret-file "$WAKE_SECRET_FILE" \
        --wake-dir "$WAKE_DIR" \
        >>"$LOG_DIR/trigger.log" 2>&1 &
    TRIGGER_PID=$!
    echo "$TRIGGER_PID" >"$TRIGGER_PID_FILE"
    wait_for_log_line "$LOG_DIR/trigger.log" 'listening on' "$TRIGGER_PID" 'webhook trigger'
    log "trigger running (pid $TRIGGER_PID; logs/trigger.log)"
}

# --- Provision + seed ---------------------------------------------------------

repo_slug() {
    repo_name "$1" | tr -c '[:alnum:]' '-' | tr '[:upper:]' '[:lower:]' | sed 's/^-*//;s/-*$//'
}

bootstrap_and_provision() {
    log 'bootstrapping admin + provisioning the single repo against the bundled 3-role workflow ...'
    # Create the admin (tolerate a pre-existing one on a re-run), then mint an
    # all-scoped token. The token stays in a shell variable; it is never echoed
    # and reaches the provision steps only via the environment. It is also kept
    # for the later seed_intake pass: the workflow's intake_author = site_admin
    # means the intake issue is authored by THIS admin (the "external filer").
    # This pass deliberately runs with --seed-intake no: it sets up the
    # org/users/repo/labels/CI and registers the webhook but does NOT file the
    # intake issue, so the workers + wake trigger can come up first and the
    # issue's creation webhook is what wakes them (see seed_intake).
    forgejo_cli admin user create --username "$ADMIN_USER" --password "$ADMIN_PASSWORD" \
        --email "$ADMIN_EMAIL" --admin --must-change-password=false \
        >"$LOG_DIR/admin-create.log" 2>&1 || true
    ADMIN_TOKEN=$(forgejo_cli admin user generate-access-token --username "$ADMIN_USER" \
        --scopes all --raw | tr -d '[:space:]')
    [ -n "$ADMIN_TOKEN" ] || die 'failed to mint an admin access token'

    _webhook_args=
    if [ "$WEBHOOKS" = "1" ]; then
        _webhook_args="--webhook-url $WEBHOOK_URL --webhook-secret-file $WEBHOOK_SECRET_FILE"
    fi
    : >"$LOG_DIR/provision.log"

    _owner=$(repo_owner "$REPO")
    _name=$(repo_name "$REPO")
    log "provisioning $REPO (labels + CI + webhook; intake filed after the workers are up) ..."
    # _webhook_args intentionally word-split: POSIX sh has no arrays and the
    # paths above are controlled by this script/config. --seed-intake no holds
    # the intake issue back for the post-launch seed_intake pass.
    # shellcheck disable=SC2086
    _status=$(TEMPER_FORGEJO_ADMIN_TOKEN="$ADMIN_TOKEN" "$PROVISION_BIN" \
        --base-url "$BASE_URL" --owner "$_owner" --name "$_name" --out "$ROLES_ENV" \
        --workflow "$WORKFLOW_PATH" --seed-intake no \
        $_webhook_args) \
        || die "provisioning $REPO failed"

    {
        printf 'repo=%s %s\n' "$REPO" "$_status"
        if [ "$WEBHOOKS" = "1" ]; then
            printf 'repo=%s webhook registered url=%s\n' "$REPO" "$WEBHOOK_URL"
        else
            printf 'repo=%s webhook disabled\n' "$REPO"
        fi
    } >>"$LOG_DIR/provision.log"
    log "$_status"
    [ "$WEBHOOKS" = "1" ] && log "  webhook registered for $REPO ($WEBHOOK_URL)"

    [ -f "$ROLES_ENV" ] || die "provision did not write $ROLES_ENV"
    # shellcheck disable=SC1090
    . "$ROLES_ENV"
}

# Files the single site-admin intake issue AFTER the worker pool and the wake
# trigger are up. This is a second, seed-only provision pass (--seed-only): the
# org/users/repo/labels/CI and the webhook already exist from
# bootstrap_and_provision, so this only creates the issue. Because it is filed
# while every worker's wake socket and the trigger are already listening, the
# issue's creation webhook is what WAKES the workers — the bot stamps it
# untriaged and the architect triages it — proving the wake path rather than the
# workers only discovering a pre-seeded issue on their next (long) poll. The
# seed title/body come from INTAKE_TITLE + the bundled INTAKE_BODY_FILE and are
# deliberately THIN (intent only) so the architect's triage rewrite has real
# design work to do. The admin token (intake_author = site_admin) is reused from
# bootstrap_and_provision via the environment; it is never echoed.
seed_intake() {
    [ -n "${ADMIN_TOKEN:-}" ] || die 'seed_intake: no admin token (bootstrap_and_provision must run first)'
    _owner=$(repo_owner "$REPO")
    _name=$(repo_name "$REPO")
    # With webhooks on, wait for the whole pool to finish its startup scan and go
    # idle before filing, so the intake's creation webhook — not a startup tick
    # racing the seed — is what wakes the workers into acting on issue #1.
    if [ "$WEBHOOKS" = "1" ]; then
        log 'waiting for the worker pool to finish its initial scan before filing intake ...'
        _idle_roles=$(sed -n "s/^TEMPER_FORGEJO_USER_[A-Z0-9_]*='\(.*\)'\$/\1/p" "$ROLES_ENV")
        for _idle_role in $_idle_roles; do
            wait_for_worker_idle "$LOG_DIR/$_idle_role.log" "role:$_idle_role"
        done
        wait_for_worker_idle "$LOG_DIR/mechanical.log" 'mechanical'
    fi
    log 'filing the site-admin intake issue now that the workers + wake trigger are ready ...'
    _status=$(TEMPER_FORGEJO_ADMIN_TOKEN="$ADMIN_TOKEN" "$PROVISION_BIN" \
        --base-url "$BASE_URL" --owner "$_owner" --name "$_name" --out "$ROLES_ENV" \
        --workflow "$WORKFLOW_PATH" --seed-only \
        --intake-title "$INTAKE_TITLE" --intake-body-file "$INTAKE_BODY_PATH") \
        || die "seeding intake issue for $REPO failed"

    _issue=$(printf '%s\n' "$_status" | sed -n 's/.*intake issue #\([0-9][0-9]*\).*/\1/p')
    {
        printf 'repo=%s %s\n' "$REPO" "$_status"
        [ -n "$_issue" ] && printf 'repo=%s intake_issue_url=%s/%s/issues/%s\n' "$REPO" "$BASE_URL" "$REPO" "$_issue"
    } >>"$LOG_DIR/provision.log"
    log "$_status"
    [ -n "$_issue" ] && log "  intake issue: $BASE_URL/$REPO/issues/$_issue (filing it should wake the workers)"
}

# --- Demo coding workspace ----------------------------------------------------

# URL-encodes one value for a git-credentials store entry. python3 is already a
# demo dependency (the bundled CI workflow runs it via actions/checkout).
percent_encode() {
    python3 -c 'import sys, urllib.parse; sys.stdout.write(urllib.parse.quote(sys.argv[1], safe=""))' "$1"
}

# Binds a coding workspace so the architect/engineer roles run their declared
# workspace external tools (triage_workspace / coding_workspace) against a real
# checkout. By default it binds the Smith pi-SDK workspace agent (#3): one
# capability/role-aware binary that temper invokes per role with the right
# checkout capability (read-only for the architect, writable for the engineer)
# and the work-item context. Set BASIC_DELIVERY_CODER=greeting for the
# deterministic engineer-head stand-in coder instead.
#
# It only runs when the operator has NOT bound their own coding workspace. It
# also replaces the provisioned commit-message-marker CI with the bundled
# pass-through workflow so a real coder PR head (which carries an ordinary commit
# message, not the demo marker) clears the landing CI gate. Every failure is
# non-fatal: the roles simply stay idle, exactly as with no binding.
setup_demo_coding_workspace() {
    if [ -n "$TEMPER_CODING_WORKSPACE_ROOT" ] || [ -n "$TEMPER_CODING_WORKSPACE_COMMAND" ]; then
        log "coding workspace: respecting operator binding (root=${TEMPER_CODING_WORKSPACE_ROOT:-unset})"
        return 0
    fi

    _key=$(role_env_key engineer)
    eval "_eng_user=\${TEMPER_FORGEJO_USER_${_key}:-}"
    eval "_eng_password=\${TEMPER_FORGEJO_PASSWORD_${_key}:-}"
    if [ -z "$_eng_user" ] || [ -z "$_eng_password" ]; then
        log "coding workspace: no engineer identity in $ROLES_ENV; workspace roles stay idle"
        return 0
    fi

    _ws_dir="$RUN_DIR/coding-workspace"
    _checkout="$_ws_dir/$(repo_slug "$REPO")"
    _creds="$_ws_dir/git-credentials"
    _remote="$BASE_URL/$REPO.git"
    _without_scheme=${BASE_URL#*://}

    log "coding workspace: cloning $REPO for the bound coding workspace ..."
    rm -rf "$_ws_dir"
    mkdir -p "$_ws_dir"
    if ! git clone --quiet "$_remote" "$_checkout" >>"$LOG_DIR/coding-workspace.log" 2>&1; then
        log "coding workspace: clone of $REPO failed (see logs/coding-workspace.log); workspace roles stay idle"
        return 0
    fi
    # Configure the checkout for the local-git coding workspace push. Guarded so
    # an unexpected git/disk failure degrades to "roles idle" rather than
    # aborting the whole launch under set -e.
    if ! { ( umask 077; printf 'http://%s:%s@%s\n' "$(percent_encode "$_eng_user")" "$(percent_encode "$_eng_password")" "$_without_scheme" >"$_creds" ) \
        && git -C "$_checkout" config user.email "$_eng_user@example.invalid" \
        && git -C "$_checkout" config user.name 'Temper Engineer' \
        && git -C "$_checkout" config credential.helper "store --file=$_creds"; }; then
        log "coding workspace: could not configure the checkout; workspace roles stay idle"
        return 0
    fi

    # Replace the provisioned marker CI with the bundled pass-through workflow so
    # the engineer's ordinary-message PR head clears the landing CI gate. The PR
    # branches off this base, so its push event runs the pass-through workflow.
    _base=$(git -C "$_checkout" rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'main')
    mkdir -p "$_checkout/.forgejo/workflows"
    if cp "$CONFIG_DIR/ci.yml" "$_checkout/.forgejo/workflows/ci.yml" \
        && ! git -C "$_checkout" diff --quiet -- .forgejo/workflows/ci.yml; then
        if git -C "$_checkout" add .forgejo/workflows/ci.yml \
            && git -C "$_checkout" commit --quiet -m 'ci: use basic-delivery demo CI workflow' >>"$LOG_DIR/coding-workspace.log" 2>&1 \
            && git -C "$_checkout" push --quiet origin "HEAD:$_base" >>"$LOG_DIR/coding-workspace.log" 2>&1; then
            log "coding workspace: applied bundled CI to $REPO@$_base"
        else
            log "coding workspace: could not apply bundled CI to $REPO (see logs/coding-workspace.log); landing CI gate may not pass"
        fi
    fi

    TEMPER_CODING_WORKSPACE_ROOT="$_checkout"
    if [ "$BASIC_DELIVERY_CODER" = "greeting" ]; then
        TEMPER_CODING_WORKSPACE_COMMAND="$SCRIPT_DIR/tools/greeting-coder.sh"
        log "coding workspace: bound deterministic greeting stand-in for $REPO (root=$_checkout)"
    else
        # The Smith pi-SDK workspace agent (#3). temper invokes it per role with
        # the checkout capability and writes the work-item context to the file it
        # reads; the engineer leaves a product diff, the architect emits the
        # single ready_code verdict. Resolved + built in
        # resolve_smith_workflow_role_decision.
        TEMPER_CODING_WORKSPACE_COMMAND="$SMITH_CODING_AGENT_BIN $SMITH_CODING_AGENT_ARGS"
        log "coding workspace: bound Smith pi-SDK agent for $REPO (root=$_checkout, command=$SMITH_CODING_AGENT_BIN $SMITH_CODING_AGENT_ARGS)"
    fi
}

# --- Workers ------------------------------------------------------------------

# Uppercases a role id and replaces non-alphanumerics with `_` (matching the
# provision binary's env_role_key), yielding the secrets-file variable suffix.
role_env_key() {
    printf '%s' "$1" | tr '[:lower:]' '[:upper:]' | tr -c 'A-Z0-9' '_'
}

# Resolves the provisioned `bot` automation identity from the secrets file. The
# mechanical worker runs as the bot: it joins the Owners team so it can land
# (merge) CI-green PRs with no approving review, and its web-UI credentials let
# it read Forgejo 7.0.x Actions status for the landing gate (ADR 0019). The
# setup-only site admin never participates in the workflow.
resolve_bot_identity() {
    [ -f "$ROLES_ENV" ] || die "missing $ROLES_ENV"
    # shellcheck disable=SC1090
    . "$ROLES_ENV"
    BOT_USER=${TEMPER_FORGEJO_BOT_USER:-}
    BOT_TOKEN=${TEMPER_FORGEJO_BOT_TOKEN:-}
    BOT_PASSWORD=${TEMPER_FORGEJO_BOT_PASSWORD:-}
    [ -n "$BOT_USER" ] || die "automation user 'bot' has no username in $ROLES_ENV"
    [ "$BOT_USER" = "bot" ] || die "automation user must be 'bot' in $ROLES_ENV, got '$BOT_USER'"
    [ -n "$BOT_TOKEN" ] || die "automation user 'bot' has no token in $ROLES_ENV"
    [ -n "$BOT_PASSWORD" ] || die "automation user 'bot' has no password in $ROLES_ENV"
}

launch_role_worker() {
    _role=$1
    _key=$(role_env_key "$_role")
    eval "_user=\${TEMPER_FORGEJO_USER_${_key}:-}"
    eval "_token=\${TEMPER_FORGEJO_TOKEN_${_key}:-}"
    eval "_password=\${TEMPER_FORGEJO_PASSWORD_${_key}:-}"
    [ -n "$_token" ] || die "no token for role '$_role' in $ROLES_ENV"

    _wake_args=
    _wake_socket=
    if [ "$WEBHOOKS" = "1" ]; then
        _wake_socket="$WAKE_DIR/$_role.sock"
        _wake_args="--wake-socket $_wake_socket --wake-secret-file $WAKE_SECRET_FILE"
    fi
    # Per-role secrets are literal env-assignment prefixes (never on argv).
    # WORKER_REPO_ARGS / ROLE_DECISION_ARGS / _wake_args intentionally word-split
    # (POSIX has no arrays); repo values are owner/name with no spaces.
    # shellcheck disable=SC2086
    TEMPER_FORGEJO_TOKEN="$_token" \
    TEMPER_FORGEJO_USERNAME="$_user" \
    TEMPER_FORGEJO_PASSWORD="$_password" \
    TEMPER_WORKFLOW_FILE="$WORKFLOW_PATH" \
    TEMPER_CODING_WORKSPACE_ROOT="$TEMPER_CODING_WORKSPACE_ROOT" \
    TEMPER_CODING_WORKSPACE_COMMAND="$TEMPER_CODING_WORKSPACE_COMMAND" \
    TEMPER_CODING_WORKSPACE_REMOTE="$TEMPER_CODING_WORKSPACE_REMOTE" \
    TEMPER_CODING_WORKSPACE_PUSH="$TEMPER_CODING_WORKSPACE_PUSH" \
    TEMPER_CODING_WORKSPACE_PR_LABELS="$TEMPER_CODING_WORKSPACE_PR_LABELS" \
    TEMPER_WORKER_ROLE_DECISION_ARGS_JSON="$ROLE_DECISION_ARGS_JSON" \
    TEMPER_WORKER_ROLE_DECISION_ENV_ALLOWLIST="$ROLE_DECISION_ENV_ALLOWLIST" \
        "$WORKER_BIN" \
        --backend forgejo --base-url "$BASE_URL" $WORKER_REPO_ARGS \
        --workflow "$WORKFLOW_PATH" \
        --kind role --role "$_role" --user "$_user" \
        $ROLE_DECISION_ARGS \
        --poll-ms "$POLL_MS" --stop-file "$STOP_FILE" --run-secs "$RUN_SECS" \
        $_wake_args \
        >"$LOG_DIR/$_role.log" 2>&1 &
    _pid=$!
    echo "$_pid" >>"$WORKERS_PID_FILE"
    if [ "$WEBHOOKS" = "1" ]; then
        wait_for_socket "$_wake_socket" "$_pid" "role:$_role"
    fi
    log "  role:$_role -> pid $_pid (logs/$_role.log)"
}

launch_workers() {
    : >"$WORKERS_PID_FILE"
    # Derive the role list from the provisioned secrets file (one TEMPER_FORGEJO_
    # USER_<KEY>=<role> per role binding) — never hardcoded. For the basic-delivery
    # workflow this naturally yields just architect + engineer (the mechanical
    # role has no queues and no role worker; it is serviced by the bot below).
    _roles=$(sed -n "s/^TEMPER_FORGEJO_USER_[A-Z0-9_]*='\(.*\)'\$/\1/p" "$ROLES_ENV")
    [ -n "$_roles" ] || die "no roles found in $ROLES_ENV"

    log "launching role workers (production binary, $BASIC_DELIVERY_ROLE_DECISION process decisions) ..."
    # The intake issue is filed only after this whole pool is up (see
    # seed_intake), so every wake listener must already exist before it lands.
    # Start every other wake listener first, then launch architect last, so both
    # the intake-created webhook and the first role-handoff webhook find all
    # downstream sockets even with a long poll.
    _architect_role=
    for _r in $_roles; do
        if [ "$_r" = "architect" ]; then
            _architect_role=$_r
        else
            launch_role_worker "$_r"
        fi
    done

    # One mechanical reconciler runs the controller plane AND lands CI-green PRs.
    # It runs as the provisioned `bot` automation user (Owners team) with its
    # REST token plus web-UI credentials for the ADR-0019 CI read fallback, and
    # polls CI status on the short CI_STATUS_POLL_MS cadence (Forgejo 7.0.x does
    # not webhook on Actions completion), backing off to IDLE_POLL_MAX_MS while
    # idle. Wakes still interrupt immediately. With no reviewer/owner the bot is
    # the SOLE landing authority: it merges once the CI gate is green.
    resolve_bot_identity
    _wake_args=
    _wake_socket=
    if [ "$WEBHOOKS" = "1" ]; then
        _wake_socket="$WAKE_DIR/mechanical.sock"
        _wake_args="--wake-socket $_wake_socket --wake-secret-file $WAKE_SECRET_FILE"
    fi
    (
        printf 'temper-worker: mechanical repositories=%s automation_user=%s ci_reader=bot idle_poll_max_ms=%s\n' "$REPO" "$BOT_USER" "$IDLE_POLL_MAX_MS"
        # shellcheck disable=SC2086
        TEMPER_FORGEJO_TOKEN="$BOT_TOKEN" \
        TEMPER_FORGEJO_USERNAME="$BOT_USER" \
        TEMPER_FORGEJO_PASSWORD="$BOT_PASSWORD" \
        TEMPER_WORKFLOW_FILE="$WORKFLOW_PATH" \
            "$WORKER_BIN" \
            --backend forgejo --base-url "$BASE_URL" $WORKER_REPO_ARGS \
            --workflow "$WORKFLOW_PATH" \
            --kind mechanical \
            --poll-ms "$CI_STATUS_POLL_MS" --idle-poll-max-ms "$IDLE_POLL_MAX_MS" \
            --stop-file "$STOP_FILE" --run-secs "$RUN_SECS" \
            $_wake_args
    ) >"$LOG_DIR/mechanical.log" 2>&1 &
    _pid=$!
    echo "$_pid" >>"$WORKERS_PID_FILE"
    if [ "$WEBHOOKS" = "1" ]; then
        wait_for_socket "$_wake_socket" "$_pid" 'mechanical'
    fi
    log "  mechanical -> pid $_pid as bot $BOT_USER (logs/mechanical.log)"

    if [ -n "$_architect_role" ]; then
        launch_role_worker "$_architect_role"
    fi
}

# --- Webhook validation -------------------------------------------------------

count_matches() {
    _pattern=$1
    _file=$2
    _count=$(grep -c "$_pattern" "$_file" 2>/dev/null || true)
    [ -n "$_count" ] || _count=0
    printf '%s\n' "$_count"
}

validate_contains() {
    _file=$1
    _pattern=$2
    _description=$3
    if grep -F -q "$_pattern" "$_file" 2>/dev/null; then
        log "ok: $_description"
        return 0
    fi
    log "missing: $_description (looked in $_file)"
    return 1
}

# Confirms the mechanical landing worker has the bot automation credentials it
# needs to merge CI-green PRs and read Forgejo 7.0.x Actions status (ADR 0019).
validate_mechanical_bot_config() {
    _ok=0
    if [ ! -f "$ROLES_ENV" ]; then
        log "missing: $ROLES_ENV not found; cannot confirm bot automation credentials"
        log 'diagnosis: Forgejo 7.0.x CI reads need web-UI credentials for the mechanical landing worker (ADR 0019)'
        return 1
    fi
    # shellcheck disable=SC1090
    . "$ROLES_ENV"
    if [ "${TEMPER_FORGEJO_BOT_USER:-}" = "bot" ] && [ -n "${TEMPER_FORGEJO_BOT_TOKEN:-}" ] \
        && [ -n "${TEMPER_FORGEJO_BOT_PASSWORD:-}" ]; then
        log 'ok: bot automation token + web-UI credentials present for the mechanical worker'
    else
        log "missing: bot automation user token/username/password in $ROLES_ENV"
        log 'diagnosis: provision the bot user and launch mechanical with its REST token plus TEMPER_FORGEJO_USERNAME/TEMPER_FORGEJO_PASSWORD for landing and the ADR-0019 CI read fallback'
        _ok=1
    fi
    return "$_ok"
}

# Checks the mechanical worker startup identity and that no CI read fallback
# error (missing/unusable web-UI credentials) was reported for the landing gate.
validate_mechanical_ci_log() {
    _ok=0
    _mechanical_log="$LOG_DIR/mechanical.log"
    if [ ! -f "$_mechanical_log" ]; then
        log 'missing: logs/mechanical.log exists for mechanical CI-read diagnostics'
        return 1
    fi
    if ! grep -F -q 'ci_reader=bot' "$_mechanical_log" 2>/dev/null; then
        log 'missing: mechanical worker startup did not record the bot automation identity'
        log 'diagnosis: restart with the updated launcher so mechanical runs as the bot user for landing and CI reads'
        _ok=1
    fi
    if grep -F -q "$CI_FALLBACK_MISSING_CREDENTIALS" "$_mechanical_log" 2>/dev/null; then
        log 'missing: mechanical worker reported missing Forgejo web-UI credentials for CI reads'
        log 'diagnosis: the landing queue needs native CI; pass the bot TEMPER_FORGEJO_USERNAME/TEMPER_FORGEJO_PASSWORD to the mechanical worker (ADR 0019)'
        _ok=1
    fi
    if grep -F -q "$CI_FALLBACK_LOGIN_FAILED" "$_mechanical_log" 2>/dev/null; then
        log 'missing: mechanical worker could not log in to Forgejo web UI for CI reads'
        log 'diagnosis: verify the bot automation credentials in secrets/roles.env'
        _ok=1
    fi
    if [ "$_ok" -eq 0 ]; then
        log 'ok: mechanical CI read fallback reported no missing/unusable web-UI credentials'
    fi
    return "$_ok"
}

cmd_validate_webhooks() {
    load_config
    _ok=0
    _trigger_log="$LOG_DIR/trigger.log"
    _provision_log="$LOG_DIR/provision.log"

    [ -d "$LOG_DIR" ] || die "no logs/ directory yet; start a run first"
    log "validating webhook wake logs under $LOG_DIR"
    log "configured repo: $REPO"
    log "configured POLL_MS=$POLL_MS CI_STATUS_POLL_MS=$CI_STATUS_POLL_MS IDLE_POLL_MAX_MS=$IDLE_POLL_MAX_MS; long-poll smoke expects POLL_MS=120000"

    validate_mechanical_bot_config || _ok=1
    validate_mechanical_ci_log || _ok=1

    validate_contains "$_provision_log" 'webhook registered url=' \
        'repo webhook registration recorded' || _ok=1
    validate_contains "$_trigger_log" 'listening on' \
        'trigger reached listening readiness' || _ok=1
    validate_contains "$_trigger_log" 'webhook accepted' \
        'Forgejo delivered at least one accepted webhook' || _ok=1
    validate_contains "$_trigger_log" 'wake_delivery outcome=sent' \
        'trigger found sockets and sent at least one wake batch' || _ok=1

    _accepted=$(count_matches 'webhook accepted' "$_trigger_log")
    _sent=$(count_matches 'wake_delivery outcome=sent' "$_trigger_log")
    _no_sockets=$(count_matches 'wake_delivery outcome=no_sockets' "$_trigger_log")
    _failed=$(count_matches 'wake_send_failed' "$_trigger_log")
    log "trigger summary: accepted=$_accepted sent_batches=$_sent no_socket_batches=$_no_sockets send_failures=$_failed"

    _workers=0
    _consumed=0
    _wake_ticks=0
    _wake_progress=0
    _wake_no_work=0
    for _log in "$LOG_DIR"/*.log; do
        [ -f "$_log" ] || continue
        grep -q 'temper-worker:' "$_log" 2>/dev/null || continue
        _workers=$((_workers + 1))
        _name=${_log##*/}
        if grep -q 'consumed authenticated wake' "$_log" 2>/dev/null; then
            _consumed=$((_consumed + 1))
            _consumed_text=yes
        else
            _consumed_text=no
            _ok=1
        fi
        if grep -q 'completed tick trigger=wake actions=' "$_log" 2>/dev/null; then
            _wake_ticks=$((_wake_ticks + 1))
            _tick_text=yes
        else
            _tick_text=no
            _ok=1
        fi
        if grep -E -q 'completed tick trigger=wake actions=[1-9][0-9]*' "$_log" 2>/dev/null; then
            _wake_progress=$((_wake_progress + 1))
        fi
        if grep -q 'completed tick trigger=wake actions=0' "$_log" 2>/dev/null; then
            _wake_no_work=$((_wake_no_work + 1))
        fi
        log "worker $_name: consumed_wake=$_consumed_text wake_tick=$_tick_text"
    done

    if [ "$_workers" -eq 0 ]; then
        log 'missing: no temper-worker logs found'
        _ok=1
    fi
    if [ "$_wake_progress" -eq 0 ]; then
        log 'missing: no wake-triggered worker tick reported actions>0'
        _ok=1
    fi
    log "worker summary: workers=$_workers consumed=$_consumed wake_ticks=$_wake_ticks wake_progress=$_wake_progress wake_no_work=$_wake_no_work"
    log 'wake_no_work>0 means a worker woke, scanned fresh Forge state, and found no queue item.'

    if [ "$_ok" -eq 0 ]; then
        log 'webhook wake validation passed'
    else
        log 'webhook wake validation failed; inspect logs/provision.log, logs/trigger.log, and worker logs'
    fi
    return "$_ok"
}

# --- Monitor ------------------------------------------------------------------

# Blocks until the stop-file appears, the server dies, or RUN_SECS elapses, so
# the EXIT/INT/TERM trap can tear everything down on Ctrl-C.
monitor() {
    log ''
    log "Forgejo UI:    $BASE_URL  (log in as any provisioned role)"
    log "Worker pool:   architect + engineer + mechanical(bot) scan: $REPO"
    log "Intake issue:  $BASE_URL/$REPO/issues"
    log "Worker logs:   $LOG_DIR/ (role logs include the resolved repo)"
    log 'The intake issue is filed once the pool is up, so its webhook wakes the'
    log 'bot (which marks it untriaged); the architect then triages it to a ready'
    log 'code issue, the engineer opens an implementation PR, CI runs and goes'
    log 'green, and the bot auto-merges it — no human.'
    log ''
    log "Press Ctrl-C (or run '$DISPLAY_SCRIPT stop') to tear everything down."

    _waited=0
    while [ ! -f "$STOP_FILE" ]; do
        sleep 2
        _waited=$((_waited + 2))
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            log 'forgejo server exited; shutting down.'
            break
        fi
        if [ "$_waited" -ge "$RUN_SECS" ]; then
            log "run-secs backstop ($RUN_SECS s) reached; shutting down."
            break
        fi
    done
}

# --- Start --------------------------------------------------------------------

cmd_start() {
    load_config
    resolve_binaries

    if [ -f "$SERVER_PID_FILE" ] && kill -0 "$(cat "$SERVER_PID_FILE" 2>/dev/null)" 2>/dev/null; then
        die "a run appears active (run/server.pid). Stop it first: $DISPLAY_SCRIPT stop"
    fi

    mkdir -p "$RUN_DIR" "$LOG_DIR"
    rm -f "$STOP_FILE"

    # From here on, tear everything down on any exit/interrupt.
    trap cleanup EXIT INT TERM

    boot_server
    boot_runner
    boot_trigger
    bootstrap_and_provision
    setup_demo_coding_workspace
    launch_workers
    seed_intake
    monitor
    # cleanup runs via the EXIT trap.
}

# --- Dispatch -----------------------------------------------------------------

case "${1:-start}" in
    start | "") cmd_start ;;
    validate-webhooks | smoke-webhooks) cmd_validate_webhooks ;;
    stop) cmd_stop ;;
    help | -h | --help) usage ;;
    *)
        usage >&2
        exit 2
        ;;
esac
