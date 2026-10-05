# 05s2a shared LLM ownership

The 05s2 baseline copied provider and OAuth codecs from Temper to preserve
behavior during extraction. That is historical evidence in
[migration-05s2.md](migration-05s2.md), not the final crate boundary.
Smith uses the existing provider-neutral `skein-llm` client. The design
correction is in domain/README.md, section 4, and domain/session.md,
sections 4, 9, 10 and 12.

## Ownership

- Skein owns the actual LLM client, HTTP/SSE, native provider codecs,
  failure translation, opaque replay envelopes and generic scripted peers.
- Smith owns conversation and run policy, application tool schemas and
  decoding, result translation, application scripts and outside expectations.
- The caller configures endpoints and lends credential bytes. Sign-in and
  refresh are the caller's or a shared credential client's; Smith keeps no
  OAuth implementation.

Smith and Skein may shape their entrances together. The Smith adapter must
use the actual shared Client, with one explicit application translation,
whole schemas and input bodies, full usage and replay metadata. Replacing
dependency names while retaining a second client is insufficient.

## Shared increments merged locally

| Increment | Source | Evidence |
|---|---|---|
| Shared Client, scripted peers and replay | Skein b2eeae9 | Gate passed; 1,045 focused / 2.777 s; 64 fuzzy / 27.913 s; 28 capture resources preserved. |
| Adopt an application's prepared Client in the world | Skein 8b83175 | Gate passed; 1,046 focused / 2.878 s; 64 fuzzy / 17.244 s; one actual Client, both dialects. |
| Canonical shared revision and workspace declarations | Smith 38c0590 | Gate passed; 459 focused / 3.060 s; 10 fuzzy / 2.973 s; all nine resolved Skein packages use 8b83175. |
| Neutral document errors and generated replay admission | Skein 4bcfe62 | Gate passed; 1,048 focused / 2.757 s; 64 fuzzy / 16.270 s; exact/one-over escaped metadata controls. |
| Separate native tool call and item identities | Skein 2c725c2 | Gate passed; 1,052 focused / 2.785 s; 64 fuzzy / 16.281 s; exact continuation and counted maximum IDs. |
| Canonical identity revision adoption | Smith 03d1b40 | Gate passed; 459 focused / 1.739 s; 10 fuzzy / 2.962 s; all nine resolved Skein packages use 2c725c2. |

Temper's matching revision adoption, e94e0508, passed its full gate with
2,292 focused tests in 6.043 seconds and 40 fuzzy tests in 24.343 seconds.
The revision is cached locally under the canonical Git source; nothing has
been pushed. Detailed coverage, deliberate codec differences and review
evidence are recorded in the source commit messages and Skein's
`docs/design/fake-llm.md`, section 6.

Both original consumer lockfiles select 2c725c2 at this merged checkpoint. Temper's matching advance,
9e86d6d4, passed formatting, all-target clippy, 2,292 focused tests in
10.339 seconds and 40 fuzzy tests in 26.092 seconds. The shared client preserves native
call and item IDs in distinct bounded fields. The temporary consumer evidence
below continues Smith's adapter work; final replacement acceptance remains open.

## Implementation and evidence still open

- Audit the copied codec tests and fixtures against Skein; move missing
  captures with provenance and preserve bounded unknown replay fields.
- Preserve actual shared terminal and reuse rights in the real consumer;
  gate any further shared repair before advancing consumer revisions.
- Gate the real Smith consumer and its worlds before removing copied provider,
  OAuth and generic peer crates. Record exact source and suite evidence here.

The original design correction, 51fdd24, changed Markdown only. The shared
increments above are merged; final consumer acceptance and copied-crate
removal remain open. Their earlier evidence updates changed Markdown only.


## Reviewed temporary consumer checkpoints

The current session cannot write original repository metadata or Smith/Skein
source. These reviewed checkpoints live under `/tmp/temper-smith-resume/`;
original main branches have not moved. The current temporary consumer pins
all 13 canonical Skein packages coherently to temporary
`5bf93a60e05fab568af6a5c2acedcb1ec5456a51`, including raw argument history,
`Exchange::at(now, wall)`, reviewed fake mechanics, raw `World::prepared`
adoption of one actual Client and bounded peer ownership/reclamation. Provider
codecs remain exclusively Skein-owned on the new runtime path. Copied packages
have no application consumers and remain
until required shared and consumer acceptance permits their coherent removal.

| Increment | Source | Evidence |
|---|---|---|
| Public boundary documentation | Smith edc583e | Reviewed; fmt/clippy pass; 489 focused / 1.937 s; 10 fuzzy / 4.294 s. |
| Original root through actual shared Client | Smith 240838d | Reviewed; fmt/clippy pass; 490 focused / 1.983 s; 10 fuzzy / 4.310 s. |
| Adapter owner, inventory and attestation controls | Smith 9b8630e | Reviewed; fmt/clippy pass; 496 focused / 1.968 s; 10 fuzzy / 4.219 s. |
| Actual submitted delivery through the host | Smith 9d2ec84 | Reviewed; fmt/clippy pass; 498 focused / 1.930 s; 10 fuzzy / 4.321 s. |
| Attained root-entry and caller-copy memory | Smith 8c51dff | Reviewed; fmt/clippy pass; 499 focused / 2.047 s; 10 fuzzy / 4.305 s. |
| Observed bounded message sweep | Smith a54edcb | Reviewed; fmt/clippy pass; 499 focused / 2.141 s; 11 fuzzy / 4.478 s. |
| Native continuation and host-origin oracle | Smith 8010bf4 | Reviewed; fmt/clippy pass; 504 focused / 2.103 s; 11 fuzzy / 4.403 s; 13 scoped controls / 0.075 s. |
| Single-lifecycle combined native memory | Smith base 8010bf4 plus frozen Rust blob 739c102e0a05575d2810b1f718b3480c0be4f33d | Exact source/lock reviewed; fmt/clippy pass; one scoped control / 0.672 s; 504 focused / 2.097 s; 11 fuzzy / 4.445 s. |

The adapter boundary increment adds six positive-first controls over actual native Clients
in both configured wire formats. It preserves matching owners at Completed,
Failed and Cancelled entrances; literal overloaded class, response evidence and
detail; exact schema and owned-result rendering inventories; and an actual
text/Wait/text call's position, name, raw input, kind and unique consumption.
Fresh actual terminals supply each corrupted-owner or attestation case.
Physical Close/Closed and repeated-settlement observations remain separate
from adapter context consumption. No application decoder or Finish policy is
invented by these tests. Idle serial measurements are 496 focused / 6.433 s
and ten fuzzy / 7.958 s, with zero skips and unchanged suite budgets.

The later host control routes a real root delivery submission through the host
kit. It retains the actual receipt in the final Turn after cancellation and
issues the exact final ACK before the last word, holding its real Send through
exit, empty tree and EOF. A separate channel-loss control retains the parent's
operation right until an actual reply, without inventing a post-EOF agent
response. Both controls pass in 0.010 s; idle serial suites pass 498 focused /
6.509 s and ten fuzzy / 8.016 s. Scope and named outside observations are in
[migration-05s4-messages.md](migration-05s4-messages.md).

The later root-memory control attains 12 Messages/six Turns and 28 Messages/14
Turns from real native records, including an actual 8,192-byte person message.
It verifies reservation-inclusive payload admission, independent count/byte
refusals, root-entry allocation peaks and separately measured caller-copy/handoff
peaks. It passes in 0.586 s; idle serial suites pass 499 focused / 6.900 s and
ten fuzzy / 7.935 s, with zero skips and unchanged budgets. That earlier
checkpoint meters root entries and caller copies; the current single-lifecycle
native extension is recorded below. Exact source, observations and logs are in
the messages ledger.

The bounded message sweep observes all 19 required actual classes through
16 pinned seeds/four fixtures and shared replay with the unchanged oracle.
It passes in 0.512 s; idle serial suites pass 499 focused / 7.104 s and eleven
fuzzy / 8.541 s, with zero skips. Scope is the typed shared-fake root world;
native Client cancellation and independent wall-clock jumps are not claimed.

The four native continuation controls use handwritten HTTP/SSE, durable replay
headers/payloads and continued native objects. Actual Codex terminals preserve
encrypted reasoning/extensions, commentary ID/phase, refusal text/ID/stop and
usage 8/3/4/0. Actual Anthropic terminals retain signed/redacted thinking and
all four input/output/cache-read/cache-write counters, 7/9/11/13. Their continued
prompts prove adapter/caller history handoff, without a new root-restore or
combined native memory claim. Original root discovery separately carries a
malformed raw Codex host call to exact Invalid feedback with no host effect,
then one corrected actual host call/result and genuine cancellation/settlement.
Actual failures retain RateLimited/Response, Unavailable/Unknown after a send
and transport loss, and Unavailable/Unsent before Start, with exact details.
These selected cases do not claim full failure-enum coverage.

The host-history oracle retains the actual assistant-message/block origin and
original call bytes from the existing completion flight. It binds only ToolUse
calls decoded as served Host asks and checks their uniquely paired User result;
older historical IDs may repeat. A positive-first regression rejects 13 local
call/receipt corruptions, wrong origins and mismatched relay name/input while
preserving prior chronology, recovery and exact feedback assertions. Idle serial
suites pass 504 focused / 7.010 s and eleven fuzzy / 8.370 s, with zero skips
and unchanged budgets. Named controls and logs are in the messages ledger.

The current memory extension keeps root/session state and all retained caller
history, prompt/Turn copies and configuration live under the same persistent
Meter across one actual Anthropic Wire's prepare, Start, native progress,
translation, Close, actual Closed, drainage and drop. Independent public-field
prices cover configuration, metadata/schema/script/resolution scratch and finite
Vec wrappers. The adapter prices one Client and the shared peer extra price
excludes it. Exact net ownership accounts for the taken Prompt and genuine
transferred terminal; final reclamation leaves all held bytes at zero. The
original 12/six and 28/14 Message/Turn counts, 8,192-byte person input, restored
real Turn, reservation-inclusive exact cap and independent count/byte refusals
remain asserted. Idle serial suites pass 504 / 7.355 s and eleven / 8.509 s,
with zero skips. This measures single lifecycles; simultaneous physical bindings,
Composition maps/traces and the full World outside envelope remain open. It
does not establish full failure-enum or opaque native root Restore coverage.

Skein's reviewed temporary fake mechanics restore query-cap, checked-overflow
and random/scripted generation-scratch coverage. Raw adoption adds two native
controls / 0.005 s. The current bounded-peer increment adds opt-in observation
caps, a checked extra price excluding the Client and real service reclamation.
Formatting/full all-target clippy pass; its four controls pass in 0.129 s,
shared-client scope passes 138 / 0.920 s and full fuzzy diagnostic 64 / 17.628 s.
The mandatory full SDK default gate fails at `io_uring` setup with `EPERM`:
eight failures out of 1,067 tests leave 1,059 unrun. Diagnostics do not replace
that gate. Earlier reviewed temporary checkpoints
are preserved in verified bundles under Temper's `target/next-domain-handoff`.

Remaining replacement acceptance:

- Complete acceptance beyond the selected native continuation and failure cases;
  the four passing controls do not establish every failure classification.
- Measure simultaneous retained physical bindings, Composition maps/traces and
  the full World outside memory envelope beyond the measured single lifecycle.
- Complete the remaining continuation/message-sweep scope; selected native
  evidence does not establish opaque native root Restore or wall-jump coverage.
- Pass final shared/consumer gates and exact independent cleanup review before
  removing copied packages or advancing original main branches.
