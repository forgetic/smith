#!/bin/sh
# Smith basic-delivery deployment installer (idempotent).
#
# Installs the production-like local deployment of the basic-delivery workflow:
#   - builds + installs the four binaries the units invoke into ~/.local/bin,
#   - copies the systemd user unit templates into ~/.config/systemd/user/,
#   - copies the config templates into ~/.config/smith/ WITHOUT clobbering any
#     file you have already edited,
#   - generates the wake/webhook secret files (0600) if absent,
#   - creates the per-role workspace parents under ~/.local/state/smith/.
#
# It does NOT provision Forgejo identities/labels/webhook or write
# secrets/roles.env (that is the Temper provisioner — see
# docs/how-to/provision-smith-dogfood.md), and it does NOT start any service.
# Bring the pool up afterward with:
#     systemctl --user daemon-reload
#     systemctl --user enable --now smith-delivery.target
#
# Re-running is safe: existing config is preserved, binaries are rebuilt only if
# cargo decides they are stale, and secret files are left untouched once present.
#
# POSIX sh only (no bashisms). Validate with `sh -n install.sh` (and shellcheck).
# Secrets are written to 0600 files, never echoed and never placed on argv.

set -eu

# --- Locations ----------------------------------------------------------------
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
DEPLOY_SYSTEMD="$SCRIPT_DIR/systemd"
DEPLOY_CONFIG="$SCRIPT_DIR/config"
DEPLOY_BIN="$SCRIPT_DIR/bin"

# Locations are pinned under $HOME, matching the units' %h-based directives and
# the existing forgejo.service. Do not switch these to XDG_CONFIG_HOME/
# XDG_STATE_HOME without also teaching the unit templates (which hardcode
# %h/.config and %h/.local) and the ExecStart shims the same base — otherwise the
# installer and the units would disagree about where config/state live.
BIN_DIR="$HOME/.local/bin"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
SMITH_CONFIG_DIR="$HOME/.config/smith"
SMITH_SECRETS_DIR="$SMITH_CONFIG_DIR/secrets"
SMITH_STATE_DIR="$HOME/.local/state/smith"
SMITH_WAKE_DIR="$SMITH_STATE_DIR/wake"

# The Temper checkout that provides temper-worker / temper-provision-forgejo /
# temper-trigger-forgejo. Defaults to a sibling of this repo; override with
# TEMPER_WORKSPACE_ROOT.
TEMPER_WORKSPACE_ROOT=${TEMPER_WORKSPACE_ROOT:-$(CDPATH= cd -- "$REPO_ROOT/../temper" 2>/dev/null && pwd || true)}

# Build profile dir name (Temper + Smith both build in the dev profile here).
CARGO_PROFILE_DIR=debug

log() { printf '[install] %s\n' "$*"; }
die() { printf '[install] error: %s\n' "$*" >&2; exit 1; }

# --- Binaries -----------------------------------------------------------------
# Build and install the four binaries the units invoke:
#   - temper-worker, temper-trigger-forgejo      (from the Temper checkout)
#   - smith-coding-agent, smith-workflow-role-decision (this repo)
# Installation is a copy into ~/.local/bin so the units have stable absolute
# paths independent of either checkout's target dir.
build_temper_binaries() {
    [ -n "$TEMPER_WORKSPACE_ROOT" ] && [ -d "$TEMPER_WORKSPACE_ROOT" ] \
        || die "Temper checkout not found. Set TEMPER_WORKSPACE_ROOT to the checkout that provides temper-worker / temper-trigger-forgejo (expected a sibling 'temper' of $REPO_ROOT)."
    if [ "${SMITH_SKIP_BUILD:-0}" != "1" ]; then
        log "building Temper binaries (cargo build -p temper) in $TEMPER_WORKSPACE_ROOT ..."
        ( cd "$TEMPER_WORKSPACE_ROOT" && cargo build -p temper ) \
            || die 'Temper cargo build failed'
    fi
    install_binary "$TEMPER_WORKSPACE_ROOT/target/$CARGO_PROFILE_DIR/temper-worker"
    install_binary "$TEMPER_WORKSPACE_ROOT/target/$CARGO_PROFILE_DIR/temper-trigger-forgejo"
}

build_smith_binaries() {
    if [ "${SMITH_SKIP_BUILD:-0}" != "1" ]; then
        log "building Smith binaries (cargo build -p smith-temper-agent-cli) in $REPO_ROOT ..."
        ( cd "$REPO_ROOT" \
            && cargo build -p smith-temper-agent-cli --bin smith-coding-agent \
            && cargo build -p smith-temper-agent-cli --bin smith-workflow-role-decision ) \
            || die 'Smith cargo build failed'
    fi
    install_binary "$REPO_ROOT/target/$CARGO_PROFILE_DIR/smith-coding-agent"
    install_binary "$REPO_ROOT/target/$CARGO_PROFILE_DIR/smith-workflow-role-decision"
}

install_binary() {
    _src=$1
    [ -x "$_src" ] || die "expected binary not found (build did not produce it): $_src"
    mkdir -p "$BIN_DIR"
    install -m 0755 "$_src" "$BIN_DIR/$(basename "$_src")"
    log "  installed $(basename "$_src") -> $BIN_DIR/$(basename "$_src")"
}

# Install the systemd ExecStart shims (smith-role-worker, smith-mechanical-worker)
# that remap the suffixed roles.env vars onto the generic names temper-worker
# reads. Always refreshed from the repo so a re-run picks up shim fixes.
install_shims() {
    mkdir -p "$BIN_DIR"
    for _shim in smith-role-worker smith-mechanical-worker; do
        install -m 0755 "$DEPLOY_BIN/$_shim" "$BIN_DIR/$_shim"
        log "  installed $_shim -> $BIN_DIR/$_shim"
    done
}

# --- systemd user units -------------------------------------------------------
# Units are stable templates with no machine-specific substitutions, so they are
# always refreshed from the repo (a re-run picks up unit fixes). Editing the unit
# behavior is done through smith.env, not by hand-editing installed units.
install_units() {
    mkdir -p "$SYSTEMD_USER_DIR"
    for _unit in smith-architect.service smith-engineer.service \
        smith-mechanical.service smith-trigger.service smith-delivery.target; do
        install -m 0644 "$DEPLOY_SYSTEMD/$_unit" "$SYSTEMD_USER_DIR/$_unit"
        log "  installed $_unit -> $SYSTEMD_USER_DIR/$_unit"
    done
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

install_config() {
    mkdir -p "$SMITH_CONFIG_DIR" "$SMITH_SECRETS_DIR" "$SMITH_CONFIG_DIR/prompts"
    install_template "$DEPLOY_CONFIG/smith.env" "$SMITH_CONFIG_DIR/smith.env" 0644
    install_template "$DEPLOY_CONFIG/workflow.json" "$SMITH_CONFIG_DIR/workflow.json" 0644
    install_template "$DEPLOY_CONFIG/prompts/architect.md" "$SMITH_CONFIG_DIR/prompts/architect.md" 0644
    install_template "$DEPLOY_CONFIG/prompts/engineer.md" "$SMITH_CONFIG_DIR/prompts/engineer.md" 0644
    install_template "$DEPLOY_CONFIG/secrets/roles.env.example" "$SMITH_SECRETS_DIR/roles.env.example" 0644
    install_template "$DEPLOY_CONFIG/secrets/README.md" "$SMITH_SECRETS_DIR/README.md" 0644
}

# --- Secret files (generated, 0600, never overwritten) ------------------------
ensure_secret_file() {
    _file=$1
    [ -f "$_file" ] && { log "  keep   $_file (already present)"; return 0; }
    mkdir -p "$(dirname "$_file")"
    (
        umask 077
        if command -v openssl >/dev/null 2>&1; then
            openssl rand -hex 32 >"$_file"
        else
            dd if=/dev/urandom bs=32 count=1 2>/dev/null | od -An -tx1 | tr -d ' \n' >"$_file"
            printf '\n' >>"$_file"
        fi
    )
    chmod 0600 "$_file"
    log "  create $_file (0600)"
}

install_secrets() {
    ensure_secret_file "$SMITH_SECRETS_DIR/webhook-secret"
    ensure_secret_file "$SMITH_SECRETS_DIR/wake-secret"
}

# --- Workspace + state parents ------------------------------------------------
# Create the per-role workspace parents and the wake-socket dir. The role
# checkouts themselves (architect read-only, engineer writable + push creds) are
# cloned by provisioning (docs/how-to/provision-smith-dogfood.md step 4); this
# only guarantees the parents exist so that step — and the units' wake sockets —
# have somewhere to live.
install_state_dirs() {
    mkdir -p "$SMITH_STATE_DIR/architect" "$SMITH_STATE_DIR/engineer" "$SMITH_WAKE_DIR"
    log "  ensured $SMITH_STATE_DIR/{architect,engineer} and $SMITH_WAKE_DIR"
}

# --- Main ---------------------------------------------------------------------
main() {
    log "installing Smith basic-delivery deployment"
    log "repo:   $REPO_ROOT"
    log "temper: ${TEMPER_WORKSPACE_ROOT:-<unset>}"

    log 'binaries:'
    build_temper_binaries
    build_smith_binaries

    log 'execstart shims:'
    install_shims

    log 'systemd user units:'
    install_units

    log 'config templates:'
    install_config

    log 'secret files:'
    install_secrets

    log 'state directories:'
    install_state_dirs

    cat <<EOF
[install] done.

Next steps:
  1. Provision Forgejo identities/labels/webhook and write roles.env:
       docs/how-to/provision-smith-dogfood.md
  2. Pre-clone the per-role workspaces:
       $SMITH_STATE_DIR/architect/smith  (read-only)
       $SMITH_STATE_DIR/engineer/smith   (writable + push creds)
  3. Set up Smith provider auth (docs/how-to/configure-provider-auth.md).
  4. Bring the pool up:
       systemctl --user daemon-reload
       systemctl --user enable --now smith-delivery.target
  5. Watch it:  journalctl --user -u smith-engineer -f

Full walkthrough: docs/how-to/run-local-delivery.md
EOF
}

main "$@"
