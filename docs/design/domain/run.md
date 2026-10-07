# The run

Provisional, 2026-10-05. What a smith run is, as a domain layer: one
activation of an agent, from the charter its host gives it to the answer
it gives back. It is the child domain `smith-domain-run`, composed with
its sessions by `smith-domain` (README.md, section 4). The mechanics are
those of skein's `programming-model.md`. What is still open is listed in
section 15.

## 1. In one page

- **A run is one activation.** A host starts it with a charter, a
  workspace and, when it resumes, a transcript; it answers once. Between
  runs nothing of the agent is live: continuity is the transcript, which
  the host keeps turn by turn.
- **A flexible LLM driver.** A chat with a person, research, a triage, a
  review, operating a system through its host's tools, a coordinator
  revising a plan, a code change: what a run is for is its charter's. The
  mechanics are the same for all.
- **Tools from four sources:** the workspace's, when it has one, which
  its sessions run;
  the run's own (`finish`, `deliver`, `wait`, sub-agents); the host's,
  declared in the charter and relayed; and, later, MCP servers. To a
  session they differ only in who answers.
- **Results are judged against a contract** given as data: a report, a
  verdict from a closed list, a change, or a declared failure, each with
  the fields it requires.
- **A change is checked, then delivered,** when a run has a writable
  workspace: the run runs its checks; its host makes the checked state
  durable and says how that went. smith knows directories, git working
  trees and checks; what delivering means is the host's.
- **One budget,** in the host's unit, across every session the run opens,
  each completion priced as it is made.
- **Failures are feedback.** A failing check, a broken contract, a
  delivery refused or a host tool's error goes back to the LLM, whole and
  bounded, for it to fix. Only what it cannot fix ends the run.

## 2. The run in the agent

```
host      starts runs; serves host tools and deliveries; keeps turns; hears the answer
run       charter, served tools, results, delivery, budget; owns its sessions
session   one conversation with an LLM; runs the workspace's tools
```

- **The run acts on the world only through its host and its
  workspace:** files written in its writable directories, deliveries the
  host makes, host tools the host answers. It never commits, pushes or
  reaches a system on its own authority.
- **Credentials never reach the domain.** A charter names endpoints; the
  protocol layer holds what they need, lent by the host or configured in
  the agent (host.md, section 7).
- **Retrying a failed run is decided above the run,** by its host, across
  attempts. A run answers once and is done.
- **Only a run opens sessions,** so ownership is a tree: a run owns its
  sessions, and which session is whose sub-agent is the run's
  bookkeeping.

## 3. Starting a run

### 3.1 The charter

What the host gives a run to set it up:

- **Instructions:** the role, as text: a chat, a researcher, a
  coordinator, a reviewer with a lens, an operator, a producer of a
  change.
- **Brief:** titled sections of text, in order, that the host wrote for
  why the run runs, already fitted to the host's budget for them. A host
  that keeps typed context renders it; smith reads only titles and text.
- **Tools:** the families the LLM may call (section 5): inspect, modify
  and shell, on the workspace, if it has one; sub-agents; `deliver`;
  `wait`; and each host tool, declared (5.2). Data, never derived from a
  role's name. A run may have no tools but `finish`.
- **Result contract:** what counts as done (section 7).
- **Conventions:** where a workspace directory keeps its guide and its
  checks (8.1); smith's defaults when absent.
- **Budget:** what the whole run may spend across all its sessions, in
  the host's unit, with each model's prices (section 9); and turns and
  wall time.
- **LLMs:** the endpoint and model the main session runs on, and the
  others sub-agents may use. The agent is configured with endpoints; a
  charter only names them.
- **Waiting:** how long the run may wait for a message before it parks
  (section 6); zero parks as soon as it waits.
- **Resuming:** whether the main session opens from the transcript in
  the start, when there is one.

### 3.2 The start

What comes with the charter:

- **The workspace,** if any (tools.md, section 2): its directories, each
  with the name the LLM calls it (one safe path component), where it
  sits, whether it may be written and whether it is a git repository; and
  for a repository that starts from a merge in progress, the files the
  merge left in conflict (8.3).
- **The transcript,** when the run resumes, and the calls its host
  answered after the last of its turns, with their answers (section 6).
- **Credential grants** for the endpoints that need them (host.md,
  section 7).
- **An activation number,** new for every start of the run and never
  reused; every call's name carries it (5.2).

Messages may follow at once, before the run has read its start.

### 3.3 The prompt

The run composes its main session's system text: the instructions; the
brief's sections; then the sections about its own mechanics, because
those are what it enforces: the workspace, if any, and what it may
write, the tools, each directory's guide, the checks, how to finish
(the contract, as the LLM must meet it) and how waiting works. The
domain decides what each says and in which order, and writes it as text,
as it writes the results and problems of the run's own tools and the
host's, so a transcript keeps exactly what the LLM was told. The
workspace tools' outcomes stay typed (tools.md, section 4).

A guide is read from each workspace directory at the path the
conventions name,
as text, UTF-8 cut at a character boundary within a limit; a file that is
not text counts as no guide.

## 4. What a run does

1. **Admits** a start, or refuses it at the entrance: busy, or invalid
   (a charter in a version it does not read, a charter that does not
   decode, a window of unacknowledged
   turns that cannot hold one turn, beyond the agent's `Limits`,
   an endpoint it is not configured with, a host tool whose name is
   smith's or appears twice, a contract that cannot be met).
2. **Prepares:** reads each workspace directory's guide and looks for its
   checks (8.1).
3. **Equips** its main session: prompt, tools, workspace authority, LLM, a
   share of the budget, and the transcript it resumes, if any.
4. **Drives** the session. A session *yields* when the LLM stops calling
   tools, and the run decides what happens next: nudge the LLM ("you have
   not finished"), pass on the messages that arrived, wait for one, or
   close it.
5. **Serves its tools and relays the host's** (section 5). It runs each
   call's deadline: a call that times out returns only once what it
   started has settled (a sub-agent ended, a delivery answered), so
   nothing it started is still writing.
6. **Tells each turn** to its host as it ends (session.md, section 3):
   what the LLM said and called, the results, what it spent, and the last
   message it read.
7. **Judges** a declared result against the contract (section 7). A
   violation goes back to the LLM as a tool error it can fix; it does not
   end the run.
8. **Accounts** every session it opens against one budget (section 9),
   its sub-agents' included, and winds down when the budget runs out.
9. **Ends once.** It closes its sessions, waits for what is in flight to
   settle, and answers its host (section 10). A cancel from the host takes
   the same path.
10. **Reports** facts as it goes (section 11).

## 5. Tools

### 5.1 Where tools come from

| Source | Tools | Who answers | Effect |
|---|---|---|---|
| the workspace | `read`, `list`, `search`; `write`, `edit`, `shell` | the session, through its tools (tools.md) | reads; writes |
| the run | `finish`, `deliver`, `wait`, sub-agents | the run | `finish` and `deliver` write; a sub-agent's follows its tools |
| the host | declared in the charter | the host, relayed | as declared |
| MCP servers, later | as each server lists them | the protocol layer, as opaque calls | as the charter declares |

Names are unique across sources; a declared tool may not take one of
smith's. The charter's families say which of smith's tools the LLM sees;
a host tool is offered exactly when it is declared.

### 5.2 Host tools

A host tool is an action of the host's that a run offers its LLM: in
temper, delegating a task, messaging, deciding a proposal, reading a
connector's system.

- **Declared as data:** a name, a description for the LLM, an input
  schema (JSON Schema, bounded in bytes), an effect (read or write) and a
  deadline. The protocol layer hands the schema to the provider as it
  came.
- **Relayed whole.** A call carries the run's name for it, the tool's
  name and the input the LLM wrote, as bytes, checked only to be a JSON
  object within the limits. The host decodes it, decides, and answers
  with text for the LLM, as a result or as an error. smith never
  interprets a host tool, and never retries one on its own judgement of
  what it did.
- **Asked again, never anew.** A call left unanswered (withdrawn past its
  deadline, or lost with a channel) or answered busy is asked again with
  the same name while the run lives, after a backoff, once the earlier
  relay of it has ended. A host is owed that a name is decided once and
  answered from its record when asked again (host.md, section 2); a name
  carries the start's activation number, so no later activation reuses
  it. Only
  when the run cannot learn the outcome is the LLM told that it is
  unknown, and then it may look (with a host tool that reads) before
  acting again.
- **Scheduled by effect,** like any tool (session.md, section 5): a write
  runs alone, adjacent reads together.
- **Refusals are the host's words:** a call beyond what the host allows
  is answered as an error saying what it lacked, and the LLM may act on
  that, by asking otherwise if the host offers a way.

### 5.3 Sub-agents

A sub-agent is a served call that the run answers by opening another
session: the same workspace; its own tools, from the run's workspace
families (often read-only), never the run's served tools or the host's;
a budget carved from the run's; and one of the LLMs the charter lists,
which the asking LLM may name, or else main's. When the child ends, its
final message is the parent's tool result; its spend is counted once,
through that result (session.md, section 6).

Within a run, sub-agents: cheap, sharing its workspace and budget, ended
with the call that asked for them. Work that should outlive the run, run
elsewhere or have its own authority is the host's, which may offer a
host tool for it, as temper's `delegate` does.

## 6. Messages, waiting, parking, resuming

- **Messages in.** A host relays messages to a live run as they arrive,
  each named, in order, with a sender's label and its text, which the
  host rendered. smith interprets none of them: a person's words, a
  delegate's result and a decision are all text to the LLM. Those that
  arrive while a session works are passed on when it next yields. With
  each turn, and when it waits, the run names the last message it has
  read, so the host knows what was taken.
- **A cancel is not a message.** It is the host's, and ends the run
  (section 10).
- **Waiting.** A run whose work depends on what comes next (a chat on its
  person, a coordinator on its delegates) calls `wait`. It yields until a
  message arrives, and its host holds it meanwhile, the watchdog paused
  (host.md, section 4).
- **Parking.** Past its charter's waiting time it parks: every turn it
  took is sent, and it answers parked. Its state is its transcript, which
  the host already has, so it hands over nothing else.
- **Resuming.** A run whose charter resumes opens its main session from
  the transcript in its start, its turns as they were, provider blocks
  included; tells the LLM, as text in its waking prompt, of the calls its
  host answered that the transcript does not hold as answered (one whose
  turn was never told, the agent having stopped between the two, or one
  the run withdrew and its host decided later), with their answers, so the
  LLM does not ask them again; and goes on with the messages that woke
  it. A transcript it cannot use
  (another version, another provider or endpoint, malformed, too large)
  fails the run as transient, saying which (session.md, section 3), and
  the host decides what the next run starts from. A run that does not
  resume starts fresh from its brief, which carries what the host thinks
  it needs.

## 7. Results

### 7.1 The result contract

A contract allows any of four forms:

- **A report:** text, bounded, with the fields the contract requires.
- **A verdict:** one label from a closed list, each label with a contract
  of its own: which fields it requires, and which items it may carry, how
  many (a least and a most) and of which kinds, each kind with the fields
  it requires.
- **A change:** the workspace's changes, delivered (section 8), with the
  fields the contract requires, and whether the checks must pass.
- **A failure:** the LLM declares it cannot do the work, with a reason.

A result is its form, its label for a verdict, its text, its fields (a
name and text each) and its items (a kind and fields each). Fields,
labels and kinds are names the host chose: temper's change requires a
title and a body, its review's verdicts are an approval and a request
for changes whose items are follow-up tasks. smith knows only that a
field is required, non-empty and within its bound.

### 7.2 Finishing

`finish` is a tool, not a convention about the last message. Its input
schema is generated from the contract by the protocol layer, which
decodes the LLM's input into a result. The run judges it and answers
with acceptance or with what is wrong; what is wrong is a list of typed
problems (a form not allowed, an unknown label, a missing field, too few
or too many items, an item of a kind not allowed), each named, which the
LLM can fix. An LLM that stops without finishing is nudged, within the
budget.

An accepted report, verdict or failure ends the run. A change is
delivered first (section 8), and ends the run once its delivery lands.

### 7.3 What is checked twice

The run checks the form of a result; the host may check it again against
what it knows, as it commits it, and refuse it as a failed attempt for
its next run to hear of. smith does not make the host's judgement.

## 8. Delivering a change

### 8.1 Checks

- **A directory says what its checks are,** by convention: an
  executable at the path the conventions name, `.smith/check` unless the
  charter says otherwise (temper says `.temper/pre-pr`), looked up in each
  writable directory when the run starts. A directory without one has no
  checks, and nothing above the agent needs to know either way.
- **Checked is delivered.** The checks run as contained processes with
  deadlines, while nothing else writes to the workspace, and the host
  then makes exactly that tree durable. Nothing else writes by
  construction: a write runs alone in its session, and a sub-agent lives
  only as long as the call that asked for it.
- **A long operation is said.** The run tells its host when checks start,
  with their deadline, and when they end, so a long test suite is not
  taken for a hang (host.md, section 4).
- **A failing check is feedback,** with its output's tail, like any
  other.
- **Not a security boundary.** An agent can change what a check runs;
  whatever guards the host's systems (temper's CI and its authority's
  requirements) guards them. The checks catch failures early, inside the
  run that can fix them.

### 8.2 Delivery

`deliver` is the run's request to its host: make the checked state
durable, with the result's fields. The host takes exactly that state, in
each writable directory that changed, and does whatever delivering means
to it: temper commits and pushes a branch; the local host commits a
repository in place and keeps a plain directory's files as they are;
another host might copy files out or attach them to a ticket.

The host answers in smith's terms:

- **delivered,** naming per directory what it made, as text the run
  passes on (a commit, a branch's head, files kept);
- **nothing,** when no directory changed, which is how a declared
  change is checked to exist: the LLM is told its delivery failed;
- **refused,** for something the LLM can fix, naming it: a conflicted
  file that still holds a marker (8.3);
- **failed,** with a reason (unreachable, refused by the target, timed
  out, broken, too large, missing, busy, unavailable, cancelled, unknown)
  and the tail of the host's diagnostic output, the last **512 bytes**
  with the number of bytes dropped before them, a cap the vocabulary seals
  and the domains' memory bounds count; with several directories, the
  first that failed in workspace order;
- **stale:** where the change goes has moved under the run, so every
  later delivery of this run would find the same. It ends the run as
  stale, and the host decides afresh.

Every answer but delivered and stale goes back to the LLM. A delivery
that lands is the result, even if the run was winding down meanwhile; a
delivery in flight is never abandoned (host.md, section 2).

### 8.3 Merges in progress

A repository may start from a merge in progress, the host having merged
and left the conflicts in the working tree; the start names the files in
conflict, and the run tells the LLM. The LLM edits them and runs the
checks; it has no git writes (tools.md, section 5). Delivered, the host
commits the tree as the merge, and refuses while a file the merge left
in conflict still holds a marker, naming it.

### 8.4 Delivering mid-run

A run whose charter grants `deliver` may deliver before it finishes, as
a chat does with a small fix before handing it on: checked, then
delivered, the same way, and the run goes on. A finish with a change
delivers what is there at finish. A mid-run delivery is not the run's
result: a run ended while one is in flight waits for its answer, then
answers as it was ending; the host has the delivery's own answer.

## 9. Budget and spend

- **One budget per run,** in the host's unit: what the run may spend
  across all its sessions, sub-agents' included; with turns and wall
  time. It is checked at the entrance against the agent's `Limits`; a run
  that asks for more is refused.
- **Prices are the charter's,** per model: integer amounts for input,
  cached input and output per a number of tokens. Each completion is
  priced as it is made (session.md, section 6). The unit means nothing to
  smith: it is temper's deployment unit, or what the local host's
  configuration says.
- **Shares.** Each session the run opens gets a share of what is left,
  and per-kind token caps as its limits. A run winds down when its budget
  runs out: no session starts another completion, and the run answers
  with what it has.
- **Reserved before it is made.** Before each completion, its
  sub-agents' included, a session reserves that completion's maximum
  cost from the run's budget: the input it sends and the output its
  `max_tokens` allows, at its model's prices. The input's maximum counts
  what the agent sends as at most one token per byte, plus an allowance
  its limits give for what the protocol layer and the provider add. When
  the completion ends, the difference is returned. A completion whose
  maximum does not fit what is left is not made, and the run ends for its
  budget. So a run never spends past its budget, and stops a little early rather
  than late. All of it is inside the run, with no call to its host.
- **Spend is told:** cumulative in each turn, and whole in the answer.

## 10. The answer

A run answers once, after every turn it counts:

- **accepted,** with its result, its turn count and what it spent;
- **parked,** with its turn count and what it spent;
- **failed,** typed so the host can act without reading prose: the model
  (a provider's failure past its retries, an account exhausted), the
  budget, policy (a call or result the run refused to make), cancelled,
  stale, or a transcript it could not resume; each with what it spent.

Refused at the entrance, it answers refused: busy, or invalid, saying
what is beyond the limits. A cancel from the host winds the run down
along the same path; a change its finish delivered meanwhile is still
its result (8.4).

## 11. Facts

Each child domain pushes what happened as typed facts into a bounded
queue (programming-model.md, section 3): a run admitted or ended, an LLM
call started, retried or finished, a tool or a check started (with its
deadline) or finished, a stream of the text the LLM writes. The protocol
layer projects them for the host: a content-free stream for liveness, and
an optional trace whose content follows a capture policy. Nothing the
agent decides depends on whether a fact is delivered. What must be
delivered (turns, calls, the answer, what was spent) is not a fact.

## 12. Below the domain

- **Schemas and decoding:** the protocol layer owns the schemas of
  smith's tools, `finish`'s generated from the contract, and passes the
  host's on as given; it decodes owned and served calls into typed
  entities, and checks host tools' inputs only for being JSON objects
  within limits.
- **Text as written:** the system text, and the results and problems the
  domain wrote (3.3), placed in each provider's request.
- **The channel** to the host (host.md, section 3).
- **Files and processes,** through io, when there is a workspace: guides
  read as text, checks run
  as contained processes with deadlines and their output's tail.

## 13. The world

The run's world runs the domain against neighbours that share none of
its types (testing-strategy.md, section 4): a scripted session, whose
yields, turns and served calls a script draws; the machine's faces for
guides and checks; and a scripted host: messages, host tools' answers
(busy, errors, lost), deliveries' outcomes, grants, cancels. The agent's
top-level world puts the run with real sessions and tools, a fake LLM
provider and the scripted host; a host's own system world (temper's
worker's) puts it under that host.

Its stories: a change produced, checked and delivered; a verdict judged
against its contract, corrected and accepted; a chat that waits, hears
its person's words, parks, and resumes from its transcript; a run whose
host tools answer busy, asked again under their names; a resolution from
a merge in progress; a sub-agent on another model; a delivery found
stale; a chat with no workspace, only host tools; a cancel in the middle
of a turn. Its referee: one answer per run, after every turn it took;
turns in order; nothing written outside the writable directories; budgets exceeded by at most one completion per
open session; every call answered once; every host call asked under one
name.

## 14. From temper

- **The run, as temper built it, stays:** charters judged at the
  entrance, sessions with budgets and retries, typed tools, finishing
  with checks, a delivery's feedback.
- **Served tools grow** from `finish` and sub-agents to `deliver` and
  `wait`; **host tools** replace the engine's tools as a closed list, and
  the forge and outlet grants, which nothing served.
- **The result contract** gains reports and declared failures, and
  carries the verdict lists temper's agent fixed in code; a change's
  title and body become fields.
- **Push becomes delivery,** its reasons the host's, its 512-byte
  diagnostic kept; a moved branch is a stale delivery.
- **Conventions** replace `.temper/pre-pr` and `AGENTS.md` in code.
- **Budgets** move from temper's token split to the host's unit with
  prices.
- **New, as temper's agent.md planned them:** messages, `wait`, parking,
  resuming from a transcript, calls asked again, delivering mid-run, and
  merges in progress.

## 15. Open questions

- **Messages mid-work:** passing a message on between completions, not
  only when a session yields, so a long job hears its person sooner.
- **A sub-agent's result:** its final message today; whether a sub-agent
  may be given a contract of its own.
- **A run's refusal on the channel:** a run that refuses its start for
  policy answers failed; whether refusal wants a kind of its own.
- **Conventions' home:** in the charter, as here, or in the agent's
  configuration, with the charter overriding.
- **Which defaults a standard agent's charter has** when the local host
  writes it (host.md, section 8).
