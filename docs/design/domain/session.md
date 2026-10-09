# Sessions

Provisional, 2026-10-05. What a smith session is, as a domain layer: one
conversation with an LLM, opened by a run. It is the child domain
`smith-domain-session`, whose own child is the tools domain (tools.md).
What is still open is listed in section 11.

## 1. In one page

- **A session is a conversation, only.** It knows the provider-neutral
  vocabulary of a conversation and nothing of runs, charters, results or
  hosts.
- **Turn by turn.** It calls the LLM, runs the tools the LLM asks for,
  sends their results back, and repeats until the LLM yields, a limit
  ends it, or its opener closes it.
- **Each turn is told** to its opener as it ends, with every name
  resolved, so that a turn means the same outside the session that made
  it: the main session's turns are its run's transcript.
- **Opened from a transcript,** a session goes on where an earlier one
  stopped, on the same provider and endpoint.
- **Provider-neutral.** Nothing in a session depends on which provider
  answers; each provider's API is skein's shared LLM client's.
- **Bounded:** turns, tokens, spend, time and bytes held, each a budget
  its opener gives it.

## 2. A session

- **Turn by turn.** A session that yields has not ended: its opener
  continues it with a new message, or closes it. Calls in the answer it
  yielded with are answered as not run when it continues.
- **Owned and delegated tools.** A session runs the tools it owns
  through its tools child domain, and delegates the rest to its opener
  (`finish`, `deliver`, `wait`, sub-agents and host tools, run.md,
  section 5) or, later, out of the domain (MCP calls, as opaque
  payloads). To the session they differ only in who answers: a delegated
  call is a ticket and an effect, and its answer is the result's bytes.
- **Everything in flight can be stopped.** Every tool call has a
  deadline, and an opener can abort its session, which cancels what is
  in flight and waits for it to settle (programming-model.md, 5.3).
- **No chains within a step.** An answer that comes back in the step that
  asked for it (a call the tools refuse at their entrance, one the opener
  answers at once) waits on the ready list (programming-model.md,
  section 2), so what one step emits and holds stays bounded.

## 3. Turns and transcripts

- **What a turn is:** one completion, its surrounding user messages, the
  provider's blocks in position, the calls with their provider ids,
  names and input as written, their results as concrete bytes, and what
  the completion used and cost. A withdrawn call has a result of its own
  kind. A historical call holds no ticket that means something only
  inside the session that made it.
- **Told as it ends.** Closing waits for actual terminal answers before
  telling the last turn, including answers that win a cancellation.
- **Versioned.** Turns and transcripts are domain values with a version;
  their bytes are the protocol layer's (`smith-transcript`), so a host
  can keep them opaque and a later agent can resume them or know it
  cannot.
- **Opening from a transcript.** A session may open with earlier turns,
  as they were told, including the provider's opaque blocks, which go
  back to the same provider and endpoint verbatim. History is checked
  before a completion starts: version, endpoint and dialect; contiguous
  turns; message and block bounds; roles and call and result ids; and no
  unresolved ticket. It must fit the message and byte limits with the
  waking prompt and room for a completion. An unanswered call in a
  yielded tail gets the same not-run result as an ordinary continuation;
  answers the history does not hold go to the LLM as text in the waking
  prompt (run.md, section 6).
- **Refused transcripts.** Another version, endpoint or dialect, a
  malformed, unresolved or oversized history are distinct refusals; the
  run treats every one as a transient failure (run.md, section 6).

## 4. Providers and retries

- **Any provider.** A session talks to the endpoint and model it was
  opened with; skein's shared client speaks each provider's API (HTTP,
  server-sent events, JSON) and its errors, and the protocol layer
  translates the session's vocabulary and tool schemas to it.
- **Retries are policy.** A session classifies a failed call
  (overloaded, rate limited, unavailable, timed out, context too long,
  invalid, unauthorised, an account exhausted), retries the transient ones
  after a jittered exponential backoff, and gives up when its retries run
  out. The protocol layer runs the attempt and its connect and idle
  deadlines.
- **The standard completion window** is at most 300 seconds per attempt,
  including active streaming. The protocol's response-head and inactivity
  waits also allow 300 seconds; connecting and TLS handshaking each allow
  10 seconds. A session or enclosing run whose budget expires sooner still
  cancels its outstanding completion. Longer reasoning does not enlarge
  token, byte, retry or outer time budgets.
- **Credentials** are the protocol layer's. A rejected credential or an
  exhausted account is a notice the session raises, which the agent sends
  to its host (host.md, section 7); an unauthorised failure is transient,
  so a refreshed credential may answer the retry.

## 5. Tools in a session

- **Tools run in parallel where that is safe.** Each tool has an effect,
  read or write. Adjacent reads in one turn run together, a write runs
  alone, and results go back in call order. A sub-agent's effect follows
  from its tools; a host tool's is as its charter declares; `finish` and
  `deliver` are writes.
- **The workspace is shared, knowledge is not** (tools.md, section 4):
  what a session's LLM has read, and at which version, is its own state.

## 6. Budgets and prices

- **Budgets:** turns, tokens (input, output, cache reads and writes, as
  the provider counts them), spend in the host's unit, and time, given by
  the opener at open; bytes held, against the agent's limits. Crossing
  a turn, token or byte budget stops the next completion, not the turn
  in flight: the calls of the completion that crossed it still run and
  settle. For spend, the session reserves each completion's maximum first
  (run.md, section 9), so spend never crosses it. Time is the exception:
  when it runs out, the session closes at once.
- **Prices** are integer input, cached and output amounts per positive
  `unit` tokens. New input and cache writes use the input rate; cache
  reads use the cached rate. The combined rational charge of **each
  completion** is rounded upwards once, using checked arithmetic; the
  result and the cumulative spend must fit `u64`. Overflow is a typed
  failure ending the session with a report of it, never a saturated
  charge: the charge that cannot be added is not, and the spend reported
  is what was charged before it.
- **Spend is cumulative** within an activation, sub-agents included. A
  delegated terminal, answered or withdrawn, carries the sub-agent's
  cumulative spend once under its call's identity; a duplicate or stale
  delivery cannot charge it again. Restoring history does not charge old
  activations again. A run counts each child once, through the parent's
  terminal answer, never adding both. Enforcing one aggregate budget
  across concurrently open sessions is the run's (run.md, section 9).

## 7. Sub-agents

A sub-agent is a session its run opens to answer a served call (run.md,
5.3). As a session it is like any other: its own tools, budget and model.
Its turns are not the run's transcript; they are summarised in the call
that asked for it, whose result is the child's final message.

## 8. Context management

Later: when a transcript nears its limit, old tool output is elided
first; a summarising session, requested from the opener like a
sub-agent, comes after. A long-lived chat needs it before its transcript
reaches the host's resume limit.

## 9. Below the domain

- **LLM providers:** skein's shared client (`skein-llm`) for each
  provider's API and its failures; smith's protocol layer for tool
  schemas and decoding; credentials lent by the host (host.md,
  section 7).
- **Turns' encoding** (`smith-transcript`): a turn's bytes, with a
  version, and every provider's opaque blocks kept verbatim with the
  dialect and endpoint they came from.

## 10. The world

skein's fake LLM provider, whose completions a script draws (tool calls,
malformed input, every class of failure, streaming), the tools domain
with the machine's faces, and a scripted opener that continues, closes,
aborts and answers delegated calls. Its stories: a conversation that
yields and is continued; reads in parallel and a write alone; every
failure class retried or given up; a budget crossed mid-turn; a session
opened from a transcript, and each refusal of one; a sub-agent's spend
counted once. Its referee: every call answered once, in call order;
turns told in order, each after its calls settled; budgets exceeded by at
most one completion.

## 11. Open questions

- **Context management** (section 8): eliding, then summarising, and how
  a summary is kept in the transcript.
- **Transcripts across providers:** whether a later version may carry a
  provider-neutral form, so a run can resume on another provider.

## 12. From temper

The session is temper's agent's, as its second version: turns told with
names resolved, transcripts versioned, prices and spend in a unit,
history checked before a completion. temper's first version, which
tickets made meaningful only inside its session, stays with temper's
legacy run until temper's cutover; smith starts at the second.
