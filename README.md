# smith

smith is a kit for flexible LLM agents and a standard agent built from
it. A host supplies a charter, tools, messages, result contracts and a
budget; smith supplies the conversation and run mechanics. Its design
starts at [domain/README.md](docs/design/domain/README.md).

The repository contains the 05s1 workspace foundation and the 05s2 copy
baseline: agent, run, session and tools domains, provider and OAuth codecs,
fake LLM peers, component worlds and the composed agent on a scripted typed
host. The [copy ledger](docs/development/migration-05s2.md) records the named
source and the preserved stories. The first 05s4 increment adds
[generic result contracts](docs/development/migration-05s4-results.md),
including real reports and declared failures. The next increment adds
[generic checked delivery](docs/development/migration-05s4-delivery.md),
including mid-run delivery and actual host evidence during shutdown.
[Declared opaque host tools](docs/development/migration-05s4-host-tools.md)
now carry bounded schemas, effects and exact input/result bytes through stable
operation names and settled bounded recovery, retaining actual terminal rights
during withdrawal and shutdown. The standalone
[V2 host supervision kit](docs/development/migration-05s6-host.md) now relays
opaque starts, messages and numbered turns, supervises contained agent processes
and retains actual call/delivery rights through shutdown. Further generic host
changes, live channel and provider protocol integration, the local host and the
executable remain later increments of temper's `05s-smith.md` migration plan.

## Layout

- `crates/`: domain and protocol crates, added with their implementations.
- `testing/`: smith-specific fakes, added with the worlds that use them.
- `tests/`: worlds and their focused and `fuzzy_*.rs` test binaries; no
  integration tests live under `crates/*/tests`.
- `docs/design/domain/`: the run, session, tools and host contracts.
- `docs/development/workflow.md`: exact-tip review, checks and budgets.

smith follows skein's foundation documents: programming-model.md for the
Rust subset and ownership, testing-strategy.md for independent worlds,
and notes.md for the memory strategy. Step crates inherit the workspace
lints and use `no_std` with `alloc`; test harnesses are ordinary Rust
whose clocks, seeds and ordered observations make runs reproducible.

## Development

Read [AGENTS.md](AGENTS.md) and the
[workflow](docs/development/workflow.md) before changing code. The gate is:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo nextest run --workspace --profile fuzzy
```

Focused tests have a 15-second workspace budget; fuzzy tests have 60
seconds. `--profile measure -j 1` measures a suite serially without the
gate's timeout. Each world keeps its own fakes and independent
expectations; generic harness mechanisms are reused from `skein_world::domain`.
`tests/world` checks that shared kit as a consumer; it adds no harness
implementation or agent behavior.

Dependencies on skein use `https://git.ekanayaka.io/ai/skein.git`, with
canonical URLs in the workspace manifest and the revision pinned in `Cargo.lock`.
smith and temper must resolve the same
skein revision when composed. Changes to shared mechanisms land in skein
first; smith and temper then take that revision through their own gates.

The shared-harness and fake-checkout dependency revision is
unpublished. Local validation supplies the exact locked commit through
Cargo's git cache and runs offline. A fresh machine cannot fetch that
revision from the canonical URL until its upstream publication is
authorized; the manifest keeps the canonical URL and no local path patch.

No smith remote is configured yet. Branches and review stay local until
the forge repository is provisioned.
