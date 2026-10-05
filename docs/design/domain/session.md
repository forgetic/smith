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
  answers. Its protocol adapter uses the shared `skein-llm` client;
  each provider's API belongs to skein.
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
- **Delegated origin.** Every opener-served call carries its concrete accepted
  completion sequence and zero-based assistant block position, in addition to
  its live callback and temporary opener ticket. V2 includes restored transcript
  history in the sequence; V1 counts only the current activation. Provider ids
  may repeat in different turns. The composing root preserves this fixed origin
  as the durable host call name, scoped by the same logical run across restart
  (run.md, section 8.2). Checked sequence overflow refuses the next completion's
  effects before any tool or host submission. Resolving tickets changes neither
  sequence nor position. Root V2 restart and concrete answered-after-transcript
  restoration use that same origin without re-executing the old effect.
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
  waking prompt and room for a completion. Results the opener gives for
  calls made after the last turn are restored before the waking prompt;
  an unanswered call in a yielded tail gets the same not-run result as an
  ordinary continuation.
- **Refused transcripts.** Another version, endpoint or dialect, a
  malformed, unresolved or oversized history are distinct refusals; the
  run treats every one as a transient failure (run.md, section 6).

### 3.1 Receiving room before work

For V2, `completion_bytes` caps the full owning translated completion: Block
cells, all text/id/name/input bytes, replay envelopes and decoded owning calls.
`completion_blocks` independently caps the number of cells. Root and adapter
additionally require a complete `Decoded` classification cell for every possible
call before provider work, including payload-free TooLarge fallbacks. The actual
completion reserves all of its call cells first, then counts dynamic decoded
payloads against the residual allowance. Oversized early input cannot consume
space needed to preserve a later refused classification. The protocol
adapter checks its configured shared-client-to-domain worst case against both
Request::Complete metadata caps before preparing a provider request. A client
answer payload limit alone is not this owning bound. Replay is a complete opaque
shared-client envelope; Text, Refusal and ToolCall keep their optional envelope,
and Opaque keeps the complete reasoning or future-block envelope. Session never
parses a provider tag or chooses a provider from these bytes.

Before every provider effect, including retry and Continue, the session retains
logical credit at least `P = 2*C + N*max(size_of(Block),size_of(Slot))`,
checked for overflow, and the complete bounded failure terminal if larger,
plus two free Message slots. C covers the actual assistant content;
the second C covers copied provider IDs and invalid-result details, and the
independent wrapper term covers every possible result skeleton. Calling and
Closing retain this credit until the actual Completed/Failed/Cancelled
terminal. Cancel emission cannot release it. An in-cap completion converts the
credit into the actual assistant and full result skeleton once. A completion
that wins cancellation records its actual assistant and NotRun for every
unstarted call, usage and full last Turn before Ended. Insufficient room
prevents the request; incompatible restoration is TooLarge before tools or a
provider starts. Completion caps do not silently reduce the history payload
ceiling: receiving room is an explicit pre-work requirement.

Before dispatching any adjacent read batch or exclusive write, a bounded
prescan reserves all maximum result payloads together. Each live call owns its
credit through close until its one actual terminal converts credit into actual
retained bytes. Delegates use `delegated_result_bytes`; owned tools use the
exhaustive call-kind cap from tools Limits. Read, List, Search, Shell and edit
ambiguity all have finite receiving bounds. Scan carries the aggregate Entry
and name byte cap described in domain/tools.md, section 5. Invalid and unstarted
result blocks and their provider IDs were secured before the provider request.
Insufficient batch room starts no underlying effect; earlier actual results
remain and only the unstarted tail becomes NotRun in the actual Turn before
TranscriptFull. Normal and closing receiving paths cannot discard a valid
actual result because later history filled the conversation.

`session_bytes` counts Block wrappers and owning payloads, including secured
result skeletons, separately from Message arrays. The worst case additionally
prices Message storage, result Slot containers including their coexistence with
an assembled result Block array, and restore Turn/Message staging. Root and
caller price their concrete transcript/Turn output envelopes and transit copies
independently. No logical reservation allocates bytes or abandons a terminal.

## 4. Providers and retries

- **Any provider.** A session talks to the endpoint and model it was
  opened with. `skein-llm` speaks the configured provider's API (HTTP,
  server-sent events, JSON) and classifies its errors. Smith translates
  conversation values and application tool schemas at that boundary.
- **Retries are policy.** A session classifies a failed call
  (overloaded, rate limited, unavailable, timed out, context too long,
  invalid, unauthorised, an account exhausted), retries the transient ones
  after a jittered exponential backoff, and gives up when its retries run
  out. The protocol layer runs the attempt and its connect and idle
  deadlines.
- **Credential bytes** are lent by the caller and bound to the configured
  shared client by the protocol layer. Sign-in and refresh are the caller's
  or a shared credential client's; smith owns no OAuth implementation.
  A rejected credential or an exhausted account is a notice the session
  raises, which the agent sends
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
  the opener at open; bytes held, against the agent's limits. Crossing a
  budget stops the next completion, not the turn in flight: the calls of
  the completion that crossed it still run and settle. Time is the
  exception: when it runs out, the session closes at once.
- **Prices** are integer input, cached and output amounts per positive
  `unit` tokens. New input and cache writes use the input rate; cache
  reads use the cached rate. The combined rational charge of **each
  completion** is rounded upwards once, using checked arithmetic; the
  result and the cumulative spend must fit `u64`. Overflow is a typed
  failure ending the session with a report of it, never a saturated
  charge.
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

- **LLM calls:** `smith-protocol-llm` translates to the actual
  `skein-llm::client::Client`. The shared client owns provider codecs,
  HTTP/SSE, wire failures and bounded replay envelopes. Smith owns application
  schemas, typed tool decoding and result text; it never inspects provider
  metadata. Whole tool schemas and raw argument bodies cross this boundary.
- **Replay:** completed text, refusals and tool calls retain optional opaque
  metadata; reasoning retains its complete opaque envelope. Smith preserves
  these bytes and their order through turns, prompt copies and restore.
  Skein checks their format and configured provider compatibility before use.
- **Credentials:** caller-supplied endpoint and bearer/account data, lent
  under the host grant contract (host.md, section 7). Smith neither signs
  in nor refreshes credentials itself.
- **Turns' encoding** (`smith-transcript`): a turn's bytes, with a
  version, and every provider's opaque blocks kept verbatim with the
  dialect and endpoint they came from.

## 10. The world

A shared `skein-fake-llm-domain` whose completions a smith-supplied script
draws (tool calls, malformed input, every class of failure, streaming), the tools domain
with the machine's faces, and a scripted opener that continues, closes,
aborts and answers delegated calls. Its stories: a conversation that
yields and is continued; reads in parallel and a write alone; every
failure class retried or given up; a budget crossed mid-turn; a session
opened from a transcript, and each refusal of one; a sub-agent's spend
counted once. Its referee: every call answered once, in call order;
turns told in order, each after its calls settled; budgets exceeded by at
most one completion.

Protocol worlds join the real shared Client to `skein-fake-llm-protocol`.
Smith owns the application scripts and outside effect expectations. Generic
codec fixtures, wire faults and peer mechanics are tested in skein rather
than copied into smith.

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

Migration 05s2 copied provider and OAuth crates to preserve the source baseline.
05s2a replaces that temporary ownership with the existing shared `skein-llm`
client and shared peers. The copy ledger remains historical evidence; the
replacement audits its codec fixtures and stories before deleting the copies.

Actual shared-client failures additionally distinguish Limit, Protocol and
unsolicited Cancelled, all nonretryable. Failed carries exact transport Evidence
(Unsent/Unknown/Response) and bounded diagnostic bytes, at most failure_bytes.
Request::Complete advertises max_failure_bytes; the adapter checks its configured
receiving diagnostic bound before prepare/effects. Root passes the exact actual
terminal to session. Policy consumes and drops detail without text-driven retry,
saved transcript text or diagnostic facts. Content-free CompletionFailed facts
and End::Failed retain the exact class and evidence. Requested Cancel still owes
one actual Completed, Failed or genuine Cancelled terminal; a failed or completed
operation that wins Cancel is never replaced by an acknowledgement.
