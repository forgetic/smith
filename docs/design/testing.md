# Testing smith

Provisional, 2026-10-08. How smith is tested. The strategy is skein's
`docs/foundation/testing-strategy.md`: the tiers, faults, what every fake
shares, what every world checks, scenarios and the referee, the two
suites, and where a failure is fixed. This document keeps what is
smith's:
- its tiers, and its live tests (section 2);
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
  that a spawn starts, so one world holds a host and its agent.
- **The real loop is one thread, one loop, one ring.** The local host
  runs as it ships, and the agent it spawns is hosted in the same loop
  over pipes. The fakes serve on loopback. Only git, rg, sh and the
  checks run outside the loop.
- **Plaintext below the real loop.** The replaying tiers connect to their
  fakes in plaintext on loopback, so they replay. TLS runs in the real
  loop.
- **Live tests run the real loop against real backends:** the
  providers, their issuers and a git remote. They are opt-in, and never
  part of the gate.
- **One scenario, every tier its fakes reach.** The local host's
  simulated world and the real loop run the same processes, scenarios
  and referee. Only the loop beneath them changes.
- **The fakes are skein's.** smith adds only the scripted neighbours its
  own worlds need: a host, a parent, a person at a terminal, a browser.

## 2. smith's tiers

| Tier (testing-strategy.md, section 2) | In smith |
|---|---|
| step tests | every crate; the codecs' goldens, drift and bounds |
| domain worlds | the agent's tools, session and run, each with the world as its parent; the agent's root with its children, on a scripted host; the host domain, on a scripted parent and agent |
| system worlds | the local host with the agent in it, a scripted person and skein's fake provider |
| machine worlds | skein's: smith has no protocol machine of its own |
| protocol worlds | the channel's two halves (protocol/channel.md, section 10); the LLM component against skein's fake provider (protocol/llm.md, section 10) |
| io worlds | skein's: smith does not retest io |
| simulated worlds | the machine component; the agent service; a host and the agent it spawns; the local host (2.1) |
| real loop | the local host as it ships, with the agent it spawns (2.2) |

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
- **The worlds:**
  - the machine component (protocol/agent.md, section 8);
  - the agent service with a scripted host on its channel and a fake
    provider (protocol/agent.md, section 8);
  - a sample host spawning the agent service: the routing another host
    copies (protocol/hosts.md, section 7);
  - the local host with a scripted terminal, a browser, the fake issuer,
    the fake provider and its checkout (protocol/hosts.md, section 7).

### 2.2 The real loop

- **One thread, one loop, one ring** (testing-strategy.md, section 2.8;
  skein's `examples.md`, section 6). In it:
  - the local host as it ships, with its agent spawned or in process;
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

### 2.3 Live tests

- **The real loop against real backends.** The world is 2.2's, with each
  fake replaced:
  - the providers' endpoints, over TLS with the machine's roots and each
    provider's identity profile;
  - the providers' OAuth issuers, for refresh;
  - a git remote, for a configured push, when one is named.
- **Opt-in.** They form a suite of their own, `live`, chosen by its
  profile: one at a time, without retries, with a generous timeout.
  Neither the focused nor the fuzzy suite runs them, so the gate never
  does.
- **Credentials come from the caller,** named by environment variables. A
  test that lacks one fails, saying which.
- **A token directory of their own.** Refresh tokens rotate, so the live
  tests keep a durable token directory, signed in once by hand with the
  binary's own sign-in, and refreshed by every run. It is never the
  user's own directory.
- **Stories,** for each provider:
  - a run refreshes its grant against the issuer;
  - a chat ends with a commit in place;
  - a second run resumes the chat from its files;
  - with a remote named, the push lands, and the test removes the branch
    afterwards.
- **Outcomes, not words.** An LLM's words vary, so a story asks for a
  precise outcome, such as a file with given content, and the referee
  checks that outcome.
- **What they show:** each provider's wire as it is today (its dialect,
  identity headers, TLS and roots), refresh and rotation at the issuer,
  and a push to a real forge. A failure is captured as a recorded
  exchange for skein's fakes, and rerun lower down
  (testing-strategy.md, section 9).

### 2.4 Beside the tiers

The binary's own tests run `smith` as a process:
- its startup refusals;
- under a pseudo-terminal, a person's interrupt reaching the host alone,
  not the agent it spawned (skein's `io.md`, section 6).

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
- the local host's chat files, token files and deliveries;
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
- **Serving a fake on loopback is skein's:** one service shape for any
  fake peer, which smith's worlds configure.

## 5. Scenarios and the referee

- **A scenario** says, in the fakes' own terms:
  - what the person types;
  - what the provider answers;
  - the issuer's accounts;
  - the workspace's repositories and checks;
  - the faults, drawn from its seed.
- **The referee watches from outside:**
  - the lines the terminal received;
  - what the fake provider and the fake issuer saw;
  - the facts the services emit;
  - the checkout, read through one small face: its head, and a commit's
    message and files. The fake checkout answers it in simulation, and
    git does in the real loop.

  It never reads a service's state, the simulator's or the fake
  machine's.
- **One referee per scenario, every tier:** the local host's simulated
  world and the real loop share theirs. A domain world's referee is its
  own, as its scenarios stay in its tier.

## 6. What the worlds check

Every world checks what the strategy lists (testing-strategy.md, section
6). In smith:

- **Facts change nothing:** a run is the same whether its facts are kept
  or dropped.
- **Memory** is measured per step, against each process's worst case, at
  every iteration in the simulated worlds and the real loop.
- **Replay** compares traces, and digests of the state once skein's
  harness takes them.
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
- **focused tests:** the step tests, and each world's scenarios, referee
  tests, replay, facts changing nothing, memory at the worst case, and
  the real loop;
- **fuzzy tests:** each world's `tests/fuzzy_*.rs`, sweeps over seeds
  under faults.

The live tests are a third suite, outside the gate: the real loop's
`tests/live.rs`, under the `live` profile only (2.3).

## 7. Layout

```
crates/*/src/tests.rs        step tests
tests/codecs                 the codecs: goldens, drift, bounds, fuzzy decoding
tests/tools                  the tools child domain's world
tests/session                the session child domain's world
tests/run                    the run child domain's world
tests/agent                  the agent's root with its children, on a scripted host
tests/host                   the host domain, on a scripted parent and agent
tests/local                  the local host with the agent in it: a system world
tests/channel                protocol world: the channel's two halves
tests/protocol-llm           protocol world: the LLM component against skein's fake provider
tests/machine                simulated world: the machine component
tests/agent-process          simulated world: the agent service
tests/hosts                  simulated world: a sample host and the agent it spawns
tests/local-process          simulated world: the local host and its peers
tests/real                   the real loop, and its live tests
tests/*/tests/*.rs           a world's focused tests
tests/*/tests/fuzzy_*.rs     its fuzzy tests
crates/smith/tests           the binary's own tests, beside the tiers
```

A world's package is `smith-<name>-world`. The world harness, the
schedule, the referee plumbing and the counting allocator are skein's;
smith keeps only its worlds, its scripted neighbours and its scenarios.

## 8. Open questions

- **The real loop in CI:** a kernel that allows `io_uring`, with git, rg and
  sh installed (skein's `shell.md`, section 10).
- **Containment in CI:** cgroup v2 delegation and user namespaces in the
  CI's containers.
- **The sample host in the real loop:** whether a second host earns a
  real loop of its own, beside the local host's.
- **Live tests on a schedule:** credentials kept as CI secrets, and how
  often to spend a subscription's usage on them.
