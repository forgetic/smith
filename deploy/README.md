# `deploy/` — production-like local basic-delivery deployment

Repo-tracked **templates** plus an idempotent **install script** for running the
basic-delivery workflow against `ai/smith` as real systemd user services —
instead of the throwaway [`examples/basic-delivery`](../examples/basic-delivery/)
launcher. Live config and state live outside the repo
(`~/.config/smith`, `~/.local/state/smith`); only the templates are tracked here.

This is issue #12 (child B of #9). The do-this-in-order operator walkthrough is
[`docs/how-to/run-local-delivery.md`](../docs/how-to/run-local-delivery.md); the
provisioning half is
[`docs/how-to/provision-smith-dogfood.md`](../docs/how-to/provision-smith-dogfood.md).

## Layout

```text
deploy/
├── install.sh                     idempotent installer (builds bins, copies templates)
├── bin/                           ExecStart shims → ~/.local/bin/
│   ├── smith-role-worker              maps roles.env → generic creds, exec temper-worker (role)
│   └── smith-mechanical-worker        maps bot creds → generic creds, exec temper-worker (mechanical)
├── systemd/                       user unit templates → ~/.config/systemd/user/
│   ├── smith-architect.service        architect role worker (read-only checkout)
│   ├── smith-engineer.service         engineer role worker (writable checkout + push)
│   ├── smith-mechanical.service       mechanical worker as `bot` (lands CI-green PRs)
│   ├── smith-trigger.service          local webhook accelerator (no ssh tunnel)
│   └── smith-delivery.target          groups the four units
└── config/                        config templates → ~/.config/smith/
    ├── smith.env                      operator knobs (NO secrets), systemd EnvironmentFile
    ├── workflow.json                  the basic-delivery 3-role spec
    ├── prompts/                       optional coding-agent overlays (issue #10)
    │   ├── architect.md
    │   └── engineer.md
    └── secrets/                       only template + .gitignore tracked
        ├── .gitignore
        ├── README.md
        └── roles.env.example          shape provisioning's roles.env must have
```

## What the units do

`smith-delivery.target` groups the pool; `systemctl --user start
smith-delivery.target` brings it up. Forgejo + the host-mode runner stay in the
existing `~/.config/systemd/user/forgejo.service`, which these units only
order after.

- **architect / engineer** — `temper-worker --kind role --role <role>` driving
  the Smith role-decision command and the Smith pi-SDK coding agent, with stable
  per-role workspaces under `~/.local/state/smith/<role>/smith`.
- **mechanical** — `temper-worker --kind mechanical` as the `bot`: the sole
  landing authority (no reviewer/owner), merging CI-green PRs. It needs the bot's
  REST token *and* web-UI username/password for the Forgejo 7.0.x CI-read
  fallback (ADR 0019), and polls CI on a short cadence because Forgejo 7.0.x does
  not webhook on Actions completion.
- **trigger** — `temper-trigger-forgejo`, the local webhook accelerator that
  fans accepted deliveries out to every worker's wake socket.

## Install

```sh
deploy/install.sh
```

Builds the four binaries (`temper-worker`, `temper-trigger-forgejo` from the
sibling `../temper` checkout; `smith-coding-agent`,
`smith-workflow-role-decision` from this repo) into `~/.local/bin`, copies unit +
config templates into place **without clobbering existing live edits**, generates
the `wake-secret` / `webhook-secret` (0600) if absent, and creates the workspace
parents. It does **not** provision Forgejo or start any service. Override the
Temper checkout with `TEMPER_WORKSPACE_ROOT=…`; skip rebuilds with
`SMITH_SKIP_BUILD=1`.

See the how-to for provisioning, per-role workspace clone, provider auth, and
bring-up/teardown.
