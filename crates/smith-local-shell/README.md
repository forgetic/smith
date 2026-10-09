# The local shell

Run an interactive host with
`smith local SETTINGS.json STATE_DIR [WORKSPACE_SETTINGS.json]`.
Its [settings and file layout](../../docs/development/local-host.md) describe
workspace overrides, both agent placements, OAuth sign-in and local deliveries.

The focused binary tier lives in `tests/end-to-end/end_to_end.rs`, with Smith-specific
setup in `tests/end-to-end/support/mod.rs`. Cargo supplies `CARGO_BIN_EXE_smith`;
`skein_world::end_to_end::Binary::start` starts `smith local SETTINGS STATE`
under `Mode::Terminal`. It launches its actual agent executable. The test's
shared real-ring loop adopts the terminal master for the local-process
scripted person and drives the shared TLS provider, issuer and browser. No
service state is inspected. The referee uses terminal/peer observations and
real git, and the scratch and trace checks consume only durable outputs.
The terminal master stays open until the binary's exit is observed. Once the
terminal observes the final `finish` call, the fixture provider closes its
connection so the shipped idle-reuse timer is not a test delay.

The first sign-in story reuses `smith_real_world::Scratch` and the real-loop
tool/check scenario. A second invocation preserves the same directories and
checks that the new agent's actual request contains the first run's history;
its trace must append fresh records. Startup refusals share the same binary
observer and terminal driver. These helpers and the shared scratch are the
starting point for the separate opt-in live tier. The binary tests consume
Skein `e8bc018`, which includes session 05's binary API and the subsequent
shared real-loop accounting prerequisite. Their four serial controls took
0.322 seconds on 2026-10-08; see the development workflow for the gate and
memory-scope evidence.
