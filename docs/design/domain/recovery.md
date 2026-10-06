# Durable agent recovery

Provisional, 2026-10-06. The shared recovery contract of the agent root, its
main session and its host. It follows programming-model.md and testing-strategy.md.
This document extends domain/run.md (sections 3, 5, 6 and 8),
domain/session.md (sections 3 and 6), and domain/host.md (sections 2, 4 and 6).
The scope entrance and commitment rules here govern fresh and resumed starts;
the ordinary settled VERSION2 transcript grammar remains unchanged.

The parent keeps durable effect identities and decisions. Smith keeps bounded
conversation context and actual outstanding commitment, lookup and transport
rights. The protocol owns concrete codecs and one canonical recovery renderer;
the domain receives their typed values. Recovery neither repeats an old effect
nor turns an old delivery into a delivery of the current activation.

This is the design to implement. Exact Rust layout, measured queue/heap bounds,
concrete codec discriminants and source/gate evidence are separate implementation
work. A design document does not establish that those checks have passed.

## 1. Required behavior and explicit scope

Both embedded history and separately supplied late recovery records may be
nonempty, equal, complementary, or contiguous extensions. Restore preserves
actual ordered context and every known effect/result witness. It rejects a real
conflict or missing evidence, not the number of sources. A checkpoint-only
initial accepted completion can supply a real initial history with no settled
Turn. Historical assembly never emits Completed, Used, Priced, a runtime Turn,
a new effect or a fictitious read acknowledgement.

Mandatory live checkpointing applies to an actual MAIN Stop::ToolUse completion
containing at least one executable Owned or Delegated call. Invalid calls or
call-looking blocks under another Stop do not acquire effects. Children retain
the existing host-tool/delivery/wait/finish prohibition and no main transcript.
Ordinary no-call yield, EndTurn and park keep their existing path. This does not
introduce mandatory persistence of every completion.

There is one completion context body per checkpoint, not one full context per
Call. The accepted completion is accounted exactly once before the gate. The
checkpoint must be durably committed before its first executable call. A second
commit of the settled context/evidence is required before a later provider
request relies on this checkpoint's results. A recovered context plan, including
new correction presentations, is also committed before fresh provider work.
These use one common finite commitment mechanism, with at most one live right
per main. They do not turn ordinary noncheckpointed yields into commitments.

Actual supplied child/owned-IO terminals and actual price witnesses can settle
old history. Missing issued child or owned-IO result/price means Unresolved.
No new child, repeated write, shell process, read result, or guessed NotRun
stands in for it. Arbitrary child/IO process-state resurrection is outside this
host late-answer requirement. Required genuine host-decision recovery positives
are in scope and must be implemented; unconditional refusal is not completion.

## 2. Identity, bounded envelope and context coverage

Use a separately versioned `SavedHistory` envelope. Its current proposed envelope
version is1; its settled Transcript component still explicitly contains VERSION2.
Concrete type names here describe required fields, not approved Rust layout.

### Fixed identities

- `worker` / existing routing `logical_run`: the parent's stable logical routing
  identity, retained across activations. It is not the durable effect namespace.
- `effect_scope: u64`: explicit parent-issued durable operation namespace,
  independent of stable routing worker and activation. Zero is a real identity,
  not an absent marker. Parent durably reserves Fresh scopes before spawn or
  admission, never reuses them, and registers imported legacy namespaces first.
  Origin/CallName is scoped by this value; no domain silently rotates it.
- `ScopeChoice::Fresh { effect_scope } | Resume { effect_scope }`: concrete Start
  disposition and sender attestation, checked under the entrance table below.
- `activation: u64`: positive parent-generated, never reused within logical_run.
  Checked increment/exhaustion is a pre-spawn refusal. Parent persists this
  counter; no lower component allocates an unbounded historical registry.
- `CommitName { effect_scope, activation, ordinal: u32 }`: positive checked ordinal within
  activation and effect_scope. One immutable commit payload per name. No hash collision is used
  as evidence of body equality. Actual duplicate submissions must compare the
  original bounded bytes/value and replay the one original terminal.
- `Origin { sequence, position }`: existing accepted historical completion and
  original Assistant block position. Identical provider IDs at distinct Origins
  remain distinct operations. The immutable root Ask/effect is bound separately.
- `ContextPosition`: checkpoint/accepted sequence plus original message and block
  ordinals, checked against the exact observed context, not an arrival timestamp.
- `CorrectionName { effect_scope, origin }`: one first-known semantic correction
  per durable operation in its exact effect_scope. Its exact first decision is part of its immutable
  binding. A different first decision is conflict, not a new correction version.

Opening carries stable routing worker, selected effect_scope and activation as
fixed fields. Restore retains old activations as historical scopes; current budget/usage begin at zero under
new activation. Sequences span the whole concrete conversation, but local host
Turn numbers restart1. Old activation accounting is not monotonic across scopes.

### Exact Start scope entrance and legacy import

Durable operation key is (effect_scope,CallName); stable worker remains solely
routing/supervision identity. Every new HostCall/RecoverHostRecord, commit
body/name, history/supplement, input/provenance/correction/render and evidence
binding carries this scope explicitly or through its fixed admitted run binding.
Scope must not be inferred from callback Token or stable worker for new work.

Before run work, checkout/provider/tool effects, root checks this concrete table:

| Charter/history selection | Required choice | Behavior |
| --- | --- | --- |
| resume=false, any bounded sources | Fresh | Drop both without decoding; fresh sequence1/zero accounting. |
| resume=true, neither source selects history | Fresh | Valid initial/fresh entrance, even if charter permits resume. |
| resume=true, selected settled/checkpoint history | Resume | Both selected sources and scope binding must match exactly. |
| Resume with no selected history | Refuse Scope | No context/namespace evidence; no new work. |
| Fresh with selected history under true resume | Refuse Scope | No silent history discard or namespace rotation. |
| Mismatched selected scopes/binding | Refuse Scope | No partial merge or fresh fallback. |

Use root Refusal::Scope with bounded ScopeRefusal::{Disposition, Mismatch,
LegacyBinding}; scope allocator reports ScopeAllocationFailure::Exhausted before
spawn. Scope refusal is a typed root admission refusal, mirrored by protocol/host, with
all-zero refused activation accounting. This new pure predicate precedes run
checkout preparation; other ordinary semantic-history preparation order remains.
Host forwards opaque bodies without decoding a discarded history. Parent's Fresh
assurance is a sender contract backed by its real durable issuer; bounded root
cannot discover every previously used namespace. The issuer checks actual reuse
before spawn; root checks detectable disposition/selected-scope contradictions.

The parent allocator atomically reserves checked unused u64 values before spawn,
never recycles after admission/spawn/cancel failure, and fails typed scope
allocation before spawn on exhaustion. No saturation/wrap, implicit rotation,
provider failure or lower/domain used-scope registry. New code must reserve old
imported namespaces before allocation; a naive counter beginning1 is invalid if
legacy scope1 already exists. Allocation strategy/used records are parent-owned.

Legacy VERSION2 Transcript bytes have no scope field. Supply explicit
LegacyScopeBinding in the new envelope or Start import metadata: original
logical-run Token and its exact old durable effect namespace (the Token's u64
identity). Resume retains that key; parent registers it already used, verifies
its actual record lineage, and attests the binding. No old record is rewritten.
A bare legacy Transcript plus authentic parent binding remains supported; no
invented context/financial fields are required. Missing old accepted context or
result is still Unresolved. False resume does not import ignored legacy bytes.
Current contiguous Turn validation, accepted sequence/block exhaustion and
historical sequence continuation remain unchanged under this chosen scope route.

### Envelope fields

`SavedHistory` contains bounded arrays with explicit count and aggregate byte
limits, each checked before allocation:

1. schema version, stable routing worker, effect_scope, endpoint and dialect;
2. optional exact settled VERSION2 Transcript;
3. ordered `CompletionCheckpoint` records and genuine context extensions;
4. `CallEvidence` records, keyed by Origin with immutable binding;
5. producer-scoped accounting snapshots/journal records;
6. typed feedback-provenance and correction-presentation records;
7. named-input context witnesses and optional planned User continuation context.

Separate `Start.answered` is a supplemental envelope using the same record
grammar, exact effect_scope and explicit anchors. It may contain checkpoints as well as results,
provenance, input and presentation witnesses. An empty byte field means no
supplement, not an empty fabricated Transcript. Absent Transcript plus a genuine
complete checkpoint starting sequence1 is admitted by a new restore entrance.
Some legacy empty-Turn Transcript is still malformed under the old validator.

A `CompletionCheckpoint` contains:

- identity, endpoint/dialect, actual Stop, actual accepted sequence and exact
  per-completion Usage; accepted provider payload including all opaque/replay,
  IDs, names, inputs and every non-call Block;
- an actual observed context span. Base anchor is the last prefix whose parent
  commitment ACK the producing Session actually received, never merely its last
  emitted/told Turn. Before first such ACK the base is0 and the span contains
  the full actual initial context. Otherwise it contains all genuine messages
  and already settled records since that base, including emitted-but-uncommitted
  Turns and the current Assistant. Existing complete base is required at merge;
- per-call classification and immutable root binding, fixed in original block
  order before effects: Invalid/Refused, Owned, DelegatedHost, DelegatedChild,
  or exact run mechanic. Classification comes from the original actual domain
  preparation, not reinterpretation against new declarations on restart;
- a conservative executable-call intent set and a real gate outcome/dispatch
  coverage witness. A pre-effect intent alone means may-have-issued after
  commitment; it does not prove that its calls stayed unstarted;
- actual producer accounting at acceptance, with scope/ordinal/coverage as §5;
- exact named input and provenance/presentation references that belong to this
  span. References index an admitted context payload; no per-name body clone.

Only one main completion can remain genuinely unsettled at the end of a spine:
source Session does not request its next completion before tools finish. Later
accepted checkpoints therefore require genuine settled earlier context/evidence.
Two contradictory unfinished heads or an extension across an unresolved head
are Malformed/Unresolved respectively. Arrays are not concatenated blindly.

## 3. Named input recovery without duplicate context or false read fences

Current root Message carries an opaque name and exact text; run retains one
`offered`, and only its actual main Turn moves that name to `read`
(root boundary.rs:131–149; run.rs:295–390). A pre-effect checkpoint can contain
an input used by its accepted completion before this actual Turn exists. Caller
may still own the unacknowledged name. Restore must retain that fact explicitly.

`InputWitness { effect_scope, name, context_position, accepted_sequence,
state }` binds a parent's name to the exact original offered text in the genuine
context. `state` distinguishes planned/unaccepted input, actually covered by an
accepted completion, and covered by a genuine previously emitted/committed read
fence. Proof comes from the original run/Session offer-to-context route, not
same bytes, provider text, a replay guess or a fabricated Turn. The checkpoint
carries the original actual binding. Runtime root/run need bounded fields and
route notifications attaching name to Session context position on continuation
and returning actual accepted coverage at the checkpoint.

A same-name retry is legitimate only under an admitted historical witness, in
the same effect_scope, with byte-exact original text. It adds no second User input
and does not by itself issue provider work or emit Waiting/read/Turn. The original
current-activation live-name pending/issued checks still reject duplicates within
that activation. A conflicting body for that old name bounces ReusedName and
adds no context/effect. Unwitnessed equal text with a different name is ordinary
new input; it cannot borrow the witness.

When a witnessed retry is actually transmitted in the current activation, it
enters the same bounded FIFO offer ledger as other current named messages, with
a `CoveredHistorical` reference instead of another text copy. It can become the
one current offered name only in FIFO order and when the exact context used by
a real main completion covers its witnessed input. The next genuine main Turn
may advance the current read fence to that actually issued current name. This
is not an old Turn replay: its number/count/Usage/Spend are the actual new Turn's.
If no genuine Turn occurs, no read acknowledgement is invented. A recovered
context alone never advances a host read watermark to a name absent from its
current actually-sent ledger.

A retry arriving while a real completion is outstanding may be credited at that
completion's actual Turn only if it is the next FIFO offer and its historical
witness is covered by that actual request context; otherwise it waits for the
next real continuation. It cannot leap over fresh messages. This requires an
explicit bounded run claim path; do not append the old text through ordinary
Session::Continue then try to deduplicate it afterward. When an ordinary fresh
input is at the FIFO head, the existing continuation order and actual Turn
fence remain authoritative.

Input witnesses are bounded by admitted historical messages/checkpoints plus
the current live message limit, not an unlimited name map. Retain witnesses with
the context they bind. Any history truncation must also retire its covered-name
recovery capability explicitly; it cannot keep a name with missing body/proof.
The parent must not reuse a durable name for a new input in the same effect_scope;
only exact witnessed retry is permitted across activations. Root and host Start,
Message, checkpoint and read validation routes must reflect this amendment.

## 4. Real commitment gate and complete settlement lifecycle

### Agent/Session/root right

Proposed typed Session Request::Commit carries CommitName, reason
`ExecutableCompletion | SettledCompletion | RecoveryContext`, absolute deadline
and one concrete context/evidence body. Root wraps the Session body with exact
root binding/input/optional actual run accounting sidecars; it does not create
another Assistant per call. Root binds a fixed live Session owner to that one
main-only commitment. Children cannot issue it.

The typed root terminal is `CommitResult::Committed | Failed(CommitFailure)`.
Committed means the parent actually durably committed this exact immutable
body/name. Failed is a real parent rejection or a real transport/deadline/
withdrawal failure, with typed provenance distinguishing known-not-committed
from unknown persistence. Channel Sent, local encode success, read admission and
Cancel request are never Committed. Parent persistence terminal is Committed or
Failed(NotCommitted, reason); the agent-side result additionally admits
Failed(Unknown, cause) only from actual transport settlement. Use fixed bounded
reasons Rejected/TooLarge/Withdrawn and transport causes Unavailable/Protocol;
no arbitrary diagnostic payload is required. Deadline expiry requests withdrawal
and selects the existing stop cause; it is not a fabricated Failed terminal. Actual EOF may produce agent-side
Failed(TransportUnknown); it does not claim the parent's persistence failed.

Session has AwaitCommit with retained accepted completion/tools plan and one
live right. Committed unlocks its intended advance only if still live, within
actual time/cancel/overflow contracts. Failure stops that gate and issues no
calls. A stopped Session remembers stop and requests actual withdrawal once;
Committed winning withdrawal supplies durability but dispatches nothing.
Accepted Usage remains billed/recorded; proved unstarted calls get actual
NotRun in the resulting genuine settled Turn. Late real results still settle
under current close rules. Stale/wrong/duplicate terminal cannot release a later
gate. Cancellation is not a commitment terminal.

Root closing remains until its live commitment terminal settles, alongside the
existing calls/completions/peers. Add typed Session End::Persistence and matching
run/host persistence failure carrying that fixed CommitFailure for an otherwise
live failed gate. Earlier decided cancel/time stays authoritative; actual hard
price/usage overflow retains its existing precedence over an unlanded success,
and already-landed acceptance/delivery evidence is never replaced. Existing hard overflow priority and landed
Accepted/Delivered evidence are preserved. Session Ended/final runtime Answer
cannot discard a live agent commitment right. A real transport failure may
settle that right; no fake parent ACK is required or authorized.

### Host parent persistence and lower-send rights

Proposed `Up::Commit { name, reason, deadline, body, context_read }` moves one
bounded opaque body once to host parent `Request::Commit`. Host retains one
fixed `CommitMeta` in stages Parent -> Queued -> Sending. `context_read` is
informational bounded named-context coverage; it never changes `agent.number`,
`agent.read`, issued-message ledger or ordinary Turn ACK watermark.

Parent `Event::CommitSettled { name, result }` consumes its one real persistence
right. Exact duplicate/stale terminal is inert by identity; an actual first
terminal is checked against the immutable pending name/body binding. Parent
can return Committed after withdrawal if actual commit won, or real Failed.
The parent owns atomic persistence, not host kit. Host then queues
`Down::CommitSettled`; lower Send has its independent actual Sent/Unsent right.
Failed ACK Send does not undo a committed body. A send-failure/EOF becomes an
agent transport failure through the actual protocol close route, never a false
parent Failed(NotCommitted).

`Up::WithdrawCommit { name }` or host shutdown requests persistence withdrawal
once while stage Parent. `Request::WithdrawCommit` informs parent; it does not
remove meta. Existing queued-result disconnect cleanup drops a not-started lower
reply when channel unusable; there was no lower Send right to fabricate. Sending
meta survives until actual Sent/Unsent. Parent-stage meta survives EOF/process
kill until the actual parent terminal. A late parent terminal after EOF settles
that parent right without launching a new write on a dead channel.

### Read capacity, progress, time and last word

Before demanding any potentially Commit record, host reserves a complete
maximum commit body and fixed metadata, alongside the existing maximum Turn,
call-result and record reservations. One commit cell and its byte reserve are
independent of existing Turn ACK count/bytes; no unused old cap is assumed.
No room means no new Read; actual outstanding Read still settles once. The
shared frame all-kind receive maximum must include the new concrete kind.
Terminating/kill drainage must retain bounded scratch for an already admitted
Commit record and all settlement records even when ordinary work is discarded.

Live parent-stage commitment pauses only progress watchdog, until its original
absolute finite commit deadline. Wall/run/provider/operation deadlines remain
independent. Metadata/read-capacity backpressure follows existing Turn pressure
pause, but commit pending is not Waiting and does not revive a cleared Waiting
claim. At deadline request withdrawal and follow the existing fault/cancel/
grace/kill schedule once; do not fabricate Failed or reset until/progress/wall.
Pending commitment is not an extra Long and cannot extend operation deadlines.
Actual valid Commit work clears an old Waiting claim like actual Call/Turn.

The agent's final Answer is allowed only after its agent commitment right has
settled. Host may still have Sending metadata for the already-started reply,
exactly because its actual Send terminal can be delayed; this metadata does not
allow a second Answer. Parent/Queued commitment cannot be abandoned by a normal
Answer. On abnormal EOF/fault the host may already have told its last word while
a parent right remains. Introduce a narrow issued-name settlement exception for
CommitSettled/WithdrawCommit handling after told/Answer/Terminating, analogous
to existing actual call/MessageBounced settlement, without accepting new work,
changing original fault or resetting shutdown clocks.

Gone requires contained tree terminal, EOF/actual Read terminal, all actual
Send and Signal terminals, calls, Turn ACK rights AND commitment parent/send
meta empty. Killing the agent does not cancel a parent persistence right.
The parent must eventually settle it after a withdrawal request, even when the
channel/process is gone. This is an explicit sender obligation; no finite
supervisor can both guarantee settlement of a nonresponding parent and invent
its persistence outcome. An actual failed lower Send or killed tree is not that
parent terminal. Current zero-new-work shutdown semantics remain.

CommitName exhaustion refuses before body allocation/external action and
selects typed receiving Limits; no wrap. If an accepted completion cannot fit
commit staging, it ends with actual accepted usage/context and no effects,
using an explicit persistence/receiving failure; no false unsent completion.
New exact MAX_OUT and handoff bounds must be derived from actual co-emissions.

## 5. Accounting witness: exact producer order, activation and inclusion

Current Session prices its own accepted Usage and rolls child bills into its
inclusive prefix. Root charges only monotonic own deltas. Raw cumulative usage
freezes the entire last representable tuple on overflow. Turn.spent is Session
inclusive historical data; root Answer.spent is a different actual global scope
(session.rs:1590; record.rs:125–143; root route.rs:246 onward; run.md §9).
Never reconstruct global totals from main Turns or charge inclusive bills twice.

Every new producer witness names `(effect_scope, activation, scope)` and a checked
monotonic ordinal local to that producer. Scopes are MainSession and optional
actual RunGlobal; a child terminal names its own real bill scope. A bare provider
ID, accepted sequence or final run Answer cannot substitute for MainSession.

Choose actual producer snapshots as the normal path. A snapshot contains its
last representable own/inclusive unit prefixes and independent flags, completion
count/four-counter raw prefix and atomic usage_overflow flag; explicit context
accepted-through marker; previous settled sequence coverage; and current
checkpoint call-bill inclusion bitmap/closed-coverage marker. Entries identify
actual bill terminals, not merely call results. Equal same-ordinal snapshot
collapses; a greater ordinal is a genuine actual producer progression only with
compatible scope/anchors/coverage. Overflow is sticky within scope. No restore
operation clears an attestation, saturates or reorders actual arithmetic.

At acceptance the producer can seal final accounting for a current remainder
whose original authoritative prepared classification proves all remaining calls
nonfinancial: Owned IO, host decisions and exact nonbilling mechanics. The
actual acceptance snapshot then has closed financial coverage, so later real
host decision results can complete a historical Turn without a lost final-price
snapshot. This seal is an explicit producer witness exposed before effects,
not the decoder inferring names or charging prices. A DelegatedChild or any
potentially billed delegate prevents the seal. Root's original pure call
preparation must expose this classification to Session without a run/session
dependency cycle; preserve exact immutable Ask/effect in the root sidecar.
Host-only recovery positives therefore work using actual old acceptance money
plus real durable decisions. No charge is inferred from absence of a bill.

If actual terminal bills are supplied but final snapshot is missing, the only
permitted alternative is an actual producer-ordered bounded journal: baseline
snapshot, strictly consecutive ordinals, identity/inclusion for each applied
bill/own-price event, actual prefix/flags after each event and final closed
coverage. Receiver validates exact baseline and reproduces the original checked
operation order solely to validate producer claims; it does not reprice Usage.
Missing ordinal, unknown inclusion or open financial coverage is Unresolved.
Different journals may overlap identically and extend contiguously. Root global
journals, if retained, need their own interleaved own-delta order and scope;
main/child journal concatenation is not a valid global journal.

Required sticky-overflow example: inclusive baseline MAX-5; later provider-block
bill4 actually applied first gives MAX-1; earlier-block bill2 then freezes
MAX-1 with overflow. Result Blocks still use original provider order, but money
must retain MAX-1, not recompute to MAX-3. Supplied overflow bill propagates
actual producer attestation in its real order. Unknown residual is never zero.

Legacy settled VERSION2 records need no invented global tuple/activation witness;
they remain opaque historical accounting values under their current validation.
Only normalization of a new unfinished checkpoint requires full scoped witness.
The historical assembler produces a record for its genuinely accepted sequence
once only when context, all concrete terminal results and closed financial
coverage exist. Fresh activation has zero spent/turns/raw counters; the normalized
old record does not emit runtime Used/Priced/Turn or seed new budget. Current
actual final run Answer always uses actual new activation Spend. No old price
is recalculated using current model prices.

## 6. Two-source assembly and exact call evidence

For resume=true selected history, perform bounded read-only preflight of both
input arrays and their complete
owned graphs before allocating merged output. Verify envelope/record versions,
selected effect_scope and stable routing identity, endpoint/dialect, context
anchors/roles/counts, original immutable
call mapping, per-activation accounting scope and all provenance references.
Use repeated scans or a checked array of at most one slot per admitted Origin;
no unbounded identity map, callback history or arrival-order sorting.

Form one chronological spine. Exact original context overlap collapses,
including opaque/replay bytes and accepted metadata. Append only genuine
contiguous extensions. Preserve User non-result text and all original Block
order. Supplemental partial results are keyed by immutable Origin, not matching
an arbitrary repeated provider ID. Multiple out-of-order answers fill their
slots, then produce one complete User result batch in original Assistant call
order. Existing validate_link remains strict.

Evidence states are typed: ProvedUnstarted, MayHaveIssued, actual invalid/
pre-effect entrance refusal, actual transport-terminal uncertainty, actual known
semantic decision/result, and actual applied bill. Full original dispatch
coverage/gate proof can establish ProvedUnstarted. A conservative checkpoint
intent, missing record, EOF, cancellation request or Busy cannot. A Busy on
one later attempt does not erase uncertainty of an earlier may-have-issued call.
Provider non-ToolUse yielded-tail NotRun follows its actual original rule and
Stop witness; never apply it to a ToolUse call which may have executed.

Exact duplicate durable decisions/results/bills collapse; actual conflicting
first semantic decisions, immutable Ask/effect, provider mapping or context
bytes are Malformed. Committed closure/proved-unstarted versus actual known
effect conflict requires authentic coverage correction evidence; no latest-wins.
Known result evidence outranks actual previous transport uncertainty without
rewriting what the provider already saw. Duplicate old RelayName callbacks do
not answer a new recovery callback.

All calls in an unfinished accepted checkpoint must have actual normalized
results or proved unstarted outcomes, and money must have closed coverage,
before provider wake. Missing issued Owned/child terminal is Unresolved. A
matching actual supplied terminal is supported; no arbitrary process recovery
is added. A newly generated ordinary post-recovery read can inspect a checkout
later, but cannot masquerade as the old read result.

`resume=false` independently admits bounded transport ownership, requires
ScopeChoice::Fresh, then drops both sources without decoding them and takes the
existing fresh path under its new effect_scope. Bounded nonempty/corrupt ignored
sources do not block this choice. Context starts sequence1 and fresh accounting0;
stable routing worker is preserved. It does not
allocate merge/correction/lookup structures. `resume=true` refuses precise
Version/Endpoint/Dialect/Malformed/Unresolved/TooLarge; arithmetic/count/ownership
overflow is TooLarge, unknown effect/accounting is Unresolved. There is no fresh
fallback. Existing ordinary checkout preparation order remains: preparation
may precede final Session restore, as run.md §6.2 specifies. New structural
root preflight precedes recovery-only lookup; do not claim all restore failure
precedes all checkout discovery IO.

## 7. Finite recovery-only durable host-record lookup

After structural/ownership admission and before any provider request, root may
issue a distinct `RecoverHostRecord` operation for unresolved host decisions.
It names the selected historical effect_scope, Origin/CallName, exact original
tool/effect/body and a
fresh attempt callback. It has no live old ticket, no new decision authority,
no Owned IO or child resurrection. Require same record binding even for reads.
This is a real new typed root/run/host/protocol route, not codec callback work
or reuse of ordinary HostCall's retry semantics.

Parent lookup terminal is Known(first immutable decision/result/evidence),
Missing, Unknown(pending/uncertain original decision), Busy, Withdrawn, TooLarge
or actual transport/deadline failure. Known is reused exactly without a new
effect or current landing proof. Missing/Unknown/Busy after the finite attempt produce Unresolved unless
another already supplied genuine witness completes the slot. A parent still
performing an old effect may wait for that actual terminal within the lookup
right/deadline; it must never decide anew because the record is absent. It must
answer once after withdrawal, allowing Known to win. Missing proves no lookup
record, not that the operation never ran.

Use one lookup at a time, one attempt per unresolved host slot, bounded by the
admitted call count, and one original absolute recovery deadline for the whole
phase. No fresh deadline per retry and no unbounded backoff. This smallest
route intentionally does not add retries. Current normal host relay policy
is unchanged. Before lookup reserve slot, full ordinary answer/delivery evidence
bytes, actual terminal credit and upstream/downstream queue space. No capacity
means pre-effect TooLarge/Busy admission, not an unpriced lookup.

On cancel/time/overflow/shutdown, dispatch no further lookup/provider work;
withdraw a current lookup once, keep its actual terminal right, record a late
Known without reviving stopped work, and preserve actual old evidence under the historical-only rule below.
EOF generates actual transport failure at agent-side lookup; independent host
parent lookup right still settles before Gone. Recovery replies have a narrow
issued-name exception after told/Terminating like normal actual calls; they
cannot create new work or change shutdown cause/clocks. No old live callback
Token is serialized as a capability.

Root retains a bounded Start/recovery reservation during preflight and lookup.
After final assembly/receiving validation, the main opens under the existing
checkout/kit preparation order, carrying PreparedRecovery and a required
RecoveryContext gate before its first Complete. The local render preparation
stage and that gate do not advance ordinary input/read fences or produce a new
Turn. Root/run deadlines begin at actual admission and are not restarted by
lookup, assembly, Session opening or commitment. The provider permission query
still runs only after this recovery gate is genuinely committed.

Known old delivery/acceptance/result evidence is retained in the historical call
result, provenance, correction and recovery envelope only. It does not enter the
fresh activation's live run::delivered, call Return/landing state or host
successful-delivery proof ledger, and cannot by itself select a fresh
Answer::Delivered or Accepted. Current live accepted/landed work retains its
existing precedence and actual proof; current stop/spend/Turn count remain
actual new-activation values. An old operation still in flight is awaited under
its original authority/deadline; lookup cannot extend that original effect
deadline. Lookup's finite current reply deadline is not new effect authority.
No current Call or Delivery is synthesized to make old receipts admissible.

Successful known-host partial union is mandatory positive behavior, including
parallel original responses arriving out of order and a process restart between
original durable decision and reply Send. The finite Unknown/Missing behavior
is a negative control, not replacement for these positives.

## 8. Typed semantic feedback and persisted correction presentation

Add typed provenance sidecars at the original actual terminal-to-Returned route:
HostAnswered(immutable semantic decision), HostTransportUnknown, Withdrawn,
predecision Busy/refusal, invalid, OwnedResult and ChildResult with exact bill
provenance as applicable. The sidecar binds Origin and exact original presented
Returned/result Block/context position. Text spelling never supplies provenance.
A real answer containing literal `host-unknown` stays a real HostAnswered.

If an old unknown result was already presented, preserve that result and context
byte-for-byte. A new real first semantic decision creates one CorrectionName at
that Origin. Exact already-known decision or exact already-presented result
adds nothing. Different first semantic decision is conflict. Legacy histories
without typed unknown provenance may replay their exact existing results; they
cannot claim a new correction solely by recognizing text.

A presentation record binds CorrectionName, full exact first decision, original
call identity/arguments and effect_scope, context insertion position and state Planned or
Observed. Planned means durably queued information for a future provider
request, not evidence the provider already saw it. Observed means a real
accepted completion's context covers that exact User message. A new actual
checkpoint/Turn retains the transition witness. This distinction avoids both
losing a correction after crash-before-provider and pretending it was already
observed. Duplicated supplements or repeated resumes add no second planned or
observed presentation. At most one record per admitted corrected Origin.

Canonical provider information is one new User message containing correction
Text blocks sorted by original sequence/block order, after the complete old
result batch and before new wake input. It is never another ToolResult for an
already resolved provider call. Required literal Text grammar is:

    smith-recovery-v1\n
    logical-run=<fixed 16 lowercase hex digits>\n
    effect-scope=<fixed 16 lowercase hex digits>\n
    call=<unsigned decimal sequence>/<unsigned decimal position>\n
    decision=<lowercase hex of canonical RecoveryDecision bytes>\n

The last field encodes the complete typed original first decision, including
error/text, provider result replay and delivery/outcome evidence where present;
it is not Debug, display text, lossy feedback or a digest. RecoveryDecision has
its explicit versioned concrete codec, exact final consumption and bounded
fields. Names/input and effect are carried in that decision's immutable original
binding. Lossless hex permits arbitrary bytes. Checked exact rendered length
is fixed literals +16+16 + at most10+1+10 decimal digits +2*encoded decision bytes;
source derives the literal constant and exact Writer measure, not guessed slack.
Replay Some also stays in the genuine original result; current Client's
Unsupported pre-effect behavior remains. Rendering it in information text does
not authorize stripping replay to make an unsupported result acceptable.

One Smith protocol owner implements canonical RecoveryDecision encoding and
correction rendering. Domain does not import a protocol crate or duplicate its
binary/text grammar. Use a bounded explicit local two-stage seam: domain pure
assembler returns RecoveryPlan (typed decisions/identities/positions requiring
presentation); the protocol prepares canonical User content and bounded
presentation records, then passes PreparedRecovery back to the same root
restore reservation before commitment/provider work. This is synchronous local
schema work, not a provider/tool/callback effect. The root holds one bounded
local prepare reservation; checked rendering/cell/byte failure is TooLarge,
invalid canonical representation is Malformed. Actual stop wins any queued local
prepare work: retire its local owned plan without provider/lookup/commit dispatch,
and ignore stale PreparedRecovery by reservation generation. An already issued
real commit/lookup still retains its actual terminal right. No local retirement
manufactures their terminals or changes fresh accounting. Domain validates identity,
count/position/context equality and the trusted protocol's canonical-rendering
attestation; the concrete decoder validates persisted marker/text equality by
that same single implementation. Caller of the typed PreparedRecovery entrance
must supply that attested exact representation; arbitrary text is not a valid
presentation marker. If a sealed validating value is chosen in implementation,
its canonical validator must be shared with this one codec owner, not copied.
The renderer and concrete codec must be usable without importing root/domain;
extract the needed provider-neutral recovery schema/types to a lower Smith
record/protocol module as necessary. No service payload enters skein-channel.

Commit the exact RecoveryContext plan, including Planned User content and its
markers, before provider start. After crash a committed Planned message is
carried exactly once in the next real context; it remains Planned until a genuine
accepted completion covers it. The old observed context remains immutable.
If this commit fails/unknown and activation stops, no provider work starts.
Repeated supplements before/after that commitment cannot duplicate messages.
A result/provenance identity has no financial consequence by itself.

## 9. Source/API owners and exact allocation contract

The implementation owners are:

- Session record/boundary/state: provider-neutral checkpoint/context/provenance,
  accepted Stop/Usage, financial snapshot and closed coverage, AwaitCommit,
  actual commit terminals, accepted-context/name witness and historical assembler.
  Track last actual parent ACK-confirmed context anchor, not just told cursor.
- Run/root: stable activation and explicit ScopeChoice/effect_scope supplied at Start; exact prepared main call binding/
  financial classification before dispatch; fixed main commit binding; complete
  recovery envelopes, lookup state and named-context FIFO claim route. Run's
  actual Turn count/read fence and real pricing paths remain authoritative.
- Host channel/boundary/state: Commit/CommitSettled/WithdrawCommit and RecoverHost
  vocabulary, separate parent/send rights, reserved read room, watchdog and Gone
  conjunction, narrow post-last-word settlement exceptions. Parent atomic
  persistence/record-lookup implementation is necessary for claimed positives.
- Smith concrete protocol: VERSION2 Transcript full codec plus separate envelope/
  decisions/provenance/input/markers codecs; single correction renderer and local
  PreparedRecovery seam; both channel ends and shared frame kind table/caps.
  No opaque/debug roundtrip substitutes for these concrete values.

Every receiving owner gets explicit limits for history/checkpoints, calls per
checkpoint, evidence/journal/provenance/presentation/input counts, aggregate
retained payload/cells, maximum commitment body/decision/result, and finite
commit/recovery deadlines. Derive counts from actual accepted history and calls;
empty cell arrays still cost. Preflight checked sums/products and all conflicts
before allocating merge slots, cloning IDs, rendering hex or performing lookup.
Never retain an unbounded identity history. Repeated scans are bounded by
admitted history×calls×evidence. Invalid input may not allocate peer-declared size.

Meter simultaneous ownership, without assuming drop timing: two original input
envelopes; actual decoded source graphs; retained live conversation plus one
checkpoint copy; optional prepared-call sidecar; root queued body/hand-off;
host parent-owned body plus retained parent/ACK/send metadata; caller retained
body for commitment; actual lower queued/in-flight frame; shared raw frame and
decoded codec staging; exact correction output and its original typed decision;
recovery slot/result and whole provider prompt/tail-ID/provider reserve. If the
codec writes hex directly from typed decision measure/write it need not retain
an intermediate decision byte allocation; if it does, price that copy too.

The accepted live conversation cannot be moved away while tools/provider resume
needs it. Exactly one deliberate checkpoint copy of its covered span is allowed
and priced; multiplying it per call is forbidden. Prepared immutable Ask/input
copies are real owners if needed alongside Block payload. Move validated merged
payload in a second pass; duplicates are released only after exact comparisons.
No reuse of old V1 unused allowances, cap weakening or arbitrary new generosity.
Measure actual type cells/MAX_OUT/Writer allocation and root denial/handoff
chains after source exists. Preserve original maxima/history/RNG and deadlines.

## 10. Required positive chronologies, faults and limits

1. Actual main ToolUse with host/Owned/sub-agent calls: accepted Priced/Used once;
   checkpoint parent commit before any actual effect; one body covers the whole
   completion. Ordinary no-call yield/park and child host prohibition unchanged.
2. Crash between acceptance/checkpoint delivery/parent commit/ACK Send/call
   recording/decision/reply/settled commit/Turn ACK. Exact actual real rights
   remain; no new call after stop, no fake ACK/Gone/NotRun.
3. Checkpoint-only genuine initial accepted prefix; identical dual-input overlap;
   complementary partial known results in out-of-order arrival; contiguous new
   checkpoint extension; exact literal assembled provider order/all bytes/replay.
4. Durable host decision returned by recovery-only same-identity lookup after
   old reply was lost: no repeated effect, actual new callback only. Missing/
   unknown/Busy produce finite Unresolved; old Busy does not erase older issue.
5. Typed old HostTransportUnknown then real known decision: immutable old result,
   one Planned canonical correction, real commit before provider, eventual
   Observed marker. Repeat supplements/resumes before and after observation add
   no duplicate. Actual known text spelling `host-unknown` is never misclassified.
6. Accepted completion includes named input before its genuine Turn. Crash and
   exact same-name retry uses the persisted context once; no old read ACK or
   runtime Turn replay. A real next Turn credits only its actually issued FIFO
   name. Different-name same text is new input; conflicting same-name text bounces.
7. Supplied actual owned/child terminals and real sealed/journal accounting
   normalize history. Missing result/bill is Unresolved, never a repeated write/
   child/read or guessed NotRun. Real delivery landing evidence survives.
8. Original producer settlement-order overflow example; atomic raw-prefix flag;
   closed nonfinancial host remainder; child bills included once; overlapping
   actual snapshots/journals; multiple activation scopes and fresh zero budget
   accounting. No old runtime Turn count/Usage charge.
9. Commit and lookup deadline/cancel/time/overflow versus late real winners;
   EOF/Unsent/parent refusal/process kill; parent terminal after Answer/told/
   Terminating; exact no-clock-revival/real Gone conjunction. Failed ACK Send
   cannot undo a committed body; Cancel cannot unlock uncommitted work.
10. Exact full and one-short counts/bytes/cells/provider reserve/record and read
    credits; complete simultaneous input+decoded+live+checkpoint+parent+frame+
    presentation overlap; checked identity exhaustion; duplicate/wrong/stale
    commit/lookup/input rights and current-activation namespace guards.
11. Old A at scopeS/CallName(1,0), then valid fresh B at new scopeT/CallName(1,0)
    with stable worker unchanged: genuine distinct once-only decisions; exact S
    Resume reuses old answer. True/no-history Fresh, false/both bounded corrupt
    sources drop; contradictory disposition/scope mismatch pre-work refusal;
    actual issuer reuse/exhaustion prevention; exact legacy import key and no
    recycle after failed spawn/admission. Sequence remains contiguous1.
12. Old Known Delivered winning lookup withdrawal during new activation cancel:
    exact old receipts remain in historical evidence; actual current stop, no
    new delivery/Call/Return/live host proof/Accepted/Delivered or old charge.
    Existing actual CURRENT landing-winning-cancel positive remains separate.
13. Genuine concrete codecs through both parent/agent ends on shared framing,
    with negative/golden/roundtrip exact-value evidence, not typed-only tests or
    provider Debug traces. Preserve all prior seeds/fates/pressure/oracles.

## 11. Recovery limits

This contract specifies effect scope and lifecycle. It does not promise
atomic recovery of arbitrary local IO with parent disk persistence. A crash
between an issued owned/child operation and durable terminal capture can remain
Unresolved. Supplied authentic terminals are supported. Host decisions have the
separate durable parent record required for positive same-identity recovery.

A nonresponding parent cannot have its pending persistence/lookup outcome
invented merely to reach Gone. Parent's actual one-terminal obligation after
withdrawal is required; a finite watchdog can kill the agent but cannot settle
the parent's durable decision. This is an explicit rights obligation, not a
hidden graceful-shutdown guarantee.

The protocol PreparedRecovery canonical attestation is a new concrete trusted
schema boundary; implementing it as an arbitrary public text marker without
that validated producer/decoder route would violate this contract. Exact Rust
representation and crate/module arrangement remain source design work subject
to independent review. It must not create root/session/protocol dependency cycles
or duplicate a renderer. Concrete wire discriminants/layout/caps also need the
future codec design and measured source; no current opaque bytes are declared
complete by choosing envelope names here.

Fresh/Resume effect scope is a typed boundary separate from stable routing
worker. Old recovered Delivery never enters current landing/result state. The
parent persistence terminal, agent remote commitment terminal and actual lower
Send terminal remain separate rights. Exact namespace issuer/import, concrete
codec layout and measured implementation bounds must satisfy sections 2, 4, 9
and 10; an opaque history blob alone proves none of those obligations.
