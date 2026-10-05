# 05s4: messages, waiting, parking and concrete resume

This is the implementation ledger for the approved messages increment. Source
is in progress; the early compiler checkpoint is
`49d15601c4346251eb3c514a4ba68058b9da6f83`, rebased onto Smith `45a2eea`.
The original author base was `51fdd24`. The current temporary increment follows
reviewed checkpoint `c645669` and adds genuine native root restoration.
Independent source review covers actual Start/discovery, concrete root records,
new Client continuation, parking and physical cleanup in both configured dialects.
Root-entry/caller-copy memory and single-lifecycle native memory were reviewed
separately; overlapping physical ownership remains a subsequent control.
These results do not claim a main merge, migration completion or copied-package
removal. The runtime path uses only the shared Skein Client.

The narrow root/Wire increment follows temporary checkpoint
`9b7990a79953ca8e662c9731522b6de9528eb64c`. It adds an opt-in actual-wire backend
to the existing agent world, using the pinned shared `Exchange::at` mechanism
without changing the default typed worlds or production domains. Independent
review and the four parent-run checks cover this increment. Public documentation
backfill at temporary `edc583ebebe2a97a936224e6cfd9aa9adfbcad2b` separately passed
the four checks, including 489 focused tests in 1.937 seconds and ten fuzzy
tests in 4.294 seconds, before integration with the root/Wire increment.
The approved root/Wire checkpoint is temporary `240838d`; boundary checkpoint
`9b8630e` adds six actual adapter owner, inventory and attestation controls,
passing formatting, all-target clippy, 496 focused tests in 1.968 seconds and
ten fuzzy tests in 4.219 seconds before the host-delivery increment.

## Source and contract

The existing Smith run/session/root machines are extended in place. The tested
copy baseline and named Temper stories remain in
[migration-05s2.md](migration-05s2.md); results, delivery and host-tool behavior
remain in their 05s4 ledgers. The real host baseline is Smith `eb46ecc`, extracted
from Temper `19735a06`/05d `e2a6a719`, with its full source-story mapping in
[migration-05s6-host.md](migration-05s6-host.md).
Shared ownership and temporary consumer pin evidence are recorded in
[migration-05s2a.md](migration-05s2a.md).

The port reference is parked Temper `c33941c3`, specifically
`crates/temper-agent-domain-run/src/v2.rs` and
`crates/temper-agent-domain/src/v2.rs`. It was read, never merged. It is an
incomplete sketch, not validated behavior or evidence for the new stories.
Its useful transition ideas are implemented in the existing Smith cells; its
numeric message ordering, worker/engine assumptions, parallel V2 machine and
old cancellation shortcuts are not adopted.

| Named source | Implemented seam | Required outside evidence |
|---|---|---|
| Temper 05f run `turn`, `message`, `continue_message`, `yielded` | Count/byte-bounded named FIFO, offered/read distinction, wake/park | Actual shared-fake prompts, real Turn read fences and precise bounces; zero/nonmonotonic names. |
| Temper 05f run `delegate`, `wind`, `resume` | Main-only settled Wait, immutable winding-down, actual late terminal rights | Wait result before Waiting; wall independent; actual winning result in final Turn. |
| Temper 05f root `step`, `from_session`, `from_run` | OpenV2, concrete history selection, Turn and AnsweredV2 translations | Actual root admission, restored post-transcript bytes and causal Turn-before-next-Complete/Answer. |
| Smith session `open_v2`, transcript validation/restore, `tell_turn` | Exact history admission and concrete records | All six precise refusals before provider/tool effects; unchanged replay and provider IDs. |
| Smith session provider and tool closing paths | Pre-effect receiving credits through actual settlement | Full history plus maximum completion/result winning Cancel stays in the last Turn. |
| Smith run delivery/host relay and root worlds | Durable names, actual receipts, recovered first-record answers | Retained mid/final delivery and relay histories with concrete continued/saved/resumed feedback. |
| Smith host numbered Turn/ACK/IO lifecycle | Parent body ownership separate from kit metadata | Exact noncumulative ACK, delayed Send and outstanding call/delivery/EOF/tree rights prevent Gone. |

Contracts are [domain/run.md](../design/domain/run.md), sections 3, 5.4, 6,
9, 10, 13 and 14; [domain/session.md](../design/domain/session.md), section 3;
[domain/tools.md](../design/domain/tools.md), section 5; and
[domain/host.md](../design/domain/host.md), sections 6 and 9. Production follows
programming-model.md; ordinary worlds and positive-first judges follow
testing-strategy.md. Shared Client/HTTP/SSE/provider codecs, replay envelopes,
scripted peers, referee infrastructure and allocator remain Skein-owned.

## Concrete ownership and choices

The root owns the original Start reply right and selected concrete history
until main consumes its opaque binding. False resume ignores supplied history;
true without history starts fresh. Every main and child opens V2. Historical
sequence includes restored turns; activation output numbering starts at one.
Each actual main Turn moves its complete body outside before the next provider
request or final answer. Children never count as main transcript turns.

Names are equality-only Tokens, including zero. Parent names are unique for
an active logical run; receiver duplicate detection is bounded to queued,
offered and current-read names. FIFO is arrival order. A message is read only
when its continuation appears in a real main Turn. A bounce changes no fence.
Main Wait is an exclusive write with concrete `waiting` feedback; Waiting
requires settled work and a subsequent yield with an empty inbox. The typed
waiting interval is positive; zero is an admission refusal. Wall time keeps
running. Parking closes main and waits for real terminals before its final
Turn and answer.

Canonical `feedback(run::Returned, max_bytes)` is a checked pure two-pass Writer
translation into `Feedback { text, error }`. There is no Render callback or
extra protocol authority. Actual host text/error and ordinary child text move
unchanged. Other child results preserve cut and typed stop beside exact text.
All semantic outcomes preserve fields, omitted-problem count, directory receipts,
marker, diagnostic, check exit/cut/tail and typed failure. Opaque bytes use quoted
ASCII literals with lowercase `\xNN` escaping, including quote and backslash;
the checked maximum prices fourfold expansion and fixed labels/counts. The
adapter consumes the resulting text verbatim.

Root advertises completion bytes, block count, failure bytes and aggregate
Decoded ownership. Adapter compatibility is checked before preparing the actual
Client. Every possible call has a full classification cell reserved, including
TooLarge fallbacks. Actual call cells are reserved together before dynamic
payload admission. Original provider bytes and complete opaque replay envelopes
are counted separately; no provider enum or provider syntax enters the domains.

Session reserves provider credit
`P = max(2*C + N*max(sizeof(Block), sizeof(Slot)), full_failure_terminal)` and
at least two Message cells before every provider request, retry and continuation.
This covers actual assistant content, copied call IDs/details and all result
skeletons. Before the first effect of a batch it reserves every maximum result
payload together. Credits survive Closing and convert exactly once on actual
terminal receipt. Insufficient room refuses before the effect. An actual bounded
terminal that wins Cancel remains complete; no full-history path clears it or
substitutes a cancellation acknowledgement.

Read, Search, Shell, ambiguity and List have exhaustive finite result bounds.
List's `list_bytes` and Scan's `max_bytes` include Entry wrappers and all names.
The lower returns a name-order bounded prefix with `more` for every omission;
an oversized first entry yields an empty prefix and complete omitted count.
There is no silently dropped completed listing.

Message and Turn arrays are priced independently from session Block/payload
bytes. Root additionally prices selected Start history, concrete handoff/output,
rewritten root prompt envelopes, decoded asks, deferred answers, simultaneous
semantic/result text, failure transit and caller-held copies. Host turn bodies
belong to the parent; kit metadata survives exact durable acknowledgement and
actual Send. Root keeps no second ACK table. Process exit, EOF or tree emptiness
cannot consume an outstanding call/delivery or ACK right.

Current Charter has no prices to ignore. Root opens V2 with zero prices and
unit one; scalar record spend is zero, while copied typed token/turn/wall budgets
and child spend remain enforced. Scalar run pricing, optional workspace and
conventions, live channel/transcript codecs and executable are later increments.
The live composition's full Debug record is a test encoding, not a production
codec claim. The opt-in actual Client binding retains each physical owner through
Reusable, explicit Close and actual Closed, even when a root callback is reused.
Final copy removal still requires the remaining shared-client replacement
evidence and final acceptance.

## Written focused evidence

Two passing controls are `actual_root_delivery_receipt_turn_and_host_rights_settle_after_cancel`
and `actual_root_submission_parent_right_outlives_exit_tree_eof_without_fabricated_return`.
These tests use the existing MidReport script and original root Start/discovery,
checks and actual Deliver request. A separate opt-in world entrance exposes at
most 256 whole bounded submissions and awaits the outside parent; existing
Schedule, flight ledger, snapshot and referee still own actual terminal routing.
Default typed and wire worlds retain their current scheduling. No automatic
terminal coexists with a bridge submission; unknown and already queued callback
replies refuse before scheduling another terminal.

The live control translates actual scope/name/deadline/opaque fields into a real
host Up::Call. It holds the parent operation through original-root cancellation
and an earlier Turn ACK Send, then maps only the actually issued sealed parent
receipt to the root. Exact paired receipt feedback must appear in a concrete
Turn; the final Delivered/Cancelled name and receipts must agree with the real
host proof. Positive evidence precedes altered-name/receipt negative controls.
The reply Send survives root settlement. While the channel still listens, the
parent acknowledges the actual final Turn before forwarding the root's last
word. Its ACK queues behind that held reply Send; only the actual reply Sent
issues `Down::Acknowledge { turn: 2 }`. That genuine final ACK Send then survives the last
word, process exit, empty tree and EOF until its actual Sent terminal. No ACK
Send is invented after the host has stopped listening.

The channel-loss control uses another actual root submission. Real host Stop and
Cancel precede process exit, generated withdrawal, empty tree and EOF. The
parent operation remains outstanding after earlier Turn ACK/Send settlement;
only its actual sealed parent reply permits Gone. The simulated root is no
longer driven after actual process exit. This control claims neither a returned
receipt nor a post-EOF Turn, and requires zero fabricated downlink answers.
Both controls passed in 0.010 seconds; independent source review and the four
parent-run checks cover the final frozen host-delivery slice.

The passing `actual_root_wire_host_feedback_and_reused_owner_close_survive_parent_cancel`
starts the original root and discovery under the Codex and Anthropic caller
endpoint fixtures.
Actual Complete metadata goes unchanged to adapter preparation; peer-decoded
queries supply observations. Actual discovery Read request and terminal precede
Complete; the exact discovered guide reaches the first query's system text.
The story selects one declared opaque host write,
checks exact incoming argument bytes/effect and the actual first host answer,
and uses handwritten native continuation arguments plus literal provider ID,
feedback and error expectations. Positive-first controls remove or rewrite the
observed result. A separate compact literal schema-extension check supplements
the existing parsed whole-schema check in the standalone client story.

The composition keeps active root callbacks separate from retained physical
bindings. The selected lower schedule holds the first won Close until the next
physical call starts, records that overlap, and requires actual Closed before
retirement. A reused callback cannot identify the older physical cleanup. The
second actual query precedes real parent cancellation; cancellation's terminal
is translated only after shared lower settlement. Root and Client/peer clocks
come from the same iteration, including independently moved wall time, and
immediate wire progress prevents a jump to the root deadline. Outside records
have a finite 256-call ceiling and four lifecycle observations per binding;
this binding story is not an attained allocation measurement. Arbitrary
application Finish decoding and allocation peaks with multiple retained bindings
remain pending. The single-lifecycle memory control below does not measure that
overlap. The passing gates cover the scoped increment; final replacement
acceptance remains outstanding.

Four passing controls in `tests/agent/tests/adapter_continuation.rs` adopt the
adapter's actual prepared Client through the shared raw-byte world, using
handwritten native HTTP/SSE and replay-envelope expectations. Codex preserves
encrypted reasoning and a nested extension, commentary message ID/phase,
refusal text/ID and Refusal stop, then sends their exact continued native
history. Anthropic preserves signed and redacted thinking, its nested extension
and all four usage counters: input/output/cache-read/cache-write are 7/9/11/13;
the Codex fixture reports 8/3/4/0. Thinking's literal expectation follows the
documented assembled head order while retaining every field. These histories
prove the native adapter/caller handoff, without a new root-restore claim.

The malformed Codex control starts original root discovery. Its exact raw
`host_action` call yields Invalid feedback and no host effect; the next actual
query retains the full raw call/ID/name and paired error. Only the corrected
call submits one actual host write, whose real result reaches the third query.
Concrete Turns retain original call replay, and real parent cancellation settles
all three physical Clients. Another control observes RateLimited/Response with
literal detail, Unavailable/Unknown after an actual send and transport loss,
and Unavailable/Unsent after actual lower closure before Start. Each retained
context consumes its actual terminal; all physical rights settle. These are
three selected failure cases, not full failure-enum coverage.

The host-history oracle now binds only actual ToolUse calls decoded as served
Host asks. The existing flight retains the actual prompt's assistant-message
index; exact block position, ID, name and input select the locally paired User
result. Older turns may reuse the provider ID. The focused positive-first
regression accepts an older malformed same-ID call before the corrected host
receipt, then rejects 13 missing/duplicate/rewritten call or receipt corruptions,
wrong origin fields and relay name/input changes. All prior relay/recovery,
terminal chronology and exact feedback assertions remain. The combined
continuation and host controls pass 13 tests in 0.075 seconds.

These groups use actual root/session/tool entrances and shared fake observations:

- `actual_wait_result_settles_before_waiting_then_opaque_fifo_names_cross_only_real_turns`;
  `bounded_message_refusals_keep_the_accepted_zero_name_and_wall_time_runs_while_waiting`;
  `actual_parked_transcript_resumes_without_recharging_history_or_reusing_activation_numbers`.
- `real_host_result_committed_after_last_transcript_restores_before_wake_without_repeating_effect`;
  `every_transient_history_refusal_is_exact_and_starts_no_provider_or_tool_effect`;
  `maximum_actual_opaque_delivery_receipt_survives_continue_saved_turn_and_resumed_prompt_exactly`.
- `cap_filled_provider_credit_preserves_actual_replay_completion_that_wins_cancel`;
  `one_byte_or_one_message_less_refuses_before_provider_and_tools`;
  `full_history_batch_credit_keeps_maximum_actual_late_results_or_prevents_every_effect`;
  `fullest_history_keeps_maximum_actual_owned_read_list_search_and_shell_after_cancel`.
- `exact_failure_classes_and_transport_evidence_survive_policy_without_diagnostic_retention`;
  `full_history_reserves_every_child_answer_before_effect_and_keeps_late_actual_bytes`;
  `decoded_batch_reserves_refusal_cells_before_any_payload`;
  `decoded_receiving_cells_are_required_before_provider_work`.
- `real_host_turn_ack_and_send_rights_survive_root_parking_and_exit_tree_empty_eof`.
  This interleaves actual root/shared-fake progress with real host entrances,
  retains parent bodies, holds a real final ACK Send through parking and cleanup,
  and requires exact commitment before Gone.

The messages referee first accepts an actual closed chat. Mutations then remove,
duplicate or corrupt Turn number/sequence/read/provider ID, invent premature
Waiting, reverse wake text, change final count or park time and erase the settled
Wait result. It sees outside input, provider terminals and actual root output;
its expected final answer never comes from input supplied to the domain.
Existing delivery and relay judges retain their actual interruption and durable
record chronology controls.

Retained session stories include concrete turn replay; all transcript admission
refusals; cumulative/overflow pricing and child spend; withdrawn/late provider
and tool terminals; post-transcript paired results; NotRun yielded tails;
closing a resting batch; exact IO outcomes; repeated provider IDs with distinct
origins; facts/replay independence and maximum-recorded-history ownership.
Retained root worlds include reports/verdicts/failures, all discovered checks,
mid delivery then final Change, actual landing during shutdown, actual timeout,
Busy/Lost recovery, first-record replay, post-withdrawal actual answer and live
relay cancellation. They now use concrete V2 Turns and feedback.

## Retained named regression inventory

`tests/session/tests/recorded.rs` keeps the original concrete V2 stories:

- `concrete_turns_resume_verbatim_without_local_tickets`;
  `transcript_refusals_precede_tools_and_completions`;
  `oversized_history_and_fresh_specs_are_refused_at_the_entrance`.
- `unit_budget_stops_after_the_crossing_turn_settles_and_child_counts_once`;
  `pricing_rounds_the_combined_completion_and_rejects_overflow`;
  `cumulative_child_spend_overflow_is_a_typed_failure`.
- `closing_preserves_withdrawn_and_late_answers_and_provider_completions`;
  `closing_a_resting_batch_keeps_its_real_result_and_marks_only_unstarted_calls`;
  `owned_io_cancellation_keeps_actual_terminal_results_in_the_turn`.
- `committed_call_results_after_a_yield_are_restored_without_tickets`;
  `yielded_historical_calls_resume_with_concrete_not_run_results`;
  `repeated_provider_ids_keep_distinct_origins_and_restore_includes_history_prefix`;
  `replay_and_facts_capacity_change_no_decision`.

Counted session stories retain
`recorded_delegated_turns_hold_exactly_the_byte_cap_and_count_their_copies`,
`restoring_a_maximum_recorded_history_stays_within_the_counted_bound` and
`an_oversized_waking_result_tail_is_refused_before_cloning_provider_ids`.
They now leave the full receiving reservation available before work, rather
than expecting an actual settled result to disappear at a full history boundary.
`randomized_priced_turns_and_terminal_races` keeps its outside observations.

Root/world delivery and relay controls retain:

- `separately_granted_mid_delivery_continues_to_a_real_report`;
  `five_actual_host_terminals_preserve_report_only_contract_and_stale_ends_it`;
  `real_mid_delivery_then_final_change_checks_and_lands_each_snapshot`.
- `mid_landing_during_explicit_shutdown_preserves_actual_receipts_and_spend`;
  `submitted_delivery_gets_a_real_timed_out_host_terminal_during_shutdown`.
- `host_busy_lost_then_replayed_answer_preserves_opaque_input_name_scope_and_exact_text`;
  `actual_answer_after_withdrawal_is_retained_without_another_relay`;
  `real_composed_cancellation_while_host_relay_is_live_retains_recorded_answer_and_stops_recovery`.

Run units add `opaque_fifo_wakes_waiting_and_read_advances_only_on_actual_main_turn`
and `bounded_messages_and_input_at_idle_deadline_preserve_existing_fifo`.
Canonical feedback public-consumer controls add complete child cut/all-stop controls, exact
non-UTF8/quote/backslash/NUL/newline receipt/diagnostic/marker/check-tail controls,
ordinary host/child move controls and exact/one-byte-short text caps. Existing
run/session/root unit admission, budgets, children, checks, output pressure,
credential generations and facts controls remain, with exhaustive translations
updated for new variants.

Prepared shared-Client controls preserve incoming streamed host arguments byte
for byte in the root completion and sealed HostInput. Continued native input
uses handwritten outside configuration data: Codex retains the spaced argument
string; Anthropic's embedded object serializes whitespace into the independently
specified compact object. Both retain the whole nested value and extra field,
exact provider/result IDs, feedback text and native error classification.
Positive controls precede changed nested value, dropped extra field, missing
feedback and rewritten feedback mutations. No production provider branch or
translator-derived expected object supplies this oracle. The prepared Client
boundary controls and actual root/Wire story provide distinct evidence: the
latter uses the original Start/discovery and actual host request/result routing.

The random root memory driver retains each actual `Complete` request's byte,
block and decoded receiving allowances. It prices root/session cells, original
fields, replay bytes and decoded ownership before emitting bounded terminals,
including exact-cap text cases. Search retains actual request caps and counts
every path byte. Existing seeds, limits, rounds, meters, semantic-invalid
finishes, oversized starts and cancellation races remain. Observations include
359 actual Check requests, 356 Deliver requests, 3,643 exact-cap search
terminals, 2,271 exact-C provider terminals and seven delivery-terminal
submissions after actual parent cancellation. Required passing/failing checks,
delivered/stale/failed delivery, refusal feedback and cancellation races all
occurred. A selected terminal submitted for an observed pending right is not
by itself proof of a particular accepted downstream outcome; that requires
separate actual Turn, prompt, answer or receipt observations. The focused
root-memory and bounded message-sweep evidence below supplement this driver.

The passing `restored_root_arrays_payload_rewrites_and_turn_copies_stay_within_the_attained_bound`
generates concrete records through original root entrances and actual native
Anthropic Client terminals, with real Wait feedback and physical Close/Closed.
Its two configurations attain 12 Messages across six Turns and 28 Messages
across 14 Turns, including one actual 8,192-byte person message. Each generated
Turn has one assistant; actual tool/result Turns have three Messages and text
Turns have one. The whole-history User predecessors establish the reachable
maximum of `messages - 4` historical Messages and half as many Turns, leaving
waking/provider slots. The configured array ceiling is a conservative bound,
not a claim that every Turn requires two Messages.

The restored Start retains selected history through outstanding discovery.
Exact reservation-inclusive payload admission emits rewritten Complete arrays
of 13 and 29 Messages, preserving all observed fields and replay bytes. The
actual native completion then emits a concrete Turn at sequence seven or 15,
activation number one, before the settled ContextFull answer. A valid extra
Message is refused with independent byte headroom; a separate extra historical
text byte is refused at the exact payload cap. Both produce precise TooLarge
with no provider or owned-tool effect.

The shared allocator meters Domain construction and root-entry peaks, including
rewrites and concrete Turn transients. Its persistent baseline counts retained
source records, saved history, restored prompt copies and a caller-held Turn
copy. Separate shared Meters check complete caller-copy and record-handoff
construction peaks against public wrapper/payload/replay ownership. The earlier
root-entry/caller-copy increment passed in 0.586 seconds and has independent
source approval.

The current increment extends the same persistent Meter across each single
actual Anthropic Wire lifecycle while root/session state, caller configuration,
retained records, history, prompts and Turn copies remain live. It measures
preparation, Start, native byte-peer progress, application translation, Close,
actual Closed, remaining drainage and Wire drop. Each peer receives the root's
actual Time/Wall snapshot; genuine settlement leaves Service calls at zero
before drop. No nested caller-copy Meter runs within this native span.

The checked price combines the root bound, independently counted outside owners,
the original taken Prompt, the adapter bound for one Client/context/translation
and the shared peer extra bound, which excludes the Client. Public wrapper and
owning-field ledgers separately price configuration construction/retention,
endpoint metadata clones, schema Vec/Box handoff, script arrays, Wait resolution
and decoder scratch, finite observation/returned vectors and the transferred
terminal. Schema construction has a separate measured peak and reclaims before
native work. Actual queries, requests, responses, literal Text/decoded Wait and
full replay ownership supply the outside observations. The native span checks
raw peak allocation and exact net ownership after Wire drop: its incoming
baseline minus the original Prompt plus the genuine transferred terminal.
Final root, configuration and caller-copy reclamation returns all held bytes to
zero. The unchanged attained arrays, restored real Turn, exact payload cap and
both independent one-over refusals still pass. This control passed in 0.672
seconds; exact source and lock have independent approval.

This checkpoint proves single-lifecycle combined component ownership. Actual
overlapping physical Clients remain unmeasured here. Passive Composition routing,
trace and referee bookkeeping are excluded by the shared heap harness contract;
the earlier demand for an analytical full World envelope was self-imposed, not
an acceptance requirement. See testing-strategy.md, section 6,
programming-model.md, section 6.3, and skein-world/src/heap.rs. Owned operational
Client/peer queues and buffers remain inside the component measurement.
The later native root-restore control is described below.

The passing `bounded_message_schedules_replay_with_every_required_actual_class`
runs 16 pinned seeds across four bounded input/cancellation fixtures, through
shared replay and the unchanged message oracle. Every scheduled input arrives
with its exact name, bytes and time before the actual final Answer. The sweep
observes all 19 required classes: actual Parked, Time-budget and Cancelled
endings; Busy, ReusedName and TooLarge bounces; zero and descending opaque read
fences; inputs during a real call and after Waiting; and both late Completed
and actual Cancelled terminals after the original root's Cancel entrance.
Cancel times come from actual baseline request/terminal observations. Two
accepted names per world produce observed 0→99 or 99→7 pairs across seeds;
there is no claimed three-name conversation. The focused sweep passes in
0.512 seconds. This is the existing typed shared-fake root world; it does not
claim native Client cancellation coverage or independent Env.wall jumps.
Detailed actual counts and seed bounds are recorded in
`/tmp/temper-next-migration/smith-message-sweep-focused-1.log`.

## Genuine native root restoration

The new `native_root_restore` story enters the existing World through original
Start and actual guide discovery, then completes two real native turns and
parks. Both caller-configured dialects supply literal opaque reasoning/thinking,
sealed content and nested extension metadata alongside an actual decoded Wait.
The saved Transcript comes only from genuine root Turns. A second activation
restores that full history through a new root and actual shared Client, commits
two new Turns, parks and physically closes every binding. A separate activation
moves the genuine first Turn's concrete Wait result into Transcript.after,
proving the real post-tail path without manufacturing feedback.

The outside oracle checks whole opaque objects in actual continued queries,
roles, call IDs and exact paired feedback; handwritten envelope literals check
opaque and optional Text/call replay metadata in concrete root Turns. Query
projects away Text phase and call item ID metadata, so those optional envelopes
are not independently observed on the next native wire request in this story.
Positive observations precede independent missing/rewritten/version/tag/proof,
role, ID and result corruptions. Real header version/endpoint/dialect refusals
start no provider or host effects. Every completion conserves all four SDK usage
fields, typed cumulative activation spend and historical sequence numbering;
history is not charged again. Record prices remain the separate zero-price
transitional contract. Actual Wait deadlines, receiving bounds before Start,
Parked endings and Completed/Reusable/Close/Closed chronology are asserted.

## Remaining checkpoint work and validation

The current checkpoint retains the approved actual wire/root binding composition,
submitted-delivery extension of live host composition, adapter boundary controls,
public documentation, attained root-entry/caller-copy memory evidence and the
single-lifecycle combined native memory control and native root-restore story.
Allocation peaks with simultaneous retained physical Client/peer owners remain
outstanding; passive trace/referee/routing bookkeeping is outside this component
contract. Native adapter continuation has actual terminal/history evidence for
its selected cases; the bounded message sweep has actual counts
and independent source approval. All named
source distinctions and existing tests remain in scope; no passing subset
replaces these requirements. New public items continue
to require full module/type/variant/field/entry documentation and independent
subset review alongside later integration work.

The reviewed backend cleanup selects a closed private Typed or Wire backend
before construction. Native worlds construct only Composition, retaining no
unused typed fake Domain, Stage or provider-call Ledger; no new Box or Client is
introduced. Original Start/discovery, typed configuration/seeds/queues, clocks,
host-call origins, cancellation and settlement contracts remain unchanged.
The corrected session LLM module documentation assigns native wire/replay formats
and classification to Skein and application vocabulary/translation to Smith.
Both exact source changes have independent scoped approval. This cleanup removes
one passive owner; it does not change the attributed component memory claim.

The approved wire composition consumes the receiving fields of the actual root
`Complete`. Its active logical-owner lookup is distinct from its retained physical bindings: one
Client terminal ends the root request, while that physical binding remains
owned through `Reusable`, an explicit `Close` and actual `Closed`. A later
request carrying the same logical owner cannot redirect an older binding's
cleanup. Actual byte-peer queries and real translated terminals, rather than a
second typed response, supply the composition's outside observations. The
passing root story exercises a declared opaque host call, its exact
continued result and parent cancellation; it makes no unsupported application
`Finish` decoder claim. All participating stages receive the same injected
monotonic and wall clocks through the shared world's clock entrance.

Six passing adapter consumer controls now check actual Completed, Failed and
Cancelled owners, including wrong-owner refusal at all three entrances; missing,
duplicate or unoffered schema inventory; missing, duplicate, unused or mispaired
result renderings; and ResolvedCall position/name/literal-input/kind, uniqueness
and consumption. The real failed fixture retains Overloaded, Response evidence
and exact `overloaded_error` detail under the matching context. The four native
continuation controls above add replay, refusal/reasoning, all four usage
counters, real malformed-call correction and three failure/evidence cases.
Broader failure-class/evidence/detail coverage remains outside this narrow slice.
The compact literal schema-extension observation now supplements parsed schema
equality; it does not discharge these other boundary requirements.

The approved live-host extension submits an actual delivery through root and
host entrances and retains its parent operation right independently from Turn
commitment and ACK Send. Only the actual parent answer consumes that operation
right; its sealed receipt agrees with the concrete Turn and final Delivered
proof before cleanup settles. Its separate channel-loss control retains the
parent right through actual withdrawal, process exit, empty tree and EOF, then
consumes the parent's actual terminal without inventing an agent response.

The remaining component memory control must measure simultaneous retained
native physical Client/peer ownership through actual Closed/drop. The earlier
single-lifecycle span keeps root/caller owners live but drops its Wire before
the next root entrance. An analytical full World price for passive harness
storage is not required by the foundation. Selected failure controls retain
their stated scope; genuine opaque root Restore is now covered below.
The bounded message sweep counts actual endings, read fences, bounces and
terminal races and requires every class to occur. Shared replay preserves its
full boundary trace and actual outcome digest; fixture selection supplies no
success evidence.

Parent owns Cargo, index, commits, checkpoint rebase, measurements and merge.
The frozen temporary draft has the following parent-run evidence on 2026-10-05:

| Check | Observed draft result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed. |
| `cargo nextest run --workspace` | 505 passed, zero skipped, 2.285 seconds. |
| `cargo nextest run --workspace --profile fuzzy` | Eleven passed, zero skipped, 4.958 seconds. |
| Idle serial focused measurement | 505 passed, zero skipped, 7.512 seconds. |
| Idle serial fuzzy measurement | Eleven passed, zero skipped, 8.431 seconds. |

Latest logs are `/tmp/temper-next-migration/`'s
`smith-native-root-restore-{fmt-final,clippy-final,default-final,fuzzy-final,serial-default,serial-fuzzy}.log`.
Final gates ran from the stable integration worktree with a fresh isolated target;
an earlier cross-worktree shared-target default run reused a different root-memory
artifact and is not acceptance evidence. No test exclusions or budget changes
were used. Frozen restore source SHA-256 is `10ea20a5f82438afd88c38ea7fb425acf1379a94f1b2faf49765edccbb796800`.
The preceding combined-memory checkpoint `1a0bdfe` passed 504 focused / 2.097
seconds and eleven fuzzy / 4.445 seconds; idle serial runs passed 504 / 7.355
seconds and eleven / 8.509 seconds. Its logs remain in
`smith-combined-native-memory-clippy-1.log`, `smith-combined-native-memory-focused-1.log`,
`smith-combined-native-memory-default.log`, `smith-combined-native-memory-fuzzy.log`,
`smith-combined-native-memory-serial-default.log` and `smith-combined-native-memory-serial-fuzzy.log`.
The preceding continuation checkpoint `8010bf4` passed 504 focused / 2.103
seconds and eleven fuzzy / 4.403 seconds; idle serial runs passed 504 / 7.010
seconds and eleven / 8.370 seconds. Its logs remain in
`smith-adapter-continuation-{clippy-5,focused-2,default,fuzzy,serial-default,serial-fuzzy}.log`.
The preceding message-sweep draft passed 499 focused / 2.141 seconds and eleven
fuzzy / 4.478 seconds; idle serial runs passed 499 / 7.104 seconds and eleven /
8.541 seconds. Those logs remain in
`smith-message-sweep-{default,fuzzy,serial-default,serial-fuzzy}.log`.
The earlier memory draft passed 499 focused / 2.047 seconds and ten fuzzy /
4.305 seconds; idle serial runs passed 499 / 6.900 seconds and ten / 7.935
seconds. Those logs remain in `smith-root-memory-{default,fuzzy,serial-default,serial-fuzzy}.log`.
The earlier host-delivery draft passed 498 focused / 1.930 seconds and ten
fuzzy / 4.321 seconds; its idle serial results were 498 focused / 6.509 seconds
and ten fuzzy / 8.016 seconds. That historical evidence remains in
`smith-host-delivery-{default,fuzzy,serial-default,serial-fuzzy}.log`;
its scoped controls and clippy are in `smith-host-delivery-focused-2.log`
and `smith-host-delivery-clippy-final.log`.
The earlier messages checkpoint remains documented by
`smith-client-messages-resume-default-6.log` and
`smith-client-messages-resume-fuzzy-4.log`. Independent receiving/counter
review approved final formatted driver blob
`f08c9c0650ea6d9fcb23484182b47947b56b64fc`; that is scoped driver approval,
not approval of all source or the remaining integration stories.

All 13 canonical Skein packages are pinned coherently to temporary revision
`5bf93a60e05fab568af6a5c2acedcb1ec5456a51`, incorporating reviewed raw history,
`Exchange::at(now, wall)`, shared fake mechanics, raw `World::prepared` adoption
and bounded peer ownership/reclamation. Raw adoption retains one actual Client
without a second preparation or new scheduling/codec authority. The checked
extra price for opt-in bounded peer observations excludes the Client; real service
reclamation follows delivered outputs. The four bounded-peer controls pass in
0.129 seconds, formatting/full all-target clippy pass, the shared-client scope
passes 138 / 0.920 seconds and the full SDK fuzzy diagnostic passes 64 / 17.628
seconds. The mandatory SDK default gate fails at sandbox `io_uring` setup
(`EPERM`): eight failures out of 1,067 tests leave 1,059 unrun. These diagnostics
are in `shared-peer-memory-{fmt-final,clippy-reference,focused-reference,scope,default-reference,fuzzy-diagnostic}.log`.
The earlier raw-adoption controls passed two / 0.005 seconds, shared-client
scope 134 / 0.855 seconds and full fuzzy diagnostic 64 / 15.773 seconds. Those
logs remain in
`shared-raw-adoption-{focused,scope,default,fuzzy-diagnostic}.log`. No full SDK
gate, copied-package deletion or main merge is claimed. Original repository
metadata remains read-only; no original main has moved for this temporary checkpoint.

The final gate remains workspace formatting, all-target clippy, focused and
fuzzy suites on the final exact rebased tip, with idle serial measurements under
the unchanged 15-second/60-second budgets. Passing this draft does not substitute
for the remaining work, final independent review or those final gates.
