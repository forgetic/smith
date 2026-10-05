# 05s4: generic checked delivery

This increment starts from Smith `14cd733`, the gated generic-results
baseline. It implements [domain/run.md, section 8](../design/domain/run.md#8-delivering-a-change)
in the run and composed agent domains. Host tools, messages, parking,
optional workspaces, scalar pricing and V1 contraction remain subsequent
05s4 increments; the process host and live channel remain 05s6 and 05s5.

## Boundaries and ownership

Generic delivery replaces the host's push result with delivered, nothing,
refused, failed and stale. Constructors seal nonempty unique per-directory
receipts, named refusals, relative marker paths and diagnostics. Receiving
runs separately validate evidence against their admitted writable mounts.
Failure tails keep at most 512 bytes and count dropped bytes. Admission
bounds delivery-capable charters at 64 mounted directories; each owned
receipt container and payload copy is included in memory accounting.

`deliver` is granted separately from permission to finish with Change,
and only main receives it. Every discovered convention check runs before
mid-run or final delivery. Report, Verdict and declared Failure finish
without checks or delivery. Generic Change fields remain opaque to Smith;
the host chooses their meaning. The previous contract's selectable check
field is removed.

Before submission, cancellation or expiry can abort a check. A check's
actual deadline terminal is handled before timers, following
programming-model.md, section 2. After submission the run waits for the
host's actual terminal, including while winding down. The host's deadline
bounds its operation; transport may convey its terminal later. Caller-only
withdrawal or expiry does not decide the run's independent stop.

Final Change delivery that lands remains accepted during shutdown. An
ordinary mid-run landing supplies actual receipts and continues. A landing
while run shutdown was already decided produces a separate Delivered
answer with the durable call name, receipts, actual typed stop and spend.
It does not fabricate an LLM-declared result. Duplicate or stale callback
generations do not create another delivery.

Durable names retain the accepted completion sequence and original
assistant block ordinal separately from callback tokens and provider IDs.
V2 transcript prefixes contribute to the sequence; checked exhaustion
refuses effects before submission. The host scopes names by the logical
run across restart. This establishes the naming boundary without claiming
that complete run restart or answered-after-transcript integration is
already implemented.

## Evidence

The run units cover all terminal forms, permission and ordinal admission,
pre-submission aborts, actual terminal races, caller-only expiry and
withdrawal, and independent later shutdown. The composed agent stories
read and edit a real fake checkout, run real checks, deliver mid-run,
continue to Report or edit again and finish Change. They cover marker
repair, shutdown landing, actual host timeout, durable origins and repeated
provider IDs. The independent referee requires landing evidence on the
correct final answer, distinguishes ordinary from interrupted delivery,
and accepts a mid-run landing followed by a separate final Change.
Positive controls precede each corrupted-history check.

Maximum receipt and interrupted-answer drivers include owned containers
and payload copies against the declared bounds. Replay, facts independence
and existing fuzzy sweeps remain. Shared schedules, ledgers, replay and
heap accounting are reused directly from `skein_world::domain`; no second
harness or allocator is introduced.

Independent production, documentation and fixture review is clear.
Parent-owned idle serial measurements passed: 394 focused tests in
4.429 seconds and nine fuzzy tests in 6.229 seconds, with no skips.
World shares are recorded in [workflow.md](workflow.md). The final tip
still requires all four gate checks before main moves. Canonical Skein
remains locked to `5e52dd9cd8793094bf49c8bf75f9cffac027814f`, matching
Temper; its unpublished-revision portability limitation remains in
[README.md](../../README.md).
