# smith's shell and product

Provisional, 2026-10-09. smith's shell layer: the crates that turn its
services into processes, the root package whose binaries are glue, and
the product `smith`, one process a person or a script runs. It covers
the commands, the settings and their presets, credentials, the
environment commands build in, and how the product composes one
process. The contracts it serves are the domain's (`domain/`) and the
protocol layer's (`protocol/`). This is the only document that holds
profile numbers (section 6); everything else derives from them
(protocol/limits.md). What is still open is listed in section 11.

## 1. In one page

- **Libraries in `crates/`, glue in `./src`.** smith is a kit taken as
  libraries (domain/README.md, section 4). Its binaries are for
  showcase and internal use: the root package's `smith` and
  `smith-agent`, each a few dozen lines that parse, compose and drive
  (section 3).
- **Two shell libraries.** `smith-agent-shell` is the agent process a
  host spawns. `smith-local-shell` is the product: commands, settings,
  presets, credentials, composition and front ends (section 2). Both are
  ordinary Rust under skein's shell bans.
- **skein first.** The loop driver, an append sink, durable replace and
  a secret store, the OAuth io driver, trust roots, terminal modes and
  the process's resource usage are generic, and land in skein before
  smith uses them (2.4).
- **One process.** The product composes the local domain, which owns
  the inline agent as its child, with the effects: one loop, one ring,
  no channel. The shell builds the agent configuration from presets and
  settings as a value, and writes no file. Spawning an agent stays a
  library capability, and `smith-agent` is the spawned agent for hosts
  that want a prebuilt one (section 9).
- **Four commands.** `smith`, the interactive chat; `smith exec`, one
  headless run; `smith check`, validation; `smith login`, sign-in
  (section 4). A headless run says how it ended in its exit code (4.3).
- **Instrumented as it ships.** `--json` and `--trace` carry the
  versioned event stream (protocol/events.md). The terminal
  renders the same records, never `Debug` text.
- **Settings are TOML,** layered, strict and bounded, with presets for
  `codex` and `anthropic`. A first run needs a model and an account.
  Every refusal names the relationship it breaks (section 5).
- **The numbers are here.** Each preset's per-model values and the
  `standard` profile's deployment and policy values are defaults that
  the probes of benchmarks.md validate. A value with thin evidence is marked
  provisional (section 6).
- **One credential source per account:** smith's own sign-in, another
  tool's login borrowed read-only, or an environment variable. A refresh
  token is never copied (section 7).
- **The build environment is declared:** a named core environment,
  writable caches, and warnings for build wrappers whose caches cannot be
  written (section 8).

## 2. Crates

### 2.1 Libraries only

`crates/` holds libraries only: no crate there declares a binary. A
project built on smith takes the layer it needs, and a binary is the
root package's (section 3). The layers are those of skein's echo
(skein's `examples.md`, section 2):

| Crate | Layer | Owns, for the shell |
|---|---|---|
| `smith-agent-service` | service | the agent's `iterate`; the `standard` profile's deployment values and their derivation into every layer's limits, as step code with its tests |
| `smith-local-service` | service | the local host's `iterate`: the local domain, which owns its agent as a child (the inline agent, or `smith-host-domain` for a spawned one), composed with the effects: the LLM and machine components, the chat store, the terminal and the event sink; the host's and the local layers' share of the profile |
| `smith-inline-agent` | domain | smith's side of the one-host draft: a run as a composed `smith-domain` that speaks `smith-host-domain`'s vocabulary (domain/host.md) |
| `smith-events` | codec | the event vocabulary (protocol/events.md), encoded over skein-json, with its goldens |
| `smith-local-protocol` | protocol | the chat store and delivery records over skein io (protocol/hosts.md, section 5.3); the formats of borrowed logins (section 7) |
| `smith-agent-shell` | shell | the agent process (2.2) |
| `smith-local-shell` | shell | the product (2.3) |

A service owns its limits and the sum of their worst cases, as skein's
echo does, so the profile is a function of the service crates and is
tested there. The shell crates only choose a profile and the values a
setting overrides.

### 2.2 The agent shell

`smith-agent-shell` is the agent process a host spawns, as a library:

- **The agent configuration:** the JSON document of protocol/agent.md,
  section 4, read whole before the loop, strict and bounded. It names a
  profile and its declared values, the endpoints and the commands'
  environment. Each endpoint declares its models' `window`, `output`,
  `reasoning_item`, `head` and `idle` (protocol/limits.md, 2.1); a
  charter's model entries carry the effective `window` and `output`, at
  most the declared ones, and a charter naming a model the configuration
  does not declare is refused (protocol/charter.md, section 2). It never
  holds a credential.
- **Startup,** in the order of skein's `shell.md`, section 6: the
  configuration and the derived limits checked, the worst case within
  the configured memory, termination signals blocked, the endpoints'
  names resolved, trust roots loaded, the seed and the kernel.
- **The one `Agent` adapter:** a `skein_shell::Host` over
  `smith-agent-service`. The service drains the domain's facts each pass
  into its event sink, an append stream through io, and onto the
  channel, each with its own policy and loss count (protocol/agent.md,
  section 5). The adapter opens or adopts the workspace's roots, and its
  hook writes only the operator's short lines to an error writer its
  caller supplies (skein's `shell.md`, section 12). `smith-agent` runs
  it with `skein_shell::drive`, and the worlds host the same adapter
  (testing.md, section 4).
- **The end** is protocol/agent.md, section 6's: its answer sent, the
  channel finished, everything it owns closed, then exit.

### 2.3 The local shell

`smith-local-shell` is the product, as a library:

- **Commands:** typed command values and their parser, so that `main`
  holds no parsing and the parser has step tests (section 4).
- **Settings:** the layers, the presets, the defaults and their
  validation (section 5), and `smith check`, a function of the resolved
  settings and what startup found (4.4).
- **Credentials:** each account's source and the sign-in's front end
  (section 7).
- **Composition:** `compose` builds `smith-local-service`: the local
  domain with the inline agent as its child, and the effects, the agent's
  LLM and machine components among them (section 9); or, when asked, the
  local domain with `smith-host-domain` as its child and a spawned
  `smith-agent`, whose JSON configuration it then generates (5.4). Either
  way the agent configuration is built from the presets and the resolved
  settings.
- **The `Local` adapter:** a `skein_shell::Host` over
  `smith-local-service`. The service's event sink writes the records
  through io; the front ends read them in the process (below); the
  adapter's hook writes only short operator lines.
- **Front ends:** the interactive renderer and its plain mode (4.1); the
  headless one, which maps an answer to output and an exit code (4.2,
  4.3). Both read `smith-events` records.

It owns no store and no loop: chats are kept by `smith-local-protocol`
over skein io, sign-in records by skein's secret store, and the loop is
`skein_shell::drive`.

### 2.4 What skein provides first

Each lands in skein, with its design, before its smith consumer
(AGENTS.md):

- **`Host` and `drive`,** moved from `skein-world` to `skein-shell`:
  `drive` is the loop of programming-model.md, section 2, with a hook
  each iteration for a shell's drains. `skein-world` re-exports `Host`,
  and no shipped binary links a testing crate.
- **An append sink:** a bounded append stream on the ring, to a file or
  an inherited descriptor, with a write deadline, that counts what it
  drops and settles exactly at exit (skein's `io.md`, 5.1). No thread.
- **Resource usage:** the CPU time and peak resident size of the process
  and of its reaped children, read by the shell at the end for
  `session.ended` (protocol/events.md, 3.2).
- **Durable replace and a secret store:** a whole file replaced over
  io's create, write, sync and rename, its directory synced; and private
  record files on it, with their permissions and links checked.
- **The OAuth io driver:** the loopback listener, the token exchange over
  skein-http, and refresh before expiry, which `skein-oauth` leaves to
  its caller (skein's `oauth.md`); and `localhost` redirects, bound to
  127.0.0.1 and compared exactly.
- **Trust roots:** the machine's roots, or one DER file, loaded at
  startup (skein's `tls.md`).
- **Terminal modes:** raw input, its restoration at exit, and the
  window's size, as shell effects. Until skein has them, the interactive
  front end runs in line mode (4.1).

## 3. The root package

### 3.1 The manifest

- **One manifest, two roles.** The root `Cargo.toml` is the workspace
  and the package `smith`. Its binaries:
  - `src/main.rs`, the binary `smith` (`default-run`);
  - `src/bin/smith-agent.rs`, the binary `smith-agent`.
- **Nothing is discovered.** `autobins`, `autotests`, `autobenches` and
  `autoexamples` are off, and every target is declared:

  ```toml
  [[bin]]
  name = "smith"
  path = "src/main.rs"

  [[bin]]
  name = "smith-agent"
  path = "src/bin/smith-agent.rs"

  [[test]]
  name = "end_to_end"
  path = "tests/end-to-end/end_to_end.rs"

  [[test]]
  name = "live"
  path = "tests/end-to-end/live.rs"
  ```

  The end-to-end and live tests are the root package's own targets,
  because cargo gives `CARGO_BIN_EXE_<name>` only to a package's own
  tests (testing.md, section 7). The world crates under `tests/` stay
  packages of their own.
- **Bare commands keep their meaning.** `default-members` lists every
  member, so a bare `cargo build` or `cargo test` covers the workspace.
  The gate names `--workspace` (workflow.md, section 1).
- **Glue.** `main.rs`:
  1. parses its arguments into a command (2.3);
  2. reads the settings and runs `smith check`'s checks, refusing with
     their exit code (5.3);
  3. blocks termination signals, draws the seed, and opens the kernel
     and the roots (skein's `shell.md`, section 6);
  4. composes the service, runs it with `skein_shell::drive`, and exits
     with the code its outcome maps to (4.3), once the service holds
     nothing (skein's `shell.md`, section 13).

  `smith-agent.rs` reads the configuration's path, starts
  `smith-agent-shell`, runs it with `drive`, and exits 0 after its
  answer or 1 when it could not answer (protocol/agent.md, section 6).
- **Files before the loop.** A shell may read whole files with std before
  its loop runs: the settings, the agent configuration. Once the loop
  runs, every file and process operation is a ring operation
  (programming-model.md, section 2.1).

### 3.2 Lints

- **The root package has its own `[lints]` table,** not the workspace's:
  the ordinary-Rust table of skein's echo shell (programming-model.md,
  section 10.2). `unsafe` is forbidden; silent casts, overflow and
  panicking indexing are out; disallowed types and methods are denied.
  The shell crates under `crates/` carry the same table. No crate allows
  the `disallowed_*` lints wholesale.
- **Two `clippy.toml` files, split by layer.** Clippy reads the nearest
  `clippy.toml` above each manifest:
  - `crates/clippy.toml` holds the step crates' rules
    (programming-model.md, section 10), for every crate below `crates/`
    without its own;
  - the root `clippy.toml` holds the shell bans of skein's echo: `File`,
    `OwnedFd`, std's sockets, `std::process`, `Instant`, `SystemTime`,
    `HashMap`, `HashSet` and `thread::spawn`. It covers the root package
    and its test targets;
  - `smith-agent-shell` and `smith-local-shell` each keep a copy of the
    root file, since the step file sits nearer to them;
  - `tests/clippy.toml` is unchanged, and `benchmarks/` keeps its own
    (benchmarks.md), since a harness starts processes.

## 4. Commands

Shared flags:

| Flag | Meaning |
|---|---|
| `-C DIR` | the workspace: one directory, writable; a git working tree is a repository. The current directory by default |
| `-m ENDPOINT/NAME` | the main model |
| `--effort LEVEL` | its reasoning effort |
| `--chat NAME` | the chat to resume or start |
| `-c KEY=VALUE` | a setting, by its dotted path, as a TOML value; repeatable (5.1) |
| `--trace PATH` | append the event stream to `PATH` |

### 4.1 `smith`

The interactive chat, the local host of domain/host.md, section 8:

- **A chat in the workspace.** It opens the workspace and the chat
  (`default`, or `--chat NAME`) under the state directory, and resumes
  its transcript.
- **Lines are messages.** A line entered is one message, and a line
  ending in a backslash continues it (protocol/hosts.md, section 5.1).
  The line that starts an activation travels in its start.
- **The run is shown from its events:**
  - the model's text as it arrives, from `text.delta` records, which the
    front end asks for (protocol/events.md, 3.5);
  - each tool call, with its verdict and duration;
  - notices, and failures in words;
  - a status line after each response: the model, tokens in, cached and
    out, notional spend, elapsed time and what is left of the budget.

  A waiting prompt appears only once every relayed line has been read.
- **Commands** begin with `/`. They are the local host's, never
  messages:
  - `/model ENDPOINT/NAME` and `/effort LEVEL` apply from the next
    activation, since a conversation's model is fixed for its life. A
    model on another endpoint starts a new chat, and says so;
  - `/new` starts a new chat, and `/resume NAME` reopens one;
  - `/quit` ends input.

  A line that begins `//` is a message that begins `/`.
- **Interrupts.** The first is the run's cancel; a second drops it now
  (section 9). After an accepted or parked answer, unread lines are
  carried to the next activation. After a cancel, a failure or a
  refusal they are shown as not delivered and dropped, and nothing starts
  by itself after an interrupt.
- **End of input** lets the run answer or park, then exits.
- **`--plain`** is a line pipe: no terminal modes and no status line; the
  run's text per turn and each tool by name. It is for scripts and the
  worlds' scripted person, and it is the mode when standard input is not
  a terminal.
- **Its event stream,** with `--trace PATH`, is appended to a file. A
  benchmark harness reads it as it grows, while a scripted person types
  at the terminal.
- **It exits** 0 once its input ends, and 2 or 3 when startup refuses.

### 4.2 `smith exec`

```sh
smith exec [FLAGS] [PROMPT | -]
```

One run, then exit:

- **The prompt travels in the start,** as its one message, labelled
  `person` (domain/run.md, section 3.2), so the run opens on it
  (domain/run.md, section 3.4). `-` reads it from standard input to its
  end, as a harness gives it. A prompt past a message's bound
  (protocol/limits.md, 3.9), an empty one, or both an argument and `-`
  is a usage error.
- **The shared flags apply:** `-m` and `--effort` choose the main model
  and its effort, `-c` sets any key a layer may, and `-C` the workspace.
  A harness pins a run with them alone, beside its user layer.
- **No wait authority by default.** Nobody is there to answer: `wait` is
  offered with a waiting time of zero, so a run that would wait parks at
  once (domain/run.md, section 6) and ends as "needs input". Its last
  text is shown, the chat is kept, and `smith exec --chat NAME` answers
  it.
- **Edits in place.** The tools write the workspace, and nothing is
  committed. The contract is a report.
- **`--deliver`** makes the contract a change, delivered in place:
  checked, committed in each writable repository, never pushed unless
  configured (protocol/hosts.md, section 5.5).
- **Output:**
  - the result's text on standard output;
  - **`--json`** puts the event stream on standard output instead, one
    record per line, the same records `--trace` writes. The result is in
    `run.completed`, and nothing else is written there;
  - **`-o FILE`** also writes the result's text to `FILE`;
  - standard error carries a human status: the chat's name, each tool
    call and its verdict, notices, and a failure in words, with provider
    text clipped.
- **`--chat NAME`** resumes or starts that chat. Without it, the run
  starts a new chat, named in `session.started` and on standard error.
- **`--ephemeral`** keeps no chat: its files live in a private temporary
  directory removed at exit. Sign-in records are kept as usual.

### 4.3 Exit codes

A headless run's code follows its answer (domain/run.md, section 10):

| Code | Meaning | When |
|---|---|---|
| 0 | accepted | an accepted answer, however long its teardown took |
| 1 | run failed | a failed answer: the model (a provider's failure past its retries, an exhausted account, a limit that fired), policy, stale, a transcript it could not resume; a spawned agent that ended without answering; a host failure while the run went on, such as its chat store failing; or a start refused as busy |
| 2 | usage or configuration | arguments, a settings refusal (5.3), a start the run refused as invalid |
| 3 | credentials | an account the run needs without a usable credential: not signed in, rejected, a borrowed login expired, a variable unset; also a run that failed because the provider refused its credential (a model failure of class `unauthorized`) |
| 4 | budget | a failed answer for its budget: spend, time or turns |
| 5 | needs input | a parked answer |
| 130 | interrupted | a cancelled answer, after an interrupt or a termination signal |

The code is also `session.ended`'s `exit`.

**Standard error's last line** says how the run ended, in words rendered
from `run.completed`'s typed failure or the startup refusal, matched
exhaustively, so a new class has no words until it is given some. Each
is one line, beginning `smith: `, its detail clipped as an operator's
line is (protocol/limits.md, 2.4). The forms, with `<...>` filled from
the typed values:

| Code | Class | Last line |
|---|---|---|
| 0 | accepted | `smith: accepted in <turns> turns, <spend> notional, <time>` |
| 1 | the model | `smith: run failed: <endpoint>/<model> <provider's class> after <attempts> attempts: <detail>`; for a limit, `smith: run failed: <limit> reached its bound of <bound>` |
| 1 | an exhausted account | `smith: run failed: account <account> exhausted, usable again at <time>` |
| 1 | policy, stale | `smith: run failed: policy: <what was refused>`; `smith: run failed: stale: <what moved>` |
| 1 | a transcript | `smith: run failed: chat <name> cannot be resumed: <reason>` |
| 1 | a spawned agent's failure | `smith: run failed: the agent <how it ended>: <its error output's tail>` |
| 2 | usage or configuration | `smith: usage: <problem>`; a setting as in 5.3; `smith: run refused: <what> is <value>, past its bound of <bound>` |
| 3 | credentials | ``smith: account <account> has no credential: run `smith login <account>` ``; the expiry notice of section 7; `smith: account <account>: the provider rejected its credential`; `smith: account <account>: <VARIABLE> is not set` |
| 4 | budget | `smith: run failed: out of <spend, time or turns>: used <used> of <limit>` |
| 5 | needs input | ``smith: needs input: answer with `smith exec --chat <name>` `` |
| 130 | interrupted | `smith: interrupted: the run was cancelled` |

A forced exit after an accepted answer adds a warning line before the
last; the code stays 0.

### 4.4 `smith check`

```sh
smith check [--json]
```

What a run would start with, resolved and checked without starting one:

- the resolved settings, credentials redacted, each value with the layer
  it came from;
- the effective limits: the declared values, the policy values and every
  value derived from them, each under protocol/limits.md's name for it
  and in its unit (protocol/limits.md, section 9), the same names the
  event stream's header uses;
- each format's version: the channel, the charter, the transcript, the
  event stream, the chat store and this document's own; and the build's
  identity;
- each account: its source, where it comes from and when it expires,
  never a value;
- the tools' paths, `sh`, `rg` and `git`, resolved;
- the commands' environment by name, and the build environment's
  warnings (section 8).

`--json` prints one versioned JSON document with the same content, for
scripts and the benchmark harness. It exits 0 when everything holds,
warnings included; 2 on a configuration problem; 3 when the only
problems are credentials. Startup runs the same checks (5.3).

### 4.5 `smith login`

```sh
smith login ACCOUNT
```

- **`sign_in` accounts:** opens the loopback listener, shows the
  authorization address, waits for the browser's redirect, exchanges the
  code and keeps the record (section 7).
- **`borrow` accounts:** says which login file it reads, and whether its
  access token is valid and until when, or which program refreshes it.
- **`env` accounts:** names the variable, and says whether it is set.

It exits 0 when the account has a usable credential afterwards, 3 when
it does not, and 2 for an unknown account. `smith exec` never starts a
sign-in: an account without a record ends it with exit 3, naming this
command.

## 5. Settings

### 5.1 Layers

Settings are TOML, strict and bounded. Each layer overrides the one
before, key by key:

1. the built-ins: the presets and defaults (section 6);
2. `$XDG_CONFIG_HOME/smith/config.toml`, the user's;
3. `<repo>/.smith/config.toml`, at the top of the workspace's
   repository, or in the workspace directory when it is not one;
4. `-c KEY=VALUE`, in order;
5. flags.

- **Tables merge, values replace.** The commands' environment is a
  table, so a layer sets one variable without restating the others.
- **The repository layer shapes runs, not authority.** A repository is
  content a person may not have written. Its layer may set instructions,
  conventions, the contract, the model and effort among configured
  endpoints, and environment values. It may not set endpoints, accounts,
  trust, inherited environment names, writable caches, the trace, the
  deployment's values, or a budget above the user's. A key it may not
  set is refused, naming the file.
- **State** lives under `$XDG_STATE_HOME/smith`: chats by workspace,
  sign-in records, temporary chats.

### 5.2 Contents and defaults

A first run needs a model and an account's source:

```toml
model = "codex/gpt-6-astra"
effort = "high"

[accounts.codex]
source = "borrow"
```

| Key | What it is | Default |
|---|---|---|
| `model`, `effort` | the main model, `endpoint/name`, and its effort | none; the provider's effort |
| `profile` | the named bundle of deployment values | `standard` |
| `instructions` | the role (domain/run.md, section 3.1) | smith's own, for a chat or for a task |
| `chat` | the interactive chat | `default` |
| `waiting_seconds` | how long an interactive run waits before it parks | 6.4 |
| `sub_agents` | whether runs may delegate | off (domain/run.md, section 5.3) |
| `[budget]` | spend, seconds, turns and the reserve | 6.4 |
| `[models."<endpoint>/<name>"]` | a model's declared values, and its prices | the preset's, 6.1 |
| `[endpoints.<name>]` | a preset, or its dialect, address, TLS name, trust, headers, identity and account | `codex` and `anthropic` |
| `[accounts.<name>]` | the credential source and its fields (section 7) | none: each account names its source; `sign_in` takes the preset's registration |
| `[deployment]` | declared deployment values | the profile's, 6.3 |
| `[workspace]` | directories, conventions, the contract, delivery fields, the title field, pushes | the workspace directory; a report |
| `[environment]` | `inherit`, the names taken from smith's own environment, and `set` | the core list (section 8) |
| `[build]` | a build preset and writable caches | none |
| `[trace]` | a path and a capture policy (protocol/events.md) | none |

- **Presets.** `codex` and `anthropic` each supply an endpoint and an
  account of that name, and their known models' declared values
  (section 6). A model a preset does not know declares its `window` and
  `output` in settings.
- **Prices.** A preset model carries notional prices, the provider's list
  prices, so that spend is real (domain/run.md, section 9). A
  subscription account's spend is shown as notional, never as a charge.
  The local host's unit is the micro-dollar; settings and the terminal
  use dollars.
- **Raw byte limits are not settings.** Tests may override them; a person
  declares quantities (protocol/limits.md).

### 5.3 Validation

- **Each layer is read strictly.** An unknown key, a wrong type, or a
  value past its bound is refused, naming the file, the line and the
  key's path. A file holds at most 1 MiB.
- **The resolved settings are checked whole,** by `smith check`'s
  checks:
  - every relationship between declared values (protocol/limits.md);
  - the budget against the profile's ceilings, and against one
    completion's maximum cost, so an impossible spend is refused before
    a run starts (domain/run.md, section 9);
  - each model known to a preset or declared;
  - each endpoint's address resolved, and each account's source usable;
  - the tools' paths.
- **A refusal names the relationship:** the keys on each side, their
  values and their layers, and the relationship's name, for example:

  ```text
  smith: budget.turns = 5000 (from -c) exceeds the standard profile's ceiling of 4096 turns
  ```

- **Startup is `smith check`.** Every command runs its checks first and
  refuses with exit 2, or 3 for credentials.

### 5.4 Generated agent configuration

Hosts generate an agent's configuration, so it stays JSON
(protocol/agent.md, section 4): `smith-agent CONFIG.json`. In the
product the shell writes the agent configuration from the presets and
the resolved settings: in one process as a value it hands the service,
with no file; when it spawns an agent (the worlds, and an `isolate`
option later), as that JSON, with the spawned agent's `exit_grace`
derived beside it (6.5).

## 6. Presets' declared values

These are the product's defaults: the per-model values each preset
declares, the `standard` profile's deployment and policy values, and
the defaults of a run the local host starts. Everything else is derived from them (protocol/limits.md) and checked at
startup. Each is validated by the probes of benchmarks.md, and moves when
a probe shows it should. "Provisional" marks a value whose evidence is
still thin. Settings override any of them (section 5). In code, the
per-model values and the run defaults are `smith-local-shell`'s
presets, and the profile's values are the service crates' `standard`
profile (2.1).

### 6.1 Per model

| Preset | Model | `window` | `output` | `reasoning_item` | `head` | `idle` |
|---|---|---:|---:|---:|---:|---:|
| `codex` | `gpt-6-astra` | 272,000 | 32,000 | 64 KiB | 60 s | 300 s |
| `codex` | `gpt-6-luna` | 272,000 | 32,000 | 64 KiB | 60 s | 300 s |
| `anthropic` | `claude-opus-5-5` | 200,000 | 32,000 | 64 KiB | 60 s | 120 s |
| `anthropic` | `claude-sonnet-5-5` | 200,000 | 32,000 | 64 KiB | 60 s | 120 s |
| `anthropic` | `claude-haiku-5-5` | 200,000 | 32,000 | 64 KiB | 60 s | 120 s |

- **`window`** is usable input tokens; **`output`** is the most output
  tokens, the wire's cap where the dialect sends one and the reservation
  in the window check (domain/session.md, section 8.1).
- **The Anthropic models take 1,000,000 input and 128,000 output
  tokens.** The preset declares less, so that a window is compacted
  before its cost grows and the check reserves a likely output. Settings
  raise them.
- **All per-model values are provisional.** The history probe validates
  `window`, the large-write probe `output`, and the slow-completion probe
  `reasoning_item`, `head` and `idle`.
- **A model no preset knows** declares `window` and `output`;
  `reasoning_item`, `head` and `idle` default to its provider's values in
  this table.
- **The small tier.** `gpt-6-luna` and `claude-haiku-5-5`, each at `low`
  effort, are the models benchmarks' probes run on (benchmarks.md,
  section 7). Each preset model takes its provider's effort names, `low`
  among them; without `effort`, the provider's default applies.
- **Every preset holds a window.** With the `standard` profile's 80%
  threshold, a window's room after two outputs, counting a byte as a
  token (protocol/limits.md, 3.9), is 96,000 tokens for the Anthropic
  models and 153,600 for the Codex models, and the headroom above the
  threshold, less one output, is 8,000 and 22,400. Both pass
  `fresh-window` and `compaction-headroom`. Halved after smith's own
  part, the room bounds one offer, and so a message, a person's line or
  a headless prompt, near 20 KiB when an Anthropic model is declared and
  near 34 KiB with the Codex models alone; `smith check` prints the
  exact values.

### 6.2 Per provider

| | `codex` | `anthropic` |
|---|---|---|
| endpoint | `chatgpt.com:443`, TLS name `chatgpt.com`, the Codex dialect | `api.anthropic.com:443`, TLS name `api.anthropic.com`, the Anthropic dialect and its identity profile |
| `connect`, `handshake` | 10 s each | 10 s each |
| `connection_keep` | 30 s, provisional | 30 s, provisional |
| sign-in | the provider's public client, where smith may use it (section 11) | the same |

The connection keep is a live service's policy, never how it ends:
pools close at the answer (section 9). It and the deadlines are policy
values of protocol/limits.md, 2.3.

### 6.3 Per deployment: the `standard` profile

The declared quantities of protocol/limits.md, section 2.2:

| Value | Default | Status |
|---|---|---|
| `conversations` | 6 | provisional; the parallel-children probe |
| `tool_payload` | 64 KiB | provisional; the large-write probe |
| `calls_per_response` | 16 | provisional; the parallel-read probe |
| `read_window` | 32 KiB; a call may ask up to `tool_payload` | decided |
| `guide` | 16 KiB, a guide kept whole up to it | decided |
| `shell_output` | 16 KiB | provisional; the tool-output probe |
| `shell_head`, `shell_tail` | 16 KiB and 48 KiB: a call may ask for up to 64 KiB, kept in that proportion | model-selectable decided; sizes provisional, the tool-output probe |
| `search_hits`, `search_bytes` | 100 and 16 KiB | provisional; the tool-output probe |
| `list_entries` | 512 | provisional |
| `llm_pool` | 128 MiB | provisional; peak memory in `session.ended` |
| `memory` | 2 GiB | decided as at most 2 GiB; the derived worst case must fit it |

### 6.4 Run defaults

| Value | Default | Status |
|---|---|---|
| `budget.spend` | 25 dollars, notional | provisional |
| `budget.seconds` | 1 hour per activation | provisional |
| `budget.turns` | 512, a loop guard | provisional |
| `budget.reserve` | 2 turns and 5 minutes | provisional; the budget probe |
| waiting time | 15 minutes interactive; 0 for `smith exec` | provisional |
| `sub_agents` | off | decided, until the reserve lands |

### 6.5 Policy values

The profile's values that bound no bytes, under protocol/limits.md's
names (section 2.3):

| Value | Default | Status |
|---|---|---|
| `max_turns`, `max_spend`, `max_time`, `max_waiting` | 4,096 turns, 1,000 dollars notional, 24 hours of wall time, 24 hours of waiting | provisional |
| `sections`, `host_tools`, `verdicts`, `items`, `fields` | the charter schema's ceilings | decided: the profile narrows none |
| `inbox` | 64 messages unread | provisional |
| `unacknowledged` | 64 turns | provisional |
| `compaction_threshold` | 80% of the window | decided as about 80%; the history probe |
| `shell_timeout` | 2 minutes | provisional; the tool-output probe |
| `tool_deadline` | 20 minutes, the most a command's call may ask for | provisional; the tool-output probe |
| `group_stop` | 1 s for each of its three steps | provisional; the process-tree probe |
| `close` | 5 s | provisional |
| `write_deadline` | 30 s for one write of the event stream | provisional |
| `cancel_grace` | 10 s | provisional |
| `exit_grace` | derived: the agent's teardown bound, two `close`s and its cancels' settling, plus one more `close` as a margin (skein's `shell.md`, section 13); a setting below the bound is refused | decided as derived; the margin provisional |

## 7. Credentials

Each account has one source:

- **`sign_in`:** smith's own OAuth sign-in (protocol/hosts.md, section
  5.4), with the preset's registration and a `localhost` redirect where
  the provider's client uses one. The record is kept in smith's private
  secret store under `$XDG_STATE_HOME/smith/tokens`, and its refresh
  token never leaves it.
- **`borrow`:** another tool's login file, such as Codex's, read-only.
  smith takes the access token and the account's identifier, and nothing
  else: it never reads the refresh token into its state, never writes
  the file, and never refreshes another tool's login. A borrowed token
  is read again when a grant is due, so a login the other tool refreshed
  meanwhile is used.
- **`env`:** a named variable holding the credential the endpoint's
  dialect sends, for CI.

What the person sees:

- **`smith login ACCOUNT`** acquires or reports a credential (4.5).
- **Interactive sign-in:** a `sign_in` account without a record shows the
  authorization address at the terminal, and the run starts once the
  record is kept.
- **The expiry notice** for a borrowed login names the file, when it
  expired, and the exact program to run:

  ```text
  smith: the codex login borrowed from ~/.codex/auth.json expired at 14:02. Run `codex` once to refresh it, then try again.
  ```

  No run starts on it: exit 3 headless, and interactively the line is
  shown as not delivered.
- **A rejected credential** mid-run is a notice to the host
  (domain/host.md, section 7). The local host re-reads a borrowed login
  or refreshes its own record, and lends the new grant; without one, the
  run fails for credentials.
- **Never shown:** credential values appear in no setting, event, trace,
  terminal line or `smith check` output.
- **Refresh tokens are never copied.** An issuer rotates them, and a
  copied one logs the other tool out. Live tests and benchmarks use the
  user's existing logins, borrowed read-only (testing.md, section 2.3;
  benchmarks.md, section 10).

## 8. The build environment

- **The commands' environment is the configuration's, whole**
  (protocol/agent.md, section 3): what `[environment]` inherits and sets,
  and nothing else. Shells and checks see it; search sees none.
- **A named core list.** By default `inherit` names `PATH`, `HOME`,
  `USER`, `LOGNAME`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TZ`, `TERM` and
  `TMPDIR`, each taken from smith's own environment when present. Names
  are listed, never matched by a pattern. A build preset adds its own:
  `rust` adds `CARGO_HOME` and `RUSTUP_HOME`.
- **Writable caches.** A build preset, or `[build].caches`, names the
  directories outside the workspace that builds write: for `rust`,
  `$CARGO_HOME/registry` and `$CARGO_HOME/git`. `smith check` confirms
  each exists and can be written. Once contained trees exist, they are
  the writable part of the view (protocol/agent.md, section 3).
- **Wrapper warnings.** A build preset names the wrappers its tools may
  be configured with, such as `rustc-wrapper` for `rust`. `smith check`
  warns when a configured wrapper's cache is not among the writable
  caches, or cannot be written, and names the variable or file that
  configured it.
- **Credential-like names.** `smith check` warns when a variable the
  commands would see has a name that suggests a credential.
- **Per-tree limits later.** CPU, memory and process limits per command
  tree come with contained trees. Until then, settings do not stand in
  for them.

## 9. One process

- **The product is one process.** `smith-local-shell` composes
  `smith-local-service`: the local domain, which owns the inline agent
  as its child domain, composed with the effects, the agent's LLM and
  machine components, the chat store, the terminal and the event sink,
  in one protocol layer: one loop, one ring, no channel
  (domain/host.md, section 9). The agent configuration is a value the
  shell builds from presets and settings, never a file. Credentials are
  lent as entities. The loop is `skein_shell::drive`.
- **Events come from the domain's owner.** The inline agent's owner
  drains each run's facts every pass into the service's event sink, so
  the event stream is the same in one process as from a spawned agent
  (protocol/events.md).
- **Stopping:** an interrupt is the run's cancel; a second, or a run past
  its grace, is dropped, and everything below it settles before the
  process ends (domain/host.md, section 9.2).
- **The end:** after its last word, the process closes everything it
  owns and exits once each has reported closed and io is empty, when
  `drive` returns (skein's `shell.md`, section 13). The LLM pool closes
  at the answer. A termination signal while closing turns the remaining
  closes into aborts. A host that spawns `smith-agent` derives its exit
  grace from the same bound (6.5).
- **The spawned capability.** Spawning an agent is a library capability:
  `smith-host-domain` and `smith-host-protocol` in a host, and
  `smith-agent-shell` in the process. `smith-agent` ships while any host
  may want a prebuilt agent process, and is the real-process proof of
  the channel. An `isolate` option of the product, which spawns its
  agent, comes with contained trees.

## 10. The world

Within the focused suite's 15 seconds and the fuzzy suite's 60:

- **Step tests in `smith-local-shell`:**
  - each command's arguments to its typed value, and each usage error;
  - each layer's merge, the environment per variable;
  - one refusal per strict rule, naming the file, line and key;
  - each key the repository layer may not set, refused;
  - preset expansion, byte-stable, against goldens;
  - one refusal per relationship, naming it;
  - the mapping from every answer to its exit code and its last line on
    standard error, one per class;
  - borrowed login files: valid, expired, malformed, and one whose refresh
    token never reaches the shell's state.
- **Step tests in `smith-agent-service` and `smith-local-service`:** the
  `standard` profile derives, and its derived worst case fits `memory`.
- **The local host's simulated world** (testing.md, section 2.1):
  - one headless story per exit code but 2;
  - events against what the fake provider saw: one `response.completed`
    per response, tool records paired by call id, usage summing to the
    answer's spend, no loss;
  - a borrowed login lent; an expired one gives the notice and no run;
    the login file is never written;
  - an `env` account.
- **End to end** (testing.md, section 2.3), against fakes:
  - `smith exec --json` to an accepted answer, and to needs input;
  - `smith check --json`, passing and failing;
  - a startup refusal of each layer;
  - `smith` on a terminal: a first sign-in and a change committed in
    place, then a second invocation that resumes; and one with
    `--trace`, its records read from the file as it grows;
  - `smith login` against the fake issuer;
  - `smith-agent`, spawned by a host in the test's loop.

## 11. Open questions

- **Public clients:** whether each provider sanctions smith's use of its
  public client for `sign_in`. Until one does, its preset's accounts use
  `borrow` or `env`.
- **The Anthropic window:** whether the preset's window should be the
  models' 1,000,000 tokens once the `history` and `cache-reuse` probes measure
  cost and quality there.
- **Tiered list prices:** Haiku 5.5's list price rises for prompts past
  100,000 tokens, and a charter's prices are flat. Proposed: the preset
  prices it at the higher rate whenever its window passes that size, so
  spend is never under-counted.
- **Kept traces:** a default place for traces, their rotation and how
  long they are kept.
- **Repository trust:** whether refusing authority keys in the repository
  layer is enough, or a person should approve a repository's settings
  once.
- **`smith-agent`'s life:** whether it outlives temper's decision on how
  its worker runs agents.
- **An `isolate` option,** once contained trees make spawning contain
  more than one process does.
