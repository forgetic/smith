# LLM calls

Provisional, 2026-10-07, revised 2026-10-09. `smith-protocol-llm` is the
agent's component for LLM calls. It translates `smith-domain`'s LLM
vocabulary to skein's LLM client and back:

- a completion asked for, and its cancel;
- the completion, the failure or the cancellation that answers it.

skein's LLM connection component carries each call: connecting, TLS and
deadlines (skein's `llm-connection.md`). This component writes tools'
schemas, decodes what the LLM calls, and renders what the tools return.

## 1. In one page

- **Translation.** A completion the domain asks for becomes one call of
  skein's client. Its blocks, usage and stop come back as one completion,
  or as one failure with its class and evidence. Retrying is the session's
  (domain/session.md, section 4); nothing retries here.
- **Schemas are the protocol layer's:**
  - smith's own tools' are written here, stating the limits they meet;
  - `finish`'s is generated from the result contract;
  - host tools' pass through as the charter declared them.

  A conversation's tools are fixed for its life.
- **A stable prefix.** Within a conversation's window, what comes before
  the history never changes, the history only grows, and encoding is
  byte-deterministic, so a provider's cache can serve each prompt's
  prefix (section 2.2).
- **Decoding is total.** Every call the LLM writes becomes a typed call, or
  an invalid call with typed problems for the domain to answer, an
  oversized call included.
- **Rendering writes words, not decisions.** The domain decides what an
  outcome keeps, such as how much of a command's output (domain/tools.md,
  section 4). This component writes that as text, and passes text the
  domain wrote as it was written.
- **Credentials come from the grant table,** read when a call starts
  (channel.md, section 6).
- **Provider-neutral.** The endpoint's dialect is the agent's
  configuration's (agent.md), and skein's client speaks it.

## 2. Prompts

A completion's prompt is built from what the domain gives:

- **the system text,** which the domain wrote (domain/run.md, 3.3), after
  any identity blocks the endpoint's configuration asks skein's client for;
- **the messages,** in order. Each block is translated, and a provider's
  replay goes back verbatim, only to its own dialect;
- **the tools offered,** with their schemas (section 3), and the
  completion's tool choice;
- **the model, and the prompt's output cap,** the charter's `output` for
  that model (charter.md, section 2): Anthropic's dialect sends it as
  `max_tokens`, and Codex's route takes none, so there it stays a local
  bound on the answer (skein's `llm.md`, sections 4.6 and 4.7);
- **the endpoint's options,** from its configuration: reasoning effort;
- **the affinity,** the conversation's (2.1).

### 2.1 Affinity

- **One key per run tree, one thread per conversation.** `Affinity { key,
  thread }`: the key is the run tree's identity, 16 opaque bytes, from
  the host's start or minted by the run from its seeded random state
  (domain/run.md, 3.2); the thread is the conversation's ordinal, main's
  being 0. The session puts the affinity unchanged into every prompt of
  the conversation (domain/session.md, section 2). This component passes
  it on and never makes one.
- **Each dialect renders it** (skein's `llm.md`):
  - Codex: the key as the body's `prompt_cache_key` and as the
    `session-id` header, and the thread as the `thread-id` header;
  - Anthropic: no effect on the wire.

  These per-call headers are skein's client's. The names `session-id` and
  `thread-id` are reserved: an endpoint's configuration that sets either
  is refused.
- **No static cache key.** An endpoint's configuration carries none.
  Affinity is the run's, not the endpoint's, and is never derived from a
  credential or a path.

### 2.2 The stable prefix

- **Fixed within a window:** the model, the endpoint's options, the
  affinity, the system text and the tools offered. History is
  append-only: the prompt for a completion is the prompt before it, then
  that completion's blocks, then what was appended after them.
- **Dynamic guidance is appended,** as messages after the latest
  results: budget notes, a final turn's note, a wind-down's. It is never
  placed in the system text or a tool's description, which are written
  once, when the conversation opens.
- **A window boundary is the one exception.** A compaction opens a new
  window, whose prompt starts afresh from the task and the summary
  (domain/session.md, section 8).
- **Byte-deterministic.** The same prompt becomes the same bytes:
  translation here, rendering (section 5) and skein's dialect encoding
  are functions of their input alone, with a fixed order of fields and no
  clock or random value.

### 2.3 Cache markers

- **Anthropic's breakpoints are skein's.** The Anthropic dialect puts
  `cache_control` on the last system block and on the prompt's tail,
  never more than four, with the dialect's default lifetime (skein's
  `llm.md`). Where they go is a function of the prompt's shape, so smith
  places none.
- **Codex** caches a prefix on its own, routed by the affinity (2.1).

## 3. Tools' schemas

| Tool | Its schema says |
|---|---|
| `read` | `path`; optionally `offset`, the first line, counting from 1, and `window`, in bytes, at most the read ceiling |
| `list` | `path`; optionally `depth`, in levels beneath it, and `glob` |
| `search` | `pattern`; optionally `path` and `glob`, to search within |
| `write` | `path`, and `content`, the whole file, at most the tool payload in bytes |
| `edit` | `path`, and `edits`, each an `old`, a `new` and optionally `all`, in the dialect's edit format |
| `shell` | `command`; optionally `timeout_seconds`, at most the effective maximum, and `output_bytes`, at most the output ceiling |
| `finish` | the result contract's forms (below) |
| `deliver` | the change's required fields |
| `wait` | nothing |
| `sub_agent` | `brief` and `tools`, among the workspace's families; optionally `llm`, among the charter's models, `max_turns`, in completions, and `max_seconds` |
| a host tool | what the charter declared, as it came |

- **Descriptions say what the domain enforces, with units and limits.**
  Every parameter has a description, and each limit it meets is given
  with its unit and its maximum: the read ceiling, the tool payload, the
  output ceiling, the deadline's maximum. They are the effective values
  (section 8), written in when the conversation opens, so the LLM meets
  no hidden cap. Paths are described as domain/tools.md, section 2 says:
  with one directory, relative to the working directory, whose name is
  never shown; with several, the working directory named and the others
  as `/name/...`. The descriptions also say that a `write` needs the
  file's current version read, that an `edit` needs each `old` to match
  once in the file as it is, and that output past a limit is cut and
  marked.
- **Batching is invited.** The descriptions say that independent reads,
  listings and searches may be called together in one response, and run
  side by side, and that writes, edits and commands may be called
  together too, and run one at a time in call order (domain/session.md,
  section 5). No description says that a tool runs alone unless the
  session runs it alone, and none asks the LLM to call a tool by itself.
- **The edit format is the dialect's.** Each dialect is offered the edit
  format its models are measured to handle best, old and new text or a
  patch, and every format decodes to the same typed `edit`
  (domain/tools.md, section 3). Until a measurement chooses otherwise,
  every dialect is offered `edits` as old and new text.
- **`sub_agent { brief, tools, llm?, max_turns?, max_seconds? }` asks for
  a share.** `max_turns` and `max_seconds` are the share asked for, with
  no maximum in the schema; the run clamps each to what is left above
  its reserve (domain/run.md, 5.3). The child is shown its effective
  share in its system text, and every sub-agent's result ends with a
  usage line. Spend is derived from the share, never asked
  for and never shown. There is no `agents` field: a sub-agent never
  opens sub-agents (section 4). The brief is the child's text in its
  window's host part, and its schema states that bound: a brief past it
  is invalid, too large (limits.md, 3.9).
- **`wait`'s result is deferred.** The run withholds it until a message
  arrives, and then the result and the message go in one request, so an
  exchange costs one completion (domain/run.md, section 6). Its
  description says that it returns with the next message.
- **The tool choice narrows; the tools never change.** A conversation is
  offered the same definitions for its life. Which of them a completion
  may call is skein's neutral `ToolChoice`: `Auto`; `None`, for a child's
  final turn; or `Only(names)`, for a wind-down's `finish` and, when
  granted, `deliver` (domain/run.md, section 9). A dialect that cannot
  express `Only` renders `Auto`, and the session answers a call outside
  the set as not run.
- **`finish`'s schema is generated from the contract** (charter.md, 2.1).
  It has:
  - one branch per form allowed;
  - each verdict's label as one of a closed list, and each item's kind
    likewise;
  - each required field by name, with its bound as a JSON Schema
    `maxLength`.

  The schema is written with skein's JSON writer, measured first.
- **Names are unique across sources.** A charter that declares a host tool
  named like one of smith's is refused at the entrance (domain/run.md,
  5.1).

## 4. Decoding

- **The input is kept as the LLM wrote it,** bytes that may be malformed,
  and decoded with skein's JSON tokenizer, which keeps an explicit bounded
  stack.
- **Problems are typed and named:**
  - an unknown tool;
  - input that is not an object;
  - a missing field;
  - a field of the wrong type;
  - a value out of range;
  - input too large, with its size and the bound it passed;
  - input cut off by the output limit;
  - nesting asked for.

  The domain answers each as an error the LLM can fix. Unknown fields are
  ignored, but for `sub_agent`'s `agents`, which asks for nesting.
- **An oversized call is the call's problem, not the completion's.**
  skein's client delivers a call whose arguments pass the bound on one
  call's input as an oversize block: its name, its provider id and its
  size, the arguments elided and never held. The completion completes,
  and its other calls are decoded as usual. The block decodes to an
  invalid call whose problem is *too large*, with the size and the bound,
  and the LLM is told how to make it fit: a smaller `write`, or an
  `edit` (domain/tools.md, section 4).
- **A call cut off** by the output limit, which skein delivers as
  `Block::Cut`, decodes to an invalid call whose problem is *cut off*,
  with the bytes of input it had. The session keeps a note and an error
  result in its place, so the history stays replayable
  (domain/session.md, section 4).
- **Neither goes back as it came.** skein refuses a history that holds
  an oversize or a cut block (skein's `llm.md`, 2.4), so this component
  replaces each when it builds a prompt from history: a call with its
  provider id and its tool's name, and an empty object as its input,
  followed by its error result, which says what was wrong and how to
  make the call again. A transcript keeps the same (transcript.md,
  sections 2 and 5).
- **A dropped reasoning item,** `Block::Dropped`, where the model's
  declaration opts in (limits.md, 2.1), never enters history: the
  completion is kept without it, and the drop is a fact the event stream
  tells (events.md, section 3.9).
- **Nesting is refused.** An `agents` field in a `sub_agent` call, or a
  sub-agent's call of `sub_agent`, which it is never offered, is invalid
  with the problem *nesting*, which tells the LLM that a sub-agent does
  its work itself (domain/run.md, 5.3).
- **A host tool's input** is checked only to be a JSON object within its
  bound, and is relayed as bytes (domain/run.md, 5.2).
- **A result for `finish`** is decoded into the domain's result: its form,
  label, text, fields and items. The domain judges it against the contract.

## 5. Rendering

- **A workspace tool's outcome** becomes the text the LLM reads:
  - lines with their numbers, which lines of how many, and the line the
    next window starts from;
  - a tree's entries, as paths beneath the directory listed, and how many
    more there were;
  - search matches;
  - what a write or an edit changed, and for an edit that failed, which
    of its edits and why;
  - a command's exit, and the head and tail of its output, saying what was
    dropped between them;
  - a request clamped to a ceiling, saying so and naming the ceiling;
  - a failure with its output.

  The same rendering serves a live call and a resumed transcript
  (transcript.md, section 5).
- **A path's failure names the path it resolved to:** relative to the
  working directory when there is one directory, as `/name/...` when
  there are several, so a directory's name written as a prefix shows as
  the path it made.
- **Valid UTF-8 passes through.** Text goes to the LLM as it is,
  multi-byte characters included, so what the LLM copies from a result
  into an `edit` matches the file. Only invalid sequences and control
  characters other than a newline and a tab are escaped, visibly.
- **Oversized output is cut, never refused.** A rendered result longer
  than its bound (section 8) is cut at a character's boundary, with an
  explicit marker saying how many bytes were left out, and the request
  goes. Startup checks that each tool's outcome at its caps renders
  within the bound, so the cut is a safeguard, not a tool's limit.
- **A compaction's results are cut to its room.** The results owed to a
  compaction are rendered within what the window leaves above the
  compaction's history, less the summary instruction and one output,
  each cut with its marker as the rest require (limits.md, 3.9). Only
  that request's tail changes: the history before it is as it was sent,
  so the cached prefix holds.
- **Deterministic.** Within a release, the same outcome renders to the
  same bytes, live and on resume, as the stable prefix needs (2.2).
- **An invalid call** is rendered with its problem, naming the field; too
  large names the size and the bound, and what to do instead.
- **Not run and withdrawn** are short fixed texts saying which. A call
  outside the tool choice is not run, and its text says which calls may
  be made now.
- **Text the domain wrote,** a served tool's or a host tool's, goes as
  written, its error flag mapped to the dialect's.

## 6. Completions and failures

- **Blocks** come up as skein's client completes them: text, refusals,
  calls, oversized and cut calls, dropped reasoning, replay. The completion follows, with the stop
  reason (the LLM ended, called tools, ran out of tokens, or refused) and
  the usage: input, cache read, cache write, output and reasoning tokens,
  each one reported or not (skein's `llm.md`).
- **Text in flight** is counted for the run's facts, never carried to
  the domain or the channel (channel.md, section 8). The deltas go to
  the service's event sink as `text.delta` records, only under the
  capture policy or when a front end asks (events.md, section 3.5).
- **Measured for the event stream:** the request's bytes as encoded,
  measured before it is sent; the time to the response's first byte; and
  the largest gap between its events. They go to the event sink beside
  the session's facts for the attempt, never to the domain, and decide
  nothing (events.md, section 3.5).
- **Failures** keep the class and the evidence skein's client gives them:
  - the classes: unavailable, timed out (with its phase), overloaded,
    rate limited or exhausted (each with how long to wait), context too
    long, invalid, unauthorized, a local limit, a protocol breach;
  - skein's `Limit { which, bound }` becomes the domain's local limit,
    naming the limit that fired by its kind and giving its bound;
  - skein's `TimedOut { phase }` becomes timed out with the same phase;
  - the evidence: unsent, possibly sent, or a response received with
    its HTTP status, `Evidence::Response { status }`, which every
    failure after a response's head carries.

  The session's classes are these, unchanged (domain/session.md,
  section 4).
- **A failure's detail is the operator's.** The provider's text goes up
  with the failure, within skein's failure detail (limits.md, 3.2). The
  operator's line on standard error shows it clipped (limits.md, 2.4),
  and the event stream keeps it whole only under content capture
  (agent.md, section 6; events.md, section 3.10). It never enters a
  prompt.
- **Unauthorized and exhausted** also go up to the domain, which raises the
  notices the host acts on (domain/host.md, section 7).
- **Deadlines measure progress.** The connection component arms them
  (skein's `llm-connection.md`):
  - connecting and the handshake;
  - the head, until the response's first byte, and idleness, the largest
    gap between events, each declared per provider and model (limits.md);
  - the whole call, bounded by the session's remaining time, given per
    call. There is no fixed ceiling per attempt.

  A timeout carries its phase: `Connect`, `Handshake`, `Head`, `Idle` or
  `Whole`. Head and idle timeouts are transient, and the session retries
  them; the whole bound's expiry is the session's time running out, and
  is not retried (domain/session.md, section 4).
- **A call that cannot be served now waits.** A call that finds the
  pool's connections or its memory at capacity waits for room, in order
  of arrival, within its deadline, rather than failing. The pool is
  bounded by the declared conversations and the LLM memory pool, both
  checked against the agent's concurrency at startup (limits.md).
- **Closing.** When its owner closes it, at the run's answer, the
  component refuses new calls; idle connections close at once and busy
  ones when their call ends, and it reports closed once everything below
  it has settled (agent.md, section 6).

## 7. Credentials

- **A call names its grant,** an account and a generation. When the call
  starts, the component reads the grant's value from the table.
- **A grant missing or lapsed** fails the call as unauthorized and unsent.
  The session retries it once a refreshed grant has come.
- **In one process,** the host's protocol layer fills the table directly
  (README.md, section 4).

## 8. Limits

- **Derived from declared quantities,** never set by hand (limits.md).
  skein's `Limits::derive` gives the LLM path's byte limits with checked
  arithmetic: the request, the answer, one call's input, strings,
  reasoning, retained tokens, skip caps, the tools, history items and
  output items (skein's `llm.md`). smith derives the rest of this
  component's the same way:
  - the tools' schemas' bytes;
  - one rendered result's;
  - a failure's detail.
- **Checked at startup, by name.** A relationship that does not hold
  refuses the configuration and names it: among them, one call's input
  covers the tool payload, and one rendered result covers each tool's
  outcome at its caps, a host's reply, a message and a sub-agent's
  result (limits.md, section 4). The system text is the run's, bounded
  by a window's room (limits.md, 3.9).
- **Stated to the LLM.** The limits a call can meet are written into the
  tools' descriptions from these effective values (section 3).
- **The worst case:**
  - the LLM memory pool, to which calls are admitted (section 6), in
    place of each call's maximum times the number of calls;
  - the decoding stack;
  - one rendered result at a time.

## 9. What the domain is owed, and what skein owes

- **The domain is owed:**
  - typed completions and failures;
  - decoded calls with typed problems;
  - rendered outcomes;
  - no retries and no timers of its own.
- **skein owes:**
  - its LLM client and dialects (skein's `llm.md`): the affinity and its
    per-call headers, `ToolChoice`, an oversized call as a block of its
    own, typed limits and HTTP statuses, Anthropic's cache markers, and
    `Limits::derive`;
  - its JSON tokenizer and writer (skein's `json.md`);
  - the LLM connection component (skein's `llm-connection.md`): deadlines
    by phase, waiting at a full pool, and closing for its owner;
  - the fake LLM peer (skein's `fake-llm.md`).

## 10. The world

The protocol world `tests/protocol-llm` runs the root domain, this
component and skein's client against scripted byte peers, in each dialect.

- **Its stories:**
  - each tool's call decoded from what an LLM writes, malformed, then
    corrected;
  - each dialect's edit format decoded to the same call;
  - an oversized call beside an ordinary one: the completion completes,
    and the LLM is told how to make the call fit;
  - a cut call and an oversized one sent back in the next request as
    calls with an empty input and their errors, which the scripted peer
    accepts as replayable;
  - a compaction whose owed results pass its room, cut with their
    markers, its history unchanged before them;
  - the request's bytes, the first byte and the largest gap measured
    for the event stream, and deltas given to it only when asked;
  - a call cut off, and nesting asked for, each answered as its problem;
  - `finish`'s schema for each form of contract, and a result decoded from
    it;
  - the descriptions, for one directory and for several, stating each
    effective limit;
  - a host tool's input passed through;
  - each failure class, with each kind of evidence, a limit naming
    itself and an HTTP failure its status;
  - replay going back after a resume;
  - a grant refreshed between two calls;
  - each deadline passing, a steady stream outliving any fixed time, and
    the session's time running out once, without a retry;
  - a call waiting at a full pool, then served;
  - the affinity on the wire in each dialect, and the same affinity across
    a resume;
  - `Only` rendered where the dialect has it, and `Auto` with a call
    outside the set answered as not run where it does not;
  - multi-byte, invalid and control bytes rendered, and an edit copied
    from a rendered read matching its file;
  - an oversized result cut with its marker;
  - a `wait` answered with its message in one request.
- **Its referee:**
  - one terminal per call;
  - every schema it writes accepted by the scripted peer as JSON Schema;
  - every rendering within its bound;
  - one affinity per conversation, distinct threads for distinct
    conversations;
  - within a window, each request's system text and tools byte-identical
    to the one before, and its history extending the one before.

All run in virtual time, in milliseconds each, within the focused suite.

## 11. From temper

- **Kept, from temper's `llm.md`:**
  - fixed schemas for the workspace's tools;
  - total decoding with typed problems;
  - failures classified and never retried below the domain;
  - opaque blocks kept verbatim;
  - cache markers placed by the dialect, now skein's.
- **Moved to skein:** the providers' codecs, the client, identity profiles,
  and captured traffic for tests.
- **Gone:** the engine's tools' schemas, now temper's host tools.

## 12. Open questions

- **More providers:** API keys, other dialects, local models. They are
  skein's client's to add, and configuration's to name.
- **Live text for a host across a channel,** such as a chat showing a
  spawned agent's words as they come: facts carry no content, and the
  event stream's `text.delta` stays in the agent's process.
- **MCP tools:** schemas passed through like host tools', once MCP is a
  tool source.
- **Edit formats:** which format each dialect is offered, once measured;
  a patch as a free-form tool needs skein's Codex dialect to offer one.
- **Routing extras:** Codex's turn-state token and `reasoning.context`,
  adopted only if a probe shows a material gain; an incremental transport
  over WebSocket is on skein's roadmap.
- **Guidance as system messages:** where a dialect takes system messages
  mid-conversation, whether appended guidance goes there.
- **Anthropic's cache lifetime:** a parent idle while its child runs past
  the lifetime loses its tail's entry; the lifetime is measured again
  with the `cache-reuse/idle-gap` variant (benchmarks.md; skein's `llm.md`).
- **A replaced call and preserved thinking:** Anthropic's current models
  check that thinking sent back belongs to an unedited history. A cut or
  oversized call sent back with an empty input changes its turn as the
  model wrote it. Whether the provider then drops that turn's thinking or
  refuses the request is to be captured; a refusal would need the turn
  sent without its thinking.
