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

Both consumer lockfiles now select 2c725c2. Temper's matching advance,
9e86d6d4, passed formatting, all-target clippy, 2,292 focused tests in
10.339 seconds and 40 fuzzy tests in 26.092 seconds. The shared client preserves native
call and item IDs in distinct bounded fields; Smith's actual adapter and its
composed-world gate remain open.

## Implementation and evidence still open

- Audit the copied codec tests and fixtures against Skein; move missing
  captures with provenance and preserve bounded unknown replay fields.
- Preserve actual shared terminal and reuse rights in the real consumer;
  gate any further shared repair before advancing consumer revisions.
- Gate the real Smith consumer and its worlds before removing copied provider,
  OAuth and generic peer crates. Record exact source and suite evidence here.

The original design correction, 51fdd24, changed Markdown only. The shared
increments above are now merged, while the real Smith adapter and copied-crate
removal remain open. This evidence companion changes Markdown only.
