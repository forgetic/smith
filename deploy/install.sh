#!/bin/sh
# Smith consolidated worker deployment installer (idempotent).
#
# Installs the Smith worker tier for the daemon/worker topology:
#   - builds + installs the smith-worker and smith-coding-agent binaries,
#   - installs the smith-worker-launcher ExecStart shim,
#   - copies the smith-worker systemd user unit template,
#   - copies config templates into ~/.config/smith/ WITHOUT clobbering files you
#     have already edited,
#   - creates the worker workspace parent under ~/.local/state/smith/.
#
# It does NOT deploy the Temper daemon, provision Forgejo, write roles.env, create
# webhook secrets, start services, or touch already-installed legacy units. Deploy
# the daemon from temper/deploy/install.sh and perform the live cutover manually.
#
# Re-running is safe: existing config is preserved, templates and binaries are
# refreshed, and no live secrets are generated or overwritten.
#
# POSIX sh only. Validate with `sh -n deploy/install.sh`.
# Secrets are read by systemd EnvironmentFile= at runtime, never echoed and never
# placed on argv by this installer.

set -eu

# --- Locations ----------------------------------------------------------------
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
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

# Build profile dir name (Smith builds in the dev profile here).
CARGO_PROFILE_DIR=debug

log() { printf '[install] %s\n' "$*"; }
die() { printf '[install] error: %s\n' "$*" >&2; exit 1; }

# --- Binaries -----------------------------------------------------------------
# Build and install only the Smith binaries the worker tier invokes:
#   - smith-worker (the daemon long-poll worker)
#   - smith-coding-agent (the coding executor's agent command)
# Installation is a copy into ~/.local/bin so the unit has stable absolute paths
# independent of this checkout's target dir.
build_smith_binaries() {
    if [ "${SMITH_SKIP_BUILD:-0}" != "1" ]; then
        log "building Smith worker binaries in $REPO_ROOT ..."
        ( cd "$REPO_ROOT" \
            && cargo build -j2 -p smith-worker --bin smith-worker \
            && cargo build -j2 -p smith-temper-agent-cli --bin smith-coding-agent ) \
            || die 'Smith cargo build failed'
    else
        log 'skipping cargo build because SMITH_SKIP_BUILD=1'
    fi

    install_binary "$REPO_ROOT/target/$CARGO_PROFILE_DIR/smith-worker"
    install_binary "$REPO_ROOT/target/$CARGO_PROFILE_DIR/smith-coding-agent"
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
        install_template "$_prompt" "$SMITH_CONFIG_DIR/prompts/$(basename "$_prompt")" 0644
    done
}

install_config() {
    mkdir -p "$SMITH_CONFIG_DIR" "$SMITH_SECRETS_DIR" "$SMITH_CONFIG_DIR/prompts"
    install_template "$DEPLOY_CONFIG/smith.env" "$SMITH_CONFIG_DIR/smith.env" 0644
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

# --- Main ---------------------------------------------------------------------
main() {
    log 'installing Smith consolidated worker deployment'
    log "repo: $REPO_ROOT"

    log 'binaries:'
    build_smith_binaries

    log 'execstart shim:'
    install_shims

    log 'systemd user unit:'
    install_units

    log 'config templates:'
    install_config

    log 'state directories:'
    install_state_dirs

    cat <<EOF
[install] done.

Next steps:
  1. Ensure the Temper daemon tier is installed and running (from temper/deploy/install.sh).
  2. Ensure $SMITH_SECRETS_DIR/roles.env contains the provisioned per-role git credentials.
  3. Review $SMITH_CONFIG_DIR/smith.env, especially WORKER_DAEMON_URL and WORKER_CAPABILITIES.
  4. Start the worker after reloading systemd:
       systemctl --user daemon-reload && systemctl --user start smith-worker.service
  5. Watch it:
       journalctl --user -u smith-worker.service -f

For the legacy cutover order, see deploy/README.md.
EOF
}

main "$@"
