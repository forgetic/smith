# Hosts

Provisional, 2026-10-05. The other side of a run: what a host owes an
agent and is owed, the channel between them, and the two host domains
smith ships: `smith-host-domain`, which a host embeds to start and
supervise agent processes, and `smith-local-domain`, the host the
`smith` binary runs. What is still open is listed in section 12.

## 1. In one page

- **A host starts runs and serves them.** It gives each a start (a
  charter, a workspace, a transcript, credentials), relays messages,
  answers host tools and deliveries, keeps the turns it is told, and hears
  the answer. temper's worker is a host; so is smith's local host, and so
  is a test's world.
- **One contract, three forms.** The same vocabulary crosses as domain
  entities (section 2), as frames over an agent process's pipes
  (section 3), or as calls between domains in one process (section 9).
- **One process per run.** An agent process runs one run; its process
  tree is the run's containment, and stopping a run means that tree is
  gone (section 4).
- **A host keeps its policy.** Which runs exist, what a delivery means,
  where transcripts live and what follows an answer are the host's
  (section 5); smith supervises, relays and enforces the channel's rules.
- **Standard features stand alone.** The local host runs a run in a
  directory, with a person at a terminal, transcripts in files and
  changes committed in place (section 8).

## 2. The contract

What a host and a run say to each other, in order.

Down, from the host:

- **the start** first (run.md, 3.2): the charter, the workspace, the
  transcript and the calls answered after it, credential grants, an
  activation number;
- **messages,** named, in order (run.md, section 6);
- **answers** to the run's calls: host tools' (text, as a result or an
  error), deliveries' (run.md, 8.2), busy, unavailable;
- **credential grants,** refreshed (section 7);
- **acknowledgements** of turns, when the host keeps them (section 6);
- **a cancel,** at most one.

Up, from the run:

- **admitted,** or a refusal;
- **calls,** each named by the run and answered once, also once the run
  has withdrawn it; **withdrawals;**
- **turns,** numbered from one, consecutive, each with its cumulative
  spend and the last message read;
- **facts,** best effort (run.md, section 11), and **a long operation**,
  its span bounded by the limits, and its end;
- **waiting,** with the last message read;
- **notices** that a credential was rejected or an account exhausted;
- **the answer,** its last word, with its turn count and its spend
  (run.md, section 10).

What a host owes:

- **Each call name is decided once.** A call asked again with the same
  name gets the answer the first had, from the host's record, never a
  second decision (run.md, 5.2). A host that cannot keep that promise
  (one without a durable record) must offer no host tool that writes.
  Names stay distinct across a run's starts: each start has a new
  activation number, never reused, which every name carries.
- **A delivery is never abandoned.** One in flight runs to its end, which
  its deadline bounds, and is answered with what it did, so one that
  landed is reported landed.
- **Nothing written after a stop.** The host touches the workspace only
  once the run and everything it started are gone, but for the delivery
  it asked for.

What breaks the channel's rules, an agent failure: a call name reused
while in flight, a turn out of order, a spend that falls, a count in the
answer that is not the turns told, anything after the last word, a
payload beyond the limits, or a message that does not decode.

## 3. The channel

- **Framed over the agent process's pipes,** one channel per run, its
  frames and payloads in `smith-channel`, which both halves use. The
  agent's half is in `smith-protocol`; a host's half is in its own
  protocol layer, on `smith-channel`.
- **Versioned.** A hello on each side says the version it speaks; a
  mismatch ends the run before its start is read.
- **Payloads are smith's vocabulary:** a charter, a workspace, a
  transcript and turns (`smith-transcript`), calls and their answers,
  results. A host may keep any of them as bytes, reading only what the
  channel exposes beside them: a turn's number, size, spend and last
  message read; a call's name, tool and effect.
- **A host carries the charter opaque** if it likes: one who prepared it
  elsewhere (temper's engine) sends it through as bytes, and the host
  adds only the workspace it prepared.
- **Bounded.** Every payload has a limit, sealed by its value
  constructor; a host's limits may be smaller, never larger.

## 4. Supervising an agent process

`smith-host-domain` is the child a host embeds to run agents. It knows
agent processes and the channel's rules, and nothing of the host's
policy.

- **Spawned within a deadline,** in a contained process tree, with an
  environment holding no credentials: what the run's LLM calls need comes
  as grants on the channel. A run whose agent could not be started fails
  as such.
- **Gone only once gone:** its process exited, its tree empty, which io
  proves, and its channel read to the end, so what it said before it went
  is heard.
- **A watchdog on progress.** Every message from the run counts as
  progress; silence past the no-progress deadline stops the run. The
  clock pauses while the run waits for a message, having read every one
  sent to it; while a call waits for its answer, which the run's own
  deadline for it bounds; and while a turn waits for room to be sent
  (section 6). It stretches to the span of a long operation the run
  reports, such as its checks, until the run says it is done. A separate
  bound covers wall time.
- **Cancel, then kill.** A cancel goes down the channel first, and the
  run winds down on its own (run.md, section 10); past a grace the
  process tree is terminated, then killed. Wall time ends the same polite
  way; a broken rule, no progress, or a channel hung up while the run is
  live terminate the tree at once. A run that has said how it finishes
  has the same grace to exit.
- **What it hands its parent:** the run's calls, turns, waiting, notices
  and answer, in order; the agent's failures, typed (not started, exited
  without answering, broke the rules, said too much, no progress, wall
  time), each with bounded detail for operators, such as its error
  output's tail, never shown to an LLM.

## 5. What a host decides

- **Which runs, with which charters,** and whether a run resumes;
- **the workspace:** whether there is one, which directories, from
  where, which writable, and preparing them, merges in progress
  included;
- **its host tools:** their schemas, their decisions, their answers'
  words, their durable records;
- **deliveries:** what making a tree durable means, and when a delivery
  is stale;
- **messages:** which reach a run, rendered how;
- **transcripts:** where they are kept, how long, and what a fresh run's
  brief carries instead;
- **what follows an answer:** retrying, holding, waking a parked run.

## 6. Turns held for the host

- **Kept until acknowledged,** when the host keeps transcripts durably,
  as temper does: a turn is held until the host says it is safe, and a run
  may have a bounded number of turns, and bytes, unacknowledged. Past it,
  the host stops reading the run's channel, and the run waits until there
  is room, its watchdog paused.
- **Before the answer.** The answer says how many turns the run took and
  is sent after them.
- **Held where?** Whether `smith-host-domain` holds turns for its parent,
  or passes each on and leaves keeping it to the host, is open
  (section 12); temper's worker holds them across its own channel's
  losses.

## 7. Credentials

- **Lent by the host:** an agent under a host gets each credential its
  endpoints need as a grant on the channel, named by account and
  generation, refreshed before it lapses; the agent's protocol layer
  holds it, and the domain names only the grant. A notice of a rejected
  credential or an exhausted account goes up for the host to act on.
- **Or the agent's own:** the local host signs in to a provider with
  skein's OAuth client, keeps the refresh token in the user's
  configuration directory, and lends the agent its grants the same way,
  so the agent has one path for credentials.

## 8. The local host

`smith-local-domain` is a host for one person on one machine, and with
the agent it is the `smith` binary:

- **A workspace in place,** or none: the directories named on the
  command line or in configuration, writable as configured.
- **A person at a terminal:** their words are messages; a chat's run
  waits for them, and parks past its waiting time, to resume when they
  return.
- **Charters from configuration:** instructions, models, prices, a
  budget, conventions, and a result contract (a report, by default, or a
  change).
- **Transcripts in files,** under the workspace's state directory, so a
  chat resumes across invocations.
- **Delivery in place:** a commit in each writable repository, with the
  result's fields as its message, nothing pushed unless configured; a
  plain directory's files kept as they are.
- **Host tools:** none of its own at first; tools from MCP servers it is
  configured with, once MCP is a tool source.

It runs the agent as a child process, as any host does, or in its own
process (section 9).

## 9. One process

A host may run smith's domain in its own process, beside its own
domains: the contract crosses as entities, translated by small total
functions, with no pipe and no frames. temper may so run its engine, a
worker and agents in one process, and the local host may run its agent
in its own.

- **What stays:** the domain's rules, the calls' names, turns and their
  acknowledgements, budgets, the tools' confinement, and commands and
  checks in contained process trees.
- **What goes:** the agent process as the run's containment boundary.
  Stopping a run is the domain's cancel and the end of its process trees;
  a run cannot be killed whole. That suits tests, development and one
  person's machine, not a host that runs agents it does not trust with
  its own process.

## 10. The world

`smith-host-domain`'s world runs it against a scripted agent (progress,
calls, turns, waits, long operations, parks and answers, but also hangs,
crashes, broken rules and ignored cancels) and the machine's process
trees. Its referee: one answer per run; no slot released before the tree
is empty; every call answered once; the watchdog paused exactly while
the run may be silent.

The local host's world runs it with the real agent, a fake LLM provider,
a person scripted at the terminal and directories on a fake disk. Its
stories: a chat with no workspace that waits, parks and resumes across
invocations; a change checked and committed in place; a run cancelled at
the terminal.

## 11. From temper

- **The contract and the channel** are temper's channel's agent hop, as
  temper's design changed it (turns, transcripts, messages carried whole,
  relayed calls with names, spend, waiting), with temper's names gone:
  runs are a host's, push is delivery, and the engine's tools are host
  tools.
- **`smith-host-domain`** is temper's worker's agent child domain, as
  temper's `worker.md` described it: one process per run, the watchdog,
  cancel then kill.
- **The local host and one process** are new.

## 12. Open questions

- **Turns held by the host kit or by each host** (section 6).
- **A host without a durable record** and host tools that write: refused
  by rule, as section 2 says, or allowed with a warning to the LLM.
- **Containment in one process:** whether commands may still run under a
  per-run cgroup, so cancelling a run in one process stops all it
  started.
- **Several runs per agent process,** for a host with many short runs.
