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
  transcript and the calls answered after it, credential grants;
- **messages,** named uniquely for the active logical run, in order (run.md,
  section 6); the kit retains outstanding names and the current read watermark,
  not an unbounded history of older acknowledged opaque names;
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
  (run.md, section 10). An interrupted mid-run landing answers delivered with
  its real stable name and receipts, the already-decided stop and spend; it is
  distinct from an LLM-declared accepted result, including for Report-only runs.

What a host owes:

- **Each call name is decided once.** A call asked again with the same
  name gets the answer the first had, from the host's record, never a
  second decision (run.md, section 5.2). Delivery's concrete name is the main
  accepted completion sequence and assistant block position, scoped by the same
  host logical run across restart. Callback slabs, temporary tickets and raw
  provider ids are not this identity (run.md, section 8.2). A host that cannot keep that promise
  (one without a durable record) must offer no host tool that writes.
- **A delivery is never abandoned.** One in flight runs to its end, which
  its deadline bounds, and is answered with what it did, so one that
  landed is reported landed.
- **Nothing written after a stop.** The host touches the workspace only
  once the run and everything it started are gone, but for the delivery
  it asked for.

Busy is an entrance refusal before forwarding an operation. Unavailable and
withdrawn are transport terminals, and do not prove that no durable effect
occurred. An immutable durable name may be retried after the original relay's
actual terminal; the parent's recorded decision wins. Neither withdrawal nor a
transport loss lets the kit fabricate a delivery terminal.

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

### 4.1 Typed child boundary and retained rights

The extraction is V2 only. `Start` carries opaque charter, optional transcript,
opaque calls answered after that transcript, optional workspace token, mount
names/writability/git kind/relative conflict paths and credential grant names. The kit
never reads charter or transcript contents. `Spawned` emits `Started`, moves
`Start` into the first lower `Send`, requests `Wait` and `Reap`, and demands
`Read`. The parent learns its agent token in `Started`; payload notices before
that are refused. Messages may queue immediately afterwards while the lower
adapter still owns the first Send, before the agent has read it. Start and queued
payloads are exclusive in the kit; the caller must count its IO-owned Start
alongside the queue.

The mount descriptors carry git kind even though the kit does not execute git.
Plain directories cannot carry conflict paths; a git directory may carry them
whether writable or read-only. That information never changes write authority.
The optional prepared workspace and its mount descriptors agree on presence;
mount names are unique safe single components and conflict paths are unique,
relative and bounded. The parent owns the actual merge and delivery policy,
while the kit preserves admitted metadata unchanged through its Start Send.

The parent inputs are `Spawn`, `Message`, `Answer`, `Acknowledge`, `Grant` and
`Stop`. The lower terminals are `Spawned`/`Unspawned`, `Sent`/`Unsent`,
`Received`/`Malformed`/`Hangup`, `Signalled`, `Exited` and `Reaped`. Each closes
exactly one corresponding `Spawn`, `Send`, `Read`, `Signal`, `Wait` or `Reap`.
`Reaped` follows `Exited` and proves the entire tree empty. Parent notifications
are `Started`, `Admitted`, `Called`, `Withdrawn`, `Turn`, `Waiting`, `Rejected`,
`Exhausted`, `Told`, `Answered`, `Faulted`, `Bounced` and `Gone`.

A stable `CallName` contains positive main completion sequence and assistant
block position, scoped by `Start.logical_run`; the callback token is only a
transport right. `Called` carries that scope, name, deadline, tool/effect and
opaque arguments. The bounded call table keeps name/kind/deadline/withdrawal and
parent/queued/sending stage. Its reservation survives until the reply's actual
`Sent`/`Unsent`, or the parent terminal when the channel is already unavailable.
`Withdrawn` is notification once, never consumption of the parent answer right.
A call's deadline bounds its watchdog pause; expiration resumes that clock and
never synthesizes an operation terminal. The parent must finish its actual
operation by its operation deadline, including an already landed result whose
response transport is late. Gone cannot release a workspace ahead of that right.

Generic host-tool replies carry opaque text and an error flag. Delivery carries
the complete sealed `Delivered`/`Nothing`/`Refused`/`Failed`/`Stale` terminal from
run.md section 8.2. At most one delivery is outstanding per agent. An agent final Answer cannot abandon a delivery whose actual terminal remains
Parent-stage or queued; Sending may already have reached the agent. Reply capacity
must fit the maximum sealed receipts before any operation is admitted; a real
landing cannot be replaced with TooLarge. One additional bounded successful
landing proof (stable name, receipts and issued downlink chronology) survives
the call's Send terminal until the last word. Replacing it temporarily uses the
Parent-stage call's reserved reply ownership; this and the separate withdrawal
snapshot are priced in `worst_case`.

A final `Delivered` must match real proof whose response was issued, preserving
the complete `Model(Fault)`, `Budget(Exhausted)`, `Receiving(ReceivingLimit)`,
`PriceOverflow`, `UsageOverflow`, `Policy(Unfinished)`, `Cancelled` or `Stale`
stop payload and attested cumulative spend. For Cancelled, the kit
also checks that its Cancel was issued before that response. It does not infer a
private agent stop decision or actual Cancel receipt from a parent's Stop.
Therefore it cannot require Delivered for every call pending at parent Stop.
An earlier ordinary landing followed by a later stop remains ordinary; the
world's independent oracle observes the scripted agent's actual stop/receipt
order to detect omitted, invented or misclassified interrupted evidence.

`Gone` requires process exit, empty tree, EOF, no lower Send/Read/Signal right,
no actual parent call/delivery right and no parent turn commitment. Calls and
turn metadata are independent of the finite supervision phase and survive
Cancelled, Draining, Exiting, Terminating and Killing. A last word and the
containment terminal are separate. Once the parent explicitly stops a run,
later channel breaches terminate the tree and emit a diagnostic fact without
inventing a parent run fault; a first valid last word may still be heard.
Wall-owned shutdown keeps its original failure cause, and a breach there is
reported. After termination ordinary working traffic is discarded while final
Answer and every actual lower/parent terminal remain receivable. Lower adapters settle every issued operation
even after termination; no destructor abandons one.

The parent reserves `max_out(limits)` free output slots before `step`/`fire`,
repeats due timers and reclaims retired slab slots only at its iteration boundary
(programming-model.md, sections 2, 4.5, 5 and 6). Facts are bounded content-free
observations; dropped facts saturate a counter and change no decision.

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
- **Held by the parent.** The kit moves each validated opaque turn body to
  its parent once; it keeps bounded number/byte metadata until exact
  `Acknowledge { turn }`. This is a durable commitment notice, not a cumulative
  fence: acknowledging turn 2 cannot release turn 1. Repeated already committed
  ACKs are inert. Metadata for a queued/sending ACK remains reserved through its
  actual Send terminal. Gone retains every uncommitted turn even after EOF.
- **Read credit reserved before demand.** A Read is issued only if metadata
  count and the maximum next-turn bytes fit. Lack of credit pauses the watchdog;
  downlink answers and ACKs still progress. During shutdown the channel is
  drained, and further out-of-credit or malformed traffic is a rules failure.
  The independent wall clock never pauses.
- **Fenced metadata.** Turns begin at one, are consecutive and carry
  nondecreasing cumulative scalar spend. Currency and raw-usage overflow
  attestations are sticky: an unknown total is a representable prefix, never an
  exact charge. Neither Turn nor Answer may clear an observed attestation. Final
  count matches exactly, and final representable spend cannot fall. A read watermark advances only through the known sent-message
  prefix; queued, unknown and regressing names fail the channel rules.

## 7. Credentials

- **Lent by the host:** an agent under a host gets each credential its
  endpoints need as a grant on the channel, named by account and
  generation, refreshed before it lapses; the agent's protocol layer
  holds it, and the domain names only the grant. A notice of a rejected
  credential or an exhausted account goes up for the host to act on. The kit
  keeps each known account's latest queued generation and greatest generation
  actually issued down the channel. Refreshes increase and coalesce per account
  without displacing other traffic. Rejection of an older generation may cross
  a queued refresh: it remains a stale notice for the parent, not an agent
  fault. Zero, unknown account or a generation beyond those actually emitted
  fails the channel. The parent owns the precise grant history, so it ignores
  obsolete names or validates skipped historical generations against its own
  record; the kit does not retain an unbounded generation history.
- **Or caller-configured:** a local host may obtain credentials through its
  caller's sign-in/refresh mechanism and lend grants through the same boundary.
  Smith's domains keep names only; provider authentication and credential bytes
  belong to the caller or a shared credential client, not a Smith OAuth crate.

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

The composed root emits a concrete typed Turn and keeps no ACK table. Its
parent owns the body after that output; a protocol face may encode it as the
host kit's opaque Turn. The host kit retains number/byte/read/spend metadata
through exact parent commitment and actual ACK Send completion, independently
of root Answer, process exit, tree emptiness and channel EOF. Committing turn 2
cannot commit turn 1. The composition world carries complete typed bodies via a
test-only full-record encoding; it does not claim a production transcript codec.

Root Turn metadata uses the actual global run scalar units and both overflow
attestations from domain/run.md, section 9. The complete typed Turn body keeps
its own inclusive session charge, exact per-completion usage and transcript
sequence. Those are distinct views when parallel children have outstanding
bills; neither the protocol face nor the host kit reprices tokens or adds a
child bill. Waiting follows the real settled run notice; the kit may pause
no-progress monitoring then, while its independent wall clock runs. Budget
implementation and gate status are recorded in
`docs/development/migration-05s4-budget.md`.

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
- **`smith-host-domain`** extracts Temper
  `19735a066cd485ca9d39e70ffb8ca8bd902ad55a`'s worker-agent behavior (05d,
  `e2a6a719`), independently of its later frozen-agent rename: one process per
  run, watchdog, cancel/terminate/kill, full V2 turns/transcripts/spend. Smith
  delivery vocabulary is the merged `2a621a5` seam. Complete source-story
  mappings and retained inventories are in
  `docs/development/migration-05s6-host.md`.
- **The local host and one process** are new.

## 12. Open questions

- **A host without a durable record** and host tools that write: refused
  by rule, as section 2 says, or allowed with a warning to the LLM.
- **Containment in one process:** whether commands may still run under a
  per-run cgroup, so cancelling a run in one process stops all it
  started.
- **Several runs per agent process,** for a host with many short runs.
