# `deploy/` — the single entry point for the Smith + Temper topology

Repo-tracked **templates** plus one idempotent **install script** that brings up
the current two-tier topology in a single command: **one `temper-daemon`
process and one `smith-worker` process**.

The **Temper daemon** is the sole Forgejo **API** writer — it owns webhook
intake, the poll backstop, queue scanning, leases, mechanical landing, and all
role-attributed Forge mutations. The **Smith worker** registers its
`(repo, role)` capabilities, long-polls the daemon for work, runs the coding
agent in persistent per-`(repo, role)` workspaces, pushes branches **as the
role** over git, and returns structured results. The worker **never calls the
Forgejo API**; the git plane is the deliberate exception.

`deploy/install.sh` (in this `smith` checkout) is the orchestrator: it builds and
installs the worker tier, **delegates the daemon tier** to the sibling
`temper/deploy/install.sh`, installs both systemd units, and performs the live
cutover from the legacy per-role pools. We run it **regularly and frequently**;
every binary is built in the **dev (debug)** cargo profile and re-running is
safe.

Live config and state live outside the repos (`~/.config/smith`,
`~/.config/temper`, `~/.local/state/smith`); only the templates are tracked here.

## Layout

```text
deploy/
├── install.sh                     SINGLE entry point: builds bins, delegates to temper/deploy, cuts over
├── bin/                           ExecStart shim → ~/.local/bin/
│   └── smith-worker-launcher          turns smith.env knobs into smith-worker argv (no secrets)
├── systemd/                       user unit template → ~/.config/systemd/user/
│   └── smith-worker.service           the worker unit (the daemon unit ships from temper/deploy/)
└── config/                        config templates → ~/.config/smith/
    ├── smith.env                      operator knobs (NO secrets), systemd EnvironmentFile
    ├── workflow.json                  the basic-delivery workflow spec
    ├── prompts/                       optional coding-agent overlays → ~/.config/anvil/prompts/
    └── secrets/                       only template + .gitignore tracked
        ├── .gitignore
        └── README.md
```

## Binaries (all dev profile, into `~/.local/bin`)

| Binary | Built from | Role |
| --- | --- | --- |
| `temper-daemon` | `../temper` (via `temper/deploy/install.sh`) | the daemon process; sole Forge API writer |
| `smith-worker` | this repo (`crates/smith-worker`) | the long-poll worker; orchestration only |
| `anvil-agent` | `../anvil` | the out-of-process coding agent the worker spawns |

## What `install.sh` does, in order

1. **Builds the worker tier** — `smith-worker` (this repo) and `anvil-agent`
   (`../anvil`), dev profile, `-j2`.
2. **Delegates the daemon tier** — runs `../temper/deploy/install.sh`, which
   builds + installs `temper-daemon` (dev), its launcher, and
   `temper-daemon.service`, and templates `~/.config/temper/daemon.env`.
3. **Installs the worker shim + unit** — `smith-worker-launcher` and
   `smith-worker.service`.
4. **Installs config templates** without clobbering live edits — `smith.env`,
   `workflow.json`, agent prompt overlays into `~/.config/anvil/prompts/`, and
   the secrets-dir README/.gitignore.
5. **Ensures state dirs** — the worker workspace parent
   `~/.local/state/smith/worker`.
6. **Cuts over** (unless `SMITH_NO_CUTOVER=1`): stops + disables the legacy
   per-role pools — `smith-`, `temper-`, `bench-`, and `jig-`
   `engineer`/`mechanical`/`trigger`/`architect` services and their
   `*-delivery.target` units (missing units are ignored) — then
   `daemon-reload`s and enables + **(re)starts** `temper-daemon.service` and
   `smith-worker.service` so the run picks up the freshly built binaries.

It does **not** provision Forgejo and generates **no secrets**.

## Install

```sh
deploy/install.sh
```

### Knobs (environment)

| Variable | Effect |
| --- | --- |
| `SMITH_SKIP_BUILD=1` | skip the `smith-worker` + `anvil-agent` builds (reuse installed bins) |
| `TEMPER_SKIP_BUILD=1` | skip the `temper-daemon` build (passed through to temper's installer) |
| `SMITH_NO_CUTOVER=1` | install only; touch no systemd state and start nothing |
| `SMITH_SKIP_TEMPER=1` | install only the worker tier; do not run `temper/deploy/` |
| `ANVIL_REPO_ROOT=…` | override the sibling `anvil` checkout location |
| `TEMPER_REPO_ROOT=…` | override the sibling `temper` checkout location |

## Preconditions the installer does NOT create

These are provisioned once and reused across redeploys:

| Path | Holds | Tier |
| --- | --- | --- |
| `~/.config/temper/secrets/roles.env` | Forge **API** tokens (bot + per role), webhook secret | daemon |
| `~/.config/smith/secrets/roles.env` | per-role **git** credentials (`TEMPER_FORGEJO_{USER,TOKEN}_<ROLEKEY>`) | worker |

Review before the first start:

- `~/.config/temper/daemon.env` — `DAEMON_REPOS`, `DAEMON_ROLES`, webhook
  secret file, poll/mechanical cadence, lease TTL.
- `~/.config/smith/smith.env` — `WORKER_DAEMON_URL`, `WORKER_CAPABILITIES`,
  `BASE_URL` (git remote base), provider auth in `ANVIL_AGENT_ARGS`.

Secrets never appear on argv: both units load their `*.env` and
`secrets/roles.env` as systemd `EnvironmentFile=`s, and each binary reads the
suffixed role variables directly from its environment.

## Credential split (who holds what)

| Tier | Holds | Used for |
| --- | --- | --- |
| Temper daemon (`../temper/deploy/`) | Forge **API** tokens (per role + bot) | webhook intake, scans, leases, PR create/update, mechanical landing |
| Smith worker (this `deploy/`) | per-role **git** credentials (`roles.env`) | clone/fetch + role-authored commit/push in persistent workspaces |

## After install

```sh
journalctl --user -u temper-daemon.service -u smith-worker.service -f
```

Then watch one issue flow end to end: issue filed → daemon dispatch → worker
pushes `agent/…` branch as the role → daemon opens the PR as the role → CI green
→ daemon mechanical backstop merges.

## Notes

- The legacy walkthrough in
  [`docs/how-to/run-local-delivery.md`](../docs/how-to/run-local-delivery.md)
  describes the superseded per-role pool; provisioning
  ([`provision-smith-dogfood.md`](../docs/how-to/provision-smith-dogfood.md)) is
  unchanged — `roles.env` is reused as-is.
- The worker links **no** agent/LLM code. The coding agent is the out-of-process
  `anvil-agent`, spawned via `--agent-command anvil-native`; the wire contract
  lives in `smith-agent-protocol`.
