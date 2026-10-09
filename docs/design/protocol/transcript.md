# Turns and transcripts

Provisional, 2026-10-07, revised 2026-10-09. The records of a
conversation kept beyond its process: a turn, as a run tells it, and a
transcript, as a run resumes from it (domain/session.md, section 3). They
are the family `smith-transcript`, versioned on their own, since a host
keeps them as long as it likes and a later agent reads them (README.md,
section 8).

## 1. In one page

- **A turn is one completion:**
  - the messages around it, in order;
  - the provider's blocks, in position;
  - its calls, as the LLM wrote them;
  - their results;
  - what it used and cost.

  A window turn is a compaction's: its messages are the new window's
  opening (domain/session.md, section 8.2).
- **A transcript is the current window's turns, in order:** from the
  conversation's first turn, or from its last window turn. A host keeps
  each turn's bytes as it was told and sends back, in order, those from
  the last turn whose `opens_window` is true to resume. It needs no codec
  of smith's to do so: the channel's turn record carries `opens_window`
  beside the body (channel.md, section 4).
- **The affinity is kept once,** in the transcript's opening turn, so a
  resumed conversation keeps it (domain/session.md, section 2).
- **Usage counts are optional.** A count the provider did not report is
  absent, not zero.
- **Opaque provider blocks travel verbatim,** tagged with the dialect that
  wrote them, and go back only to the same endpoint and dialect.
- **Results are kept as the domain holds them.** Text that the domain wrote
  is kept as text. A workspace tool's outcome is kept typed, and rendered
  for the LLM again when the conversation resumes.
- **Names resolved.** A turn means the same outside the session that made
  it: no ticket and no slot number, only the run's call names and the
  provider's ids.
- **Readers beyond the agent:** a host that shows turns, such as temper's
  web, decodes them with `smith-transcript` and renders them as it likes.

## 2. A turn

A versioned record. Every turn starts with its version.

- **kind:** ordinary, or window, a compaction that opened a new window
  (domain/session.md, section 8.2);
- **endpoint:** the name of the endpoint it ran on, as the agent's
  configuration names it, and the dialect of its provider;
- **its place in the conversation:** from one, going on across the
  activations that resume it and across windows. The channel numbers turns
  per activation (channel.md, section 4);
- **affinity,** in an opening turn only (section 3): the run tree's
  identity, sixteen opaque bytes, and the conversation's thread, an
  ordinal, main's being zero (domain/session.md, section 2);
- **usage:** the tokens the provider counted, each optional and absent
  when not reported: input, output, cache reads, cache writes and
  reasoning tokens (domain/session.md, section 6);
- **spend:** the run's spend so far, in the host's unit, sub-agents included
  (domain/session.md, section 6);
- **messages,** oldest first, each from the user or the assistant, each a
  list of blocks:
  - **text,** with the provider's replay, if any;
  - **a refusal,** with the provider's replay, if any;
  - **a call:** the provider's id for it, the tool's name, and its input as
    the LLM wrote it, with the provider's replay, if any;
  - **a cut call:** the provider's id for it, the tool's name, and how many
    bytes of input it had when the completion reached its output limit.
    Its input is not kept, and its result is invalid with the problem *cut
    off* (domain/session.md, section 4);
  - **an oversized call:** the provider's id for it, the tool's name, and
    the size of the input skein counted and never held. Its result is
    invalid with the problem *too large* (domain/session.md, section 5);
  - **a result:** the call's id, and one of:
    - a workspace tool's outcome, typed (domain/tools.md's vocabulary: lines read,
      entries listed, an edit made, a command's exit and output);
    - text the domain wrote, as a result or an error: a served tool's, or a
      host tool's answer;
    - invalid, with the problem found in the call (domain/tools.md,
      section 4);
    - not run;
    - withdrawn;
  - **an opaque block:** the provider's bytes, tagged with its dialect.

A call's typed form is not kept. The agent decodes the input again when it
resumes, as it did when the call was made.

A window turn's messages are one user message holding text blocks: the
conversation's task, the summary, and the messages held back for the new
window. It holds no call. Its usage and spend are the compaction's.

## 3. A transcript

- **Its turns, in order,** each as its own bytes, from its opening turn:
  the conversation's first turn, or its last window turn. They must share a
  version, an endpoint and a dialect, and be numbered without a gap from
  the opening turn's place.
- **`opens_window`** is true of a turn that can open a transcript: one
  of kind window, and the conversation's first turn, at place one. A
  told turn carries it, on the channel's turn record and in the run's
  vocabulary to a host in one process (domain/host.md, section 2), so a
  host finds the current window without decoding a turn. A host resuming
  a run sends back only the turns from the last one with `opens_window`.
- **The opening turn carries the affinity,** and no other turn does. Every
  window turn opens a transcript, and so carries it too.
- **Earlier windows are not part of it.** A host may keep their turns for
  people to read, but never sends them back. Once a window turn is told,
  the transcript the next resume needs starts there.
- **Answers the transcript lacks** are not part of it. A call whose turn
  was never told, or one the run withdrew and its host decided later, has
  no open place in the history. The start carries the host's answers to
  them beside the transcript, and the run tells the LLM of them as text in
  its waking prompt (channel.md, section 3).

What a host keeps is therefore the turns' bytes, and nothing it must
assemble.

## 4. Resuming

The codec checks what a schema can:

- bounds;
- tags;
- text;
- one version, endpoint and dialect across the turns;
- numbers without a gap;
- an affinity on the first turn and on no other;
- a first turn that is a window turn or at place one, and no window turn
  after it.

The session checks the rest before a completion starts (domain/session.md,
section 3):

- calls and results paired by their ids;
- no unresolved ticket;
- room for the waking prompt and a completion within its byte limits. A
  history that has that room but fails the window check is compacted
  before its first ordinary completion (domain/session.md, section 8.1).

Each refusal is distinct, and the run fails as transient, saying which
(domain/run.md, section 6):

- another version;
- another endpoint or dialect;
- malformed;
- unresolved;
- too large.

## 5. Rendering

- **A workspace tool's outcome is rendered by the LLM component**
  (llm.md), live and on resume alike. A later release may render an old
  outcome in other words; what the LLM learns from it is the same.
- **Text the domain wrote** goes back to the LLM as it was written, so a
  served tool's and a host tool's results read the same after a resume.
- **A cut or oversized call** goes back to the provider as a call with an
  empty input, answered by its error, so the history stays replayable
  (llm.md, section 4).
- **The affinity is not text.** The session puts it into every prompt, and
  skein's dialect renders it (domain/session.md, section 2).
- **For people,** a host renders turns itself. smith offers the decoded
  records, not a presentation.

## 6. Versions

- **Version 2 is the first,** continuing the conversation vocabulary whose
  first version stays with temper's legacy run (domain/README.md,
  section 7).
- **Version 3 is current.** It adds:
  - a turn's kind, and the window turn;
  - the affinity, in a transcript's opening turn;
  - usage counts as optional values, with reasoning tokens;
  - the cut and oversized calls, and the problem *cut off*;
  - blocks per message derived from calls per response (section 7).
- **A reader reads one version,** the current, like every format
  (README.md, section 8). Any other is the transcript refusal *another
  version*, and nothing is translated: a version-2 transcript is
  refused, and its host starts the run afresh from its brief
  (domain/run.md, section 6).
- **A host may keep turns of several versions** for people to read. One
  transcript holds one version, and only the current version's resume.

## 7. Limits

- **A turn** is bounded by the bytes a session may hold (domain/session.md,
  section 6), within the schema's ceilings.
- **A message's blocks** are bounded by a number derived from the declared
  calls per response (protocol/limits.md). It is never below the
  completion's block bound, and leaves room for the text a message holds
  beside calls or results: the messages appended after results, and a
  window's opening.
  The schema's ceiling is the vocabulary's, and the derived bound is
  within it. A configuration that breaks either relationship is refused at
  startup, naming it, so every completion a session accepts can be told.
- **A transcript** is bounded by the resume limit, which is the host's and
  the agent's configuration's: the turns and bytes the agent takes in a
  start. Since a transcript holds one window, the resume limit is at least
  the most a session holds in a window (protocol/limits.md), so every
  transcript a live session leaves can be resumed, however long its
  conversation.
- **A host that keeps turns** bounds what it keeps unacknowledged with the
  channel's window (channel.md, section 7).

## 8. What the domain is owed, and what skein owes

- **The domain is owed** turns translated from `smith-domain-session`'s
  entities as they are told, and back on resume, with their kind and an
  opening turn's affinity. Endpoint names are resolved through the agent's
  configuration both ways.
- **skein owes** the codec generator (skein's `codec.md`), and providers'
  replay as bounded bytes, tagged by dialect, with a call cut off at the
  output limit marked as such (skein's `llm.md`).

## 9. The world

- **The codec:**
  - golden bytes for every block and result, each kind of turn, an
    opening turn's affinity, and usage counts absent and zero;
  - each decoder fuzzed;
  - a domain turn encoded and decoded back unchanged;
  - each refusal: an affinity on a later turn, a window turn after the
    first, a message's blocks over the bound.
- **Resuming,** in the channel's protocol world (channel.md, section 10):
  - from turns kept by a scripted host;
  - from a window turn after several compactions, earlier windows not
    sent;
  - each refusal;
  - a transcript from another dialect;
  - a version-2 transcript refused as another version;
  - `opens_window` on a conversation's first turn and on each window
    turn, and on no other, and a host resuming from the last of them.
- **A large transcript:** at the resume limit, one window at its largest,
  measured against the worst case.

## 10. From temper

- **Kept:** temper's second conversation version: turns told with names
  resolved, versioned, with opaque blocks verbatim and spend in a unit.
- **Gone:** snapshots, and tickets meaningful only inside the session that
  made them.
- **New:** windows, the affinity kept once, and optional usage.

## 11. Open questions

- **Another provider:** whether a later version carries a provider-neutral
  form, so a run can resume elsewhere (domain/session.md, section 11).
