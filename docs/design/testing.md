# Testing smith

Provisional, 2026-10-08, revised 2026-10-09. How smith is tested. The
strategy is skein's `docs/foundation/testing-strategy.md`: the tiers,
faults, what every fake shares, what every world checks, scenarios and
the referee, the two suites, and where a failure is fixed. This document
keeps what is smith's:
- its tiers, end to end included, and benchmarks beside them (section 2);
- its neighbours and their faces (section 3);
- its fakes (section 4);
- its scenarios and referee (section 5);
- what its worlds check beyond the strategy (section 6);
- its layout (section 7);
- what is still open (section 8).

The stories each world tells are in the design of what it tests, in that
document's "The world" section.

## 1. In one page

- **Behaviour is tested in the domain layer.** The agent's child domains,
  the agent's root, the host domain and the local host each have worlds
  of their own. The local host's world runs the agent inside it, and so is
  a system world.
- **Protocol worlds join smith's own halves:** the channel's host and
  agent halves over byte streams cut at random, and the LLM component
  against skein's fake provider.
- **Simulated worlds run processes** on skein's world harness: the
  machine component, the agent service, a host spawning the agent
  service, and the local host with its peers. The harness hosts a service
  that a spawn starts, so one world holds a host and its agent. That
  agent is the shipped adapter, never a test-only copy.
- **The real loop is one thread, one loop, one ring.** The product runs
  as it ships, with its agent inline or spawned and hosted in the same
  loop over pipes. The fakes serve on loopback. Only git, rg, sh and the
  checks run outside the loop.
- **Plaintext below the real loop.** The replaying tiers connect to their
  fakes in plaintext on loopback, so they replay. TLS runs in the real
  loop.
- **End to end, smith runs as it ships:** the root package's binaries as
  processes, against fakes in the focused suite, and against the real
  providers, issuers and a git remote in a live suite run by choice.
- **Benchmarks are not tests.** Real agents on real models are measured
  in a tier of their own, outside both suites and the gate (2.4).
- **One scenario, every tier its fakes reach.** The local host's
  simulated world and the real loop run the same processes, scenarios
  and referee, in both placements where placement matters. Only the loop
  beneath them changes.
- **People react, and no line is lost.** The scripted person types on
  what it observes, with its timing drawn from the seed, and the referee
  checks that every line ends once (section 5).
- **Worlds end as the product ends:** with shipped keep and idle times,
  and under skein's teardown invariant (section 6).
- **The fakes are skein's.** smith adds only the scripted neighbours its
  own worlds need: a host, a parent, a person at a terminal, a browser.

## 2. smith's tiers

| Tier (testing-strategy.md, section 2) | In smith |
|---|---|
| step tests | every crate; the codecs' goldens, drift and bounds; the shell crates' parsing, settings and refusals; the profile's derivation properties (2.5) |
| domain worlds | the agent's tools, session and run, each with the world as its parent; the agent's root with its children, on a scripted host; the host world, on a scripted parent: the host domain with a scripted agent, and the inline agent with smith's real domain, under one referee (domain/host.md, section 10) |
| system worlds | the local host with the agent in it, a scripted person and skein's fake provider |
| machine worlds | skein's: smith has no protocol machine of its own |
| protocol worlds | the channel's two halves (protocol/channel.md, section 10); the LLM component against skein's fake provider (protocol/llm.md, section 10) |
| io worlds | skein's: smith does not retest io |
| simulated worlds | the machine component; the agent service; a host and the agent it spawns; the local host, in both placements (2.1) |
| real loop | the product as it ships, with its agent inline or spawned (2.2) |
| end to end | the root package's binaries: `smith` and `smith-agent`, against fakes or, live, real backends (2.3) |
| benchmarks | not a test tier: real agents on real models, measured (2.4) |

Domain worlds use domains and fakes only, never protocol crates.

### 2.1 Simulated worlds

- **On skein's world harness** (skein's `examples.md`, section 6): each
  process's `iterate` over the simulator, one loop, the referee beside
  them. A spawn of the agent service starts it in the same world, joined
  to its host by simulated pipes (skein's `simulator.md`, section 3).
- **The machine sits behind the simulator** (3.2). The fake peers listen
  on the simulated network, in plaintext.
- **Calm and faulted.** Every world runs its focused scenarios calm, and
  sweeps seeds under the simulator's faults in its fuzzy tests,
  asserting that each fault fell (testing-strategy.md, section 3).
- **One shared agent adapter.** Wherever a world spawns an agent, the
  harness hosts `smith-agent-shell::Agent`, the adapter `smith-agent`
  runs (shell.md, section 2.2). No world keeps a copy of it.
- **The worlds:**
  - the machine component (protocol/agent.md, section 8);
  - the agent service with a scripted host on its channel and a fake
    provider (protocol/agent.md, section 8);
  - a sample host spawning the agent service: the routing another host
    copies (protocol/hosts.md, section 7);
  - the local host with a scripted terminal, a browser, the fake issuer,
    the fake provider and its checkout (protocol/hosts.md, section 7), in
    both placements (2.6).

### 2.2 The real loop

- **One thread, one loop, one ring** (testing-strategy.md, section 2.8;
  skein's `examples.md`, section 6). In it:
  - the product as it ships: the local host with the inline agent, or
    with the agent it spawns;
  - the agent it spawns, hosted by the harness and joined to the host by
    pipes;
  - the scripted terminal, on pipes;
  - the browser, following the sign-in's redirect to the host's loopback
    listener;
  - the fake provider and the fake issuer, serving TLS on loopback with
    skein's test certificates.
- **Outside the loop:** git, rg, sh and the checks, as children in a
  scratch directory. The directory holds:
  - the settings;
  - the state and token directories;
  - the workspace's repositories.
- **Signals:** the local host reads the process's signals as it ships,
  and the referee sends it real ones. The agent's arrive through the
  harness's pipe.
- **The local host's simulated world's scenarios and referee** run here
  unchanged.
- **What it shows:**
  - real programs' output, git's included;
  - TLS;
  - signals;
  - a process tree that ends;
  - once contained trees exist, containment (section 6).
- It runs in the focused suite, and fails, saying why, where `io_uring`,
  git, rg or sh is missing.

### 2.3 End to end

- **smith as it ships** (testing-strategy.md, section 2.9): the root
  package's binaries (shell.md, section 3), each a process with its own
  loop, started by the test by its program:
  - `smith`, the product in one process: interactive, `exec`, `check`
    and `login`;
  - `smith-agent`, the agent a host spawns.

  The tests are the root package's own targets, so cargo names the
  binaries to them (section 7).
- **The test's side is one loop on the real ring,** driving:
  - the scripted person, on a pseudo-terminal;
  - the browser;
  - the fake provider and the fake issuer, serving TLS on loopback with
    skein's test certificates;
  - for `smith-agent`, the sample host, which spawns it by its program.
- **The scratch directory** is 2.2's. The settings' user layer and the
  state directory are in it, so no test reads the user's own.
- **The referee reads what shows from outside:**
  - the terminal's transcript;
  - what the fakes saw;
  - the files and git;
  - the exit codes and standard error;
  - the event stream, from `--json` or `--trace`, read as `smith-events`
    records, never as text (protocol/events.md).
- **Against fakes,** in the focused suite and within its budget. These
  tests are few, one per path through `main`:
  - the startup refusals: one per settings layer, each naming its key,
    with exit 2; and an account without a credential, with exit 3;
  - `smith check --json`, passing and failing;
  - `smith exec --json` to an accepted answer, its events and exit 0; and
    to a parked one, exit 5;
  - `smith` on a terminal: a first run that signs in and ends with a
    commit in place; a second invocation that resumes the chat from its
    files; an interrupt that cancels the run. The interrupt reaches the
    process alone (skein's `io.md`, section 6);
  - `smith login` against the fake issuer and the browser;
  - `smith-agent` spawned by the sample host: the real-process proof of
    the channel.
- **Live,** the real backends replace the fakes:
  - the providers' endpoints, over TLS with the machine's roots and each
    provider's identity profile;
  - their OAuth issuers;
  - a git remote, for a configured push, when one is named.

  The live tests form a suite of their own, `live`, chosen by its
  profile: one at a time, without retries, never at the gate. They check
  that each wire is right; they measure nothing, which is benchmarks'
  (2.4).
  - **Credentials come from the caller,** named by environment variables.
    A test that lacks one fails, saying which.
  - **A token directory of their own.** Refresh tokens rotate, so the live
    tests keep a durable token directory, signed in once by hand with
    `smith login` and refreshed by every run. It is never the user's own
    directory, and no refresh token is copied into it.
  - **One credential guard, shared with `smith-bench`.** The same guard
    refuses a token directory or a dedicated home placed under a user's
    own tool directories, for the live tests and the benchmarks alike
    (benchmarks.md, section 10). Neither keeps a copy; which crate holds
    it is benchmarks.md, section 15's.
  - **The small tier by default.** The live tests run on each provider's
    small-tier model unless the caller names another (benchmarks.md,
    section 7).
  - **Stories,** for each provider, through `smith exec --json`:
    - a run refreshes its grant at the issuer;
    - a run ends with a commit in place (`--deliver`);
    - a second run resumes it (`--chat`);
    - with a remote named, the push lands, and the test removes the
      branch afterwards;
    - a borrowed login, from the other tool's dedicated login, lends a
      grant, and its file is unchanged.
  - **Outcomes, not words.** An LLM's words vary, so a story asks for a
    precise outcome, such as a file with given content, and the referee
    checks that outcome and the typed events, read through `smith-events`
    and never as text: the answer, its exit code, and where the task
    appears in the prompts.
  - **What they show:** each provider's wire as it is today, refresh and
    rotation at the issuer, and a push to a real forge. A failure is
    captured as a recorded exchange for skein's fakes, and rerun lower
    down.

### 2.4 Benchmarks

- **Not a test tier.** Benchmarks run real agents (smith, Codex and
  Claude Code) on real models, and measure what a change does in the
  world: that a fix works, and keeps working. They are designed in
  benchmarks.md.
- **Outside both suites and the gate.** Their offline part (the
  harness's parsers, task manifests and seeds) is ordinary tests in the
  focused suite.
- **Beside the live suite, not inside it.** A live test checks once that
  a wire is right, by its outcome. A benchmark repeats, measures and
  compares.
- **What they share:** dedicated credentials behind the one credential
  guard (2.3), and the one typed reader of the event stream,
  `smith-events`. Neither keeps a guard or a reader of its own.
- **A probe's failure is a finding,** reproduced and fixed in the lowest
  tier that shows it (2.5). The probe stays, as the fix's guard in the
  world.

### 2.5 Which tier a test belongs to

- **A boundary is tested where it lives.** An exact bound (at the cap,
  and one past it) is a test of the tier that owns the bound, with tiny
  limits: a decoder's in skein's tier, the session's in its world. A
  higher tier does not reach a lower one's boundary through a pump of its
  own (testing-strategy.md, section 9).
- **Derivation properties replace pinned constants.** A profile's test
  states relationships, not numbers: for declared values drawn at random,
  the derivation either refuses, naming the relationship, or yields
  limits that keep every relationship of protocol/limits.md; and the
  `standard` profile derives and fits its memory. No test asserts a
  shipped value; shell.md, section 6 holds them.
- **Profile tests live with the profile,** in the service crates.
- **End to end is for paths through `main`** (2.3). Behaviour is shown in
  the domain worlds first.

### 2.6 Placement

- **A parameter where both placements matter.** The local host's
  simulated world and the real loop run such a scenario with the inline
  agent and with the spawned agent, under one referee.
- **Where it matters:** what crosses the agent's boundary differently in
  each: messages and their terminals across activations; interrupts and
  stops; failures and how they show; resume and replay; the event stream.
- **Elsewhere,** a scenario runs in the product's placement, the inline
  agent.
- **Fuzzy sweeps** draw the placement from their seed.

## 3. Neighbours and their faces

### 3.1 Peers

A peer's fake grows the layers that the smith component facing it grows,
and each tier joins the two at the lowest layer both have
(testing-strategy.md, section 4.1):

```
agent        domain ─ protocol ─ io   ⇄   io ─ protocol ─ domain   fake LLM provider
local host   domain ─ protocol ─ io   ⇄   io ─ protocol ─ domain   fake OAuth issuer
host         domain ─ protocol ─ io   ⇄   io ─ protocol ─ domain   agent
```

- **A host and its agent are each other's peer.** Both halves are smith's.
  Their worlds use the other half, or skein's fake channel scripted with
  the channel's records (protocol/channel.md, section 10).
- **The person** meets the local host as lines at a terminal, and as a
  browser that follows the sign-in's redirect.

### 3.2 The machine

The machine's fake grows no protocol or io layers (testing-strategy.md,
section 4.3). It shows a face to whichever layer of smith sits just above
it:

| smith is real down to | The machine's face | It answers |
|---|---|---|
| the domain | skein's fake checkout | the tools' files, searches, commands and checks; the local host's git |
| io | behind the simulator: skein's fake machine and fake checkout, with smith's adapters for its programs | the file and process operations the simulator passes on |
| everything | none | the real kernel, in a scratch directory; in contained trees once they exist |

Every component's files and processes go through it:
- the agent's tools and checks;
- the local host's chat files, token files and deliveries, and a
  borrowed login's file, which the scenario writes and smith only reads;
- a host's agent process.

## 4. The fakes

- **The LLM provider:** skein's fake provider. Its domain face serves the
  domain worlds. Its protocol face serves bytes to the protocol worlds.
  As a service on io, it listens on loopback for the simulated worlds and
  the real loop. Its recorded exchanges are skein's.
- **The OAuth issuer:** skein's fake issuer, with the same three faces.
- **The machine:** skein's fake checkout and fake machine (3.2). smith
  seeds repositories for its scenarios, and adapts programs to them.
- **The channel's far end:** skein's fake channel, a scripted peer that
  speaks the channel's records.
- **smith's scripted neighbours** are ordinary Rust, each in the world
  that needs it: the host in the agent's world, the parent in the host
  domain's, the partner in the run's. The terminal and the browser run in
  both the local host's simulated world and the real loop, so they are
  step machines (testing-strategy.md, section 4).
- **No copies of shipped code.** A world hosts what ships: the agent
  process is `smith-agent-shell::Agent` and the local host is
  `smith-local-shell`'s `Local`. The agent's standard error is a writer
  the world observes, so the referee can check the tail its host
  reports.
- **Fakes do not end things for their clients.** A fake peer stays live
  until its client closes, unless the scenario is about the peer hanging
  up (section 6).
- **Serving a fake on loopback is skein's:** one service shape for any
  fake peer, which smith's worlds configure.

## 5. Scenarios and the referee

- **A scenario** says, in the fakes' own terms:
  - what the person types, and on what it waits before typing it;
  - what the provider answers;
  - the issuer's accounts;
  - the workspace's repositories and checks;
  - the faults, drawn from its seed.
- **The person script** types on what it observes: a turn, an answer,
  waiting, a line shown, a notice, or a number of steps drawn from the
  seed. It may close its input at any point. In the fuzzy sweeps its
  lines, their triggers, the store's delays and where input ends are
  drawn from the seed.
- **The referee watches from outside:**
  - the lines the terminal received;
  - what the fake provider and the fake issuer saw;
  - the facts the services emit, and the event stream;
  - the checkout, read through one small face: its head, and a commit's
    message and files. The fake checkout answers it in simulation, and
    git does in the real loop.

  It never reads a service's state, the simulator's or the fake
  machine's.
- **Referees check message terminals** (domain/run.md, section 6):
  - every message a run accepted ends exactly once: read (a turn's fence
    names it or a later one), refused at admission, or unread at the
    answer;
  - fences never go back, and the answer carries the last;
  - a line is relayed at most once per activation, and again only if it
    was never read;
  - the host's report of unread names comes once;
  - a waiting notice appears only when every relayed line was read.
- **No line lost** is liveness: by the end, every line the person entered
  was read or shown as not delivered, a refused one with why
  (domain/host.md, section 8). A line still pending when the referee's
  deadline fires fails the test, naming it.
- **One referee per scenario, every tier:** the local host's simulated
  world and the real loop share theirs. End to end, the referee checks
  the same outcomes from outside (2.3). A domain world's referee is its
  own, as its scenarios stay in its tier.

## 6. What the worlds check

Every world checks what the strategy lists (testing-strategy.md, section
6). In smith:

- **Facts change nothing:** a run is the same whether its facts are kept
  or dropped.
- **Events are complete:** the event stream matches what the fakes saw:
  one response record per provider response, tool records paired by the
  provider's call id, usage summing to the answer's spend, and every loss
  counted exactly (protocol/events.md).
- **Memory** is measured per step, against each process's worst case,
  derived from the declared values the world chose, at every iteration
  in the simulated worlds and the real loop.
- **Replay** compares traces, and digests of the state once skein's
  harness takes them.
- **The teardown invariant,** inherited from skein's world harness: a
  world that ends by itself ends with no deadline firing after the last
  word, other than io's close and retry deadlines (testing-strategy.md,
  section 6). In smith, an agent exits on its own after its answer, and
  its host sends it no signal.
- **Shipped times.** Keep and idle times, and every other time that
  decides how something ends, are the shipped values in every world,
  never ones chosen for tests (shell.md, section 6). No world shortens
  the LLM connections' idle keep, or any other, so that it ends sooner:
  a world ends because its services close what they own after their
  last word, as the product does. Tiny limits are for counts and bytes.
  Simulated time makes the shipped times free.
- **The real loop, once settled:**
  - the scratch directory changed only where the scenario expects;
  - nothing is left running: the agent finished, and every child was
    reaped.
- **Containment, once contained trees exist** (protocol/agent.md, sections
  2 and 3; protocol/hosts.md, section 3), in the real loop:
  - **writes stay in the view:** commands that try to write outside the
    writable directories, a git directory included, fail, and the
    scratch directory shows no such write;
  - **nothing outlives its tree:** commands that fork away, ignore the
    termination signal or hang carry a marker, and none is left once
    their call is answered;
  - **the environment is the configuration's, whole:** a command sees only
    the configured variables and its own pipes;
  - **the agent's tree holds the run:** a host that kills its agent leaves
    nothing the run started.

  These need cgroup v2 delegated to the user and unprivileged user
  namespaces. A test fails, saying so, where they are missing.

### 6.1 The suites

The strategy's two suites (testing-strategy.md, section 8) make the gate,
run by the commands in `docs/development/workflow.md`:
- **focused tests:** the step tests; each world's scenarios, referee
  tests, replay, facts changing nothing and memory at the worst case; the
  real loop; the end-to-end tests against fakes; and the benchmark
  harness's offline tests;
- **fuzzy tests:** each world's `tests/fuzzy_*.rs`, sweeps over seeds
  under faults; the run's world also draws message timings, budgets and
  shares (domain/run.md, section 13), and the session's usage and
  appended sizes (domain/session.md, section 10).

The live tests are a third suite, outside the gate, under the `live`
profile only (2.3). Benchmarks are no suite: they run by their own
command (2.4).

## 7. Layout

```
Cargo.toml                       the workspace, and the root package smith
src/main.rs                      the binary smith: glue
src/bin/smith-agent.rs           the binary smith-agent: glue
crates/*/src/tests.rs            step tests
crates/smith-*-service           the profile, and its derivation properties
crates/smith-*-shell             the shells' parsing, settings and refusals
tests/codecs                     the codecs: goldens, drift, bounds, fuzzy decoding
tests/tools                      the tools child domain's world
tests/session                    the session child domain's world
tests/run                        the run child domain's world
tests/agent                      the agent's root with its children, on a scripted host
tests/host                       the host world: the host domain and the inline agent, on a scripted parent
tests/local                      the local host with the agent in it: a system world
tests/channel                    protocol world: the channel's two halves
tests/protocol-llm               protocol world: the LLM component against skein's fake provider
tests/machine                    simulated world: the machine component
tests/agent-process              simulated world: the agent service
tests/hosts                      simulated world: a sample host and the agent it spawns
tests/local-process              simulated world: the local host and its peers
tests/real                       the real loop
tests/*/tests/*.rs               a world's focused tests
tests/*/tests/fuzzy_*.rs         its fuzzy tests
tests/end-to-end/end_to_end.rs   end to end against fakes: a root package target
tests/end-to-end/live.rs         the live suite: a root package target
tests/end-to-end/support/        their shared setup
benchmarks/                      smith-bench, the benchmark harness (benchmarks.md)
```

- **The end-to-end and live tests are the root package's `[[test]]`
  targets** (shell.md, section 3.1). Cargo gives a package's own tests
  the paths of its binaries, and `tests/end-to-end/` holds no manifest,
  so it is not a package of its own.
- A world's package is `smith-<name>-world`. The world harness, the
  schedule, the referee plumbing and the counting allocator are skein's;
  smith keeps only its worlds, its scripted neighbours and its scenarios.

## 8. Open questions

- **The real loop in CI:** a kernel that allows `io_uring`, with git, rg and
  sh installed (skein's `shell.md`, section 10).
- **Containment in CI:** cgroup v2 delegation and user namespaces in the
  CI's containers.
- **The sample host in the real loop:** whether a second host earns a
  real loop of its own, beside the local host's, now that an end-to-end
  story spawns `smith-agent` from it.
- **Live tests on a schedule:** credentials kept as CI secrets, and how
  often to spend a subscription's usage on them.
- **A reacting person in skein:** whether a scripted neighbour that acts
  on what it observes is generic enough for skein's world kit. It moves
  there when a second service needs it.
- **Both placements within the budgets:** which scenarios run twice once
  the inline agent is the product, and whether the spawned placement's
  share moves to the fuzzy sweeps.
