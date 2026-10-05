# 05s6 host-domain migration

Author source: Temper `19735a066cd485ca9d39e70ffb8ca8bd902ad55a`,
worker-agent behavior `e2a6a719` (05d); the frozen-agent rename is a separate
change. The original author base was Smith main `2a621a5`, including the complete
reviewed DELIVERY seam (the results predecessor was `14cd733`). Integration is
rebased onto main `9b9b7c6`, preserving the subsequently merged host-tools
increment and its JSON dependency lock entries. No Temper dependency is
introduced. This extraction implements the full typed V2 host contract;
framing/hello negotiation remains the protocol increment, and durable policy,
credential bytes, local host and real process adapters remain their named owners.

The source supervision, channel metadata and boundary behavior are implemented
in `crates/smith-host-domain/src/{domain,boundary,channel,limits,facts}.rs`.
`delivery.rs` preserves the complete Smith generic sealed delivery shape.
Modules and every public entry/type/variant/field cite `domain/host.md` and the
applicable Skein foundation documents. Own-enum matches are exhaustive; the kit
uses bounded Skein containers, checked capacity arithmetic and injected clocks.

## Retained inventory and boundary choices

| Owner | Bounded retained state | Released by |
| --- | --- | --- |
| Kit before Started | opaque charter/transcript/post-transcript answers, directory conflicts and grants | moving Start to first lower Send |
| Caller/IO | every emitted payload, including Start coexisting with queued pre-read messages | actual lower terminal/parent consumption |
| Kit channel | one read, one send kind, ordered messages, sent names and current read watermark; two Busy rights | actual terminals / known sent-prefix fence |
| Kit call ledger | callback, stable completion/block name, kind, deadline, Parent/Queued/Sending and withdrawal flag | actual parent answer plus reply Send terminal, or parent answer after channel loss |
| Kit proof | one additional maximum sealed successful landing, stable name and downlink issue order | last word; latest successful proof replaces previous |
| Parent + kit | parent owns opaque turn payload; kit owns exact number/byte commitment metadata and ACK send stage | exact parent ACK plus its actual Send terminal |
| Kit credential names | per-account latest queued grant and greatest actually emitted generation | physical agent cleanup |
| Kit supervision | process exit, empty-tree tail, EOF, signal rights, progress/long/wall/grace clocks | every lower and parent right, then Gone |
| Kit diagnostics | bounded content-free facts and saturating loss count | parent drain; dropping changes no behavior |

Only one submitted delivery is active per agent. Each real delivery receives a
complete actual terminal even after withdrawal, call deadline, fault, EOF and
empty tree. A queued/Parent-stage delivery cannot be abandoned by a final
Answer; an already-issued Send may have reached the agent before its terminal.
Reply capacity must fit all sealed receipts at construction, so TooLarge never
replaces actual landed evidence. Generic Unavailable/Withdrawn transport
terminals do not imply absence of a durable effect; the parent owns immutable
same-name retry decisions. The kit scopes stable names with logical_run and
retains active reservations, without an invented durable decision cache.

Parent Stop is not proof of a private agent stop or Cancel receipt. Production
checks matching real landing evidence and its known send chronology; the
independent oracle additionally sees the scripted agent's actual stop while a
specific operation is pending. Thus it requires complete evidence for an actual
interrupted landing and rejects omitted, invented, changed-name/receipt,
changed-cause and later-stop classifications. An earlier ordinary landing plus
a later Stop remains an ordinary answer. Complete stop payloads are preserved:
Model's six fault kinds, Budget's Turns/Input/Output/CacheRead/CacheWrite/Time,
Policy's exact nudge/rejection counts, Cancelled and Stale.

Explicit parent Stop owns its shutdown: later rule breaches terminate and
retain a diagnostic Fact::Faulted without manufacturing a parent run fault;
the first valid last word is still heard. Wall-owned shutdown still reports a
breach and preserves the original wall cause when the agent reports Cancelled.
Terminating/Killing discard ordinary working records while retaining final
Answer and all actual lower/parent terminals. These are source-law controls,
not behavior narrowed for easier tests.

Turns start at one, spend never falls, and final count/spend are fenced. Exact
ACK cannot release another row, and ACK transmission metadata remains reserved
until Sent/Unsent. Maximum next-turn count/byte credit is reserved before Read.
Messages must have unique opaque names for the active logical run; bounded kit
metadata rejects outstanding/current-watermark reuse and unknown/regressing
fences, while the parent owns the older namespace history. Credential rejection
may cross a queued refresh: nonzero older emitted generations remain notices;
unknown/future generations fail. Precise skipped historical generation names
are the parent's record, not unbounded kit state.

`worst_case` separately prices slab/map/queue storage, retained start-or-queued
payloads, additional sealed proof, retained tails and the bounded withdrawal
snapshot. Start/queue exclusion follows pre-Started refusal and movement into
emitted Send. A proof replacement's transient clone fits the Parent-stage call's
unused queued-reply reserve. Memory drivers retain a maximum actual caller-owned
Start alongside the entire pre-read message queue, and count both owners;
small other capacities prevent an unrelated proof reserve from concealing that
ownership. Process-exited + empty-tree + EOF + all lower/call/turn rights gate
Gone, and slab reclaim stays at the iteration boundary.

## World and source-story inventory

`tests/host` uses Skein's Stage, Schedule, Ledger, Trace, replay helper, Referee
and Counting/Meter. It defines only Smith-specific scripted boundary observation
and expectations. No second scheduler, allocator or generic referee exists.
The focused histories are full V2, with opaque start bytes and no workspace,
numbered turns, exact ACK, spend/read fences, generic effects and stable delivery
names. Source V1 compatibility stories map to V2 admission/typed-boundary
rejection, and old push title/body stories map to generic opaque delivery fields
and sealed full terminals. The physical proof stories distinguish process exit
from delayed empty tree and EOF and settle every Send/Read/Signal right.

The table maps every exact source story to its V2 assertion group. Unprefixed
names are in `tests/host/tests/simulation.rs`; `unit`, `delivery unit`, `memory`
and `fuzzy` identify their respective test modules. Many source stories share one
V2 group with separate positive and negative histories, rather than duplicating
legacy enum dispatch.

| Temper source story | Smith V2 assertion group |
| --- | --- |
| `a_spawn_beyond_the_slots_is_refused_as_busy` | `unit: a_spawn_beyond_the_slots_is_refused_as_busy_without_displacing_the_active_owner` |
| `a_spawn_beyond_the_limits_is_refused_as_invalid` | `admission_refusals_preserve_separate_process_rights` |
| `a_spawned_process_starts_its_run_with_the_start_message_first` | `process_started_precedes_agent_admitted_and_start_is_first` |
| `a_process_that_cannot_be_spawned_is_gone_with_the_tail_of_its_detail` | `admission_refusals_preserve_separate_process_rights` |
| `a_live_run_calls_tells_waits_and_finishes` | `a_live_run_calls_tells_waits_and_finishes` |
| `a_run_may_fail_as_it_reports_it` | `a_run_may_fail_as_it_reports_it` |
| `a_call_beyond_the_runs_limit_is_answered_busy_and_a_second_pauses_reading` | `busy_overflow_reserves_names_and_pauses_reads_until_actual_send_terminal` |
| `a_call_reusing_a_name_in_flight_breaks_the_rules` | `durable_call_names_and_callback_owners_are_independently_fenced` |
| `an_overflow_call_reusing_a_name_awaiting_its_busy_answer_breaks_the_rules` | `an_overflow_name_is_fenced_until_its_busy_answer_terminal_then_reusable` |
| `an_overflow_calls_name_is_reusable_once_its_busy_answer_goes_down` | `an_overflow_name_is_fenced_until_its_busy_answer_terminal_then_reusable` |
| `payloads_beyond_the_limits_break_the_rules` | `payloads_beyond_the_limits_break_the_rules` |
| `a_malformed_message_breaks_the_rules_and_ends_reading` | `malformed_hangup_and_unsent_have_actual_lower_terminals` |
| `a_hangup_before_the_finish_is_an_exit_without_answering` | `malformed_hangup_and_unsent_have_actual_lower_terminals` |
| `inbound_events_go_down_in_order_or_are_bounced` | `inbound_messages_are_ordered_bounded_and_named_opaquely` |
| `an_answer_too_large_goes_down_as_such` | `an_oversized_generic_answer_is_too_large_but_actual_delivery_always_fits` |
| `a_stop_cancels_the_run_behind_what_waits_and_its_finish_is_still_heard` | `explicit_stop_cancels_once_and_does_not_restart_grace` |
| `silence_past_the_no_progress_deadline_stops_the_run` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `the_clock_pauses_while_a_call_waits_for_the_client` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `the_clock_pauses_while_the_run_waits_having_read_every_event` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `a_long_operation_holds_the_clock_until_its_deadline` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `the_wall_time_cancels_a_run_and_its_finish_stands` | `wall_clock_never_pauses_and_cancelled_spend_is_preserved` |
| `a_run_the_wall_time_cancelled_that_says_it_was_cancelled_is_faulted_for_it` | `wall_clock_never_pauses_and_cancelled_spend_is_preserved` |
| `a_run_the_wall_time_cancelled_that_exited_and_says_it_was_cancelled_is_faulted_for_it` | `wall_shutdown_hangup_exit_and_breach_report_the_actual_original_cause` |
| `a_run_its_client_stopped_that_says_it_was_cancelled_finishes_so` | `explicit_stop_cancels_once_and_does_not_restart_grace` |
| `a_run_past_its_wall_time_that_does_not_finish_is_faulted_past_the_grace` | `wall_clock_never_pauses_and_cancelled_spend_is_preserved` |
| `a_run_past_its_wall_time_that_hangs_up_is_faulted_at_once` | `wall_shutdown_hangup_exit_and_breach_report_the_actual_original_cause` |
| `a_stop_past_the_wall_time_takes_the_fault_back` | `stop_and_answer_during_wall_or_draining_preserve_original_shutdown_deadline` |
| `a_breach_while_the_wall_time_winds_a_run_down_is_told` | `wall_shutdown_hangup_exit_and_breach_report_the_actual_original_cause` |
| `a_withdrawn_call_is_still_answered_once` | `a_withdrawn_call_keeps_its_actual_once_only_answer_right` |
| `a_call_withdrawn_twice_before_its_answer_breaks_the_rules` | `a_withdrawn_call_keeps_its_actual_once_only_answer_right` |
| `a_long_operation_is_bounded_and_ends_when_done` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `a_finish_read_after_the_exit_is_heard` | `buffered_answer_after_exit_is_heard_before_tree_empty_and_gone` |
| `an_exit_without_a_finish_is_a_fault_at_the_hangup_or_the_grace` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `a_run_that_stops_reading_its_channel_is_drained` | `malformed_hangup_and_unsent_have_actual_lower_terminals` |
| `a_draining_agent_stopped_by_its_client_is_no_longer_faulted` | `stop_and_answer_during_wall_or_draining_preserve_original_shutdown_deadline` |
| `a_malformed_message_while_draining_is_a_fault_while_live` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `a_cancelled_run_still_calls_and_tells_until_it_finishes` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `a_cancelled_run_that_breaks_the_rules_is_terminated_untold` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `a_cancelled_run_past_the_grace_is_terminated_then_killed` | `explicit_stop_cancels_once_and_does_not_restart_grace` |
| `a_finish_after_a_fault_is_dropped_while_terminating_and_killing` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `a_cancelled_run_that_hangs_up_or_exits_is_waited_for` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `anything_after_the_finish_breaks_the_rules` | `trailing_records_terminate_without_fabricating_a_second_run_answer` |
| `a_finished_run_that_outstays_the_grace_is_terminated` | `an_agent_has_gone_only_after_every_actual_io_right_even_after_answer` |
| `an_agent_has_gone_only_once_nothing_asked_of_io_is_in_flight` | `an_agent_has_gone_only_after_every_actual_io_right_even_after_answer` |
| `a_terminated_tree_reads_its_channel_to_the_end` | `an_agent_has_gone_only_after_every_actual_io_right_even_after_answer` |
| `the_worst_case_is_bounded_or_refused` | `memory: checked_capacity_arithmetic_refuses_unrepresentable_or_incompatible_caps` |
| `facts_beyond_their_room_are_dropped_and_counted` | `facts_change_nothing_when_none_are_kept` |
| `push_diagnostics_keep_the_allowed_tail_and_count_omitted_bytes` | `delivery unit: every_sealed_ownership_cap_can_be_filled_exactly` |
| `waiting_names_are_opaque_and_bounces_echo_the_record` | `inbound_messages_are_ordered_bounded_and_named_opaquely` |
| `grant_refreshes_coalesce_without_displacing_inbound_or_answers` | `credential_refresh_coalescing_preserves_other_traffic_and_actual_answer_order` |
| `finding_reused_acknowledged_name_leaves_the_second_delivery_unacknowledged` | `old_acknowledged_name_history_is_a_parent_namespace_promise` |
| `v2_turn_pauses_reads_and_watchdog_until_parent_credit` | `exact_ack_metadata_preserves_parent_payloads_and_shutdown_rights` |
| `v2_finish_parks_without_snapshot_and_reports_spend` | `buffered_answer_after_exit_is_heard_before_tree_empty_and_gone` |
| `v2_turn_read_watermark_releases_delivered_message_credit` | `read_watermarks_cover_only_sent_names_and_release_exact_prefix` |
| `legacy_spawn_refuses_new_turn_and_v2_spawn_refuses_legacy_finish` | `admission_refusal_and_working_traffic_are_distinct` |
| `v2_push_bounds_both_title_and_body_before_forwarding` | `payloads_beyond_the_limits_break_the_rules` |
| `a_calm_world_finishes_every_run_and_settles` | `a_live_run_calls_tells_waits_and_finishes` |
| `spawns_beyond_the_slots_are_refused_as_busy` | `unit: a_spawn_beyond_the_slots_is_refused_as_busy_without_displacing_the_active_owner` |
| `spawns_beyond_the_limits_are_refused_as_invalid` | `admission_refusals_preserve_separate_process_rights` |
| `processes_that_cannot_be_spawned_are_gone_unstarted` | `admission_refusals_preserve_separate_process_rights` |
| `a_cancelled_run_winds_down_and_its_finish_is_heard` | `explicit_stop_cancels_once_and_does_not_restart_grace` |
| `an_ignored_cancel_is_terminated_then_killed` | `explicit_stop_cancels_once_and_does_not_restart_grace` |
| `hung_runs_are_stopped_by_the_watchdog` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `a_wait_that_crosses_an_inbound_event_does_not_pause_the_watchdog` | `a_wait_crossing_an_unread_or_queued_message_does_not_pause` |
| `runs_that_close_their_output_hang_up_live_or_cancelled` | `malformed_hangup_and_unsent_have_actual_lower_terminals` |
| `withdrawn_calls_are_answered_once` | `a_withdrawn_call_keeps_its_actual_once_only_answer_right` |
| `runs_that_keep_making_progress_are_stopped_only_by_their_wall_time` | `continual_actual_progress_reaches_only_the_independent_wall_deadline` |
| `an_agent_that_exits_without_a_word_has_failed` | `cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault` |
| `an_agent_that_stops_reading_its_channel_is_drained` | `malformed_hangup_and_unsent_have_actual_lower_terminals` |
| `every_breach_of_the_channels_rules_is_caught` | `payloads_beyond_the_limits_break_the_rules` |
| `slow_answers_and_waits_pause_the_watchdog` | `the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs` |
| `children_that_outlive_their_agent_are_terminated_then_killed` | `surviving_descendants_keep_reap_right_through_terminate_and_kill` |
| `a_seed_replays_to_the_same_run` | `a_seed_replays_the_same_v2_boundary_history` |
| `facts_change_nothing_when_none_are_kept` | `facts_change_nothing_when_none_are_kept` |
| `an_agent_child_domain_with_every_slot_full_stays_within_its_worst_case` | `memory: maximum_v2_starts_and_full_queued_replies_fit_every_slot` |
| `every_entry_point_stays_within_the_worst_case` | `memory: proof_replacement_clones_and_shutdown_rights_stay_priced` |
| `v2_full_transcripts_start_paths_and_queued_conflict_replies_fit_the_child_bound` | `memory: maximum_v2_starts_and_full_queued_replies_fit_every_slot` |
| `random_worlds_settle_and_reach_every_ending` | `fuzzy: random_worlds_settle_and_reach_every_ending` |

Exact bounded unspawned tails are retained and asserted at the world boundary.
Recurring actual scheduled Fact records keep the watchdog alive until the
independent wall deadline, and delayed descendant Reap remains outstanding
through both Terminate and Kill before real EOF/tree/signal terminals settle.

New V2 groups add exact turn count/spend/ACK/fence negatives; separate Started
and Admitted; queued refresh versus actually issued generation races; all five
full delivery terminals including both forms of refusal; real landing proof
surviving its CallEntry; pending/queued premature final-answer rejection and
issued-response positive; actual parent delivery right surviving process/tree/
EOF; and the independent actual-agent interrupted chronology oracle.

Four measured drivers cover maximum starts/paths/answered records and queued
replies in every slot, caller-owned Start plus full pre-read message queues,
maximum proof replacement plus late terminal/shutdown scratch, and checked
arithmetic/incompatible receipt caps. The fuzzy world runs 240 deterministic
seeds over configured pre-admission, ordinary, failure, cancellation, broken
channel, silence, wall and exit-draining paths, including actual parent
terminals after tree/EOF and withdrawn delivery rights. It asserts all configured
endings are reached and every world settles its independent terminal ledger.

## Validation evidence

The original author base was Smith `2a621a5`; the full extraction was rebased
onto integration main `9b9b7c6`, preserving the merged host-tools seam.
Independent review approved the complete production boundary, supervision,
call/turn/credential rights, landing proof, ownership bounds, outside oracle and
all 78 named source-story mappings. Corrections preserve caller-owned versus
wall-owned shutdown, opaque zero message names, exact bounded failure tails,
continual progress to the wall deadline and surviving descendants through both
signal stages. The oracle checks actual forwarded answers, rather than fixtures.

Parent checks passed scoped all-target clippy, 48 host focused tests in 0.043
seconds and one host fuzzy test in 0.018 seconds. On the rebased tree, full
workspace all-target clippy and formatting also passed. Idle serial workspace
measurements passed 459 focused tests in 5.001 seconds and ten fuzzy tests in
6.569 seconds, with no skips; world shares are in [workflow.md](workflow.md).

The exact committed source `eb46ecc` passed all four workflow gates before its
fast-forward to main: formatting, workspace all-target clippy, 459 focused tests
in 1.606 seconds and ten fuzzy tests in 2.593 seconds, with no skips. No wire codec,
compatibility mode, real IO process runner or local host is claimed by this
typed-domain extraction. Final agent policy and provider protocol remain their
own increments; the host neither invents private stops nor durable decisions.
