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

## Implementation and evidence still open

- Audit the copied codec tests and fixtures against Skein; move missing
  captures with provenance and preserve bounded unknown replay fields.
- Gate the shared client and peers first, then adopt one canonical Skein
  revision in Smith and Temper; preserve actual terminal and reuse rights.
- Gate the real Smith consumer and its worlds before removing copied provider,
  OAuth and generic peer crates. Record exact source and suite evidence here.

This increment changes Markdown only. No client implementation, crate removal,
test execution or dependency update is claimed by this design correction.
