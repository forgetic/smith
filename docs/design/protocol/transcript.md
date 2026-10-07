# Turns and transcripts

Provisional, 2026-10-07. The records of a conversation kept beyond its
process: a turn, as a run tells it, and a transcript, as a run resumes
from it (domain/session.md, section 3). They are the family
`smith-transcript`, versioned on their own, since a host keeps them as long
as it likes and a later agent reads them (README.md, section 8).

## 1. In one page

- **A turn is one completion:**
  - the messages around it, in order;
  - the provider's blocks, in position;
  - its calls, as the LLM wrote them;
  - their results;
  - what it used and cost.
- **A transcript is turns, in order.** A host keeps each turn's bytes as it
  was told and sends them back, in order, to resume. It needs no codec of
  smith's to do so.
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

- **endpoint:** the name of the endpoint it ran on, as the agent's
  configuration names it, and the dialect of its provider;
- **its place in the conversation:** from one, going on across the
  activations that resume it. The channel numbers turns per activation
  (channel.md, section 4);
- **usage:** the tokens the provider counted (input, output, cache reads
  and cache writes);
- **spend:** the run's spend so far, in the host's unit, sub-agents included
  (domain/session.md, section 6);
- **messages,** oldest first, each from the user or the assistant, each a
  list of blocks:
  - **text,** with the provider's replay, if any;
  - **a refusal,** with the provider's replay, if any;
  - **a call:** the provider's id for it, the tool's name, and its input as
    the LLM wrote it, with the provider's replay, if any;
  - **a result:** the call's id, and one of:
    - a workspace tool's outcome, typed (domain/tools.md's vocabulary: lines read,
      entries listed, an edit made, a command's exit and output);
    - text the domain wrote, as a result or an error: a served tool's, or a
      host tool's answer;
    - invalid, with the problems found in the call;
    - not run;
    - withdrawn;
  - **an opaque block:** the provider's bytes, tagged with its dialect.

A call's typed form is not kept. The agent decodes the input again when it
resumes, as it did when the call was made.

## 3. A transcript

- **Its turns, in order,** each as its own bytes. They must share a version,
  an endpoint and a dialect, and be numbered without a gap.
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
- numbers without a gap.

The session checks the rest before a completion starts (domain/session.md,
section 3):

- calls and results paired by their ids;
- no unresolved ticket;
- room for the waking prompt and a completion.

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
- **For people,** a host renders turns itself. smith offers the decoded
  records, not a presentation.

## 6. Versions

- **Version 2 is the first,** continuing the conversation vocabulary whose
  first version stays with temper's legacy run (domain/README.md,
  section 7).
- **A reader keeps a range,** and translates older turns into the current
  domain's terms. A version it does not read is the transcript refusal
  *another version*.
- **A host may keep turns of several versions** across runs. One transcript
  holds one version, because a resumed conversation started once.

## 7. Limits

- **A turn** is bounded by the bytes a session may hold (domain/session.md,
  section 6), within the schema's ceilings.
- **A transcript** is bounded by the resume limit, which is the host's and
  the agent's configuration's: the turns and bytes the agent takes in a
  start.
- **A host that keeps turns** bounds what it keeps unacknowledged with the
  channel's window (channel.md, section 7).

## 8. What the domain is owed, and what skein owes

- **The domain is owed** turns translated from `smith-domain-session`'s
  entities as they are told, and back on resume. Endpoint names are
  resolved through the agent's configuration both ways.
- **skein owes** the codec generator (skein's `codec.md`), and providers'
  replay as bounded bytes, tagged by dialect (skein's `llm.md`).

## 9. The world

- **The codec:**
  - golden bytes for every block and result;
  - each decoder fuzzed;
  - a domain turn encoded and decoded back unchanged.
- **Resuming,** in the channel's protocol world (channel.md, section 10):
  - from turns kept by a scripted host;
  - each refusal;
  - a transcript from another dialect;
  - an older version translated.
- **A large transcript:** at the resume limit, measured against the worst
  case.

## 10. From temper

- **Kept:** temper's second conversation version: turns told with names
  resolved, versioned, with opaque blocks verbatim and spend in a unit.
- **Gone:** snapshots, and tickets meaningful only inside the session that
  made them.

## 11. Open questions

- **Another provider:** whether a later version carries a provider-neutral
  form, so a run can resume elsewhere (domain/session.md, section 11).
- **Summaries:** how a summarising session's result sits in a transcript,
  once context management comes (domain/session.md, section 8).
