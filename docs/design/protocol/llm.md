# LLM calls

Provisional, 2026-10-07. `smith-protocol-llm` is the agent's component for
LLM calls. It translates `smith-domain`'s LLM vocabulary to skein's LLM
client and back:

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
  - smith's own tools' are fixed here;
  - `finish`'s is generated from the result contract;
  - host tools' pass through as the charter declared them.
- **Decoding is total.** Every call the LLM writes becomes a typed call, or
  an invalid call with typed problems for the domain to answer.
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
- **the tools offered,** with their schemas (section 3);
- **the model and the most output tokens,** from the charter (charter.md);
- **the endpoint's options,** from its configuration: reasoning effort, and
  a cache key where the dialect takes one.

Caching follows from the order. System text and tools come first, so a
conversation's prefix stays the same from one completion to the next, and
the dialect's cache markers go where skein's client puts them.

## 3. Tools' schemas

| Tool | Its schema says |
|---|---|
| `read` | a path; the first line and how many lines, optionally |
| `list` | a directory's path |
| `search` | a pattern; a path and a glob to search within, optionally |
| `write` | a path and the whole content |
| `edit` | a path, the text to replace and its replacement |
| `shell` | a command, and its deadline, optionally, within the charter's |
| `finish` | the result contract's forms (below) |
| `deliver` | the change's required fields |
| `wait` | nothing |
| a sub-agent | its task, the model it runs on, optionally, among the charter's |
| a host tool | what the charter declared, as it came |

- **Descriptions are the protocol layer's words.** They tell the LLM what
  the domain enforces: a file must be read before it is written, paths stay
  within the workspace, a write runs alone.
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
  - input too large.

  The domain answers each as an error the LLM can fix. Unknown fields are
  ignored.
- **A host tool's input** is checked only to be a JSON object within its
  bound, and is relayed as bytes (domain/run.md, 5.2).
- **A result for `finish`** is decoded into the domain's result: its form,
  label, text, fields and items. The domain judges it against the contract.

## 5. Rendering

- **A workspace tool's outcome** becomes the text the LLM reads:
  - lines with their numbers;
  - a directory's entries;
  - search matches;
  - what a write or an edit changed;
  - a command's exit, and the head and tail of its output, saying what was
    dropped between them;
  - a failure with its output.

  The same rendering serves a live call and a resumed transcript
  (transcript.md, section 5).
- **An invalid call** is rendered with its problem, naming the field.
- **Not run and withdrawn** are short fixed texts saying which.
- **Text the domain wrote,** a served tool's or a host tool's, goes as
  written, its error flag mapped to the dialect's.

## 6. Completions and failures

- **Blocks** come up as skein's client completes them: text, refusals,
  calls, replay. The completion follows, with the stop reason (the LLM
  ended, called tools, ran out of tokens, or refused) and the usage.
- **Text in flight** is counted for the run's facts, never carried
  (channel.md, section 8).
- **Failures** keep the class and the evidence skein's client gives them:
  - the classes: unavailable, timed out, overloaded, rate limited or
    exhausted (each with how long to wait), context too long, invalid,
    unauthorized, a local limit, a protocol breach;
  - the evidence: unsent, possibly sent, or a response received.

  The session's classes are these, unchanged (domain/session.md,
  section 4).
- **Unauthorized and exhausted** also go up to the domain, which raises the
  notices the host acts on (domain/host.md, section 7).
- **Deadlines** are the domain's, per call. The connection component arms
  them: connecting, the response's head, idleness between events, and the
  call as a whole.

## 7. Credentials

- **A call names its grant,** an account and a generation. When the call
  starts, the component reads the grant's value from the table.
- **A grant missing or lapsed** fails the call as unauthorized and unsent.
  The session retries it once a refreshed grant has come.
- **In one process,** the host's protocol layer fills the table directly
  (README.md, section 4).

## 8. Limits

- **Set per call by the domain:**
  - the request's bytes;
  - the tools' schemas' bytes;
  - a completion's bytes and blocks;
  - a decoded call's bytes;
  - a failure's detail.
- **The worst case:**
  - skein's client's;
  - the connection component's;
  - the decoding stack;
  - one rendered result at a time.

## 9. What the domain is owed, and what skein owes

- **The domain is owed:**
  - typed completions and failures;
  - decoded calls with typed problems;
  - rendered outcomes;
  - no retries and no timers of its own.
- **skein owes:**
  - its LLM client and dialects (skein's `llm.md`);
  - its JSON tokenizer and writer (skein's `json.md`);
  - the LLM connection component (skein's `llm-connection.md`);
  - the fake LLM peer (skein's `fake-llm.md`).

## 10. The world

The protocol world `tests/protocol-llm` runs the root domain, this
component and skein's client against scripted byte peers, in each dialect.

- **Its stories:**
  - each tool's call decoded from what an LLM writes, malformed, then
    corrected;
  - `finish`'s schema for each form of contract, and a result decoded from
    it;
  - a host tool's input passed through;
  - each failure class, with each kind of evidence;
  - replay going back after a resume;
  - a grant refreshed between two calls;
  - each deadline passing.
- **Its referee:**
  - one terminal per call;
  - every schema it writes accepted by the scripted peer as JSON Schema;
  - every rendering within its bound.

## 11. From temper

- **Kept, from temper's `llm.md`:**
  - fixed schemas for the workspace's tools;
  - total decoding with typed problems;
  - failures classified and never retried below the domain;
  - opaque blocks kept verbatim;
  - cache markers placed by the dialect.
- **Moved to skein:** the providers' codecs, the client, identity profiles,
  and captured traffic for tests.
- **Gone:** the engine's tools' schemas, now temper's host tools.

## 12. Open questions

- **More providers:** API keys, other dialects, local models. They are
  skein's client's to add, and configuration's to name.
- **Live text for a host,** such as a chat showing words as they come:
  facts carry no content, and a trace is not streamed to the host.
- **MCP tools:** schemas passed through like host tools', once MCP is a
  tool source.
