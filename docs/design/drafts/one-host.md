# smith's side of jig's one host

Draft, 2026-10-07. jig adopted one host for its runs (jig's
`domain/hosts.md`, sections 5 and 6): one hub hosts runs in the engine and
on workers, and its agent is a capability of two kinds: smith's host
domain, one process per run, on a worker; and an inline agent, smith's
domains in the host's own process, in the engine. jig builds the inline
agent first, as `jig-inline-agent`, beside its hub. This draft is what
that asks of smith, for a pass once jig's extraction plan and smith's
protocol plan are done. Nothing here is adopted: smith's `host.md`
describes what is designed. Section 4 lists what would change; section 5
what is open.

## 1. In one page

- **smith has two ways to run an agent today, and three compositions of
  the second:**
  - `smith-host-domain` supervises agent processes (`host.md`,
    section 4);
  - the one-process form (`host.md`, section 9) is written by each host
    that wants it: `smith-local-domain` owns `smith-domain` in its own
    process (`protocol/hosts.md`, 5.6), jig's engine did the same in its
    local host, and jig now has its inline agent.
- **The proposal: one inline agent, smith's,** the twin of
  `smith-host-domain`: a child that runs a run as a composed
  `smith-domain` instead of a process, and speaks `smith-host-domain`'s
  vocabulary to its parent. Any host's root then treats the two kinds of
  agent alike, with the same small total functions.
- **Its users:** jig's hub in an engine, and smith's local host in its
  one-process form. jig's `jig-inline-agent` moves to smith, or gives way
  to smith's, once smith's local host needs it.
- **What it is not:** a second host. It keeps no slots, no fencing and no
  turns for acknowledgement; those are the host's (jig's hub, smith's
  local host), as they are around `smith-host-domain`.

## 2. The inline agent

A child a host embeds, beside or instead of `smith-host-domain`:

- **The same vocabulary down:** a start (the charter as bytes in smith's
  charter codec, the transcript as turn bodies, the calls answered after
  the last turn, the activation, the grants, the window), messages with a
  label and text, answers to calls, grants, a cancel.
- **The same vocabulary up, in the same order:** calls, turns, waiting,
  notices of long operations, facts, one answer; and the agent's
  failures, typed, from the same list (`host.md`, section 4), of which it
  can produce a subset: not started (a start the domain refuses), and
  gone without answering (dropped past the grace).
- **A run is a composed `smith-domain`,** one per run in flight. The
  contract crosses as entities. Only what the vocabulary carries as
  bytes is encoded: the charter, and turns, which the host keeps as
  bytes anyway.
- **Requests below the domain go up:** LLM completions and, when the run
  has a workspace, the machine's operations (files, searches, commands,
  checks) leave as requests the host's root routes to the protocol layer
  (`smith-protocol-llm`, `smith-protocol-machine`), and their answers come
  back. jig's engine gives its runs no workspace, so it routes
  completions only.
- **Stopping** is the domain's cancel: the run winds down and answers.
  One that does not within the grace is dropped, which is when it is
  gone, the twin of "gone only once gone" for a process. Commands and
  checks the run started end with their own process trees (`host.md`,
  section 9, what stays).
- **No watchdog on progress.** A composed domain cannot hang the way a
  process can; its waits are bounded by its own deadlines and those of
  the requests it hands up. A bound on wall time stays.
- **Bounded:** each run's worst case is `smith-domain`'s; the inline
  agent's is the sum over its slots, which its host gives it.

## 3. What it buys

- **One composition of the one-process form,** written, reviewed and
  tested once, for jig's engine and smith's local host alike.
- **One host shape everywhere:** a host's root composes its own logic,
  an agent of either kind, and a workspace if it has one, and translates
  the agent the same way whichever kind it is.
- **One process when wanted** (`host.md`, section 9): an engine, a worker
  and agents in one process, or the local host and its agent, with the
  same parts as across processes.

## 4. What would change

- **smith's documents:**
  - `host.md`: section 1, a host runs agents of two kinds; section 4,
    the inline agent beside `smith-host-domain`; sections 8 and 9, the
    local host's one-process form composes the inline agent; section 10,
    the host world runs both kinds;
  - `protocol/hosts.md`, 5.6: the local host in one process owns the
    inline agent and routes its requests to the LLM and machine
    components, rather than owning `smith-domain` itself;
  - `README.md`'s crate tree.
- **Crates:**
  - a new crate beside `smith-host-domain`, such as `smith-host-inline`,
    or a second child within `smith-host-domain`: section 5;
  - `smith-local-domain` composes it in its one-process form instead of
    `smith-domain`;
  - jig's `jig-inline-agent` is retired in favour of it, and jig repins
    smith.
- **Worlds:** the host world's stories run with both kinds of agent; the
  local host's world uses the inline agent in its one-process stories.
- **Plans:** smith's protocol plan's local-host session builds the
  one-process form on today's design. This pass reworks it onto the
  inline agent afterwards, unless that session is still to run when this
  is adopted, in which case it can build on the inline agent directly.

## 5. Open

- **One crate or two:** the inline agent as a sibling of
  `smith-host-domain`, sharing its boundary types from a common crate, or
  as a second child inside it. A shared vocabulary crate keeps the root's
  translations identical by construction.
- **Entities or bytes:** in one process, the charter and turns could
  cross as entities. The vocabulary carries bytes so the two kinds stay
  interchangeable; whether the extra encoding ever shows.
- **When jig switches:** jig's `jig-inline-agent` exists now. Whether
  smith's replaces it as soon as smith has it, or at jig's next repin of
  smith after both plans finish.
- **The watchdog:** whether a wall-time bound alone is enough for runs
  in one process, or progress should be watched there too, against an
  LLM component that never answers.
