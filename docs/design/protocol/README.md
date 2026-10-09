# smith's protocol layer

Provisional, 2026-10-06, revised 2026-10-09. The protocol layer of smith
turns smith's vocabulary into bytes and back. It sits between an agent and
its peers: its host, its LLM providers and its machine. It follows skein's
`programming-model.md`, sections 4 and 8, and the domain design in
`docs/design/domain/`, cited as `domain/<file>.md`.

This page covers four things:

- what the layer is made of, and the shells and binaries above it;
- how a project built on skein uses it;
- what carries a version;
- what smith needs from skein.

What is still open is listed in section 11.

## 1. In one page

- **Translation, nothing else.** The protocol layer turns bytes into smith's
  domain entities and back. Every decision is the domain's: deadlines,
  retries, what a failure means, whether to accept a peer. The protocol
  layer runs the timers and the attempts (programming-model.md, section 4).
- **Three peers today, one component each.** The agent talks to:
  - its host, over a channel;
  - LLM providers, through skein's LLM client;
  - the machine (files and processes), through skein's io.

  MCP servers come later, as a fourth.
- **Components, not bundles.** No component makes the stream its channel
  runs on: none spawns, listens or connects for it. Whoever composes the
  components decides how an agent and its host meet: a process spawned, a
  daemon connected to, or one process with no channel at all (section 4).
- **A consumer writes routing and policy.** A project built on skein that
  hosts or embeds smith owns smith's components as children of its own
  layers and routes between them (section 6). It writes no codec of
  smith's records, none of the channel's rules and none of the agent's LLM
  calls. Its policy is what smith leaves to hosts (domain/README.md,
  section 5).
- **No host vocabulary on the wire.** What belongs to a host travels as
  data smith does not interpret:
  - host tools' schemas and inputs;
  - answers' text;
  - messages;
  - results' fields.
- **Generic mechanisms are skein's** (section 9):
  - framed channels;
  - codecs generated from schemas;
  - contained processes, connections, TLS and HTTP;
  - LLM providers and OAuth.

  The same mechanisms carry any project's own protocol, such as temper's
  between its engine and its workers. smith keeps what is about agents.
- **Versions.** The channel's version is agreed as it opens. What outlives
  a channel, or crosses more than one hop, carries its own version: the
  charter with its result, the transcripts, and the event stream. Every
  format is pre-release: each has one version, a change a reader would
  notice is a new one, and a reader refuses any other (section 8).
- **Sized and bounded.** Every payload is a record encoded to a schema,
  with a bound on every length. The bound is checked before anything is
  set aside for the payload (programming-model.md, section 8).
- **Limits are declared, then derived.** A deployment declares a few
  quantities; every limit is derived from them, and every relationship
  between limits is checked at startup, by name (limits.md).
- **Facts have a policy per sink** (domain/run.md, section 11). Inside
  the process none is lost. The channel projects them for liveness,
  dropping and counting what does not fit, and never holds anything
  back. The event stream, a trace file or a headless run's output, is
  complete by default (agent.md, section 5; events.md).
- **The event stream is documented and versioned:** one vocabulary of
  JSON lines, the same for `smith exec --json` and a trace file, which
  changes with smith (events.md).

## 2. Reading order

1. **README.md:** this page.
2. **channel.md:** the channel between a run and its host: its opening and
   versions, the kinds each way and their records, their order, what
   breaks its rules, flow control, grants and facts.
3. **charter.md:** the charter and the result. A party that decides runs
   writes the charter and reads the result, and may not be the host.
4. **transcript.md:** turns and transcripts, as hosts keep them and agents
   resume from them.
5. **llm.md:** the agent's LLM calls: prompts, tools' schemas, decoding and
   rendering, failures and credentials.
6. **agent.md:** the agent process: the machine's files and processes, its
   configuration, its facts and events, and how it starts and ends.
7. **events.md:** the event vocabulary: the stream a process writes, its
   records, what feeds them, and how they change.
8. **limits.md:** declared quantities, the limits derived from them, the
   relationships checked at startup, memory, and what a limit does to one
   call.
9. **hosts.md:** the host's side of a channel, whether the host spawns the
   agent or connects to it, and the local host's protocol layer.

## 3. Names

Each layer names its entities differently, so they are not confused
(programming-model.md, 4.1).

| Name | What it is |
|---|---|
| stream | bytes in order: one way over a pipe, both ways over a socket or TLS's plaintext (programming-model.md, 4.3) |
| channel | one run's framed conversation with its host, over a stream each way |
| half | the agent's or the host's side of a channel |
| frame | a channel's unit: a kind, a length and a body |
| kind | what a frame is, with the direction it goes |
| record | a value of smith's vocabulary as bytes, encoded to its schema |
| schema | a record's fields and their bounds, from which its codec is generated; the wire's specification |
| opening | the exchange that agrees a channel's version before any record crosses |
| component | a crate of smith's protocol layer that a consumer's protocol layer owns as a child and routes to |
| consumer | a project built on skein that hosts smith's agents or embeds them |
| event | one record of the event stream, a JSON line (events.md) |
| sink | where the agent's facts go: the channel's projection, or the event stream, each with its own policy and count of loss |
| declared quantity | a value a deployment declares, from which limits are derived (limits.md) |
| profile | a named bundle of declared quantities and policy values (limits.md) |

## 4. Shapes

The same components make different shapes, and the consumer chooses one.
These three are examples, not a closed list: an agent that connects to its
host would be another.

| Shape | How agent and host meet | The host runs | The agent runs |
|---|---|---|---|
| a spawned agent | the host spawns `smith-agent`, and the channel runs over its standard input and output | skein's contained processes and smith's host half | the `smith-agent` binary |
| a connected agent, later | `smith-agent` runs as a daemon; a host connects to it, with one connection per run, over TLS when remote | a connection and smith's host half | the `smith-agent` binary, listening |
| one process | the consumer owns smith's inline agent as a child, with no channel: the charter and turns cross as bytes, everything else as entities (domain/host.md, 9.4) | its own domain and protocol layer | as the consumer's child, `smith-inline-agent`: smith's domain, with its LLM and machine components in the consumer's protocol layer |

- **A spawned agent** is how temper's worker runs smith. The local host
  offers it later, with contained trees (hosts.md, section 5.6). The
  process tree is the run's containment (domain/host.md, section 4).
- **A connected agent** is not available yet: it waits on the domain. The
  components allow it, but admission, cancellation, a lost connection and
  the daemon's own life have no contract yet. The domain today:
  - runs one run per agent process;
  - supervises only a process its host spawned (domain/host.md, sections 1
    and 4).

  With a connected agent, as in one process, the agent's process is not
  the run's containment: a cancel stops the run and its commands' trees,
  not the daemon. A host that does not share the agent's file system
  cannot prepare or deliver its workspace (section 11).
- **One process** is the product's shape: `smith` composes the inline
  agent in its local host. The inline agent speaks `smith-host-domain`'s
  vocabulary, so a host treats both kinds of agent alike, and its
  requests below the domain go to smith's LLM and machine components in
  the consumer's own protocol layer, which also hands the LLM component
  its credentials directly. What the vocabulary carries as bytes still
  crosses as bytes: the charter, encoded with `smith-charter` and decoded
  at the start, and turns, encoded with `smith-transcript` as they are
  told, which a host keeps as bytes anyway (domain/host.md, 9.4). The
  rest crosses as entities, with no frames. The event stream is encoded
  with `smith-events`.

## 5. Components

| Crate | Role | What it does |
|---|---|---|
| `smith-channel` | codecs | the channel's kinds and its records, for both halves (channel.md) |
| `smith-charter` | codecs | the charter and the result (charter.md) |
| `smith-transcript` | codecs | turns and transcripts (transcript.md) |
| `smith-events` | codecs | the event vocabulary: its records as JSON lines, written and read with skein's JSON (events.md) |
| `smith-protocol-channel` | component | the agent's half: skein's channel and smith's codecs on the streams it is given, translated to and from `smith-domain` |
| `smith-protocol-llm` | component | `smith-domain`'s LLM calls to skein's LLM client and back, with tools' schemas, decoding and rendering (llm.md) |
| `smith-protocol-machine` | component | the workspace's files, searches, commands, checks and guides, through skein's io (agent.md) |
| `smith-host-protocol` | component | the host's half: skein's channel and smith's codecs on the streams it is given, translated to and from `smith-host-domain` (hosts.md) |
| `smith-local-protocol` | protocol layer | the local host's: the terminal, transcripts in files, signing in and committing in place (hosts.md) |
| `smith-inline-agent` | domain | an agent capability in its host's process: one composed `smith-domain` per run, speaking `smith-host-domain`'s vocabulary; smith's side of jig's one host (domain/host.md, section 9) |
| `smith-agent-service`, `smith-local-service` | service | the agent process's and the product's `iterate`; each holds its profiles as step code, the `standard` profile among them, derives its limits and checks them (limits.md), and owns its event sink (events.md) |
| `smith-agent-shell` | shell | the agent process, as a library: its configuration, its startup, and the one adapter that its binary and the worlds both run (agent.md) |
| `smith-local-shell` | shell | the product, as a library: its commands, settings, credentials, composition and front ends (`docs/design/shell.md`) |
| `smith` | binaries | the root package: `smith`, the product, and `smith-agent`, the agent process a host spawns, each glue in `src/` over its shell crate |
| `smith-mcp`, later | component | MCP servers as a tool source |

These are the roles of programming-model.md, section 4:

- **Codecs** depend on lib only, as protocol machines do, and keep no state.
  `smith-events` writes and reads its lines with skein's JSON machine.
- **A component** is a crate of a protocol layer. It depends on lib, io,
  skein's machines and compositions, and the domain whose vocabulary it
  translates: `smith-domain`, the root, for the agent's components, which
  keeps 4.5's rule that only a root faces the protocol layer. Like a child
  domain, it has its own limits, worst case and most outputs per step, its
  own deadlines, and its own tests.
- **Records are smith's own types,** not its domains'. A host needs no
  agent domain to write a charter, a consumer needs none to read events,
  and each component translates between records and its domain with
  small total functions.
- **A service** composes its domain and components, holds its profiles,
  derives its limits from them and checks every relationship before its
  loop starts (limits.md). Its event sink takes the domain's facts and
  the measurements of its layers and shell, and writes records through
  io (agent.md, section 5).
- **Shells are libraries** in ordinary Rust, under skein's shell rules
  (programming-model.md, 2.1). `crates/` holds libraries only; the
  binaries are glue in the root package (`docs/design/shell.md`).

The agent process:

```
smith-agent                        the binary: glue in the root package
└── smith-agent-shell              the shell: configuration, startup, the loop's adapter
    └── smith-agent-service        its iterate, profiles and limits
        ├── smith-domain           the agent (domain/README.md, section 4)
        └── its protocol layer
            ├── smith-protocol-channel     to its host, on the streams it is given
            ├── smith-protocol-llm         to providers, through skein's LLM client
            ├── smith-protocol-machine     to files and processes, through skein's io
            └── its event sink             records, with smith-events, through skein's io
```

The product:

```
smith                              the binary: glue in the root package
└── smith-local-shell              the shell: commands, settings, credentials, front ends
    └── smith-local-service        its iterate, profiles and limits
        ├── smith-local-domain     the local host
        │   └── smith-inline-agent     its agent: a composed smith-domain per run
        └── its protocol layer
            ├── smith-local-protocol       the terminal, chats in files, sign-in, commits
            ├── smith-protocol-llm         the inline agent's LLM calls
            ├── smith-protocol-machine     the inline agent's files and processes
            └── its event sink             to standard output or a trace file
```

A host:

```
a host (temper's worker, smith's local host, ...)
├── its domain
│   └── its agents, of either kind
│       ├── smith-host-domain      agents as processes: the channel's rules, the watchdog
│       └── smith-inline-agent     agents in its process: a composed smith-domain per run
└── its protocol layer
    ├── smith-host-protocol        one channel per run, on the streams it is given
    ├── skein's contained processes, or a connection: how the streams are made
    └── smith-protocol-llm, smith-protocol-machine: the inline agent's requests
```

## 6. What a consumer writes

- **Routing:**
  - **In its domain,** an event variant that wraps `smith-host-domain`'s
    events whole, and the same for its requests (programming-model.md,
    4.5).
  - **In its protocol layer,** the same for smith's components.
  - **Between a component and io,** the consumer's protocol layer is one
    more boundary of programming-model.md, 4.2. It gives io a token of its
    own for each entity a component asks for, and keeps the component's
    token in that entity, so io's events reach the component that asked.
  - **Timers:** each component keeps its own deadlines. The consumer's
    protocol layer reports the earliest of them, and fires each component's
    at the one point where it fires its own (programming-model.md,
    section 9).

  It is boilerplate, written once, of a few hundred lines.
- **Policy, as smith leaves it to hosts** (domain/host.md, section 5):
  - its charters, written with `smith-charter`, and its results, read with
    it;
  - its host tools' schemas, the decoding of their inputs and the text of
    their answers;
  - its messages' text;
  - what a delivery does;
  - where turns are kept.

  A party that writes charters elsewhere, such as temper's engine, depends
  on `smith-charter` alone, and the host carries charters and results as
  bytes (domain/host.md, section 3).
- **How the streams are made:**
  - for a spawned agent, `smith-agent` started with skein's contained
    processes;
  - for a connected agent, a connection, with TLS when it is remote, and
    the credential its opening carries (hosts.md).
- **Its worlds,** with smith's fakes:
  - a scripted agent, for its host domain;
  - smith's agent with skein's fake LLM provider, in its system worlds
    (domain/host.md, section 10).

## 7. Conventions

- **A bare file name** names a document of this directory: `channel.md` is
  smith's channel.
- **Other documents** are cited by their own names:
  - smith's domain documents as `domain/<file>.md`;
  - skein's foundation documents as their file names (`programming-model.md`,
    `testing-strategy.md`, `notes.md`);
  - smith's other design documents by their path, as
    `docs/design/shell.md`, the product's shell, and
    `docs/design/testing.md`;
  - skein's and temper's design documents with their project's name, as
    skein's `channel.md`. temper's are examples, never relied on.
- **Byte layouts are the schemas'.** These documents name records, their
  fields and their bounds; the schemas, in the crates, give the bytes. The
  event stream's lines are JSON, and events.md gives them field by
  field.
- **Each document** says the following, in this order:
  - in one page, what it is;
  - its parts;
  - what it owes the domain, and what skein owes it;
  - its world;
  - what it keeps from temper;
  - what is open.

## 8. Versions

Every format is pre-release: it changes with smith, with no obligation
to an older reader or writer. Each has one version at a time.

- **One version per format.** The channel, the charter with its result,
  turns and transcripts, the event stream, and what a host keeps in files
  each carry a version of their own. Any change a reader would notice is
  a new version. Nothing is translated, and no older version is read.
- **A reader refuses any other version,** each in its own terms:
  - **the channel** is versioned whole: its frames, its kinds and its
    records. Each side's opening says the version it speaks; with none
    in common, the channel ends before a run starts (channel.md, section
    2). Both halves move together.
  - **the charter** carries its own version, since the party that writes
    it may not be the host: temper's engine writes charters that its
    workers carry unread. A charter in another version makes the start
    invalid (domain/run.md, section 4). The run's result is written in its
    charter's version, so the charter's writer reads what it asked for.
  - **turns and transcripts** carry their own version, since a host keeps
    them and a later agent resumes from them (domain/session.md, section
    3). A transcript in another version fails the run as transient, and
    its host starts afresh (domain/run.md, section 6).
  - **the event stream** carries its version on every record, and a
    reader refuses a stream in another (events.md, section 6).
- **Where each is.** The channel, the charter and the transcripts move to
  these together, in one release, with the event vocabulary's first:
  - **the channel's second version:** a start's messages and the run's
    identity, the answer's final fence, messages checked at ingress, and
    facts projected only while the run is admitted (channel.md, section
    5);
  - **the charter's second:** each LLM's `window` and `output`, a
    cache-write price, and the budget's reserve (charter.md, section 5);
  - **transcripts' third:** a turn's kind and the window turn, the
    affinity, optional usage with reasoning tokens, the cut call, and a
    message's blocks from calls per response (transcript.md, section 6).
    The conversation vocabulary's first version stays with temper's
    legacy run (domain/README.md, section 7);
  - **the events' first** (events.md).
- **A build says which it speaks.** `smith check` prints each format's
  version (`docs/design/shell.md`), and the event stream's header carries
  them (events.md, section 3.2), so a mismatch shows before a run.

## 9. What smith needs from skein

Each item is generic and says nothing about agents. Each is also what
temper's channel between its engine and its workers is to be built on.
Each lands in skein, with its design, before the smith change that uses
it.

- **A framed channel** over a stream each way, or one stream both ways. It
  knows no calls, phases or payloads, and keeps no timers. It provides:
  - an opening that agrees a version and carries an opaque credential;
  - each side's largest bodies;
  - unknown kinds skipped;
  - reading as a demand;
  - measured writes;
  - a last word;
  - keepalive frames, scheduled by its owner.

  Designed in skein's `channel.md`.
- **Codecs generated from schemas:**
  - bounded records with versions;
  - measured encoding;
  - bounded decoding with typed problems;
  - each record's worst case;
  - golden bytes.

  Designed in skein's `codec.md`.
- **Owners that close, and a process that ends** (programming-model.md,
  section 5.2; skein's `shell.md`, section 13):
  - a component that keeps entities beyond the requests that made them
    takes its owner's close, refuses new work, drains what it keeps and
    reports closed;
  - a process ends when everything it owns has reported closed and io is
    empty, bounded only by io's close deadlines, and a termination signal
    while it closes turns graceful closes into aborts;
  - the teardown invariant, checked once in skein's world harness, so
    every world inherits it (testing-strategy.md, section 6);
  - a step's trace output with its room reserved, settling the trace
    queue (notes.md).
- **Process groups now, contained trees later:**
  - each child io spawns leads a process group of its own, which a
    deadline or a stop signals whole (skein's `io.md`, section 6);
  - contained trees: spawned within a deadline, with an environment given
    whole and a view of the file system in which only named directories
    are writable, stopped politely, then terminated, then killed, and
    proved empty (domain/tools.md, section 5; domain/host.md, section 4).
    Drafted in skein's `draft/process.md`. Resource limits and write
    scopes per tree come with them.
- **Connections for LLM calls,** as a component: connecting, TLS, and the
  deadlines skein's LLM client leaves to its owner (skein's
  `llm-connection.md`), with:
  - its owner's close, draining: idle connections closed at once, busy
    ones when their call ends, then closed reported;
  - deadlines by phase, and the expired phase in `TimedOut`;
  - a call that finds the pool full waiting for a connection, within its
    deadline, and a pool sized from its owner's concurrency.
- **The LLM client** (skein's `llm.md`):
  - a run's affinity, a key and a thread, rendered per dialect with
    per-call headers, and the affinity's header names reserved;
  - Anthropic's cache breakpoints;
  - `Usage` with each count optional, cache writes and reasoning tokens
    included;
  - `ToolChoice`: `Auto`, `None` or `Only`;
  - an oversized call as a block of its own, and a call cut off by the
    output limit marked;
  - typed `Limit` kinds, and HTTP statuses in failures;
  - `Limits::derive` from declared quantities, with a reasoning bound
    apart from replay's metadata (limits.md, section 3.1).
- **JSON and HTTP:** provider events decoded as they stream and
  selectively, by a general collector, and tokens kept compactly
  (skein's `json.md` and `http.md`).
- **io and the shell** (skein's `io.md` and `shell.md`):
  - an append stream on a file, and standard output adopted as a stream,
    for the event stream (`io.md`, 5.1);
  - durable replacement of a whole file, and private files for secrets
    (`io.md`, 5.2 and 5.3);
  - the loop's host interface and its drive, out of the testing crates
    (`shell.md`, section 12);
  - trust roots loaded from the machine or a file (`shell.md`, 6.2);
  - the kernel's resource usage of the process and its children.
- **Sockets and TLS** for a connected agent.
- **OAuth** for the local host's sign-in (skein's `oauth.md`), with
  `localhost` redirects, records that hold an access token alone, and the
  io that drives a sign-in.
- **Fakes:**
  - a scripted peer on a channel;
  - skein's fake LLM provider, taking the affinity and cache markers, and
    its fake checkout;
  - protocol worlds that join two ends with streams cut at random
    (testing-strategy.md, 2.5).
- **Later, and only if measured:** Codex's routing state and reasoning
  context, and an incremental transport over WebSocket.

## 10. From temper

- **Kept:**
  - from temper's channel: its framing, opening, bounds and golden frames
    (channel.md);
  - from its LLM translation: tools' schemas, decoding with typed problems,
    failure classes, and providers' opaque blocks (llm.md).
- **Gone:**
  - temper's magic;
  - a name and a secret on a pipe's opening;
  - snapshots;
  - endpoints' addresses, since skein's LLM client owns transport;
  - temper's payloads;
  - a push as a request of its own, since delivery is smith's.
- **temper's `channel.md`** keeps the channel between its engine and its
  workers, to be rebuilt on skein's channel and codecs. Its hop to the
  agent is this directory's.
- **The agent's half of `temper-agent-protocol`** becomes this directory's
  components. temper keeps its own half: rendering the brief, mapping the
  charter, and its tools' schemas and answers (temper's `agent.md`,
  section 11).

## 11. Open questions

- **After pre-release:** which formats, if any, take on obligations to
  older readers and writers once smith has outside users, and how a
  charter's writer then learns which versions its agents read
  (domain/README.md, section 8).
- **A connected agent in the domain:**
  - several runs in one agent process (domain/host.md, section 12);
  - the channel's rules apart from supervising a process
    (domain/host.md, section 4);
  - who issues the credential its opening carries, and how the agent
    decides on it;
  - a run reattaching after a lost connection;
  - a workspace on a file system its host cannot reach. Until the domain
    says otherwise, a connected agent shares its host's file system, or has
    no workspace.
- **Named calls in skein:** whether calls answered once by name, withdrawn
  and asked again, are generic enough for skein's channel. temper's channel
  will show whether it has the same shape.
- **MCP:** a component for MCP servers, over a process's pipes or HTTP, and
  how much of JSON-RPC is skein's.
