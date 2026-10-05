# smith

Provisional, 2026-10-05. smith is a kit for building flexible LLM
agents, and a standard agent built from it. Like temper, it is built on
skein and follows skein's foundation documents. It grew from temper's
agent (temper's `docs/design/domain/agent.md` before this split), and
temper is its first host. Nothing here depends on temper: temper is
named only as an example, and section 7 says where temper's agent went.
How smith is built from temper's code is temper's migration plan
(temper's `docs/plans/next-domain/`).

## 1. In one page

- **A kit and an agent.** smith covers what a flexible LLM agent does,
  whatever it is for: conversations with any provider, tools (its own on
  a workspace, its host's, MCP servers'), sub-agents, budgets, results
  judged against a contract, changes checked and handed over, transcripts
  to resume from. The same crates build `smith`, an agent with standard
  features that runs on its own or under any host that speaks its
  protocol.
- **Not only a coding agent.** A chat assistant, a researcher, a triager,
  a reviewer, an operator acting through its host's tools, a coordinator
  of other agents, and a producer of code changes are all runs of the
  same agent, told apart by their charters. Nothing in smith assumes code,
  repositories or a workspace at all.
- **One process, one protocol.** An agent is one process running one run
  at a time, and talks to its host over one channel whose vocabulary is
  defined here (host.md). A host (temper's worker, smith's local host, a
  test's world) starts runs, serves what they ask of it and hears how
  each ends.
- **skein's layering** (`programming-model.md`): every part is a domain,
  a protocol layer and io, so a project built on smith imports it at the
  layer it needs: the domains in its worlds, the protocol crates to talk
  to an agent or read its transcripts, the binary to run one, or
  everything in one process (section 4).
- **The domain is complete** (programming-model.md, section 4). A world
  of domains and fakes runs everything an agent does, with no protocol and
  no io.
- **Policy is data.** A host shapes a run entirely by its charter and its
  answers: instructions, a brief, the tools it may call and those the host
  serves, what counts as done, where to look for checks and guides, a
  budget. smith interprets no workflow vocabulary: no roles, task kinds,
  verdict names, message kinds, forges, branches or pull requests
  (section 5).
- **A workspace is optional, and generic.** A run may be given
  directories to work in, some writable; one that is a git working tree
  gets git's protections and may start from a merge in progress. A run
  without one works through its host's tools, MCP servers and its
  messages. A change is what the writable directories hold, checked; what
  making it durable means (files kept, a commit, a push, a pull request)
  is the host's: smith asks for a delivery and tells the LLM how it
  went.
- **Any model, any provider.** The conversation vocabulary is
  provider-neutral. The protocol layer uses `skein-llm`'s shared client;
  provider APIs, HTTP/SSE and replay metadata belong to skein.
  A run starts on the model its charter names and may give its sub-agents
  others.
- **Failures are visible.** The LLM sees what went wrong, bounded in size:
  a failing check's output, an edit that matched nothing, a result that
  broke its contract, a host's refusal in the host's own words. A failure
  is never replaced by a fixed message.

## 2. Reading order

1. **README.md:** this page: what smith is, its parts, how it stays
   generic.
2. **run.md:** one activation: the charter, the tools a run serves and
   relays, messages and waiting, results and their contracts, delivering
   a change, the budget, the answer.
3. **session.md:** one conversation with an LLM: turns, transcripts,
   providers, prices, sub-agents.
4. **tools.md:** what a session does to the workspace.
5. **host.md:** the other side: what a host owes a run and is owed, the
   channel, supervising agent processes, the local host, one process.

## 3. Names

| Name | What it is |
|---|---|
| agent | smith's agent: a process running one run at a time, or the loop inside it |
| host | what starts runs and serves them: temper's worker, smith's local host, a world |
| run | one activation of an agent, started with a charter, answering once (run.md) |
| charter | how a run is set up: instructions, brief, tools, result contract, conventions, budget, models, waiting time (run.md, 3.1) |
| start | the charter, with the workspace, the transcript and credentials (run.md, 3.2) |
| brief | titled sections of text the host wrote for why the run runs |
| workspace | the directories a run works in, if any, each writable or not; a git working tree among them is a repository (tools.md, section 2) |
| repository | a workspace directory that is a git working tree |
| conventions | the files a run looks for in a workspace: guides and checks (run.md, 8.1) |
| guide | a directory's instructions for agents, such as `AGENTS.md` |
| checks | a directory's executable that says whether a change is good |
| session | one conversation with an LLM (session.md) |
| turn | one completion of a session, with its calls and their results, told as it ends |
| transcript | the main session's turns, versioned, kept by the host, to resume from |
| owned tool | a tool a session runs itself, on the workspace (tools.md) |
| served tool | a tool the run answers: `finish`, `deliver`, `wait`, sub-agents (run.md, section 5) |
| host tool | a tool the charter declares and the host answers, relayed (run.md, 5.2) |
| call | a run's request to its host, named by the run, answered once |
| message | what a host relays to a live run, named, read in order (run.md, section 6) |
| result, result contract | what a run declares to finish; what counts as done (run.md, section 7) |
| delivery | the host making a checked change durable (run.md, section 8) |
| stale | a run whose deliveries can no longer land, as its host says |
| spend, unit, prices | what completions cost, in the host's unit, at each model's prices (run.md, section 9) |
| endpoint | a provider and its credentials, configured in the agent; a charter names it |
| credential grant | a credential a host lends a run, refreshed over the channel (host.md, section 7) |
| fact | what happened, best effort, for liveness and traces (run.md, section 11) |
| answer | how a run ends: accepted, parked or failed (run.md, section 10) |

## 4. Structure

```
smith-domain                  the agent loop: one run, its sessions; the host boundary
├── smith-domain-run          one activation: charter, served tools, results, delivery, budget
└── smith-domain-session      one conversation with an LLM
    └── smith-domain-tools    read, list, search, write, edit, shell

smith-host-domain             a host's half: one agent process per run, supervised
smith-local-domain            the local host: a workspace in place, a person at a terminal
```

The agent's tree follows programming-model.md, 4.5: each child domain is
a step machine with its own world; a parent owns its children's state and
routes between them; siblings share no domain types, so the run and the
session meet only through translations in `smith-domain`. Its vocabulary
is what crosses the domain boundary: the host's messages, the run's
calls, turns and answer, calls to LLM providers, and the file and process
operations the tools and the checks ask of io.

`smith-host-domain` and `smith-local-domain` are domains of the other
side (host.md): the first is a capability a host embeds as a child, to
start and supervise agent processes; the second is a whole host, the one
the `smith` binary runs.

| Layer | Crates | What a project built on smith takes |
|---|---|---|
| domain | `smith-domain` and its children | the agent in its worlds, or in its own process (host.md, section 9) |
| domain | `smith-host-domain` | supervising agent processes, as its host's child |
| protocol | `smith-protocol` | the agent process's protocol layer: the channel's agent half, application tools and routing |
| protocol | `smith-protocol-llm` | translating the agent's conversation and application tools to the shared `skein-llm::client::Client` |
| protocol | `smith-channel` | the channel's frames and payloads, both halves, versioned |
| protocol | `smith-transcript` | encoding and decoding turns, for a host that keeps or shows them |
| shared protocol | `skein-llm` | provider-neutral calls, native provider codecs and opaque replay envelopes |
| protocol | `smith-mcp`, later | MCP servers as a tool source |
| shared testing | `skein-fake-llm-domain`, `skein-fake-llm-protocol` | generic scripted LLM and byte peers; smith supplies application scripts and expectations |
| testing | a scripted agent, a scripted host | smith's own neighbours and application worlds |
| binary | `smith` | the agent process a host spawns, and the local host with an agent |

io is skein's: contained process trees, files, HTTP, pipes.

The LLM adapter owns smith's tool declarations, application argument decoding
and result translation. It owns no provider selection branches, provider wire
fields, HTTP/SSE machine or sign-in client. Endpoint configuration and credential
bytes arrive from the caller. Acquiring and refreshing those credentials belongs
to the host or a shared credential client; smith receives grants and reports
credential failures (host.md, section 7).

The generic client and peer interfaces may change together with their consumers.
An application translates at the boundary once; it does not copy a shared
machine or add a callback to compensate for a missing shared entrance. Bounds
cover both the shared values and smith's retained application data.

**What goes to skein.** smith and temper share skein; what both would
otherwise each keep goes there, as generic kit, when it is extracted:

- **framed channels:** frames, a hello with versions, bounds sealed by
  constructors, a channel's state machine; smith's and temper's channels
  keep only their payloads;
- **an OAuth client:** sign-in, refresh, tokens kept as secrets; the
  caller supplies endpoint configuration and lends credentials to smith;
- **supervised processes:** spawning within a deadline, cancel then
  terminate then kill, proof that a tree is empty; `smith-host-domain`
  adds the channel's rules and the watchdog;
- **worlds' common fakes,** such as a scripted peer on a channel.

Conversation, tool and run policy stays in smith; generic LLM clients, codecs
and wire peers belong to skein. Tasks, authority and connectors stay in temper.

## 5. How smith stays generic

smith decides the mechanics of an agent; its host decides everything
that is policy. The line between them:

| smith decides | the host decides |
|---|---|
| how a conversation runs, retries, budgets and prices | which runs exist, and with which charters |
| what the workspace's tools do, and their confinement | whether there is a workspace: which directories, from where, which writable |
| how a result is judged against a contract | the contract, and what an accepted result means |
| checks before a change leaves the run | what delivering it means: a commit, a push, a pull request |
| how a host tool's call is named, scheduled, asked again | the host tools: their names, schemas, decisions, answers' text |
| reading messages in order, waiting, parking | which messages reach a run, in which words |
| a transcript's shape and version | where transcripts are kept, and whether a run resumes |
| the answer's types | retrying, holding, what comes next |

What varies between agents, and between hosts, arrives as data:

- **Host tools,** declared in the charter by name, description, input
  schema and effect, relayed with the LLM's input as written, answered
  with text the host wrote (run.md, 5.2). A host's whole repertoire of
  actions (temper's delegating, messaging, deciding, its connectors'
  reads) is host tools; smith never decodes one.
- **The result contract:** a report, a verdict from a closed list, a
  change, a declared failure, each with the fields and items it requires,
  named by the host (run.md, section 7). A pull request's title and body
  are a change's fields; a review's follow-ups are a verdict's items.
- **Delivery:** the run asks, the host does it and answers in smith's
  terms: delivered, nothing to deliver, refused, failed or stale (run.md,
  section 8). The host's git, forge and branches stay its own.
- **Conventions:** the guide and checks file names, from the charter,
  with smith's defaults (run.md, 8.1).
- **The brief:** titled text sections. A host that keeps typed context
  renders it itself; smith adds only the sections about its own
  mechanics, which it enforces.
- **Messages:** named, with a sender's label and text the host rendered;
  smith reads their order, never their kinds (run.md, section 6).
- **Money:** a unit and each model's prices; smith prices, the host
  funds.

So a host needs no code built into the agent: temper's workers run
smith's stock binary. A project that wants an action inside the agent's
own process, beside the workspace's tools, offers it as an MCP server
first (run.md, 5.1); tools compiled in are an open question (section 8).

## 6. Conventions

- **A bare file name** names a document of this directory: `run.md` is
  smith's run.
- **skein's foundation documents** are named as their own file names:
  `programming-model.md`, `testing-strategy.md`, `notes.md`, cited by
  section.
- **A host's documents** are named with their project: temper's
  `worker.md`. They are examples, never relied on.
- **Each document** says in one page what it is, then its parts, what it
  owes and is owed below the domain, its world, what it keeps from temper,
  and what is open.

## 7. From temper

temper's agent, as its `docs/design/domain/agent.md` described it before
this split:

| temper's agent.md | Now |
|---|---|
| 1. In one page | this page, section 1; temper's own in temper's agent.md, section 1 |
| 2. The agent in the system | run.md, section 2; host.md, section 2; temper's place in temper's agent.md, section 2 |
| 3. Structure | section 4 |
| 4.1 Charter | run.md, 3.1 and 3.2; temper's charter in temper's agent.md, section 4 |
| 4.2 What a run does | run.md, section 4 |
| 4.3 Messages and the engine's tools | run.md, 5.2 and section 6; temper's tools in temper's agent.md, 4.3 |
| 4.4 Finishing | run.md, sections 7 and 8; pushing in temper's agent.md, section 6 |
| 4.5 Waiting, parking, resuming | run.md, section 6 |
| 5. Sessions | session.md |
| 6. Tools | tools.md |
| 7. Facts | run.md, section 11 |
| 8. Below the domain | each document's; the channel in host.md, section 3 |
| 9. The world | each document's |
| 10. Open questions | run.md, section 15; session.md, section 11; tools.md, section 8 |
| 11. From today | each document's "From temper" |

What changes in becoming smith, for temper's migration:

- **Closed tool sets open.** A run served `finish` and sub-agents, and
  the design added the engine's tools by name; now a run serves `finish`,
  `deliver`, `wait` and sub-agents, and relays any host tool its charter
  declares. temper's grants for the forge and outlets go: both are host
  tools.
- **A change is no longer a pull request.** Its title and body are fields
  temper's contract requires; pushing is temper's delivery; a branch that
  moved is a stale delivery.
- **`.temper/pre-pr` and `AGENTS.md` are conventions** temper's charters
  name; smith's defaults are `.smith/check` and `AGENTS.md`.
- **Verdict lists fixed in code go;** the contract carries them.
- **The brief arrives rendered.** Its typed sections and their rendering
  are temper's.
- **Budgets are in the host's unit,** with prices; the token split of
  temper's engine goes.
- **The agent's protocol layer** is smith's, and its channel smith's own,
  which temper's worker speaks as a host (host.md, section 3).
- **One transcript version.** smith starts at the conversation
  vocabulary's second version; the first, and the legacy run that opens
  it, stay with temper until its cutover.

## 8. Open questions

- **Moving together:** how smith and temper change in step while
  temper's migration is under way; versioning smith's channel and
  transcripts across releases.
- **Tools compiled in:** whether a project may add owned tools at build
  time, as a child domain the run composes, or only through MCP servers.
- **Workspaces beyond directories:** a remote filesystem, an object
  store, a sandbox; today a host mounts them as directories, or offers
  them as host tools.
- **The host kit's scope:** whether `smith-host-domain` keeps turns until
  acknowledged for every host, or leaves that to each (host.md, 6).
