# The run

Provisional, 2026-10-05, revised 2026-10-09. What a smith run is, as a
domain layer: one activation of an agent, from the charter its host gives
it to the answer it gives back. It is the child domain `smith-domain-run`,
composed with its sessions by `smith-domain` (README.md, section 4). The
mechanics are those of skein's `programming-model.md`. What is still open
is listed in section 15.

## 1. In one page

- **A run is one activation.** A host starts it with a charter, a
  workspace, the messages that triggered it and, when it resumes, a
  transcript; it answers once. Between runs nothing of the agent is live:
  continuity is the transcript, which the host keeps turn by turn.
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
  each completion priced as it is made. Time and spend plan the work;
  turns only guard the loop. A reserve keeps main's last word: when the
  rest is spent, its sub-agents close and it finishes with what it has.
- **Every message ends once:** read by a turn, refused at the entrance, or
  unread when the run answers, and the answer names the last one read.
- **Fixed, then appended.** What a conversation's model is told first
  (its system text and its tools) never changes while it lives; what
  changes as the run goes is appended, so each request extends the last.
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
  the host's unit, with each model's prices; its wall time; its turns, a
  loop guard; and the reserve it keeps for finishing (section 9).
- **LLMs:** the endpoint and model the main session runs on, and the
  others sub-agents may use, each with its prices, the window of input it
  may use and the most output a completion may have, as the host chose
  them within the agent's configuration (session.md, section 8). The
  agent is configured with endpoints; a charter only names them.
- **Waiting:** how long the run may wait for a message before it parks
  (section 6), its first message included (3.4); zero parks as soon as it
  waits. A run is *wait-granted* when its charter's tools include `wait`,
  whatever its waiting time: a headless host grants `wait` with a waiting
  time of zero, so a run that would wait parks at once and its host
  reports that it needs input (host.md, section 8).
- **Resuming:** whether the main session opens from the transcript in
  the start, when there is one.

### 3.2 The start

What comes with the charter:

- **The workspace,** if any (tools.md, section 2): its directories, each
  with the name the LLM calls it (one safe path component), where it
  sits, whether it may be written and whether it is a git repository; and
  for a repository that starts from a merge in progress, the files the
  merge left in conflict (8.3).
- **Messages,** if any: those that triggered the run, such as a person's
  first line or a headless run's prompt, each named and labelled like any
  message (section 6). They enter the inbox when the start is admitted,
  before any message relayed after it, and a start refused at the
  entrance refuses them with it.
- **The run's identity:** opaque and of the fixed size the vocabulary
  seals (protocol/limits.md, section 2.4), the same for every
  activation and resume of the run, which its host keeps with the run. A
  start without one gets one the run draws from its seeded randomness as
  it admits the start. Every conversation of the run carries it, with an
  ordinal of its own (main's is zero), as its affinity (session.md,
  section 2), so a provider can route the run's requests to where their
  prefixes are cached; a main session resumed from a transcript keeps the
  affinity its transcript records. It is never derived from a credential
  or a path.
- **The transcript,** when the run resumes, and the calls its host
  answered after the last of its turns, with their answers (section 6).
- **Credential grants** for the endpoints that need them (host.md,
  section 7).
- **An activation number,** new for every start of the run and never
  reused; every call's name carries it (5.2).

More messages may follow at once, before the run has read its start.

### 3.3 The prompt

The run composes its main session's system text: the instructions; the
brief's sections; then the sections about its own mechanics, because
those are what it enforces: the workspace, if any, and what it may
write; the tools; each directory's guide; the checks; the budget
(`## Budget`, 9.2); sub-agents, when granted (5.3); how to finish (the
contract, as the LLM must meet it); and how waiting works, when `wait`
is granted. The domain decides what each says and in which order, and
writes it as text, as it writes the results and problems of the run's
own tools and the host's, and the notes it appends (9.2), so a
transcript keeps exactly what the LLM was told. The workspace tools'
outcomes stay typed (tools.md, section 4).

- **The working directory, named.** The workspace's section says
  concretely where paths start (tools.md, section 2). With one directory,
  it is the working directory: paths are relative to it, and its name
  appears in no path the prompt shows. With several, the first is the
  working directory, and the prompt says how each other is reached, as
  `/name/...`.
- **Fixed for the conversation's life.** The system text is written once,
  when the run opens a session, and does not change while that
  conversation lives, across its windows (session.md, sections 3 and 8):
  it holds no clock and nothing else that moves. What moves is appended
  to the conversation as text the run wrote (a nudge, a sub-agent's usage
  line, the wind-down's note, a message), never placed in the system text
  or a tool's description, so each request extends the one before and a
  provider's cache keeps it. A session that resumes the conversation in a
  later activation has it written afresh, from the start that resumes it.
- **The summary instruction.** With each session it opens, the run gives
  the instruction the session sends when it compacts its conversation
  into a new window (session.md, section 8): what a summary must keep for
  the work to go on. The session decides when; the run owns the words, as
  it owns every prompt.

A guide is read from each workspace directory at the path the
conventions name, as text, UTF-8: whole within the guide limit, which
the product's presets size so that a guide is normally whole, and past
it cut at a character boundary, the prompt saying that the file goes on;
a file that is not text counts as no guide.

A sub-agent's system text is its brief, then the same mechanics for the
tools it has, with the same guides: `## Your share` in place of
`## Budget`, and how to answer, within its answer cap, in place of how
to finish (5.3).

### 3.4 The opening

A run opens its main session once it has prepared and has work: a brief
that is not empty, or a message.

- **The opening** is main's first user message: the instruction to begin
  the work the brief describes, when the brief is not empty, then every
  message queued, in arrival order, at most one offer's worth (section
  6); the rest stay queued for main's next yield. The first turn told
  names the last of them as read (section 6). A run that resumes puts
  first, in its waking prompt, what its transcript does not hold
  (section 6).
- **Awaiting.** A run with an empty brief, no message and `wait` granted
  opens nothing: no session, and no request to a provider. It tells its
  host it is waiting, and the first message opens main, as its opening.
  Its time awaiting counts against its wall time (section 9), and it
  parks at its charter's waiting time like any waiting run, having taken
  no turn; its host starts it again with the message that wakes it
  (3.2). A run whose ordinary time runs out while it awaits parks then,
  as any waiting run does, having nothing to finish (9.1).
- **Instructions only.** A run with an empty brief, no message and no
  `wait` has only its instructions to go on, and opens on them, with the
  instruction to begin the work they describe.

## 4. What a run does

1. **Admits** a start, or refuses it at the entrance: busy, or invalid
   (a charter in a version it does not read, a charter that does not
   decode, a window of unacknowledged turns that cannot hold one turn,
   beyond the agent's `Limits`, an endpoint it is not configured with, a
   model its configuration does not declare for that endpoint, a window
   or an output above what the configuration declares for the model or
   too small for the agent's limits, charter text too large for a
   window's host part, a host tool whose name is smith's or appears
   twice, a contract that cannot be met, a budget that cannot hold its
   reserve). The start's messages are admitted or refused with it.
2. **Prepares:** reads each workspace directory's guide and looks for its
   checks (8.1).
3. **Equips** its main session: prompt, tools, workspace authority, LLM,
   the run's identity, a share of the budget, the summary instruction,
   and the transcript it resumes, if any; and opens it once there is
   work, or awaits its first message (3.4).
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
   its sub-agents' included, keeps a reserve for finishing, and winds
   down when the rest runs out (9.1).
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
a share of what is left above the run's reserve; and one of the LLMs
the charter lists, which the asking LLM may name, or else main's. Its
conversation carries the run's identity with an ordinal of its own
(3.2). When the child ends, its final message is the parent's tool
result; its spend is counted once, through that result (session.md,
section 6).

- **The call:** `sub_agent { brief, tools, llm?, max_turns?,
  max_seconds? }`: the brief the child works on; the tool families it
  gets, no wider than the asker's; the LLM it runs on; and the turns and
  seconds it asks for. Its effective share is what it asked, each part
  clamped to what is left above the reserve when it asks (9.1), or all of
  that when it asked for none. Its spend share is derived the same way,
  and never asked for or shown: the host's unit means nothing to a model.
  The brief takes the charter's place in the child's window, so a brief
  larger than a window's host part is answered as too large, and no child
  opens (protocol/limits.md, section 3.9).
- **No nesting.** A child never opens children, and is never offered
  `sub_agent`. A call that asks for nesting anyway is refused with a
  typed problem saying that a sub-agent does its work itself
  (protocol/llm.md, section 4).
- **Told its share.** The child's system text has `## Your share`: its
  effective turns and seconds, the most its answer may hold, and that its
  last turn has no tools. Like main's `## Budget` (9.2), it says what was
  set at open, never what is left.
- **A final turn.** The run gives each child, at open, a final-turn note
  and a wrap-up allowance of time. When the child's own turns are down to
  one, or its time to that allowance, the session gives it its final turn
  (session.md, section 7): the note, and no tool to choose, the tools
  offered unchanged. The child answers with what it has, and the run
  gives that answer to its parent with `stop: budget`: an answer that its
  share ended, not one the child chose to give. Its parent hears it like
  any other answer. A child returns unanswered only when it fails or is
  closed (9.1).
- **The result.** The child's final message, up to the answer cap it was
  told and cut past it with an explicit marker, then a usage line the run
  writes: whether the child stopped for its budget, what it used, in
  turns and time, and the turns and time the run has left above its
  reserve (9.2).
- **One writer at a time: the lease.** Write authority over overlapping
  directories has at most one holder at a time. A child granted modify or
  shell holds the run's lease for its call's life: its call is a write,
  which runs alone (session.md, section 5), so its parent waits and writes
  nothing meanwhile. Children granted only inspect hold no lease, and run
  side by side, with each other and with the reads around them in their
  parent's turn. Checks and delivery hold the lease over the whole
  workspace (8.1). The lease follows from how calls are scheduled; naming
  it lets a later step change who holds it while keeping what it means.
  The first such step, a read-only parent working beside one writing
  child, is a later document's (section 15).
- **Focused briefs.** Main's system text says how to delegate: a child
  starts cold, knowing only its brief, the guides, its tools and its
  share, so a brief names the paths, the decisions already made and the
  scope; independent reading splits among inspect-only children, which
  run side by side; a writing child runs alone.

Within a run, sub-agents: cheap, sharing its workspace and budget, ended
with the call that asked for them. Work that should outlive the run, run
elsewhere or have its own authority is the host's, which may offer a
host tool for it, as temper's `delegate` does.

## 6. Messages, waiting, parking, resuming

- **Messages in.** A host relays messages to a live run as they arrive,
  each named, in order, with a sender's label and its text, which the
  host rendered; a start may carry the first (3.2). smith interprets none
  of them: a person's words, a delegate's result and a decision are all
  text to the LLM. The run admits each at its entrance or refuses it
  there, saying why: too large (its label and text as they will be
  rendered, against the run's message limits), the inbox full, a name
  still in use, or the run ending, its answer decided. Each refusal goes
  to its host, typed, with the message's name and the reason, and is
  that message's terminal (host.md, section 2). Host messages reach
  main only; sub-agents never receive them.
- **Offered in order.** Admitted messages queue in arrival order. The run
  offers them at main's opening (3.4), with a `wait`'s result, and
  whenever main yields: every message queued, in order, at most one
  offer's worth, a number of messages and of their rendered bytes that
  the run's limits give, sized so that a window's opening can hold an
  offer (protocol/limits.md). What an offer leaves stays queued, in
  order, for the next. A compaction carries the messages offered and not
  yet read into its new window (session.md, section 8).
- **The fence.** With each turn, and when it waits, the run names the last
  message read: its fence. Messages go in order, so one name covers every
  message before it. A message is read when a told turn holds it, so the
  transcript the host keeps has what was read.
- **One terminal each.** Every message the run accepts ends exactly once:
  - **read:** a told turn's fence names it or a later message;
  - **refused:** at the entrance, as above;
  - **unread:** the run answered without reading it, whether it was still
    queued or offered to a turn that was never told.

  The answer carries the final fence (section 10), so a host in any
  placement knows which went unread; what it does with them is its policy
  (host.md, section 5). No message is dropped in silence.
- **A finish crossing a message.** In a run whose charter grants `wait`,
  a `finish` while messages are queued is not accepted: the run answers
  the call saying that messages came, and gives the model the messages
  in the same request, so it decides with them in view. In a run without
  `wait`, a finish is judged as it comes, and what was queued goes
  unread.
- **A cancel is not a message.** It is the host's, and ends the run
  (section 10).
- **Waiting.** A run whose work depends on what comes next (a chat on its
  person, a coordinator on its delegates) calls `wait`. With messages
  queued, the call is answered at once, with them. Otherwise its result
  is withheld until a message arrives, and the result and the messages
  then go to the model in one request, so an exchange costs one
  completion. Meanwhile the run tells its host it is waiting, naming its
  fence, and its host holds it, the watchdog paused; the run is idle only
  when that fence covers every message its host sent (host.md, section
  4). Time spent waiting counts against wall time (section 9).
- **Awaiting** a first message is waiting without a session (3.4): told,
  timed and parked the same way. It has read nothing, so its waiting
  names no fence.
- **Parking.** Past its charter's waiting time it parks: every turn it
  took is sent, and it answers parked. A run whose ordinary time runs out
  while it waits parks too: it has nothing to finish, so it does not
  wind down (9.1). Its state is its transcript, which the host already
  has, so it hands over nothing else. A run that parks while its `wait`
  is withheld tells its last turn without that call's result. Resumed, it
  supplies that result in its first request, with the messages that wake
  it, as a live `wait` is answered, just as it supplies the answers its
  transcript does not hold (below; session.md, section 3).
- **Resuming.** A run whose charter resumes opens its main session from
  the transcript in its start (its current window, session.md, section
  8), its turns as they were, provider blocks
  included; tells the LLM, as text in its waking prompt, of the calls its
  host answered that the transcript does not hold as answered (one whose
  turn was never told, the agent having stopped between the two, or one
  the run withdrew and its host decided later), with their answers, so the
  LLM does not ask them again; and goes on with the messages that woke
  it, which its start carries (3.2). A transcript it cannot use
  (another version, another provider or endpoint, malformed, too large)
  fails the run as transient, saying which (session.md, section 3), and
  the host decides what the next run starts from. A run that does not
  resume starts fresh from its brief, which carries what the host thinks
  it needs.
- **Messages mid-work,** designed here and built later. Between
  completions, not only when main yields, the run offers its queued
  messages to the working session, which appends them after the results
  of the calls it just ran, in its next request; the turn that reads them
  is told with them, its fence naming the last. Nothing in flight is
  interrupted and nothing already sent changes, so the prefix holds and
  every rule above stands. The run's threshold notes on its budget take
  the same path, as text the run wrote, with no name (9.2). Until then, a
  message waits for main to yield, also while main waits on a sub-agent.

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
budget. A `finish` that crosses a queued message, in a run that may
wait, is answered with the messages instead of judged (section 6).

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
  deadlines, and the delivery that follows is made, under the lease over
  the whole workspace (5.3): nothing else holds write authority from the
  checks' start to the delivery's answer, so the host makes exactly the
  tree that was checked durable. That holds by construction: a write runs
  alone in its session, and a sub-agent lives only as long as the call
  that asked for it.
- **A long operation is said.** The run tells its host when checks start,
  with their deadline, and when they end, as it does for each LLM
  completion and each command its sessions run, so a long test suite, a
  slow completion or a long build is not taken for a hang (host.md,
  section 4).
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
  and the tail of the host's diagnostic output with the number of bytes
  dropped before it, within a cap the vocabulary seals
  (`protocol/limits.md`) and the domains' memory bounds count; with
  several directories, the first that failed in workspace order;
- **stale:** where the change goes has moved under the run, so every
  later delivery of this run would find the same. It ends the run as
  stale, and the host decides afresh.

Every answer but delivered and stale goes back to the LLM. A delivery
that lands is the result, even if the run was ending meanwhile; a
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
  across all its sessions, sub-agents' included; with wall time and
  turns. It is checked at the entrance against the agent's `Limits`; a
  run that asks for more is refused, naming what was too large and the
  most the agent admits.
- **Each part has a role.** Time bounds how long the work takes and spend
  what it costs: those are what a run plans by, and what its model is
  told (9.2). Turns are a loop guard, set generously, never the unit of
  planning, since a turn's worth differs across tools and providers.
- **Wall time runs from admission.** Preparing, awaiting a first message
  and waiting for one all count against it (section 6).
- **Spend is real.** Every host prices completions. A host that pays a
  flat rate prices them notionally, so spend still bounds the work and
  weighs uncached input as a metered host's would; such prices are for
  the budget, not a bill.
- **Prices are the charter's,** per model: integer amounts for input,
  cached input, cache writes and output per a number of tokens, cache
  writes at the input's rate unless priced apart. Each completion is
  priced as it is made (session.md, section 6). The unit means nothing to
  smith: it is temper's deployment unit, or what the local host's
  configuration says.
- **Shares.** Each session the run opens gets a share of what is left
  (main all of it, a sub-agent its effective share, 5.3), and per-kind
  token caps as its limits. When what is left above the reserve is spent,
  the run winds down (9.1).
- **Reserved before it is made.** Before each completion, its
  sub-agents' included, a session reserves that completion's maximum
  cost from the run's budget: the input it sends and the output its
  `max_tokens` allows, at its model's prices. The input's maximum counts
  what the agent sends as at most one token per byte, plus an allowance
  its limits give for what the protocol layer and the provider add. When
  the completion ends, the difference is returned. A completion whose
  maximum does not fit what is left to it is not made: ordinary work
  that does not fit above the reserve starts the wind-down (9.1), and a
  completion that does not fit what is left at all ends the run for its
  budget. So a run never spends past its budget, and stops a little early
  rather than late. All of it is inside the run, with no call to its
  host.
- **A compaction is a completion** like any other (session.md, section
  8): reserved, priced, and counted as a turn.
- **Spend is told:** cumulative in each turn, and whole in the answer.

### 9.1 The reserve and the wind-down

- **The reserve** is the charter's `reserve { turns, time }`, which the
  product's presets give a default (`docs/design/shell.md`). Its spend is
  derived: its turns, each at the main model's maximum completion cost
  (its window of input and its most output, at its prices). A budget that
  cannot hold its reserve is refused at the entrance.
- **The ordinary pool** is what is left above the reserve, in each part
  of the budget. Main's ordinary work and every sub-agent's share draw
  from it; only main's finishing draws on the reserve.
- **The wind-down** begins when the ordinary pool is spent in any part:
  turns or time down to the reserve's, or a completion whose maximum does
  not fit above it. It is for a run that is working. A run waiting for a
  message when its ordinary time runs out, or awaiting its first, parks
  instead (section 6): it has nothing to finish until a message comes,
  and its host decides what wakes it. The run's budget is in one of two
  stages, ordinary or winding down, and the change is a fact (section
  11).
  1. Live sub-agents are closed and settled, each call answered once.
  2. Main then gets at most the reserve's turns of completions, the first
     with the run's note that the budget is spent and it is to finish now
     with what it has. They offer only `finish`, and `deliver` when
     granted: the same tool definitions are sent, and the model is told to
     choose among those only (session.md, section 5).
  3. A result accepted then is accepted (section 10). A main that has not
     finished when the reserve's turns are used, or its time is out,
     answers failed for its budget.
- **A sub-agent's share** runs out on its own: its final turn (5.3) ends
  it, and its parent goes on.

### 9.2 What the model is told

All of it is text the run writes, kept in the transcript, and computed
from the domain's clock, so a world sees it exactly.

- **`## Budget`,** in main's system text: its wall time and turns as they
  stand when it opens; that spend is bounded too; the reserve and the
  wind-down; and how sub-agents draw on what is above the reserve (5.3).
- **`## Your share`,** in a sub-agent's (5.3).
- **A usage line** at the end of every sub-agent's result: whether the
  child stopped for its budget (5.3), what it used, in turns and time,
  and the turns and time the run has left above its reserve.
- **The wind-down's note** (9.1) and **a child's final-turn note** (5.3),
  appended.
- **Threshold notes,** with messages mid-work (section 6): as the run's
  time and turns pass set marks, a note saying what is left.

No amount of spend is shown: the host's unit means nothing to a model
(section 15).

## 10. The answer

A run answers once, after every turn it counts, with its final fence:
the last message it read (section 6).

- **accepted,** with its result, its turn count and what it spent; also
  when the budget forced its finish, the result accepted in the wind-down
  (9.1);
- **parked,** with its turn count and what it spent;
- **failed,** typed so the host can act without reading prose: the model
  (a provider's failure past its retries, with its class, an account
  exhausted, or a limit that fired, naming the limit and its bound), the
  budget (naming what ran out), policy (a call or result the run refused
  to make), cancelled, stale, or a transcript it could not resume; each
  with what it spent.

The type is also what a host shows: a host words a failure from its
type, in every placement, rather than saying only that the run failed
(host.md, section 8).

Refused at the entrance, it answers refused: busy, or invalid, saying
what is beyond the limits; the start's messages are refused with it. A
cancel from the host ends the run along the same path; a change its
finish delivered meanwhile is still its result (8.4).

## 11. Facts

Each child domain pushes what happened as typed, content-free facts into
a queue whose room its step reserves like any other output
(programming-model.md, section 3), so no fact is lost inside the
process. Each carries when it happened, taken by the step that emits it,
never when a sink takes it:

- a run admitted or ended;
- a conversation opened or closed (main, a sub-agent or a compaction, a
  compaction's with the conversation it summarises), with its parent; a
  sub-agent's with its effective share and the reasons each part of it
  was clamped, and on its return what it used: turns, time and spend;
- an LLM call started, retried or finished, with its usage;
- a tool or a check started (with its deadline) or finished, a tool's
  with the provider's id for its call;
- a message received, or refused with its reason, then read by a told
  turn or unread at the answer, so every message's terminal is a fact;
- the budget, as it stands at each charge, and its stage, ordinary or
  winding down, when it changes (9.1);
- text arriving from the LLM, counted, never carried.

The protocol layer turns them into the event vocabulary
(`protocol/events.md`), and each sink applies its own policy and counts
what it loses: the event stream (a trace file, or a headless run's
output) is complete by default, held back by its stream rather than
dropping; the channel's projection, for the host's liveness, drops what
does not fit, counting it as not projected, and never holds the run.
Content (prompts, completions, also as their text streams, and
tools' input and output) goes only to the event stream, as its capture
policy allows. The records that say how the run and its process ended are
built from the answer and the process, not from facts, and are never
dropped. Nothing the agent decides depends on a fact: a sink may slow the
run, never change what it does. What must be delivered (turns, calls,
the answer, what was spent) is not a fact.

## 12. Below the domain

- **Schemas and decoding:** the protocol layer owns the schemas of
  smith's tools, `finish`'s generated from the contract, and passes the
  host's on as given; it decodes owned and served calls into typed
  entities, and checks host tools' inputs only for being JSON objects
  within limits.
- **Text as written:** the system text, and the results and problems the
  domain wrote (3.3), placed in each provider's request.
- **Affinity and tool choice:** the run's identity with each
  conversation's ordinal, and the tools a turn may choose among, rendered
  for each dialect by skein's LLM client (protocol/llm.md, sections 2.1
  and 3).
- **Events:** facts encoded for each sink (protocol/events.md).
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
of a turn.

For messages:

- a start that carries messages, opened with the brief's instruction and
  then the messages in order;
- an empty brief that awaits, asking no provider anything until its
  first message, then opening on it; one that awaits past its waiting
  time and parks with no turn; one whose ordinary time ends first,
  parked the same way;
- a headless run granted `wait` with a waiting time of zero, parking as
  soon as it would wait;
- more messages queued than one offer holds, the rest offered at the
  next yield, in order;
- an empty brief without `wait`, opened on its instructions;
- a message arriving at each point of a run: as it prepares, as it
  awaits, during a completion, while main waits on a sub-agent, as main
  finishes, as a change lands, as it parks, after a cancel;
- a finish crossing a queued message, given the messages instead;
- a `wait` answered with its message in one request, and one withheld
  through a park and supplied on resuming.

For budgets and sub-agents:

- a sub-agent asking more than is left above the reserve, clamped, and
  told its share;
- a child that reaches its final turn and answers for its budget;
- a child asking for sub-agents, refused;
- a sub-agent that spends the ordinary pool: it is closed, then main
  finishes in its final turns, accepted;
- the ordinary time running out during a sub-agent;
- the ordinary time running out while main waits, parked, not wound
  down;
- final turns ignored, failing once for the budget;
- a compaction, counted as a turn and as spend, the system text
  unchanged;
- inspect-only children side by side, and a writing child alone.

Its referee: one answer per run, after every turn it took; turns in
order; nothing written outside the writable directories; budgets
exceeded by at most one completion per open session, spend never; every
call answered once; every host call asked under one name. And:

- every message accepted ends exactly once, read, refused or unread;
  fences never go back; the answer's fence is the last told turn's;
- no request to a provider before the run has work;
- a session's system text and offered tools never change, and each
  request in a window extends the one before;
- a sub-agent's effective share is within what was left above the
  reserve when it was asked;
- at most one lease holder over overlapping directories, and checks and
  deliveries only under the whole workspace's;
- with a reserve of a turn or more, no budget failure before main's last
  final turn.

The agent's top-level world checks the same against the fake provider:
requests carry `## Budget`, `## Your share` and the usage lines, and
final turns offer no tool to choose, or only `finish` and `deliver`.
These are domain-speed stories under a deterministic clock and seed,
within the focused suite's budget; the fuzzy suite draws message
timings, budgets and shares within its own (`docs/design/testing.md`).

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
- **Push becomes delivery,** its reasons the host's, its bounded
  diagnostic tail kept; a moved branch is a stale delivery.
- **Conventions** replace `.temper/pre-pr` and `AGENTS.md` in code.
- **Budgets** move from temper's token split to the host's unit with
  prices.
- **New, as temper's agent.md planned them:** messages, `wait`, parking,
  resuming from a transcript, calls asked again, delivering mid-run, and
  merges in progress.

## 15. Open questions

- **A sub-agent's result:** its final message today; whether a sub-agent
  may be given a contract of its own.
- **A run's refusal on the channel:** a run that refuses its start for
  policy answers failed; whether refusal wants a kind of its own.
- **Conventions' home:** in the charter, as here, or in the agent's
  configuration, with the charter overriding.
- **Spend in `## Budget`:** none is shown (9.2), since the host's unit
  means nothing to a model; whether a share of it, as a fraction, would
  help a model plan.
- **A forced acceptance, marked:** whether the answer says that an
  accepted result came in the wind-down, so a host can tell a finished
  task from one the budget cut short.
- **The reserve and the checks:** whether a charter whose contract allows
  a change must keep its checks' deadline within its time reserve, and is
  refused otherwise.
- **A minted identity across resumes:** a resumed main keeps its
  transcript's affinity, but a run whose host gives no identity draws a
  new one in each activation for its sub-agents; whether the run takes
  the transcript's instead.
- **A child's context:** whether a sub-agent may open from its parent's
  transcript, bounded by its limits and counted in its share, so it
  starts warm.
- **Work beside sub-agents,** a later document's: a read-only parent
  working beside one writing child that holds the lease, which needs
  messages mid-work, a rule for when results join, `finish` and `deliver`
  waiting for or closing live children, and no parking with them live;
  then write scopes over directories, with contained trees; then
  asynchronous children, whose spend shares become reservations and to
  which messages may go, under the same terminals.
