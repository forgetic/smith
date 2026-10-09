# Sessions

Provisional, 2026-10-05, revised 2026-10-09. What a smith session is, as
a domain layer: one conversation with an LLM, opened by a run. It is the
child domain `smith-domain-session`, whose own child is the tools domain
(tools.md). What is still open is listed in section 11.

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
- **One prefix per window.** Within a window the prompt only grows: the
  model, the affinity, the system text and the tools stay as they were,
  and history is appended to, never edited. Near the model's context
  window the session compacts, and a summary opens a new window.
- **Opened from a transcript,** a session goes on where an earlier one
  stopped, from its last window, on the same provider and endpoint and
  with the same affinity.
- **Provider-neutral.** Nothing in a session depends on which provider
  answers; each provider's API is skein's shared LLM client's.
- **Bounded:** turns, tokens, spend, time and bytes held, each a budget
  its opener gives it; and each prompt by its model's window.

## 2. A session

- **Turn by turn.** A session that yields has not ended: its opener
  continues it with a new message, or closes it. Calls in the answer it
  yielded with are answered as not run when it continues.
- **A conversation's affinity.** Its opener gives each session an
  affinity: the run tree's identity as its key, and the conversation's
  thread, its ordinal in the run, main's being zero (run.md, sections 3.2
  and 5.3). The session puts it unchanged into every prompt. skein's
  dialect renders it, or ignores it where the provider has nothing to
  render it as (skein's llm.md). It is fixed for the conversation's life:
  across its windows (section 8), and across activations through its
  transcript, which records it (section 3). A session opened from a
  transcript keeps the affinity the transcript records.
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
  kind, and a call cut off at the output limit is kept as a note of it,
  with its error result (section 4). A historical call holds no ticket
  that means something only inside the session that made it.
- **A turn has a kind:** an ordinary turn, as above, or a window turn,
  the compaction that closed one window and opened the next, holding the
  new window's opening as its messages (section 8.2).
- **Told as it ends.** Closing waits for actual terminal answers before
  telling the last turn, including answers that win a cancellation. The
  one exception is a delegated call whose answer its opener withholds
  and closes the session without giving, as a run parks with its `wait`
  withheld: the last turn is told with that call unanswered, and the
  opener answers it when it opens the conversation again (run.md,
  section 6). A turn's messages are those new since the turn before, so
  each message is told once and an ordinary turn's prompt is its
  transcript up to it. A trace records prompts the same way
  (protocol/events.md).
- **The stable prefix** is a contract, so that a provider can serve each
  prompt's prefix from its cache:
  - Within a window, the model, the endpoint's options, the affinity, the
    system text and the tools offered are fixed, and history is
    append-only: each completion's prompt is the one before it, then that
    completion's blocks, then what was appended after them.
  - The system text and the tools offered are fixed for the
    conversation's life. A session never changes them; a run that resumes
    the conversation composes them again from its charter (run.md,
    section 3.3). A completion's tool choice narrows what the model may
    call (section 5) and changes no definition.
  - Dynamic guidance is appended as messages, never placed in the system
    text or a tool's description: budget and threshold notes, the
    wind-down and final-turn notes, and messages passed on between
    completions (run.md, sections 6 and 9).
  - A window boundary is the only exception (section 8.2). There, history
    is replaced whole; the system text, the tools and the affinity stay.
  - A prompt's encoding is byte-deterministic, so an unchanged prefix is
    unchanged bytes (protocol/llm.md, section 2.2).
- **Calls per response.** A completion's block bound is the declared
  calls per response; skein refuses a completion with more as a typed
  limit (section 4). A turn's messages are bounded by the transcript's
  blocks per message, derived from the same declared number and never
  below it (protocol/limits.md), so every completion the session accepts
  can be told.
- **Versioned.** Turns and transcripts are domain values with a version;
  their bytes are the protocol layer's (`smith-transcript`), so a host
  can keep them opaque and a later agent can resume them or know it
  cannot. A transcript records its conversation's affinity once, in its
  opening turn (protocol/transcript.md, section 3).
- **Opening from a transcript.** A session may open with earlier turns:
  its conversation's current window, from the conversation's first turn
  or from its last window turn, as they were told, including the
  provider's opaque blocks, which go back to the same provider and
  endpoint verbatim. History is checked before a completion starts:
  version, endpoint and dialect; contiguous turns; the affinity in the
  opening turn; message and block bounds; roles and call and result ids;
  and no unresolved ticket. It must fit the session's byte limits with
  the waking prompt and room for a completion. A history that fits them
  but fails the window check (section 8.1) is compacted before its first
  ordinary completion. An unanswered call in the tail gets the same
  not-run result as an ordinary continuation, unless the opener answers
  it as the session opens, as a run answers the `wait` it withheld when
  it parked; answers the history does not hold go to the LLM as text in
  the waking prompt (run.md, section 6).
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
  invalid, unauthorised, an account exhausted, a local limit), retries
  the transient ones after a jittered exponential backoff, and gives up
  when its retries run out. The protocol layer runs the attempt and its
  deadlines. A provider's failure carries its HTTP status where it had
  one.
- **Timeouts carry their phase, and are retried by it:** connecting, the
  handshake, the response's head, idleness between events, or the whole
  call.
  - The head and idle deadlines are the model's, which its opener gives
    with it. The whole call's bound is the session's remaining time.
    There is no per-attempt ceiling: a stream that keeps making progress
    is cut only when the session's time runs out.
  - A timeout in any phase but the whole call is transient, and retried
    like any transient failure. A whole-call expiry is not retried: the
    session's time is up (section 6).
- **A full pool means waiting, not failure.** A call that finds the LLM
  pool at capacity, in connections or in memory, waits below the session
  for room, within its deadline. It is not a failed call and spends none
  of the session's retries.
- **A local limit is typed:** `Limit { which, bound }`, naming the limit
  that fired and its bound. It is not retried, since the same request
  would meet it again.
- **A request too large for skein's request bound** does not follow a
  passed admission. The session's own bounds on a window's messages,
  blocks and bytes are derived within skein's request and history
  bounds, with the system text and the tools at their largest, and are
  checked before each completion with the window (8.1), so a history
  that would pass skein's is compacted first (protocol/limits.md, section
  3.2). Should one fire anyway, a fault in that derivation, it is taken
  as *context too long*: one compaction, and a second in the same window
  fails the session.
- **Context too long** compacts the window once; a second in the same
  window fails the session (section 8.2).
- **A call cut off at the output limit,** which skein marks, never enters
  history as written: its input is incomplete, and a provider would
  refuse it on replay. The session replaces it in history with a note of
  it (its provider id, its tool and how many bytes of input it had) and
  answers it with an error result, the invalid call's problem *cut off*
  (protocol/llm.md, section 4), so the model can make it again with
  less. The session then goes on as after any answered call.
- **An oversized reasoning item** fails the completion with a limit
  naming the reasoning item. Dropping it instead is an explicit opt-in
  of the agent's configuration: the completion is kept without it, so
  the item never enters history, and the drop is reported as a fact.
- **The last failure's detail is the operator's.** The session keeps
  the detail of the last failure it met (the provider's text, clipped,
  or the limit and its bound) within a bound its limits give, each later
  failure replacing it. It goes with the session's end to its opener,
  and on to the agent's log and, as the capture policy allows, its trace
  (protocol/events.md). It never enters the model's history.
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
- **Tool choice.** Each completion is asked with a tool choice: any tool
  offered (`Auto`), none (`None`), or only some (`Only(names)`). The
  tools offered never change (section 3); the choice narrows what may be
  called.
  - The opener gives the choice when it continues the session, as a run
    winding down offers only `finish` and `deliver` (run.md, section 9).
    The session sets `None` on its own final turn (section 7).
  - A call outside the choice, one a dialect that cannot express `Only`
    let through or one the model wrote anyway, is answered as not run.
    The session then goes on by its ordinary rules.
- **A call too large to take** arrives as an oversize block with its
  size. The session answers it as an invalid call the model can fix,
  never as a failed completion (protocol/llm.md, section 4).
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
  when it runs out, the session closes at once. A compaction is a
  completion like any other: it counts as a turn, and is reserved,
  charged and counted against these budgets (section 8.2).
- **Context is a dimension of its own.** The window bounds one prompt
  (section 8.1); the token budgets bound what every completion used,
  summed. Neither stands for the other: a session well within its token
  budgets may have to compact, and one far from its window may cross a
  budget.
- **Usage** is what the provider counted: input, output, cache reads,
  cache writes and reasoning tokens. Each count is optional, and *not
  reported* is distinct from zero. Input, cache reads and cache writes
  are disjoint, as skein reports them: input is what was neither read
  from the cache nor written to it, so the three sum to the prompt's
  size. A token budget counts what was reported. Reasoning tokens are
  counted within output; they are reported for what they are, never
  charged again. A completion that
  reports no input or no output count is charged the reservation made
  for it, so spend is never under-counted (section 11).
- **Prices** are integer input, cached, cache-write and output amounts
  per positive `unit` tokens. New input uses the input rate, cache reads
  the cached rate, and cache writes the cache-write rate, which is the
  input rate when the charter gives none. The combined rational charge
  of **each completion** is rounded upwards once, using checked
  arithmetic; the result and the cumulative spend must fit `u64`.
  Overflow is a typed failure ending the session with a report of it,
  never a saturated charge: the charge that cannot be added is not, and
  the spend reported is what was charged before it.
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

- **The final turn.** A child's opener gives it, at open, a final-turn
  note and a wrap-up allowance of time (run.md, section 5.3). When the
  child's own turns are down to one, or its remaining time to that
  allowance, its next completion is its final turn: the session appends
  the note and asks with `ToolChoice::None`. Calls the model writes
  anyway are answered as not run. The session then yields that answer as
  its last, its budget spent, and the run gives it to the parent with
  `stop: budget` (run.md, section 5.3), so a capped child answers instead
  of ending unanswered.
- **Its answer is bounded,** and its system text states the bound
  (run.md, section 3.3).
- **A main session has no final turn of its own.** Its run's wind-down
  gives it its last completions, with a narrowed tool choice (run.md,
  section 9).

## 8. Context management

A session keeps its conversation within its model's context window by
compaction into a new window. Nothing is edited in place: within a
window, history only grows (section 3).

### 8.1 Admission against the window

- **The window comes with the model.** The opener gives the model's
  usable input tokens and the most output tokens a completion may have,
  as the charter's model entry says (protocol/charter.md).
- **What the session tracks:** the last prompt's size, from its usage,
  the sum of its input, cache reads and cache writes, which do not
  overlap (section 6), each counted as reported; and an upper bound on
  what has been appended since: that completion's output, then the
  results and messages after it, counting a byte as at most one token,
  since a token is at least a byte. With no earlier prompt in the window
  (a conversation's first completion, a window's first, the first after
  a resume from a window turn), or with usage that reports no input
  count, the bound is the whole prompt's bytes plus the allowance the
  session's limits give for what the protocol layer and the provider add
  (run.md, section 9). In a new window's first check, the summary that
  opens it counts as the output tokens its compaction reported, or the
  model's most output when none were reported, never as its bytes.
- **The check,** before each completion: the last prompt, plus the last
  output, plus the appended bound, plus the output reservation, is at
  most the window times its threshold. The output reservation is the
  completion's most output tokens; the threshold is a fraction of the
  window that the session's limits give (protocol/limits.md). The
  arithmetic is checked. A completion that fails the check is not made:
  the session compacts instead (8.2).
- **The headroom holds a compaction.** Two relationships are checked at
  startup (protocol/limits.md):
  - the window above its threshold holds the most one turn can append,
    the summary instruction and a completion's output, so a compaction
    the check triggers always fits the window;
  - a new window's opening at its largest (the system text, the task, a
    summary of a completion's most output, and the messages held back,
    at most one offer of its opener's, which the opener bounds for this
    in messages and bytes, run.md, section 6) passes the check, so a
    window just opened can make its first completion. Where a declared
    window would not hold that material, the derivation lowers the
    material's bounds, never the window (protocol/limits.md).
- **The history's other bounds** (its messages, blocks and bytes) are
  derived so that the window check is reached first (protocol/limits.md).
  A history that would cross one of them anyway is compacted as a full
  window would be. A session never ends because its history grew.

### 8.2 Compaction

- **Its trigger:** a failed window check (8.1), or a provider's *context
  too long* (section 4).
- **The compaction completion** has the same model, affinity, system
  text, tools and tool choice as the completion it stands in for, and
  the window's history with every result owed to it, then one appended
  user message: the summary instruction, which the run gives at open
  (run.md, section 3.3). The results owed to it are rendered within the
  room the window leaves for them, each cut with its marker where they
  would pass it (protocol/limits.md, section 3.9). Nothing before them
  differs from an ordinary prompt, so the request reads the cached
  prefix. Messages the
  opener passed since the last completion are held back for the new
  window. Calls in the compaction's answer are not run.
- **The new window** opens with one user message: the conversation's
  task (its opening message, carried from window to window), the
  summary (the answer's text), then the held-back messages. The
  compaction's own blocks are not kept, and the old window's history is
  released.
- **Told as a window turn,** like any other turn: its usage and spend
  are the compaction's, and its messages are the new window's opening
  (section 3). It records the conversation's affinity, so a transcript
  can start there: transcripts and resumes carry only the current window
  (protocol/transcript.md, section 3).
- **It counts as a turn and as spend.** It is reserved, charged and
  counted against the session's budgets like any completion (section 6).
  A budget that leaves no room for it ends the session for that budget.
- **Failure on repeat.** A window is compacted once. *Context too long*
  on the compaction itself, or a failed check or *context too long* in a
  window that has made no ordinary completion yet, fails the session
  with a limit naming the window. A compaction that fails otherwise
  fails as any completion does, after its retries (section 4).

### 8.3 What else keeps context small

- **Results bounded at their source** stay the first defence: a read's
  window, a command's head and tail, search hits and list entries, each
  within the bounds the tools domain keeps (tools.md, section 4).
- **Children are the main isolation tool.** A child's reading never
  enters its parent; only its answer does (section 7).
- **No elision in place.** Old results are never shortened or removed
  within a window: that would edit history, lose the provider's cache
  from the edit onwards, and break providers' preserved reasoning. Only
  a window boundary drops history.

## 9. Below the domain

- **LLM providers:** skein's shared client (`skein-llm`) for each
  provider's API and its failures, rendering the affinity and the tool
  choice in each dialect; smith's protocol layer for tool schemas and
  decoding; credentials lent by the host (host.md, section 7).
- **Turns' encoding** (`smith-transcript`): a turn's bytes, with a
  version, every provider's opaque blocks kept verbatim with the dialect
  and endpoint they came from, and a transcript's affinity, once.
- **Limits** derived from declared quantities (protocol/limits.md): the
  window's threshold and headroom, the history's bounds, the
  completion's block bound and the failure detail's.

## 10. The world

skein's fake LLM provider, whose completions a script draws (tool calls,
malformed input, calls cut off at the output limit, oversized reasoning
items, usage reported and not, every class of failure with each timeout
phase, streaming), the tools domain with the machine's faces, and a
scripted opener that continues, closes, aborts, narrows the tool choice
and answers delegated calls. Windows are small, so a conversation
compacts within a few turns and long stories stay within the suites'
budgets.

- **Its stories:**
  - a conversation that yields and is continued;
  - reads in parallel and a write alone;
  - every failure class retried or given up; head and idle timeouts
    retried, a whole-call expiry not; a full pool waited for;
  - a call cut off at the output limit, answered and made again; an
    oversized reasoning item failing, and dropped where opted in;
  - a budget crossed mid-turn, and one that leaves no room for a
    compaction;
  - a session opened from a transcript, and each refusal of one; one
    opened from a window turn; one whose tail holds a call its opener
    withheld at closing, answered by the opener as it opens;
  - a conversation compacted at the threshold, and after a context too
    long; context too long on the compaction, failing; calls in a
    compaction's answer not run; a request skein finds too large, taken
    once as context too long;
  - a wind-down offering only some tools, with a call outside them; a
    child's final turn answered with `stop: budget`;
  - a sub-agent's spend counted once; cache writes priced at their own
    rate; usage not reported, told apart from zero; a prompt's size
    taken as the sum of its disjoint input and cache counts.
- **Its referee:**
  - every call answered once, in call order;
  - turns told in order, each after its calls settled;
  - budgets exceeded by at most one completion;
  - every prompt of a conversation carrying its affinity, and distinct
    conversations distinct ones;
  - within a window, each prompt extending the one before; one window
    boundary per compaction;
  - no prompt whose bound exceeds its window;
  - no failure detail in a prompt.
- **The fuzzy suite** draws usage and appended sizes, and checks that no
  request's bound exceeds its window and that no session ends for its
  history's length.

## 11. Open questions

- **Transcripts across providers:** whether a later version may carry a
  provider-neutral form, so a run can resume on another provider.
- **A window's contents:** the task, the summary and the held-back
  messages, as section 8.2 has them; whether the last results should
  also be kept is for the history probe to measure.
- **A compaction cut short or refused:** whether a summary that reached
  the output limit, or a refusal, may open a window. Proposed: a cut
  summary opens it as written; a refusal or an answer with no text fails
  the session.
- **A compaction due on a final turn:** a child with one turn left whose
  window is full cannot both compact and answer. Proposed: the final-turn
  trigger counts a due compaction, so the child compacts with two turns
  left and answers with its last.
- **An unreported input or output count:** section 6 charges the
  reservation in its place. To confirm against each dialect's reporting.
- **Calls past the completion's bound:** failing the completion, as
  section 3 has it, or skein skipping them so that the session answers
  them as not run.
- **Routing state per turn:** if a probe shows that a provider's
  per-turn routing token is worth adopting, the session carries it
  within a turn and never keeps it in a transcript.
- **Provider-native compaction:** a dialect's own compaction items,
  replayed verbatim, as a later optimisation of section 8.2.

## 12. From temper

The session is temper's agent's, as its second version: turns told with
names resolved, transcripts versioned, prices and spend in a unit,
history checked before a completion. temper's first version, which
tickets made meaningful only inside its session, stays with temper's
legacy run until temper's cutover; smith starts at the second. Windows,
the affinity and tool choice are new.
