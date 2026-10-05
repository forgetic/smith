# 05s4: messages, waiting, parking and concrete resume

This is the implementation ledger for the approved messages increment. Source
is in progress; the early compiler checkpoint is
`49d15601c4346251eb3c514a4ba68058b9da6f83`, rebased onto Smith `45a2eea`.
The original author base was `51fdd24`. The frozen temporary host-delivery draft
passes formatting, full workspace/all-target clippy, 498 focused tests and ten
fuzzy tests, with idle serial measurements recorded below. Its root/Wire source,
host-delivery composition, adapter ownership/inventory/attestation controls and
public documentation backfill have independent scoped approval. Attained memory,
observed message sweep and further adapter evidence remain pending. These
results do not claim full replacement acceptance, a main merge, migration
completion or legacy adapter removal.

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
this is not an attained allocation measurement. Arbitrary application Finish
decoding, remaining receiving continuation
controls, full memory attainment and a bounded actual message schedule sweep
remain pending. The passing gates cover the scoped increment; final replacement
acceptance remains outstanding.

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
separate actual Turn, prompt, answer or receipt observations. This driver does
not replace the pending attained root/handoff memory story or message sweep.

## Remaining checkpoint work and validation

The current checkpoint retains the approved actual wire/root binding composition,
submitted-delivery extension of live host composition, adapter boundary controls
and public documentation backfill. The attained many-tiny Message/Turn plus
cap-filled payload transit memory driver and bounded randomized message race
sweep with observed classification counts remain outstanding. All named source distinctions and
existing tests remain in scope; no passing subset replaces these requirements. New public items continue
to require full module/type/variant/field/entry documentation and independent
subset review alongside later integration work.

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
and exact `overloaded_error` detail under the matching context. Remaining continuation evidence includes
broader failure-class/evidence/detail cases, replay, refusal/reasoning, all four
usage counters and malformed-call no-effect behavior.
The compact literal schema-extension observation now supplements parsed schema
equality; it does not discharge these other boundary requirements.

The approved live-host extension submits an actual delivery through root and
host entrances and retains its parent operation right independently from Turn
commitment and ACK Send. Only the actual parent answer consumes that operation
right; its sealed receipt agrees with the concrete Turn and final Delivered
proof before cleanup settles. Its separate channel-loss control retains the
parent right through actual withdrawal, process exit, empty tree and EOF, then
consumes the parent's actual terminal without inventing an agent response.

The remaining memory driver must attain independent Message and Turn array
caps using many tiny records, alongside a cap-filled payload; selected Start
history, restored input, concrete emitted Turn, rewritten provider input,
translated terminal and caller-held observations are separately owned prices.
The bounded message sweep must count actual observed endings, read fences,
bounces and terminal races, assert that its required classes occurred, and
replay each seed through the shared kit. A seed's chosen scenario is not
evidence that its requested outcome occurred.

Parent owns Cargo, index, commits, checkpoint rebase, measurements and merge.
The frozen temporary draft has the following parent-run evidence on 2026-10-05:

| Check | Observed draft result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed. |
| `cargo nextest run --workspace` | 498 passed, zero skipped, 1.930 seconds. |
| `cargo nextest run --workspace --profile fuzzy` | Ten passed, zero skipped, 4.321 seconds. |
| Idle serial focused measurement | 498 passed in 6.509 seconds. |
| Idle serial fuzzy measurement | Ten passed in 8.016 seconds. |

Runtime logs are `/tmp/temper-next-migration/`'s
`smith-host-delivery-default.log`, `smith-host-delivery-fuzzy.log`,
`smith-host-delivery-serial-default.log` and `smith-host-delivery-serial-fuzzy.log`.
Scoped host controls are recorded in `smith-host-delivery-focused-2.log`; final
all-target clippy is recorded in `smith-host-delivery-clippy-final.log`.
The earlier messages checkpoint remains documented by
`smith-client-messages-resume-default-6.log` and
`smith-client-messages-resume-fuzzy-4.log`. Independent receiving/counter
review approved final formatted driver blob
`f08c9c0650ea6d9fcb23484182b47947b56b64fc`; that is scoped driver approval,
not approval of all source or the remaining integration stories.

Smith's canonical Skein dependency is pinned to temporary shared-clock revision
`e86a7d69fcee41028337ed1288e5758015921faa`, including reviewed raw-history work.
The shared `Exchange::at(now, wall)` entrance synchronizes the actual
root/Wire composition. Separately reviewed shared fake mechanics are at
temporary SDK revision `3bdc669bf512e2f566cb432b52cd55f1f834e28c`: its expanded
shared-client scope passed 132 tests in 0.838 seconds and its full SDK fuzzy
diagnostic passed 64 tests in 18.997 seconds. Those mechanics changes are
tests/docs only; Smith remains pinned to the clock revision. The full SDK
default gate remains blocked by sandbox `io_uring` permission (`EPERM`). No
full SDK gate or main merge is claimed. Original repository metadata remains
read-only; no original main has moved for this temporary checkpoint.

The final gate remains workspace formatting, all-target clippy, focused and
fuzzy suites on the final exact rebased tip, with idle serial measurements under
the unchanged 15-second/60-second budgets. Passing this draft does not substitute
for the remaining work, final independent review or those final gates.
