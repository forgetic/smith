# 05s4: messages, waiting, parking and concrete resume

This is the implementation ledger for the approved messages increment. Source
is in progress; the early compiler checkpoint is
`49d15601c4346251eb3c514a4ba68058b9da6f83`, rebased onto Smith `45a2eea`.
The original author base was `51fdd24`. The later frozen temporary draft passes
formatting, full workspace/all-target clippy, 489 focused tests and ten fuzzy
tests, with idle serial measurements recorded below. These are draft results,
not a final rebased-tip gate or full combined-source approval. Actual root/wire
composition, further composed ownership evidence, final public documentation
and independent final review remain pending. This checkpoint is not a main
merge, migration completion or legacy adapter removal.

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
codec claim. Actual Client binding integration and final copy removal belong to
the shared-client replacement and must retain each physical binding through
Reusable, explicit Close and actual Closed, even when a root owner is reused.

## Written focused evidence

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
translator-derived expected object supplies this oracle. These are prepared
Client boundary stories; actual root/wire composition remains pending.

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

The early checkpoint intentionally precedes completion of the actual wire/root
binding composition, the submitted-delivery extension of live host composition,
attained many-tiny Message/Turn plus cap-filled payload transit memory driver,
and bounded randomized message race sweep with observed classification counts.
All named source distinctions and existing tests remain in scope; no passing
subset replaces these requirements. Full public module/type/variant/field/entry
documentation and subset review remain pending alongside the integration work.

The wire composition must consume the receiving fields of the actual root
`Complete`, rather than reconstructing them from a token allowance. Its active
logical-owner lookup is distinct from its retained physical bindings: one
Client terminal ends the root request, while that physical binding remains
owned through `Reusable`, an explicit `Close` and actual `Closed`. A later
request carrying the same logical owner cannot redirect an older binding's
cleanup. Actual byte-peer queries and real translated terminals, rather than a
second typed response, supply the composition's outside observations. The
first required root story exercises a declared opaque host call, its exact
continued result and parent cancellation; it makes no unsupported application
`Finish` decoder claim. All participating stages receive the same injected
monotonic and wall clocks through the shared world's clock entrance.

The live-host extension must submit an actual delivery through the root and
host entrances and retain its parent operation right independently from Turn
commitment, ACK Send, process exit, EOF and empty-tree proofs. A withdrawal is
an observation, not an operation terminal. Only the actual parent answer can
consume that delivery right; its sealed receipt and subsequent concrete Turn
must agree before durable ACK and physical cleanup settle.

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
| `cargo nextest run --workspace` | 489 passed, zero skipped, 1.895 seconds. |
| `cargo nextest run --workspace --profile fuzzy` | Ten passed, zero skipped, 4.362 seconds. |
| Idle serial focused measurement | 489 passed in 6.292 seconds. |
| Idle serial fuzzy measurement | Ten passed in 7.926 seconds. |

Runtime logs are `/tmp/temper-next-migration/`'s
`smith-client-messages-resume-default-6.log`,
`smith-client-messages-resume-fuzzy-4.log`,
`smith-client-messages-resume-serial-default.log` and
`smith-client-messages-resume-serial-fuzzy.log`. Independent receiving/counter
review approved final formatted driver blob
`f08c9c0650ea6d9fcb23484182b47947b56b64fc`; that is scoped driver approval,
not approval of all source or the remaining integration stories.

Smith's canonical Skein dependency is pinned to temporary shared-clock revision
`e86a7d69fcee41028337ed1288e5758015921faa`, including reviewed raw-history work.
The shared `Exchange::at(now, wall)` entrance is available for the pending
root/wire composition. Separately reviewed shared fake mechanics are at
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
