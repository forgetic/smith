# smith's protocol layer

Provisional, 2026-10-06. The protocol layer of smith turns smith's vocabulary
into bytes and back. It sits between an agent and its peers: its host, its LLM
providers and its machine. It follows skein's `programming-model.md`, sections
4 and 8, and the domain design in `docs/design/domain/`, cited as
`domain/<file>.md`.

This page covers four things:

- what the layer is made of;
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
  charter with its result, and the transcripts (section 8).
- **Sized and bounded.** Every payload is a record encoded to a schema,
  with a bound on every length. The bound is checked before anything is
  set aside for the payload (programming-model.md, section 8).
- **Facts are best effort** (domain/run.md, section 11). They cross the
  channel when there is room and are dropped, counted, before they hold
  anything back. A trace, where one is kept, follows the agent's
  configuration (agent.md).

## 2. Reading order

1. **README.md:** this page.
2. **channel.md:** the channel between a run and its host: its opening and
   versions, the kinds each way and their records, their order, what
   breaks its rules, flow control, grants and facts.
3. **charter.md:** the charter and the result. A party that decides runs
   writes the charter and reads the result, possibly with another release
   of smith than its agents run.
4. **transcript.md:** turns and transcripts, as hosts keep them and agents
   resume from them.
5. **llm.md:** the agent's LLM calls: prompts, tools' schemas, decoding and
   rendering, failures and credentials.
6. **agent.md:** the agent process: the machine's files and processes, its
   configuration, its facts and traces, and the `smith` binary.
7. **hosts.md:** the host's side of a channel, whether the host spawns the
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

## 4. Shapes

The same components make different shapes, and the consumer chooses one.
These three are examples, not a closed list: an agent that connects to its
host would be another.

| Shape | How agent and host meet | The host runs | The agent runs |
|---|---|---|---|
| a spawned agent | the host spawns `smith`, and the channel runs over its standard input and output | skein's contained processes and smith's host half | the `smith` binary |
| a connected agent | `smith` runs as a daemon; a host connects to it, with one connection per run, over TLS when remote | a connection and smith's host half | the `smith` binary, listening |
| one process | the consumer owns smith's domain as a child, and nothing crosses between them as bytes (domain/host.md, section 9) | its own domain and protocol layer | as the consumer's child: smith's domain, its LLM and machine components, and no channel |

- **A spawned agent** is how temper's worker runs smith, and one of the
  local host's two ways. The process tree is the run's containment
  (domain/host.md, section 4).
- **A connected agent** waits on the domain. The domain today:
  - runs one run per agent process;
  - supervises only a process its host spawned (domain/host.md, sections 1
    and 4).

  With a connected agent, as in one process, the agent's process is not
  the run's containment: a cancel stops the run and its commands' trees,
  not the daemon. A host that does not share the agent's file system
  cannot prepare or deliver its workspace (section 11).
- **One process** composes smith's LLM and machine components in the
  consumer's own protocol layer, which also hands the LLM component its
  credentials directly. The charter, messages and turns cross as entities,
  so a charter is never encoded. Transcripts the consumer keeps are still
  encoded, with `smith-transcript`.

## 5. Components

| Crate | Role | What it does |
|---|---|---|
| `smith-channel` | codecs | the channel's kinds and its records, for both halves (channel.md) |
| `smith-charter` | codecs | the charter and the result (charter.md) |
| `smith-transcript` | codecs | turns and transcripts (transcript.md) |
| `smith-protocol-channel` | component | the agent's half: skein's channel and smith's codecs on the streams it is given, translated to and from `smith-domain` |
| `smith-protocol-llm` | component | `smith-domain`'s LLM calls to skein's LLM client and back, with tools' schemas, decoding and rendering (llm.md) |
| `smith-protocol-machine` | component | the workspace's files, searches, commands, checks and guides, through skein's io (agent.md) |
| `smith-host-protocol` | component | the host's half: skein's channel and smith's codecs on the streams it is given, translated to and from `smith-host-domain` (hosts.md) |
| `smith-local-protocol` | protocol layer | the local host's: the terminal, configuration, transcripts in files, signing in and committing in place (hosts.md) |
| `smith-agent-service`, `smith-local-service` | service | the agent process's and the local host's `iterate` |
| `smith` | shell | the binary: the agent on its standard input and output or as a daemon, or the local host |
| `smith-mcp`, later | component | MCP servers as a tool source |

These are the roles of programming-model.md, section 4:

- **Codecs** depend on lib only, as protocol machines do, and keep no state.
- **A component** is a crate of a protocol layer. It depends on lib, io,
  skein's machines and compositions, and the domain whose vocabulary it
  translates: `smith-domain`, the root, for the agent's components, which
  keeps 4.5's rule that only a root faces the protocol layer. Like a child
  domain, it has its own limits, worst case and most outputs per step, its
  own deadlines, and its own tests.
- **Records are smith's own types,** not its domains'. A host needs no
  agent domain to write a charter, and each component translates between
  records and its domain with small total functions.

The agent process:

```
smith                              the shell
└── smith-agent-service            its iterate
    ├── smith-domain               the agent (domain/README.md, section 4)
    └── its protocol layer
        ├── smith-protocol-channel     to its host, on the streams it is given
        ├── smith-protocol-llm         to providers, through skein's LLM client
        └── smith-protocol-machine     to files and processes, through skein's io
```

A host:

```
a host (temper's worker, smith's local host, ...)
├── its domain
│   └── smith-host-domain          agents' runs: the channel's rules, the watchdog
└── its protocol layer
    ├── smith-host-protocol        one channel per run, on the streams it is given
    └── skein's contained processes, or a connection: how the streams are made
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
  - for a spawned agent, `smith` started with skein's contained processes;
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
  - skein's and temper's design documents with their project's name, as
    skein's `channel.md`. temper's are examples, never relied on.
- **Byte layouts are the schemas'.** These documents name records, their
  fields and their bounds; the schemas, in the crates, give the bytes.
- **Each document** says the following, in this order:
  - in one page, what it is;
  - its parts;
  - what it owes the domain, and what skein owes it;
  - its world;
  - what it keeps from temper;
  - what is open.

## 8. Versions

- **The channel** is versioned whole: its frames, its kinds and its records.
  - **Agreeing a version:** each side's opening says the range of versions
    it speaks, and the highest version both speak is chosen. With none in
    common, the channel ends before a run starts.
  - **Adding a kind:** a kind may be added within a version, as optional.
    A peer that does not take it leaves it out of its terms; the writer,
    refused at its entrance, carries on without it (skein's `channel.md`,
    section 5.2). Every other change is a new version.
- **The charter carries its own version.** The party that writes it may
  not be the host, and may run another release of smith: temper's engine
  writes charters that its workers carry unread. The run's result is
  written in its charter's version, so the charter's writer can read it.
  A charter in a version the agent does not read makes the start invalid
  (domain/run.md, section 4).
- **Turns and transcripts carry their own version.** A host keeps them as
  long as it likes, and a later agent resumes from them (domain/session.md,
  section 3). A transcript in a version the agent does not read fails the
  run as transient (domain/run.md, section 6).
- **What a host keeps in files carries its own version,** as the local
  host's transcripts do.
- **A reader keeps a range of versions; a writer writes one.**
- **Where each starts:**
  - transcripts at the conversation vocabulary's second version
    (domain/README.md, section 7);
  - everything else at its first.

## 9. What smith needs from skein

Each item is generic and says nothing about agents. Each is also what
temper's channel between its engine and its workers is to be built on.

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
- **Contained process trees:**
  - spawned within a deadline;
  - with an environment given whole and a view of the file system in
    which only named directories are writable;
  - stopped politely, then terminated, then killed, and proved empty
    (domain/tools.md, section 5; domain/host.md, section 4).

  Designed in skein's `process.md`.
- **Connections for LLM calls,** as a component: connecting, TLS, and the
  deadlines skein's LLM client leaves to its owner. Designed in skein's
  `llm-connection.md`.
- **Sockets and TLS** for a connected agent, and **OAuth** for the local
  host's sign-in, as skein has them.
- **Fakes:**
  - a scripted peer on a channel;
  - skein's fake LLM provider and fake checkout;
  - protocol worlds that join two ends with streams cut at random
    (testing-strategy.md, 2.5).

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

- **Moving together:** how long a reader keeps old versions of the charter
  and transcripts, and how a charter's writer learns which versions its
  agents read (domain/README.md, section 8).
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
