# 05s4: declared opaque host tools

This increment starts from Smith `2a621a5`, the gated generic-results and
checked-delivery baseline. It implements [domain/run.md, sections 5.1 and
5.2](../design/domain/run.md#52-host-tools) through the run and composed agent
boundaries, following the effect scheduler in
[domain/session.md, section 5](../design/domain/session.md#5-tools-in-a-session).
The named copy source remains Temper `25ac2ad`; its closed forge and outlet
grants are replaced by host declarations. Provider schema adapters and the
live channel remain 05s5. The process host is 05s6. Complete run transcript
restart, messages, optional workspaces/conventions and scalar pricing remain
later increments.

## Boundaries and ownership

`HostTool` carries bounded name, description and schema bytes, a declared
read/write effect and a positive relay timeout. Admission checks the declaration
count and complete aggregate charter ownership, nonempty fields, unique names,
reserved Smith names and receiving retry bounds before IO. Text is
protocol-attested UTF-8; the domain neither parses encoding nor interprets
schema or host policy. Main receives admitted declarations; children receive
none. The root checks each decoded name and effect before scheduling the
completion's calls. Adjacent host reads may run together; writes run alone.

The root's provider-neutral prompt carries whole declarations. Its protocol face
attests one complete JSON object before constructing `HostInput`; the domain
checks only the fixed byte cap and object exterior. A relay preserves the exact
input bytes, including whitespace and escapes, along with tool and effect.
Results and errors preserve the host's bounded text and error bit.

`Start.worker` is the parent's stable logical run scope across recovery and
restarted activations, distinct from live run and callback tokens. Durable
`CallName` retains accepted completion sequence and original assistant-block
position. `RelayName` adds the current callback generation and bounded attempt.
The host decides a logical scope/name once and replays its exact first record
when that operation is asked again. Full restart integration is not claimed by
this typed naming boundary.

Each relay deadline is the minimum of declared timeout, the receiving timeout
ceiling and remaining caller/run time. Busy is predecision; Lost or Withdrawn
is an actual settled attempt whose outcome remains unknown. Recovery waits for
that terminal and bounded positive backoff, preserves all immutable input, and
stops at the receiving attempt cap. There is never a duplicate live attempt.
Withdrawal requests settlement and retains the original terminal right.

Caller expiry and run shutdown forbid fresh recovery. Actual Answered wins
after withdrawal or shutdown and is conveyed exactly once. Unresolved Lost or
Withdrawn evidence remains unknown through a later Busy: exhausted or stopped
recovery cannot claim that nothing happened. Pure Busy exhaustion remains Busy.
The existing submitted-delivery ownership and cancellation behavior are
unchanged.

## Evidence

Run units cover declaration admission, name/effect refusal, Busy/Lost recovery,
stale callback refusal, exact error feedback, sticky uncertainty through Busy
and attempt exhaustion, caller withdrawal/expiry, relay timeout and retained
actual answers during run shutdown. The root unit drives concurrent reads,
exclusive write sequencing, exact error feedback and effect mismatch refusal
before relay.

The composed agent world uses the real fake LLM domain and real root, run,
session and tools domains. Its protocol translator uses the real `skein-json`
tokenizer to attest complete object syntax; malformed interiors, multiple
documents, arrays and invalid UTF-8 are refused there. Valid opaque input is
forwarded without extracting or normalizing fields. Host stories cover Busy,
a lost committed answer replayed from one host-owned record, exhausted
uncertainty, declared-timeout withdrawal followed by recovery, and an actual
answer arriving after withdrawal.

The independent relay history judge reads public provider call IDs,
submissions, withdrawals, actual terminals, subsequent prompts and actual
shutdown observations. It requires immutable logical scope/name/tool/effect/
input, distinct sequential callback attempts, no live duplicate, and actual
terminal before recovery. Normal continuation owes one paired exact result,
including its error bit. Actual Answered forbids another attempt before or
after feedback. Positive closed controls precede corrupted histories for live
duplicates, terminal/recovery chronology, changed scope, fabricated or erased
known/unknown feedback, missing feedback and post-answer/post-shutdown retry.
A translated-restart history varies live callback owner while preserving the
logical worker scope and immutable operation.

The real cancellation control derives a cut from a baseline's observed relay
admission, then cancels the real composed run while that relay is live. It
requires admission before actual Cancel before one actual Answered terminal,
one recorded host decision, no retry or next LLM prompt, and the final
Cancelled run answer after settlement. A claimed shutdown without an outside
observation, or one that abandons a live relay, fails the history judge.

Counted memory includes complete declarations in the charter and root peer,
one immutable input per retained logical call, fixed attempt/timeout state,
receiving answer caps and retained root feedback copies. The focused counting
allocator driver reaches a complete aggregate declaration, a 65,536-byte
attested input, a 65,536-byte actual answer and Lost then Busy recovery through
real run entry points. It measures each transition, drops receiver-owned
outputs before checking the run's bound, and proves terminal reclamation.
Replay compares real observed boundaries and final answers. Shared schedules,
ledgers, replay and counting allocator are reused from `skein_world::domain`.

## Validation

Parent-owned precommit checks passed workspace formatting and all-target clippy,
411 focused tests in 1.684 seconds and nine fuzzy tests in 3.551 seconds,
with no skips. Idle serial measurements passed 411 focused tests in 4.764
seconds and nine fuzzy tests in 15.212 seconds; affected world shares are in
[workflow.md](workflow.md). These measurements include the actual composed
cancellation control and cap-reaching memory driver, without raising budgets.

The parent reruns all four workflow gates on the committed source before main
moves and records that source's exact counts/times in the evidence companion.
Independent static review approved the complete implementation, docs, outside
judge and ownership bounds. No channel/provider schema adapter, complete run
restart or local-host completion is claimed by this increment.
