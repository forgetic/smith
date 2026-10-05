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
original main branches have not moved. Canonical Smith dependencies in these
checkpoints use shared revision `e86a7d69`, including raw argument history and
`Exchange::at(now, wall)`. Provider codecs remain exclusively Skein-owned on
the new runtime path. Copied packages have no application consumers and remain
until required shared and consumer acceptance permits their coherent removal.

| Increment | Source | Evidence |
|---|---|---|
| Public boundary documentation | Smith edc583e | Reviewed; fmt/clippy pass; 489 focused / 1.937 s; 10 fuzzy / 4.294 s. |
| Original root through actual shared Client | Smith 240838d | Reviewed; fmt/clippy pass; 490 focused / 1.983 s; 10 fuzzy / 4.310 s. |
| Adapter owner, inventory and attestation controls | This temporary increment | Reviewed; fmt/clippy pass; 496 focused / 1.968 s; 10 fuzzy / 4.219 s. |

The last increment adds six positive-first controls over actual native Clients
in both configured wire formats. It preserves matching owners at Completed,
Failed and Cancelled entrances; literal overloaded class, response evidence and
detail; exact schema and owned-result rendering inventories; and an actual
text/Wait/text call's position, name, raw input, kind and unique consumption.
Fresh actual terminals supply each corrupted-owner or attestation case.
Physical Close/Closed and repeated-settlement observations remain separate
from adapter context consumption. No application decoder or Finish policy is
invented by these tests. Idle serial measurements are 496 focused / 6.433 s
and ten fuzzy / 7.958 s, with zero skips and unchanged suite budgets.

Skein's reviewed temporary fake mechanics restore query-cap, checked-overflow
and random/scripted generation-scratch coverage. Its focused shared-client
scope passes 132 tests / 0.838 s and full fuzzy diagnostic 64 / 18.997 s.
The mandatory full SDK default gate is still blocked at `io_uring` setup with
`EPERM`; diagnostics do not replace that gate. Earlier reviewed temporary checkpoints
are preserved in verified bundles under Temper's `target/next-domain-handoff`.

Remaining replacement acceptance:

- Retain explicit full usage/replay, refusal/reasoning and malformed-call
  no-effect feedback controls, alongside remaining failure classifications.
- Finish actual submitted-delivery host composition, attained root/handoff
  allocation evidence and an observed bounded message schedule sweep.
- Pass final shared/consumer gates and exact independent cleanup review before
  removing copied packages or advancing original main branches.
