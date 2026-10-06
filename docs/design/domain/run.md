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
  role's name. A run may have no external tools; main still has `finish` and `wait`.
- **Result contract:** what counts as done (section 7).
- **Conventions:** where a workspace directory keeps its guide and its
  checks (8.1); smith's defaults when absent, `AGENTS.md` and `.smith/check`.
- **Budget:** what the whole run may spend across all its sessions, in
  the host's unit, with each model's prices (section 9); and turns and
  wall time.
- **LLMs:** the endpoint and model the main session runs on, and the
  others sub-agents may use. The agent is configured with endpoints; a
  charter only names them.
- **Waiting:** how long the run may wait for a message before it parks
  (section 6). The current typed boundary requires a positive interval within
  the receiving waiting limit; zero is refused before admission.
- **Resuming:** whether the main session opens from the transcript in
  the start, when there is one.

The typed charter supplies separate `instructions: Box<[u8]>` and
`brief: Brief`, where `Brief.sections` is an ordered `Box<[Section]>` and
each Section owns its `title` and `text`. The receiving `brief_sections`
limit admits the exact section count before inspecting the sections or
starting effects. The checked aggregate `run_bytes` includes instructions,
all Section array cells and every title/text payload, alongside the other
charter fields and Start workspace. Inline Box wrappers belong to retained
run storage. Empty instructions and briefs are allowed; repeated titles
keep their supplied order. The host attests UTF-8 as for other charter text.
There are no additional per-title or per-text caps. Actual rendered
openings still obey the receiving session byte bound.

The typed charter supplies `conventions: Option<Conventions>`. A custom pair
replaces both default paths unchanged, with no fallback to a legacy path.
The host attests UTF-8 as for its other charter text. Each path is nonempty,
relative, at most 4,096 bytes, and has no NUL, ASCII control byte, backslash,
empty component, `.` or `..` component. Unsafe paths are refused as
`Invalid::Conventions` before admission or IO. Both owning payloads count
against the aggregate receiving `run_bytes`; their inline Box wrappers belong
to the charter's slab storage. Discovery, main and child prompt labels and
actual check execution use the same immutable selection. A valid charter may
still discover guides before its rendered opening is refused by the session's
receiving byte limit; conventions do not change that opening contract.

### 3.2 The start

What comes with the charter:

- **The workspace,** if any (tools.md, section 2): its directories, each
  with the name the LLM calls it (one safe path component), where it
  sits, whether it may be written and whether it is a git repository; and
  for a repository that starts from a merge in progress, the files the
  merge left in conflict (8.3).
- **The transcript,** when the run resumes, and waking messages about host
  calls answered after a completion missing from it (section 6).
- **The activation number,** a positive host-supplied number never reused for
  another activation of the same logical worker (sections 5.2 and 8.2).
- **Credential grants** for the endpoints that need them (host.md,
  section 7).

The typed start owns `workspace: Option<Workspace>` separately from the
charter. `None` is the canonical absence; `Some` contains at least one
`Directory`. Each directory supplies its name, root token, write permission,
git kind and initial relative conflict paths. Names and root tokens are unique
within the workspace. A name is a nonempty single path component, excluding
NUL, `.` and `..`; conflict paths use the existing relative marker spelling,
with no empty, `.` or `..` components or NUL. Both kinds of text are host-attested
UTF-8. Plain directories have no conflicts. A read-only git directory may carry
conflict metadata; that information grants no write or delivery authority.

Receiving limits bound directory count, each name, conflicts per directory and
each conflict path, with paths at most the existing 4,096-byte marker capacity.
The checked aggregate `run_bytes` counts charter and workspace ownership:
directory cells, names, conflict-path Box cells and payloads. Counts and owned
bytes are admitted before pairwise duplicate checks and before discovery,
provider or tool effects. A malformed workspace refuses as `Invalid::Workspace`.
A Change contract or a delivery grant without any writable directory is refused
as an impossible outcome or grant before effects; delivery-capable workspaces
also fit the existing 64-directory receipt vocabulary.

Without a workspace, inspect, modify and shell families are unavailable even if
the charter names them. Host tools, finish, wait and permitted sub-agents keep
their own contracts. No guide read, check probe, file operation, check or delivery
is introduced merely to start or resume a workspace-free run. The host supplies
the workspace again on resume; transcript history does not retain it.

Messages may follow at once, before the run has read its start.

### 3.3 The prompt

The run composes its main session's system text: the instructions; the
brief's sections; then the sections about its own mechanics, because
those are what it enforces: the workspace, if any, and what it may
write, the tools, each directory's guide, the checks, how to finish
(the contract, as the LLM must meet it) and how waiting works. The
domain decides what each says and in which order; the protocol layer
renders it. Workspace metadata distinguishes plain directories from git working
trees and names the actual initial conflict paths. Those paths describe files;
they are not executable instructions or authority grants.

The current typed 05s4 entrance builds owned `Opening.system` bytes with the
run's pure prompt renderer; the adapter carries that rendered system into the
native request. Protocol extraction and rendering ownership remain part of
05s5, rather than being established by this typed increment.

The main prefix preserves supplied bytes: nonempty instructions first, then
each section as `## `, its title, two newlines and its text. Each paragraph
ends with one newline when the payload has none, followed by another
newline. Empty section title/text still renders its section delimiter.
No sections are sorted, merged or discarded. Extra delimiter ownership
is bounded by two bytes plus seven bytes per admitted section; queued
rendered openings coexist with the retained charter and caller input.

A guide is read from each workspace directory at the path the
conventions name,
as text, UTF-8 cut at a character boundary within a limit; a file that is
not text counts as no guide.

## 4. What a run does

1. **Admits** a start, or refuses it at the entrance: busy, or invalid
   (beyond the agent's `Limits`, an endpoint it is not configured with, a
   host tool whose name is smith's or appears twice, a contract that
   cannot be met).
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
| the run | `finish`, `deliver`, `wait`, sub-agents | the run | `finish`, `deliver` and `wait` write; a sub-agent's follows its tools |
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
  answered from its record when asked again (host.md, section 2). Only
  when the run cannot learn the outcome is the LLM told that it is
  unknown, and then it may look (with a host tool that reads) before
  acting again.
- **Scheduled by effect,** like any tool (session.md, section 5): a write
  runs alone, adjacent reads together.
- **Refusals are the host's words:** a call beyond what the host allows
  is answered as an error saying what it lacked, and the LLM may act on
  that, by asking otherwise if the host offers a way.

The typed 05s4 boundary makes the lifecycle explicit. `HostTool` carries bounded
name, description and schema bytes, `HostEffect` and a positive per-relay
`timeout`. `Grants.host_tools` and `Opening.host_tools` carry those declarations;
children receive none. Count and aggregate charter bytes are admitted before
IO; reserved and repeated names, empty declaration fields, zero timeouts, and invalid
receiving bounds are refused. The root also refuses undeclared names or effect
mismatches before scheduling a completion's calls. The protocol face parses the
complete JSON document and attests that it is an object; `HostInput::attested`
checks only its fixed storage cap and object exterior. Text fields are
protocol-attested UTF-8; the domain never validates their encoding. This constructor
is not a general JSON parser. JSON-object parsing/attestation and provider wire formats
remain the protocol's responsibility (section 12).

`Start.worker` is a parent-supplied stable logical scope, retained across relay
recovery and restarted activations. `Start.activation` is a positive host-supplied
number never reused within that worker. It is distinct from the live admitted run
and callback slab tokens. `HostCall` forwards this scope, the activation-qualified
`CallName`, immutable tool/effect/input, a separate `RelayName { owner, attempt }`
and the effective deadline. The deadline is the minimum of declared timeout,
receiving `Limits.host_timeout` and remaining caller/run time. Every emitted
attempt is owed exactly one actual `HostReturned` terminal. The host bounds the
text by `Limits.host_reply_bytes`; `HostAnswer` preserves text and the error bit.
A decided logical scope/name must replay exactly its first recorded answer:
recovery does not authorize another decision or effect.

`HostReply::Busy` means this attempt made no decision. `Unanswered(Lost)` and
`Unanswered(Withdrawn)` are actual settled attempts whose outcome cannot yet be
learned. They permit recovery, while the run is working, only after the previous
actual terminal and the positive `Limits.host_backoff`, up to
`Limits.host_attempts` including the first. The next relay uses the same logical
scope/name/tool/effect/input and a new attempt-qualified callback. There is never
a second live relay for that operation. A stale callback cannot answer a later
attempt. `WithdrawHost` requests settlement; it does not supply a terminal,
release original ownership, or permit an early retry. Declared relay expiry
requests withdrawal and waits for the actual terminal before recovery.

Caller expiry and run shutdown forbid fresh recovery. An actual `Answered`
always wins, including after withdrawal: its exact result or error reaches the
conversation once. Exhausted or stopped recovery after any unresolved
`Unanswered` returns `HostUnknown`; later `Busy` cannot erase that uncertainty.
Pure Busy exhaustion remains Busy, and a caller stop between only known
predecision attempts may return Cancelled/TimedOut. The unknown answer does not
claim that nothing happened. Submitted delivery ownership follows section 8
unchanged. Root V2 transcript restart preserves the concrete settled result and
its provider identity; a restored result is not another host effect. The stable
logical scope remains the parent's across activations.

Retained memory is bounded by declaration count and aggregate charter bytes,
call slots, one immutable name/tool/effect/body per logical call, fixed attempt
state and receiving input/reply caps. Recovery retains no attempt history.
Each emitted input/result/declaration copy belongs to its receiver; the root
counts retained declaration and feedback copies in its own bound. Worlds reach
full declaration/input/reply storage and actual recovery paths with measured
ownership, and replay observed inputs, callback attempts and exact answers.

### 5.3 Sub-agents

A sub-agent is a served call that the run answers by opening another
session: the same workspace; its own tools, from the run's workspace
families (often read-only), never the run's served tools or the host's;
a budget carved from the run's; and one of the LLMs the charter lists,
which the asking LLM may name, or else main's. When the child ends, its
final message is the parent's tool result. Its inclusive subtree bill travels
through that result for the parent's share and Turn; run-wide own-completion
charges were already counted once (domain/session.md, section 6; section 9).

Within a run, sub-agents: cheap, sharing its workspace and budget, ended
with the call that asked for them. Work that should outlive the run, run
elsewhere or have its own authority is the host's, which may offer a
host tool for it, as temper's `delegate` does.

A child receives the asking session's raw task text as its prompt prefix,
followed by the shared workspace guides and the run's child mechanics.
It does not inherit the main charter's instructions or titled Brief.
The SubAgent task and tool authority contracts remain unchanged. Its explicit
scalar share follows section 9.1: child-own completion count and inclusive
subtree spend, both clamped to the global remainder.

Each main/child opening owns its optional workspace copy until authority
translation moves its names into the tools' existing mount vocabulary. Prompt
rendering retains git/conflict information; tools need only roots and write
permissions. The root prices these copies and rendered metadata alongside
queued opening and translation ownership. Children see the same directories
and initial conflict paths, with the charter's narrowed tool families; no
workspace is fabricated for a child of a workspace-free run.

### 5.4 Concrete application feedback

The root translates a settled run result directly into the session's concrete
`AnsweredV2` text/error terminal. `feedback(returned, max_bytes)` consumes one
semantic terminal and returns `Feedback { text, error }`, or `TooLarge` before
allocation; `feedback_worst_case` supplies its checked receiving bound. This is
a pure application translation using the same checked two-pass Writer pattern
as the run's prompt. It creates no protocol callback, operation, retry or new
terminal right. The client adapter uses the canonical text verbatim.

Actual `HostAnswered` text/error moves unchanged. An ordinary child answer with
`cut = 0` and `EndTurn` likewise moves unchanged. Other child answers retain the
exact child text followed by its cut count and typed stop annotation; MaxTokens,
NoCalls and Refusal cannot become ordinary EndTurn. All other statuses use fixed
ASCII labels and preserve every semantic field: problem names and omitted count,
receipt directory/text pairs, delivery reason and diagnostic, marker name,
repository/check exit/cut/tail, and the complete typed child failure.

Opaque evidence is never presumed UTF-8. Quoted byte literals use printable
ASCII directly except quote and backslash; every other byte, including those
two delimiters, uses lowercase `\xNN`. This is lossless, valid protocol text,
with at most four text bytes per opaque byte. It introduces no encoding parser
or replacement character. The maximum includes expansion and all labels/counts.
The root refuses incompatible receiving limits before any work, prices the
simultaneously owned semantic result and rendered text, then retains the actual
concrete text through its queued handoff, session history and Turn output. An
actual admitted result cannot be substituted or truncated after an effect.

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
  included; and goes on with the messages that woke it. If a host call was
  answered but its completion is absent from the transcript, the host describes
  that answer in the waking message as text. It does not invent a tool result
  or reissue the old call. A transcript it cannot use (another version,
  another provider or endpoint, malformed, too large) fails the run as
  transient, saying which (session.md, section 3), and the host decides
  what the next run starts from. A run that does not resume starts fresh
  from its brief, which carries what the host thinks it needs.

### 6.1 Named input and settled waiting

`Event::Message` names an admitted live run and an opaque parent-chosen Token,
including zero. Parent names are unique for the active logical run; arrival order
is FIFO and numeric token order carries no meaning. The receiver detects reuse
among queued, currently offered and current-read names. It does not retain an
unbounded history of acknowledged names. Text already includes the parent's
sender label and is attested UTF-8 at the protocol face.

The queue has independent count and per-message byte caps. Busy, TooLarge,
Inactive and ReusedName bounce the actual input without retaining its bytes or
advancing a read fence. A queued message enters main only when it yields. The
name remains offered until an actual main Turn includes that continuation;
only that Turn advances `read`. Children cannot advance main's fence. Waiting
repeats the last actual read name, never a queued or merely offered name.

Every main is offered `wait`, an exclusive write call; children are not. Its ordinary deferred
result is the concrete successful text `waiting`. The call records intent;
Waiting is emitted only when that turn and every actual call have settled and
main yields with an empty inbox. An input wakes the same session and cancels
its idle-park alarm. Input delivered before an alarm at the same clock instant
wins. At the positive idle deadline an empty waiting run closes main and waits
for all actual terminals, tells the last Turn, then answers Parked. Independent
run wall time continues while waiting. A submitted delivery and a requested
cancel still require their real terminal; Waiting cannot manufacture settlement.

### 6.2 Concrete history and activation numbering

Root starts every main and child session through OpenV2. It owns the original
Start reply right and any selected concrete Transcript while the run prepares;
an opaque binding transfers the selected history only to main. False `resume`
drops supplied history and starts from the brief. True with no history starts
fresh. Version, Endpoint, Dialect, Malformed, Unresolved and TooLarge remain six
precise transient transcript failures, with no automatic fresh fallback and no
provider/tool effect before semantic admission. Checkout preparation precedes
main opening as in an ordinary start.

The session restores all typed historical provider blocks and optional complete
replay envelopes, followed by the parent's concrete post-transcript results in
`Transcript.after` and the waking prompt. Those results keep exact provider ID,
text, error and supported replay bytes. They are not executed again. These
post-transcript results belong only to calls present in the transcript's yielded
tail. A host call from a completion absent from that transcript is reported in
waking message text, not added to `Transcript.after`.
Historical sequence continues the prior transcript; the new activation's main
Turn number starts at one. Each root Turn moves its complete body outside before the next
completion or final Answer. Root opaque handoff cells retain bodies only until
that move; parent durable ownership and host ACK metadata are separate.

Accepted, Failed, Delivered and Parked carry the exact number of actual main
Turns emitted in this activation, plus cumulative typed spend. Refused has no
admitted work. Child turns are not main transcript turns. Main Turns carry the
current read fence, including through close when a real completion or result
wins cancellation; an emitted cancel cannot replace that terminal.

## 7. Results

### 7.1 The result contract

A contract allows any of four forms:

- **A report:** text, bounded, with the fields the contract requires.
- **A verdict:** one label from a closed list, each label with a contract
  of its own: which fields it requires, and which items it may carry, how
  many (a least and a most) and of which kinds, each kind with the fields
  it requires.
- **A change:** the workspace's changes, delivered (section 8), with the
  fields the contract requires. Every discovered writable check must pass
  before either final or mid-run delivery (section 8).
- **A failure:** the LLM declares it cannot do the work, with a reason.

A result is its form, its label for a verdict, its text, its fields (a
name and text each) and its items (a kind and fields each). Fields,
labels and kinds are names the host chose: temper's change requires a
title and a body, its review's verdicts are an approval and a request
for changes whose items are follow-up tasks. smith knows only that a
field is required, non-empty and within its bound.

The host supplies inclusive minimum and maximum byte lengths for report
text and a declared failure reason. A zero minimum permits empty text; a
failure contract may require a reason by choosing a positive minimum. A
verdict text may be empty and has its own maximum. Required named fields
remain non-empty, each within its host-declared cap. Extra fields are
allowed, including empty extra values, but all owned names and values count
toward the checked aggregate result-byte limit. Names may occur only once
within each result or item. A contract must have at least one allowed form,
unique nonempty declared names and kinds, valid ranges, and enough aggregate
space for the smallest accepted value of every allowed form, including its
required field and item storage. Otherwise admission refuses it before
preparation, session or IO requests.

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

Every answer but delivered and stale goes back to the LLM as correctable
feedback. Refusal carries a nonempty opaque named explanation, at most 512
bytes, optionally locating a remaining marker by directory and relative path
(at most 4096 bytes). Marker paths are nonempty, relative, have no zero byte,
empty component, `.` or `..` component. Other host-policy refusals need not
claim a conflicted file. Failure carries the generic reason and its sealed
512-byte tail; dropped-byte accounting saturates at the largest integer.

Successful receipts are constructor-sealed: one to 64 distinct directory
ordinals, each with one to 512 opaque bytes. A delivery-capable charter admits
at most 64 mounted directories, also within the receiving directory limit,
so every changed mount is representable. A Report-only charter without the
separate delivery grant does not acquire this cap. The run revalidates every
receipt or marker ordinal against its admitted writable mounts; malformed
host evidence becomes broken feedback and never successful delivery. The host
alone knows which writable directories changed and supplies the complete set.
Each retained receipt copy, including outputs and interrupted final answers,
is charged at container storage plus payload bytes.

A submission carries two different names. The callback owner is a live slab
name, used only for its actual terminal. Its durable host name includes the
host-supplied activation, accepted main completion sequence and assistant block
position (including non-call blocks before it), scoped by the host's logical
run. The host must preserve that logical run identity and issue a new activation
number for each start. Provider ids may repeat in different turns;
neither those ids, callback tokens nor translation tickets are durable names.
Version-two sessions include the restored contiguous transcript prefix in the
sequence; root V2 includes restored history while its live Turn count names
only the current activation. Checked
sequence or block-position exhaustion refuses the completion's effects before a host request.
Zero completion is refused before checks. Calls retain their origin through
ticket translation and repeated provider names. A restarted activation can use
the same sequence and position without reusing a host operation name.

Before submission a withdrawal or expiry may abort checks, and no delivery
begins once the caller deadline has passed. After submission no host cancellation
exists: the actual operation runs to its deadline, the earlier of caller expiry
and the receiving delivery timeout. The host returns exactly one actual terminal
by that deadline (delivery, failure or another typed outcome); it must report a
landing that won the deadline race. Transport may deliver that terminal later.
A caller-only withdrawal or expiry does not decide run shutdown: an actual
mid-run landing returns receipts and continues while the run still works.
The run waits for it even while winding down. Duplicate or stale callback
generations are inert, never another semantic delivery.

A final Change that lands is the accepted result even during shutdown. A
mid-run landing normally returns receipts and continues. If shutdown was already
decided while a mid-run delivery was pending, its final answer is **delivered**:
the real durable name and receipts, the already-decided typed stop and accepted
spend. This is host evidence distinct from an LLM-declared accepted result; a
Report-only charter produces neither an invented Report nor an undeclared Change.

### 8.3 Merges in progress

A repository may start from a merge in progress, the host having merged
and left the conflicts in the working tree; the start names the files in
conflict, and the run tells the LLM. The LLM edits them and runs the
checks; it has no git writes (tools.md, section 5). Delivered, the host
commits the tree as the merge, and refuses while a file the merge left
in conflict still holds a marker, naming it.

The initial conflict list is host-supplied metadata for a git working tree,
including an informative list on a read-only directory. Checks and deliveries
act only on writable directories. The host's actual merge state decides whether
markers remain and whether a merge commit can land; the run does not infer a
successful merge from its initial list or an emitted delivery request.

### 8.4 Delivering mid-run

A run whose charter grants `deliver` may deliver before it finishes, as
a chat does with a small fix before handing it on. The grant carries its own
required named-field caps, checked minimum storage and aggregate ownership
bound; it is independent of the final result contract and is offered only to
main. Final Change permission alone does not grant `deliver`. Preparation looks
for writable checks when either route is allowed, and every such executable runs
for every delivery. Report, Verdict and declared Failure finishes run no checks.

Main's delivery is an exclusive write batch: no other write or sub-agent can
intervene between the first check and host submission. Delivered receipts
normally return as tool feedback and the run continues to an allowed final
result. A concurrent stop preserves the typed delivered answer described in
8.2; a later independent stop after continuation follows the ordinary stop
path, since the host already observed the earlier landing. A finish with a
Change delivers the state at that finish.

## 9. Budget and spend

### 9.1 Admission and model prices

The host gives one `Budget { turns, spend, time }` for the whole activation,
across main and every child. `spend` is an integer in the host's unit; smith
assigns it no currency or deployment meaning. All three allowances must be
positive and fit the corresponding receiving `Limits.budget`. An unworkable or
oversized budget is refused before discovery, session opening or provider IO.
Per-kind input, output, cache-read and cache-write token caps remain session
receiving limits; they are not four run financial budgets.

Every main or listed child `Llm` carries `Prices { input, cached, output, unit }`.
The divisor `unit` must be positive; a zero divisor refuses that model before
effects, including a listed model not selected yet. Rates may be zero. Session
alone prices each actual completed call: fresh input and cache writes use the
input rate, cache reads use the cached rate, and output uses the output rate.
The sum of the three rational terms is rounded upwards once per completion
using checked arithmetic (domain/session.md, section 6). Provider-neutral usage
already separates fresh input from both cached counters; no cached token is
charged again as fresh input. Skein supplies usage and wire mechanics, never
these host prices or financial policy.

A child `Share { turns, spend }` is clamped to the run's remaining allowances.
Its turns cap counts that child session's own completions; every descendant
completion also counts against the global tree-wide turn cap. Its spend cap is
inclusive of descendants. Its wall deadline inherits the remaining run time. An absent share gets that
remainder. Zero turns, zero spend or no remaining time refuses the call before
opening the child, even for a zero-rate model. No token split or implicit price
conversion remains in the run contract.

### 9.2 Own charges and inclusive child bills

Every session reports cumulative `own_spent` from its own actual completions
and inclusive `spent` from those completions plus its descendants. Both start
at zero for each activation. A child's terminal, whether answered, failed or
withdrawn, carries its inclusive bill under the original call identity. The
parent's existing once-only terminal guard precedes charging; duplicates and
stale generations are inert. The parent adds that bill only to its inclusive
sum and local share. Its Turn then tells the inclusive sum after its real calls
settle. Restored Turns keep their original recorded spend but are never charged
again to the new activation.

The run charges each monotonic own-completion delta immediately, once, across
all sessions. A parent child-bill update has an own delta of zero. The run never
adds a child's inclusive bill a second time and never waits for a parent's
terminal to discover a crossing charge. Raw provider counters and completion
counts are observed separately from currency. Child Turns remain child records;
only main Turns form the host transcript.

### 9.3 The next-completion gate

Before root publishes any actual completion request, it checks that the
conversation is live and Running, then the run's current scalar spend and turn
count. Reaching either cap prevents the next request:
there is no credential lease, retained provider context, Client, retry or
external cancellation for the denied request. Root returns a typed unsent
budget denial directly to the calling session. That session releases its
reserved provider credit, closes its kit and emits its one real Ended terminal
after KitClosed. It creates no Usage, price, Turn or provider cancellation.
A queued but unpublished request whose conversation or run is already closing
uses a separate unsent-close terminal; it cannot acquire a financial denial or
replace the run's actual cancel, deadline or hard-fault reason.

A completion already admitted before another session crosses may finish and is
charged once. Ordinary finite scalar or turn exhaustion preserves every call
of the crossing completion, including reads, writes, host calls, deliveries and
waiting, until its real terminal. Already-running children also settle. A valid
Finish in that same turn may still succeed; without Finish, the run fails as
budget exhaustion after its genuine Turn. No later completion starts. Overshoot
is therefore at most one already-admitted completion per open session. Time
expiry and explicit cancellation retain their immediate close behavior.

### 9.4 Overflow and reporting

Financial and raw-usage arithmetic is checked independently. A failed cumulative
addition retains the last representable prefix and sets a sticky attestation;
that prefix is never described as the exact total. Raw usage checks all four
additions before changing any cumulative field. Actual per-completion usage
remains exact. Own and inclusive currency sums have independent overflow flags:
an unknown child bill cannot suppress a still-representable own charge.

A structurally valid accepted completion still produces its actual Turn once,
with the spend-overflow attestation, before the typed PriceOverflow or
UsageOverflow terminal. An unknown delegated bill carries its representable
prefix and attestation, never an invented zero charge. Run, root, derived facts
and host metadata preserve the corresponding currency and usage attestations;
terminal accounting never subtracts an unknown cumulative usage total to
reconstruct a remainder.

Hard arithmetic failure takes precedence over an unlanded Finish, unlike normal
finite-cap exhaustion. A final Change that already landed remains Accepted with
its durable outcome and attested spend, following section 8.2; that answer has no
separate typed stop
field. An interrupted mid-run landing remains Delivered with its actual receipts,
typed stop and attested spend. Neither form loses landed evidence to a later
accounting fault. Late actual completions during closing still update
representable charges and overflow attestations. Every admitted answer reports
its activation-only raw counters, turns and scalar units, including both
attestations. Source and gate status for this contract are tracked in
`docs/development/migration-05s4-budget.md`; a draft is not an acceptance claim.

## 10. The answer

A run answers once, after every turn it counts:

- **accepted,** with its result, its turn count and what it spent;
- **delivered,** only when an actual mid-run delivery lands during an already
  decided shutdown: its durable name, per-directory receipts, typed stop and
  cumulative spend. It is not an LLM-declared accepted result;
- **parked,** with its turn count and what it spent;
- **failed,** typed so the host can act without reading prose: the model
  (a provider's failure past its retries, an account exhausted), the
  budget, policy (a call or result the run refused to make), cancelled,
  stale, or a transcript it could not resume; each with what it spent.

Refused at the entrance, it answers refused: busy, or invalid, saying
what is beyond the limits. A cancel from the host winds the run down
along the same path; a final Change landing remains accepted, and a mid-run landing during that
shutdown preserves its distinct delivered answer (8.2 and 8.4).

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
- **Rendering:** the system text (3.3), outcomes and problems, as the
  text the LLM reads.
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

The Workspace increment exercises a workspace-free native host call, wait,
park and genuine transcript restore in both wire configurations; mixed
git/plain mounts and independent child authority; and an actual shared-git
initial merge whose marker refusal precedes resolution, checks and a
two-parent commit. Receiving and component ownership controls remain in
the run, host and native root worlds; the increment ledger records their
exact source and full-suite evidence.

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
  resuming from a transcript, calls asked again, and
  merges in progress.

The first 05s4 RESULTS and DELIVERY increments implement generic final forms,
separately granted main delivery, all discovered writable checks, sealed actual
host terminals and stable transcript-derived naming. The host-unit budget contract replaces the copied token split as described
in section 9; its current source and gate status are tracked separately. The Instructions/Brief increment
separates the main role from ordered titled sections (3.1 and 3.3), preserving
the child's own raw task scope (5.3); its reviewed temporary source and gates
are recorded in
`docs/development/migration-05s4-brief.md`. The CONVENTIONS increment replaces fixed discovery/check paths
with the bounded host-supplied pair described in section 3.1; Temper-style
callers explicitly select `.temper/pre-pr`. The HOST TOOLS increment replaces closed forge
and outlet grants with bounded declarations and opaque durable relays, settled
recovery and exact first-record answers. The MESSAGES increment implements
named FIFO input, settled main-only wait/wake/park, concrete root V2 Turns,
activation counts and exact selected-history/post-transcript restoration. It
also repairs session receiving ownership before provider and tool effects
(domain/session.md, section 3.1), preserving maximum actual late terminals.
Provider-neutral canonical feedback lives in the root; shared Client/peer and
replay codecs belong to Skein. The WORKSPACE increment separates optional workspace
from the charter, carries git kind and initial conflict paths into main/child
openings, and admits its aggregate ownership before effects (section 3.2).
Its source and world evidence are tracked separately. The BUDGET increment
tracks scalar prices, own/global conservation, inclusive child bills and the
next-completion gate in `docs/development/migration-05s4-budget.md`. System
rendering extraction, live channel/transcript codecs and the executable remain
open. Source and validation status are recorded in
`docs/development/migration-05s4-messages.md` and
`docs/development/migration-05s4-conventions.md`, and
`docs/development/migration-05s4-workspace.md`; a source draft alone is not a
gate claim.

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
