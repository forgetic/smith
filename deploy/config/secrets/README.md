# `~/.config/smith/secrets/`

Live deployment secrets for the basic-delivery pool. **Nothing real here is
tracked** — see `.gitignore`. The repo ships only this README, the `.gitignore`,
and `roles.env.example`.

| File | Who writes it | Contents |
| --- | --- | --- |
| `roles.env` | the Temper provisioner (`temper-provision-forgejo --out …`) | per-role + bot Forgejo tokens/passwords, mode `0600` |
| `webhook-secret` | `deploy/install.sh` | HMAC secret Forgejo signs webhook deliveries with (provisioning *reads* it) |
| `wake-secret` | `deploy/install.sh` | shared secret the trigger uses to authenticate wakes to workers |
| `roles.env.example` | tracked template | documents the shape `roles.env` must have |

`deploy/install.sh` generates `webhook-secret` and `wake-secret` if they are
absent (mode `0600`) but never writes `roles.env` — that comes from provisioning
(`docs/how-to/provision-smith-dogfood.md`). Run `install.sh` before provisioning,
since the provisioner reads `webhook-secret` to register the wake webhook.
Secrets travel only via these files and the systemd `EnvironmentFile=` directive,
never on a command line.
