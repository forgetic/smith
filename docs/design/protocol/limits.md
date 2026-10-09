# Limits

Provisional, 2026-10-09. How an agent's limits are made:

- the quantities a deployment declares;
- how every limit below them is derived, layer by layer;
- the relationships checked at startup, each by name;
- memory, as admission to a shared LLM pool against a real target;
- what happens to one call at a limit, and the deadlines that measure
  progress;
- how a limit that fires is reported.

The derivation's skein half is skein's `llm.md`. The values a product
ships are its presets, in `docs/design/shell.md`. This document holds
the rules and the arithmetic's shape, not the values.

## 1. In one page

- **Declared, never set by hand.** A deployment declares a few
  quantities: per model, its window, its output, its largest reasoning
  item and its progress deadlines; per deployment, its concurrency, its
  tool payload, its calls per response, what a tool's output may hold,
  its LLM pool and its memory (section 2). No byte limit is
  configuration. A test may override a derived limit; nothing else may.
- **A profile is a named bundle** of declared values and policy values.
  It is step code in the service crates (README.md, section 5), so a
  profile is tested where it is made, and its values are the product's
  presets.
- **Every limit is derived, layer by layer,** with checked arithmetic:
  skein-llm's `Limits::derive` gives the LLM path's, and smith derives
  its session's, transcript's, rendering's, machine's and channel's from
  the same declarations and from the layers below (section 3). An
  overflow refuses the configuration.
- **The window bounds the material, not the reverse.** A byte counts as
  at most one token. What a window holds at its opening, and what a
  compaction carries, is bounded from the model's window, so every
  window can open and close within it (3.9).
- **Every relationship is checked at startup, by name** (section 4). A
  violation refuses the configuration and names the relationship and the
  values that broke it. What startup can find is never found in the
  middle of a run.
- **Memory is a real target** (section 5). LLM calls are admitted to a
  shared pool of declared size and wait while it is full, so the worst
  case is the pool's size, not a product of independent maxima. The sum
  of every worst case must fit the declared memory, which is real
  memory.
- **A limit meets one call, not the conversation** (section 6). An
  oversized call is that call's problem, told to the model as one it can
  fix. Oversized rendered output is cut with a marker. A call that finds
  the pool full waits.
- **Deadlines measure progress** (section 7): connecting, the handshake,
  the response's head and the gaps between its events. The whole call is
  bounded only by the session's remaining time.
- **A limit that fires says which** (section 8): `Limit { which, bound }`
  in the failure, in the answer, in the event stream and in the
  operator's words.

## 2. Declared quantities

### 2.1 Per model

Declared for each model an endpoint serves, in the agent's configuration
(agent.md, section 4). A provider preset supplies known models' values;
settings override them (`docs/design/shell.md`).

| Quantity | Unit | What it is |
|---|---|---|
| `window` | tokens | the usable input window: the most one prompt may hold |
| `output` | tokens | the most one completion may produce, thinking included: sent where the dialect takes it, and what the session reserves |
| `reasoning_item` | bytes | the largest opaque reasoning item the model returns for replay |
| `head` | duration | from the request sent to the response's first byte |
| `idle` | duration | the largest gap between two events of a streaming response |

- **The charter carries the effective values.** A host puts a model's
  `window` and `output` in each charter's model entry, at most what the
  configuration declares for it (charter.md, section 2). The agent
  derives its limits from the configuration's, the largest any charter
  may name; the session enforces the charter's.
- **An oversized reasoning item fails by default.** A model's
  declaration may opt into dropping it instead: the completion is kept,
  the item is left out of later requests, and the event stream says so
  (events.md, section 3.9).

### 2.2 Per deployment

| Quantity | Unit | What it is |
|---|---|---|
| `conversations` | count | the most conversations open at once across a run's tree: main, its sub-agents and compactions; so the most LLM calls in flight |
| `tool_payload` | bytes | the most a tool exchanges in one piece: a `read`'s largest window, a `write`'s content, the arguments of one call |
| `calls_per_response` | count | the most tool calls one completion may make, and so the most result blocks the next message holds |
| render caps | bytes, count | what a tool's output may hold (below) |
| `llm_pool` | bytes | the memory every LLM call in flight shares, sending and receiving (section 5) |
| `memory` | bytes | the process's real memory: every worst case must fit within it (section 5) |

The render caps, each the default or the ceiling a tool states to the
model (domain/tools.md, section 4):

| Cap | What it bounds |
|---|---|
| `read_window` | a `read`'s window when the call asks for none; the ceiling is `tool_payload` |
| `shell_output` | a command's kept output when the call asks for none |
| `shell_head`, `shell_tail` | the ceiling on a command's kept output, and how it splits between head and tail; a check's output keeps the same tail |
| `search_hits`, `search_bytes` | a search's matches and their bytes |
| `list_entries` | a listing's entries |
| `guide` | a guide kept whole in the system text before it is cut (domain/run.md, 3.3) |

### 2.3 Policy values

Values of a profile that bound no bytes, checked like any. Each has the
name `smith check` and the event stream's header print it under
(section 9); the product's values are `docs/design/shell.md`'s, section
6.5.

| Name | Unit | What it is |
|---|---|---|
| `max_turns`, `max_spend`, `max_time`, `max_waiting` | count; the host's unit; duration; duration | the most a charter's budget and waiting time may ask for (domain/run.md, section 9) |
| `sections`, `host_tools`, `verdicts`, `items`, `fields` | count | the most of each a charter may hold, at most the charter schema's (charter.md, section 6) |
| `inbox` | count | the messages a run holds unread (domain/run.md, section 6) |
| `unacknowledged` | count | the most turns a start's window may ask to hold unacknowledged (channel.md, section 7) |
| `compaction_threshold` | fraction | the share of a window at which a session compacts (domain/session.md, section 8) |
| `connect`, `handshake` | duration, per endpoint | connecting, and TLS's handshake (section 7) |
| `connection_keep` | duration, per endpoint | how long an idle connection stays open in a live service; never how the process ends, since the pool closes at the answer (skein's `llm-connection.md`, section 7) |
| `tool_deadline` | duration | a tool call's longest deadline: the most a command's call may ask for, and the most any tool call's deadline may be, a host tool's included (domain/tools.md, section 5) |
| `shell_timeout` | duration | a command's deadline when its call asks for none |
| `group_stop` | duration | each of the three steps that stop a command's group (agent.md, section 2) |
| `close` | duration | io's close deadline (section 7) |
| `write_deadline` | duration | the most one write of the event stream may stay in flight before its destination is given up (events.md, section 5.2) |
| `cancel_grace` | duration | the inline agent's grace for a cancel, before it drops a run (domain/host.md, 9.2) |
| `exit_grace` | duration | a host's grace for a spawned agent to exit after its answer, above the agent's teardown bound (section 4) |

### 2.4 Fixed by the vocabulary

A few bounds are the vocabulary's own, sealed by its schemas and the same
in every deployment:

- **a delivery's diagnostic:** the last 512 bytes of the host's
  diagnostic output, with the count of bytes dropped before them
  (domain/run.md, 8.2);
- **a run's identity:** 16 opaque bytes (domain/run.md, 3.2;
  domain/host.md, section 2);
- **a credential:** a grant's value and what travels beside it, such as
  an account's id, within the bound the channel's schema seals for a
  grant (channel.md, section 6). Every source the local host lends from
  is read within it, and the request head's limit counts it at that
  bound (3.1);
- **an operator's line:** one line on standard error, a failure's
  detail clipped to its first 512 bytes at a character's boundary
  (agent.md, section 6).

## 3. The derivation

Each layer derives its own `Limits` in its own crate, from the
declarations and from the limits of the layers it depends on, with
checked arithmetic: `None` on overflow, which refuses the configuration
naming the limit it was deriving. A derivation documents its factors
(a byte per token, an escape's expansion, a frame's overhead) where it
uses them. The service composes the chain once at startup and checks the
relationships between layers (section 4). Domains derive from what they
are given and never from a lower layer's types: the service passes each
the numbers it needs.

### 3.1 The LLM path: skein

`skein_llm::Limits::derive(&Declared)` (skein's `llm.md`, sections 4.1
and 4.2) runs once per endpoint. Its `Declared` takes the endpoint's
maxima, the largest `window`, `output` and `reasoning_item` of the models
the configuration declares for it, and the deployment's `tool_payload`,
`calls_per_response` and `conversations`. Its two factors, an escape's
expansion and the bytes a token may stand for, are skein's.

It gives every limit of the LLM path, per dialect: the request, the
answer, one call's input, a reasoning item, replay's metadata, a
completion's output items, the strings it keeps, the tokens it retains,
a call's receiving reservation, the bytes it skips, the tools and the
history items. Events are decoded as they stream and selectively
(skein's `json.md`), so what a provider echoes that smith never reads
costs a scan, not memory, and no limit is sized by it. Text smith sends
has no cap of skein's: smith bounds what it renders (3.4), and the
request bounds the whole.

skein's LLM connection component is given, from the same declarations
(skein's `llm-connection.md`, sections 3 and 7):

- **`calls`:** `conversations`, the calls it holds at once, running or
  waiting;
- **`connections`:** `conversations` across the pool, and as many per
  endpoint. skein checks connections ≥ calls when the component is made
  (skein's `llm.md`, section 4.3), so the service refuses to start
  otherwise;
- **`memory`:** `llm_pool`, the bytes calls may reserve at once;
- **`idle keep`:** each endpoint's `connection_keep`;
- **the request head:** measured from the endpoint's headers, the
  affinity's fixed rendering and a credential at its bound (2.4).

### 3.2 The session

`smith-domain-session` derives, from skein's limits and the
declarations:

- **a completion's blocks:** its calls, at most `calls_per_response`,
  and the few text, refusal and reasoning blocks a dialect adds;
- **one completion's output:** the charter's `output`, at most the
  model's;
- **a window's history:** the messages, blocks and bytes one window may
  hold, set so that the window's check is reached before any of them
  (domain/session.md, 8.1). Its bytes are skein's request bound for the
  endpoint's dialect, less the system text and the tools' schemas at
  their largest and the prompt's allowance, each escaped at its largest;
  its messages and blocks are skein's history items, less the system
  text's and the tools'. So a prompt the session admits is within
  skein's request, and a request-size `Limit` cannot follow a passed
  admission: should one fire, a fault in this derivation, the session
  takes it as *context too long* (domain/session.md, section 4). With
  compaction, a session holds one window, so this is what a session
  holds, not what a run accumulates;
- **what a window holds at its opening, and what a compaction carries:**
  section 3.9;
- **the prompt's allowance:** what the protocol layer and the provider add
  to a prompt beyond the bytes the session counts, from the LLM
  component's framing, for the window's check and the budget's
  reservation (domain/run.md, section 9);
- **a decoded call's bytes:** `calls_per_response` decoded calls, each
  within one call's input;
- **a failure's detail,** the last failure's, kept for the operator: the
  provider's text within the largest failure detail skein's dialects
  keep (skein's `llm.md`, 4.2), or a limit's name and bound. The session
  keeps one, each later failure replacing it (domain/session.md, section
  4); the event stream carries it whole under content capture, and the
  operator's line clips it (2.4).

### 3.3 The transcript

`smith-transcript`'s schema seals ceilings; the agent's limits sit within
them (transcript.md, section 7):

- **a message's blocks:** from `calls_per_response`, the largest of:
  - a completion's blocks, skein's output items: twice
    `calls_per_response`, and two;
  - a message of results: `calls_per_response` results, then what is
    appended after them, one offer of messages (3.9) and the run's
    notes;
  - a window's opening: the task's blocks, its instruction and one
    offer, then the summary and one offer of held-back messages.

  So one declared number bounds both the completion and the message of
  its results, in the transcript's third version;
- **a turn's bytes:** from a completion's answer, its calls' inputs, its
  results at their render bounds and its reasoning items;
- **a transcript's,** the resume limit: at least one window's turns, as
  a session holds them, since a transcript carries only the current
  window (transcript.md, section 7).

### 3.4 Rendering

`smith-protocol-llm` derives one rendered result's bound (llm.md,
section 8):

- **each workspace tool's outcome at its caps,** rendered: its kept bytes,
  with every escape at its largest expansion, and what the rendering
  adds (line numbers, paths, the head and tail's separator, a clamp's
  note);
- **a host tool's answer,** with the prefix the rendering adds; its text
  is at most `tool_payload`, like any piece the model reads at once;
- **a message,** with its label's prefix, at its bound (3.9);
- **a sub-agent's result:** its answer cap, the marker that cuts it, and
  the usage line;
- **the cut's marker,** which section 6's truncation adds.

The system text is not a result: the run writes it, and a window's room
bounds it (3.9).

### 3.5 The machine

`smith-protocol-machine` derives from the tools' limits, never beside
them (agent.md, section 2):

- **a command's captured output:** at least `shell_head` and
  `shell_tail`;
- **a file's load and store:** at least the tools domain's file bound,
  and a store at least `tool_payload`;
- **a scan and a search:** at least `list_entries`, `search_hits` and
  `search_bytes`;
- **paths:** at least the tools domain's path bound;
- **the environment:** the configured environment's encoding, exactly
  (agent.md, section 3).

### 3.6 The channel

`smith-protocol-channel` derives its half's limits, and with them its
terms (channel.md, sections 2 and 7):

- **a turn's frame:** at least the largest turn (3.3);
- **a start's frame:** the charter at its run bound, one window's
  transcript, the answers it lacks and the start's messages;
- **a message's frame:** a message at its bound (3.9);
- **its queue:** the window's turns, the calls in flight (at most
  `conversations`), the notices, the long operations, the answer and a
  reserve for facts.

The host's terms are checked against these at the opening, not at
startup: a host that takes less finds out before a run starts.

### 3.7 The event sink

The largest record follows from the capture policy (events.md, section
5): under `everything`, a response's prompt delta is at most one turn's
growth, its results at their render bounds. The sink's output cap holds
that record and the room it reserves for authoritative records.

### 3.8 The service

`smith-agent-service`, and `smith-local-service` for a run in one
process, compose the chain, run the startup checks, and sum the worst
cases (section 5). Each holds its profiles as step code (README.md,
section 5).

### 3.9 A window's room

A window is counted in tokens and most of what fills it in bytes. A
token is at least one byte, so a byte count bounds a token count: the
session's check and these bounds count each byte as one token
(domain/session.md, 8.1). Where that would not fit, the derivation bounds
what a window holds, never the window, which is the model's.

For each model the configuration declares:

- **The room** is its `window` at `compaction_threshold`, less two
  `output`s: one for the summary that opens a new window, one reserved
  for the completion that follows it. The summary counts as the tokens
  its compaction reported, or `output` when none was, never as its bytes.
- **smith's own part** is taken first: the run's sections at their
  largest, smith's tools' schemas, the opening's and the summary's
  instructions, and the prompt's allowance (3.2). smith's code fixes
  them, and the service measures them at startup.
- **The rest is split in halves,** a factor of the derivation's own:
  - **the host's part:** the charter's instructions, its brief and its
    host tools' descriptions and schemas, beside a guide at `guide` for
    each of the start's directories. A start whose charter, with its
    directories' guides at their bound, passes it is refused at the
    entrance. A sub-agent's brief takes the charter's place, and a
    `sub_agent` call whose brief passes it is answered as too large
    (llm.md, section 4);
  - **the window's messages:** two offers (below). The task is the
    opening's instruction and one offer, and a new window carries the
    task, the summary and at most one offer of held-back messages.
- **One offer** is the most the run gives the model at once (domain/run.md,
  sections 3.4 and 6): at most `inbox` messages, and at most half the
  window's messages in rendered bytes. What an offer leaves stays
  queued, in order, for the next one.
- **A message's bound** is at most one offer's bytes, and at most
  `tool_payload`. So every message a run admits fits an offer and can be
  read, and a new window always has room for one.

The derivation takes the smallest room among the declared models, so one
set of bounds holds whichever model a charter names. A charter may lower
a model's `window` or `output`, but not so far that its room falls below
the one the limits were derived for (charter.md, section 6).

A window closes by compaction: its history, the results owed to it, and
the summary instruction, then a summary of up to `output`
(domain/session.md, 8.2). The check before each completion keeps the
prompt and its output within the threshold, so what the compaction adds
must fit above it. **The results owed to a compaction** are rendered
within what the window leaves after the history before them, the
summary instruction and one `output`, never less than the window above
its threshold less those two; each is cut with its marker where they
would pass it (llm.md, section 5). History before them is unchanged, so
the compaction still reads the cached prefix.

## 4. Checked at startup

Each relationship has a name. A configuration that breaks one is
refused before the loop starts, and the refusal names the relationship,
the limits on each side and their values (agent.md, section 6;
`smith check` in `docs/design/shell.md`).

skein checks its own as it derives and composes, each refused by its
name (skein's `llm.md`, section 4.3): one call's input within the
answer; the output items and strings its rules give; one largest call's
request and receiving reservation within `llm_pool`; at least as many
connections as `conversations`; each demand within its cap; and
Anthropic's output within what its route accepts. smith checks the rest:

| Name | Relationship |
|---|---|
| `derived` | every derived limit fits its type; the first that overflows is named |
| `fresh-window` | for each declared model, a new window at its largest (smith's own part, the host's part, the task as its instruction and one offer, a summary of `output` tokens, and one offer of held-back messages) and one `output` fit within the window at the compaction threshold, counting a byte as a token (3.9). The parts are derived so that this holds whenever smith's own part fits the room, for every shipped preset among them (`docs/design/shell.md`, 6.1); the check refuses a model whose room smith's own part fills |
| `compaction-headroom` | for each declared model, the window above its threshold holds one `output`, the summary instruction and a cut result's marker for each of `calls_per_response` results, so the compaction the check calls for always fits, its results cut to what is left (3.9; domain/session.md, 8.1) |
| `output-in-window` | each model's `output` is less than its `window` |
| `window-first` | a window's history reaches the compaction threshold before its message, block or byte bound |
| `calls-in-transcript` | a message's blocks, derived from `calls_per_response`, are at least a completion's blocks and within the transcript schema's ceiling |
| `resume-holds-window` | the resume limit holds the most a session holds in one window |
| `turn-in-transcript` | the largest turn fits the transcript schema's ceilings |
| `turn-in-channel` | the largest turn, start and message fit the channel's frame ceilings |
| `render-in-result` | every tool's outcome at its caps, a host's answer, a message and a sub-agent's result render within one rendered result's bound |
| `read-in-payload` | `read_window` is at most `tool_payload` |
| `machine-holds-tools` | the machine's output, file, scan, search and path limits hold the tools' (3.5) |
| `environment` | the configured environment's names are valid and distinct |
| `request-head` | a request's head holds a credential at its bound (2.4), the affinity's headers and the endpoint's configured headers, none of which is a reserved name |
| `ceilings-in-vocabulary` | the policy ceilings fit the charter's and the channel's schemas |
| `deadlines` | every deadline is positive, and `shell_timeout` ≤ `tool_deadline`, so no command's deadline, asked for or not, passes what the session allows one tool call |
| `memory` | the sum of every worst case, with the LLM pool at its size, fits `memory` |

Two more hold across processes, and the side that knows both checks
them:

| Name | Who checks | Relationship |
|---|---|---|
| `grace-over-teardown` | a host that writes its agent's configuration | `exit_grace` exceeds the agent's teardown bound (section 7), with a margin, as skein's `shell.md`, section 13 has a supervisor derive its grace (agent.md, section 6; domain/host.md, section 4) |
| `watchdog-over-silence` | the host kit | its no-progress deadline exceeds the longest deadline the run may be silent for between reports: io's short operations and its own steps (domain/host.md, section 4) |

What a run asks for is checked at its entrance, not at startup
(domain/run.md, section 4): a model the configuration does not declare;
each model entry's `window` and `output` against the model's
declaration, and its room against the one the limits were derived for
(3.9); the charter's text, with its directories' guides at their bound,
against the host's part; the budget against the ceilings; and the
reserve against the budget.

## 5. Memory

- **A real target.** `memory` is what the process may use. The product's
  presets keep it within the real memory a person's machine can spare
  (`docs/design/shell.md`). The shell refuses to start when the worst
  case exceeds it (programming-model.md, section 6.3).
- **Admission to the LLM pool.** Every LLM call in flight draws on
  `llm_pool`, which skein's connection component keeps (skein's
  `llm-connection.md`, section 7). At admission a call reserves its
  request, measured exactly before it is encoded, and its receiving
  reservation, which skein derives (skein's `llm.md`, section 4.2); it
  releases both at its terminal. A call that does not fit what is left
  waits, in order of arrival, within its deadline (section 6).
- **So the LLM path's worst case** is the pool's size, plus each
  connection's own state (its socket's buffers, TLS) and the component's
  tables. It no longer grows with the request's bound times the number of
  connections.
- **The rest is counted at its derived limits:** each conversation's
  window of history; the channel's queue and frames; the machine's
  captures; io's buffers; the event sink's output cap. Each layer's
  `worst_case` adds what its containers report (programming-model.md,
  section 6.3).
- **Waiting is the backpressure.** A full pool slows the run's calls; it
  never refuses one that fits the pool alone (programming-model.md,
  section 7).

## 6. One call at a limit

| What | Outcome |
|---|---|
| a tool call whose arguments pass one call's input | the call arrives as an oversize block with its byte count; the completion completes; the model is told the size, the bound, and how to make it fit (llm.md, section 4) |
| a rendered result past its bound | cut at a character's boundary with an explicit marker saying how many bytes were left out; the request goes (llm.md, section 5) |
| a call that finds the pool's connections or memory at capacity | it waits, in order of arrival, bounded by `conversations`, within its deadline (llm.md, section 6) |
| a reasoning item past `reasoning_item` | a typed limit failure, unless the model's declaration drops it (2.1) |
| a tool call cut off by the output limit | marked by skein; the session keeps a note and an error result in its place (llm.md, section 4) |
| history that reaches the window's threshold | a compaction, not a failure (domain/session.md, section 8) |
| results owed to a compaction past what its headroom leaves | cut with their markers in the compaction's request only (3.9) |
| messages queued past one offer | they wait, in order, for the next offer (3.9) |
| a provider's "context too long" | one compaction; a second in the same window fails the session |
| a completion past `calls_per_response` | a typed limit failure of the completion; the tools' descriptions state the bound |
| a message, a host's answer, a charter or a start past its bound | refused at its entrance, naming the bound (domain/run.md, sections 4 and 6) |

## 7. Deadlines

| Deadline | Runs | Its value | On expiry |
|---|---|---|---|
| connect | from the connection's start to its socket connected | the endpoint's | transient |
| handshake | from connected to TLS ready | the endpoint's | transient |
| head | from the request sent to the response's first byte | the model's `head` | transient |
| idle | between two events of the response | the model's `idle` | transient |
| whole | the call | the session's remaining time | not retried: the session's time is up |

- **Progress, not duration.** A response that keeps streaming is never
  cut by a fixed bound. There is no ceiling per attempt.
- **`TimedOut` carries its phase:** `Connect`, `Handshake`, `Head`,
  `Idle` or `Whole` (skein's `llm-connection.md`). The session retries by
  phase (domain/session.md, section 4).
- **A call waiting for the pool** is within its whole bound; past it, it
  fails unsent.
- **Tools and checks** have the domain's deadlines: a host tool's from
  the charter, at most `tool_deadline`; a command's from its call, or
  `shell_timeout`, clamped to `tool_deadline`; the checks' from the run
  (domain/tools.md, section 5).
- **The end of the process** is bounded only by io's close deadlines
  (skein's `shell.md`, section 13). After its answer the agent closes its
  components together, its LLM connections, its machine's roots and its
  channel, and then its event stream, after `session.ended` (agent.md,
  section 6): two `close` deadlines in turn, and the moment its cancels
  take to settle, which io's close deadlines already bound and which adds
  nothing to them. That is its **teardown bound**, from which a host
  derives `exit_grace` (section 4).

## 8. A limit that fires

- **Typed.** A limit that fires is `Limit { which, bound }`: which limit,
  and its value. The LLM path's kinds are skein's (skein's `llm.md`);
  smith's are its own layers' (a session's history, a decoded call, a
  message). An HTTP failure carries its status.
- **Carried whole.** The session keeps it in its failure, the run in its
  answer (domain/run.md, section 10), the event stream in
  `response.completed` and `run.completed` (events.md, section 3), and
  the operator sees it in words: on standard error (agent.md, section
  6), and in the local host's account of the failure (hosts.md, section
  5.1).
- **Refusals name what they compared.** A startup refusal names the
  relationship (section 4). An entrance refusal names the bound and the
  value asked.

## 9. Effective limits

The declared values, the policy values and every derived limit, each
under its name in its unit, are the **effective limits**. The declared
and policy values are named as section 2 names them; a derived limit is
named by its layer and its field in that layer's `Limits`, such as
`llm.request` or `session.history_bytes`, per endpoint or model where it
is one. `smith check` prints them (`docs/design/shell.md`, 4.4), and the
event stream's header carries them (events.md, section 3.2). Their names
and units are part of the event vocabulary, so a change to them is a new
version of it (events.md, section 6).

## 10. What the domain is owed, and what skein owes

- **The domains are owed** their limits as numbers, derived before the
  loop, with every relationship between layers already checked, so no
  domain meets another layer's limit in the middle of a run.
- **skein owes:**
  - `Limits::derive` for the LLM path, with its documented factors and
    its own relationships checked (skein's `llm.md`, section 4);
  - selective, streamed decoding of provider events (skein's `json.md`
    and `http.md`);
  - an oversized call as a block of its own, typed `Limit` kinds and
    HTTP statuses, and a reasoning bound apart from replay's metadata
    (skein's `llm.md`);
  - waiting for a connection or for room in the memory pool, the expired
    phase in `TimedOut`, and a pool sized from the owner's concurrency
    (skein's `llm-connection.md`, sections 4.1, 6 and 7);
  - each container's `worst_case` (programming-model.md, section 6.3).

## 11. The world

- **Derivation properties,** in each deriving crate's step tests: for
  declared values drawn at random within their types, derivation either
  refuses, naming one relationship, or gives limits for which every
  relationship of section 4 holds. Nothing panics or wraps.
- **Each relationship's refusal:** one configuration per row of section
  4 that breaks it, refused with its name.
- **Every profile:** each shipped profile, with every preset model
  declared, derives, passes every check, `fresh-window` and
  `compaction-headroom` included, and its worst case fits its `memory`.
- **A window's room,** in the session's world with small windows: a
  window just opened at its largest, its first completion made; more
  messages queued than one offer holds, the rest offered at the next
  yield; a compaction whose owed results pass its headroom, cut and
  fitting; a charter too large for the host's part, refused.
- **No pinned values.** Tests assert properties of the derivation, never
  that a limit equals a number. Worlds declare tiny values
  (testing-strategy.md, section 3), so every limit is reached in a world.
- **The LLM path,** in the protocol world `tests/protocol-llm` (llm.md,
  section 10): calls waiting at a full pool, then served; an oversized
  call beside an ordinary one; a reasoning item at its bound and one past
  it; a response streaming past any fixed bound; a stall timed out
  `Idle` and retried; the session's time ending once, without a retry.
- **Rendering:** a result past its bound cut with its marker; random
  outcomes at random caps rendered within the bound.
- **Memory:** skein's counting allocator against the formula, with the
  LLM pool full and every conversation open, in the agent's simulated
  world (agent.md, section 8).

## 12. From temper

- **Kept:** limits per layer, refusal at the entrance, and the worst case
  checked at startup (programming-model.md, sections 6 and 7).
- **New:** declared quantities and their derivation, relationships
  checked by name, the LLM pool, and limits as per-call outcomes.

## 13. Open questions

- **Calls past `calls_per_response`:** whether skein marks the calls
  beyond the bound as calls of their own, answered as not run, so the
  completion completes, as an oversized call's does.
- **A ceiling per attempt:** whether a model's declaration may add one,
  for a provider that streams without end.
- **A host answer's own bound:** whether host tools' answers get a
  quantity of their own rather than `tool_payload`. Messages now take
  theirs from one offer (3.9).
- **A window's halves:** the room left after smith's own part goes half
  to the host's text and half to messages. Proposed as a first split;
  the history probe measures whether a host's text or a person's
  messages want more of it; what it gives the presets is
  `docs/design/shell.md`'s, 6.1.
- **The rest of the codecs' ceilings:** whether the channel's and the
  transcript's decoding also draws on a shared pool, as LLM calls do, or
  its worst case at its derived limits stays small enough.
