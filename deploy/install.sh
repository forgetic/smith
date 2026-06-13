#!/bin/sh
# Smith + Temper two-tier deployment installer (idempotent).
#
# This is the SINGLE entry point for the current topology: exactly one
# Temper daemon process and one Smith worker process. Running it:
#   - builds + installs the smith-worker binary (orchestration only) and the
#     anvil-agent binary from the sibling anvil checkout (the out-of-process
#     coding agent the worker spawns via --agent-command anvil-native),
#   - delegates the daemon tier to ../temper/deploy/install.sh, which builds +
#     installs the temper-daemon binary, its launcher, and its unit,
#   - installs the smith-worker-launcher ExecStart shim and the
#     smith-worker.service systemd user unit,
#   - copies worker config templates into ~/.config/smith/ and agent prompt
#     templates into ~/.config/anvil/ WITHOUT clobbering files you have already
#     edited,
#   - creates the worker workspace parent under ~/.local/state/smith/,
#   - performs the live cutover (unless SMITH_NO_CUTOVER=1): stops + disables the
#     legacy per-role pools (smith/temper/bench/jig engineer+mechanical+trigger
#     and their *-delivery.target units), reloads systemd, then enables and
#     (re)starts temper-daemon.service and smith-worker.service.
#
# All binaries are built in the DEV (debug) cargo profile, per the dogfood host's
# memory budget. We deploy this regularly and frequently; re-running is safe:
# existing config is preserved, templates + binaries are refreshed, and no live
# secrets are generated or overwritten.
#
# Secrets are read by systemd EnvironmentFile= at runtime, never echoed and never
# placed on argv by this installer.
#
# Knobs (environment):
#   SMITH_SKIP_BUILD=1   skip the smith-worker + anvil-agent cargo builds
#   TEMPER_SKIP_BUILD=1  skip the temper-daemon cargo build (passed through)
#   SMITH_NO_CUTOVER=1   install only; do not touch systemd state (start nothing)
#   SMITH_SKIP_TEMPER=1  install only the worker tier; do not run temper/deploy
#   ANVIL_REPO_ROOT=...  override the sibling anvil checkout location
#   TEMPER_REPO_ROOT=... override the sibling temper checkout location
#
# POSIX sh only. Validate with `sh -n deploy/install.sh`.

set -eu

# --- Locations ----------------------------------------------------------------
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ANVIL_REPO_ROOT=${ANVIL_REPO_ROOT:-$(CDPATH= cd -- "$REPO_ROOT/../anvil" && pwd)}
TEMPER_REPO_ROOT=${TEMPER_REPO_ROOT:-$(CDPATH= cd -- "$REPO_ROOT/../temper" && pwd)}
DEPLOY_SYSTEMD="$SCRIPT_DIR/systemd"
DEPLOY_CONFIG="$SCRIPT_DIR/config"
DEPLOY_BIN="$SCRIPT_DIR/bin"

# Locations are pinned under $HOME, matching the units' %h-based directives and
# the existing local dogfood layout. Do not switch these to XDG_CONFIG_HOME /
# XDG_STATE_HOME without also updating the unit templates and launcher.
BIN_DIR="$HOME/.local/bin"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
SMITH_CONFIG_DIR="$HOME/.config/smith"
SMITH_SECRETS_DIR="$SMITH_CONFIG_DIR/secrets"
SMITH_STATE_DIR="$HOME/.local/state/smith"
SMITH_WORKER_STATE_DIR="$SMITH_STATE_DIR/worker"
# The agent reads its prompt overlays from its own config dir (ANVIL_CONFIG_DIR
# or ~/.config/anvil), not from the worker's ~/.config/smith.
ANVIL_CONFIG_DIR="$HOME/.config/anvil"

# Build profile dir name (Smith builds in the dev profile here).
CARGO_PROFILE_DIR=debug

# The two units the current topology runs.
NEW_UNITS="temper-daemon.service smith-worker.service"

# The legacy per-role pools this topology replaces. Stopped + disabled during
# cutover. Missing units are tolerated (the loop ignores not-found).
LEGACY_TARGETS="smith-delivery.target temper-delivery.target bench-delivery.target jig-delivery.target"
LEGACY_SERVICES="
smith-engineer.service smith-mechanical.service smith-trigger.service smith-architect.service
temper-engineer.service temper-mechanical.service temper-trigger.service temper-architect.service
bench-engineer.service bench-mechanical.service bench-trigger.service bench-architect.service
jig-engineer.service jig-mechanical.service jig-trigger.service jig-architect.service
"

log() { printf '[install] %s\n' "$*"; }
die() { printf '[install] error: %s\n' "$*" >&2; exit 1; }

have_systemctl() { command -v systemctl >/dev/null 2>&1; }

# --- Binaries -----------------------------------------------------------------
# Build and install the worker-tier binaries:
#   - smith-worker (the daemon long-poll worker; orchestration only)
#   - anvil-agent  (the out-of-process coding agent, from the sibling anvil
#                   checkout; the worker spawns it via `--agent-command anvil-native`)
# The temper-daemon binary is built + installed by temper/deploy/install.sh.
build_smith_binaries() {
    if [ "${SMITH_SKIP_BUILD:-0}" != "1" ]; then
        log "building smith-worker (dev) in $REPO_ROOT ..."
        ( cd "$REPO_ROOT" \
            && cargo build -j2 -p smith-worker --bin smith-worker ) \
            || die 'smith-worker cargo build failed'
        log "building anvil-agent (dev) in $ANVIL_REPO_ROOT ..."
        ( cd "$ANVIL_REPO_ROOT" \
            && cargo build -j2 --bin anvil-agent ) \
            || die 'anvil-agent cargo build failed'
    else
        log 'skipping smith-worker + anvil-agent cargo build because SMITH_SKIP_BUILD=1'
    fi

    install_binary "$REPO_ROOT/target/$CARGO_PROFILE_DIR/smith-worker"
    install_binary "$ANVIL_REPO_ROOT/target/$CARGO_PROFILE_DIR/anvil-agent"
}

install_binary() {
    _src=$1
    [ -x "$_src" ] || die "expected binary not found (build did not produce it): $_src"
    mkdir -p "$BIN_DIR"
    install -m 0755 "$_src" "$BIN_DIR/$(basename "$_src")"
    log "  installed $(basename "$_src") -> $BIN_DIR/$(basename "$_src")"
}

# Install the systemd ExecStart shim. It contains no secrets; it only translates
# smith.env knobs into smith-worker argv and leaves roles.env variables untouched.
install_shims() {
    mkdir -p "$BIN_DIR"
    install -m 0755 "$DEPLOY_BIN/smith-worker-launcher" "$BIN_DIR/smith-worker-launcher"
    log "  installed smith-worker-launcher -> $BIN_DIR/smith-worker-launcher"
}

# --- systemd user unit ---------------------------------------------------------
# Unit templates have no machine-specific substitutions, so they are always
# refreshed from the repo. Runtime behavior is controlled through smith.env.
install_units() {
    mkdir -p "$SYSTEMD_USER_DIR"
    install -m 0644 "$DEPLOY_SYSTEMD/smith-worker.service" "$SYSTEMD_USER_DIR/smith-worker.service"
    log "  installed smith-worker.service -> $SYSTEMD_USER_DIR/smith-worker.service"
}

# --- Temper daemon tier (delegated) -------------------------------------------
# The daemon tier lives in the sibling temper checkout and owns its own
# idempotent installer. We invoke it so one run brings up both tiers; the
# TEMPER_SKIP_BUILD knob passes through. It installs temper-daemon (dev),
# temper-daemon-launcher, and temper-daemon.service, and templates daemon.env
# without clobbering a live one. It starts nothing — cutover happens below.
install_temper_tier() {
    if [ "${SMITH_SKIP_TEMPER:-0}" = "1" ]; then
        log 'skipping temper daemon tier because SMITH_SKIP_TEMPER=1'
        return 0
    fi
    [ -x "$TEMPER_REPO_ROOT/deploy/install.sh" ] \
        || die "temper installer not found/executable at $TEMPER_REPO_ROOT/deploy/install.sh (set TEMPER_REPO_ROOT or SMITH_SKIP_TEMPER=1)"
    log "delegating daemon tier to $TEMPER_REPO_ROOT/deploy/install.sh ..."
    # Pass TEMPER_SKIP_BUILD through; the temper installer reads it itself.
    "$TEMPER_REPO_ROOT/deploy/install.sh"
}

# --- Config templates (never clobber live edits) ------------------------------
# Copies a template into place ONLY if the destination does not already exist, so
# an operator's edited config survives a re-run. Reports skip vs. install.
install_template() {
    _src=$1
    _dst=$2
    _mode=$3
    mkdir -p "$(dirname "$_dst")"
    if [ -e "$_dst" ]; then
        log "  keep   $_dst (already present; not overwritten)"
        return 0
    fi
    install -m "$_mode" "$_src" "$_dst"
    log "  create $_dst"
}

install_prompt_templates() {
    for _prompt in "$DEPLOY_CONFIG"/prompts/*; do
        [ -f "$_prompt" ] || continue
        install_template "$_prompt" "$ANVIL_CONFIG_DIR/prompts/$(basename "$_prompt")" 0644
    done
}

# Like install_template, but migrates a STALE env that belongs to the retired
# topology. If the destination exists but lacks `_required_key`, it is a legacy
# file: back it up to <dst>.bak.<n> and install the fresh template (with a
# warning) rather than leaving the new unit to crash-loop on missing knobs. A
# destination that already has the key is a real operator file — never clobbered.
install_or_migrate_env() {
    _src=$1
    _dst=$2
    _mode=$3
    _required_key=$4
    mkdir -p "$(dirname "$_dst")"
    if [ ! -e "$_dst" ]; then
        install -m "$_mode" "$_src" "$_dst"
        log "  create $_dst"
        return 0
    fi
    if grep -q "^[[:space:]]*$_required_key=" "$_dst" 2>/dev/null; then
        log "  keep   $_dst (already present; not overwritten)"
        return 0
    fi
    # Legacy file: it predates the current topology (no $_required_key). Find a
    # free numbered backup so we never overwrite an earlier one.
    _n=1
    while [ -e "$_dst.bak.$_n" ]; do _n=$((_n + 1)); done
    cp -p "$_dst" "$_dst.bak.$_n"
    install -m "$_mode" "$_src" "$_dst"
    log "  MIGRATE $_dst (legacy config missing $_required_key; backed up to $_dst.bak.$_n, installed fresh template)"
    printf '[install] warning: review %s and re-apply any host-specific values from %s.bak.%s\n' \
        "$_dst" "$_dst" "$_n" >&2
}

install_config() {
    mkdir -p "$SMITH_CONFIG_DIR" "$SMITH_SECRETS_DIR" "$ANVIL_CONFIG_DIR/prompts"
    # smith.env carries the worker's identity/capability knobs. A file without
    # WORKER_CAPABILITIES belongs to the retired per-role pool — migrate it.
    install_or_migrate_env "$DEPLOY_CONFIG/smith.env" "$SMITH_CONFIG_DIR/smith.env" 0644 WORKER_CAPABILITIES
    install_template "$DEPLOY_CONFIG/workflow.json" "$SMITH_CONFIG_DIR/workflow.json" 0644
    install_prompt_templates
    install_template "$DEPLOY_CONFIG/secrets/README.md" "$SMITH_SECRETS_DIR/README.md" 0644
    install_template "$DEPLOY_CONFIG/secrets/.gitignore" "$SMITH_SECRETS_DIR/.gitignore" 0644
}

# --- Workspace + state parents ------------------------------------------------
install_state_dirs() {
    mkdir -p "$SMITH_WORKER_STATE_DIR"
    log "  ensured $SMITH_WORKER_STATE_DIR"
}

# --- Live cutover -------------------------------------------------------------
# Retire the legacy per-role pools and (re)start the two current-topology units.
# Idempotent: disabling an already-disabled/absent unit is a no-op, and we
# restart the new units so a redeploy always picks up the freshly built binaries.
cutover() {
    if [ "${SMITH_NO_CUTOVER:-0}" = "1" ]; then
        log 'skipping systemd cutover because SMITH_NO_CUTOVER=1 (nothing started)'
        return 0
    fi
    if ! have_systemctl; then
        log 'systemctl not found; skipping cutover (install-only on this host)'
        return 0
    fi

    log 'retiring legacy per-role pools (stop + disable; missing units ignored) ...'
    # `disable --now` both stops and removes the enablement symlinks. Tolerate
    # not-found / not-loaded units so the list can be a superset of any host.
    for _unit in $LEGACY_TARGETS $LEGACY_SERVICES; do
        systemctl --user disable --now "$_unit" >/dev/null 2>&1 || true
    done

    log 'reloading systemd user manager ...'
    systemctl --user daemon-reload || die 'systemctl --user daemon-reload failed'

    log "enabling + (re)starting current-topology units: $NEW_UNITS"
    # enable wires them under default.target; restart picks up the new binaries
    # whether or not they were already running.
    # shellcheck disable=SC2086
    systemctl --user enable $NEW_UNITS >/dev/null 2>&1 || true

    # Restart the daemon first, wait for it to bind, then the worker — so a
    # redeploy doesn't make the worker log spurious connection-refused retries
    # while the daemon is still coming up. (The worker self-heals regardless;
    # this just keeps the logs clean.)
    systemctl --user restart temper-daemon.service || die 'failed to start temper-daemon.service'
    log '  restarted temper-daemon.service'
    wait_for_daemon

    systemctl --user restart smith-worker.service || die 'failed to start smith-worker.service'
    log '  restarted smith-worker.service'
}

# Best-effort wait until the daemon's bind address accepts a TCP connection, so
# the worker starts against a listening socket. Bounded (~5s); on timeout we
# proceed anyway (the worker's register loop retries). Reads DAEMON_BIND from the
# live daemon.env; falls back to the documented default.
wait_for_daemon() {
    _bind=$(sed -n 's/^[[:space:]]*DAEMON_BIND=//p' "$HOME/.config/temper/daemon.env" 2>/dev/null | tail -n1)
    _bind=${_bind:-127.0.0.1:8080}
    _host=${_bind%:*}
    _port=${_bind##*:}
    [ -n "$_port" ] || return 0
    _i=0
    while [ "$_i" -lt 25 ]; do
        # Prefer a real connect probe; fall back to a short sleep if no probe
        # tool is available (the worker will retry on its own either way).
        if command -v nc >/dev/null 2>&1; then
            nc -z "$_host" "$_port" >/dev/null 2>&1 && { log "  daemon listening at $_bind"; return 0; }
        elif command -v bash >/dev/null 2>&1; then
            bash -c "exec 3<>/dev/tcp/$_host/$_port" >/dev/null 2>&1 \
                && { log "  daemon listening at $_bind"; return 0; }
        else
            sleep 1; log "  (no connect probe; proceeding)"; return 0
        fi
        _i=$((_i + 1))
        sleep 0.2
    done
    log "  daemon not confirmed listening at $_bind after ~5s; starting worker anyway (it retries)"
}

# --- Main ---------------------------------------------------------------------
main() {
    log 'installing the Smith + Temper two-tier deployment (1 daemon + 1 worker)'
    log "smith repo:  $REPO_ROOT"
    log "temper repo: $TEMPER_REPO_ROOT"
    log "anvil repo:  $ANVIL_REPO_ROOT"

    log 'worker-tier binaries:'
    build_smith_binaries

    log 'daemon tier:'
    install_temper_tier

    log 'execstart shim:'
    install_shims

    log 'systemd user unit:'
    install_units

    log 'config templates:'
    install_config

    log 'state directories:'
    install_state_dirs

    log 'cutover:'
    cutover

    cat <<EOF
[install] done.

Topology: 1 temper-daemon + 1 smith-worker.

Preconditions for a healthy run (NOT created by this installer):
  - $HOME/.config/temper/secrets/roles.env  (bot + per-role Forge API tokens; webhook-secret)
  - $SMITH_SECRETS_DIR/roles.env            (per-role git credentials for push)
Review:
  - $HOME/.config/temper/daemon.env  (DAEMON_REPOS, DAEMON_ROLES, webhook, cadence)
  - $SMITH_CONFIG_DIR/smith.env      (WORKER_DAEMON_URL, WORKER_CAPABILITIES, BASE_URL)

Watch the two services:
  journalctl --user -u temper-daemon.service -u smith-worker.service -f

Knobs: SMITH_SKIP_BUILD=1, TEMPER_SKIP_BUILD=1, SMITH_NO_CUTOVER=1, SMITH_SKIP_TEMPER=1.
EOF
}

main "$@"
