# Agent configuration

Run one agent with `smith agent CONFIG.json`. The JSON file is read once before
the channel on standard input and output opens. Unknown fields, missing fields,
oversized files, invalid endpoints and a worst case above `memory_bytes` stop
startup with a reason on standard error.

```json
{
  "profile": "standard",
  "memory_bytes": 1000000000000,
  "grace_ms": 250,
  "endpoints": [
    {
      "name": "codex",
      "number": 1,
      "dialect": 1,
      "account": 0,
      "provider": "codex",
      "address": "chatgpt.com:443",
      "transport": "tls",
      "server_name": "chatgpt.com"
    }
  ],
  "environment": [
    { "name": "PATH", "value": "/usr/bin:/bin" }
  ],
  "trace": { "path": "/tmp/smith-agent.jsonl", "capture": "calls" }
}
```

The `standard` profile fixes the domain, channel, LLM, machine, IO and queue
limits. `memory_bytes` is the checked process ceiling. The profile currently
has a conservative bound of roughly 354 GB; the example ceiling permits that
bound and does not eagerly allocate it. Reducing retained limits or tightening
the bound remains later work. `grace_ms` is the duration of each process stop
step. Children use Skein's plain process mechanism until contained trees are
available. An endpoint may additionally set `authority`,
`target`, `headers` (an array of `{ "name", "value" }`), `reasoning_effort`,
`cache_key`, and `identity` (`plain` or `claude-code`). `trust_der` is an optional
path to one DER root certificate; without it, the system trust store is loaded.
`transport` is `"tls"` by default, requiring `server_name` and using the
configured trust. For a server on this machine, set `"transport": "plaintext"`
and an address resolving to loopback (IPv4 or IPv6); omit `server_name` and
`trust_der`. Any non-loopback plaintext destination, unknown transport or
TLS-only field on plaintext refuses startup before the channel opens.
The local host carries the same inline endpoint settings into its child's
configuration. Addresses are resolved before the channel opens. The file contains no bearer
tokens; the host sends grant values over the channel.

The optional trace appends JSON lines on a bounded writer thread. `none`
records content-free facts, `calls` also records tool-call names and inputs and
tool-result byte counts, and `everything` additionally records typed prompts,
completion text and usage. Prompt and call bytes use hex encoding so opaque bytes
survive. A prompt above 64 KiB or a call field above 16 KiB is dropped as a
whole. A full writer queue drops records and increments the count reported on
standard error when the run ends.
The trace never receives grant values.

Run an interactive host with
`smith local SETTINGS.json STATE_DIR [WORKSPACE_SETTINGS.json]`.
Its [settings and file layout](../../docs/development/local-host.md) describe
workspace overrides, both agent placements, OAuth sign-in and local deliveries.

The shared agent shell is `smith::agent_shell::Agent`. `Agent::read` reads this
configuration; `Agent::new` takes an already parsed `config::Configuration`.
Both take `Resources` (channel input/output, signal descriptor and injected
seed) and an error writer. The writer receives startup and final diagnostics
once. `Resources::roots` supplies inherited mount descriptors in Start order
for hosted worlds; omitted roots are opened on the machine, as in the binary.
The caller owns any provided roots that the Start does not consume.
`Agent` implements Skein's `Host`: the caller reaps into `completions`, calls
`iterate(now, wall)`, and submits `submissions`. That pass drains the same
trace and opens/adopts the same roots as the binary. `result` is available
only after the channel and every lower child have settled.
`agent_shell::run` is the binary's kernel-and-clock driver; its failures are
already written to the supplied error output.

The focused binary tier lives in `tests/end_to_end.rs`, with Smith-specific
setup in `tests/support/mod.rs`. Cargo supplies `CARGO_BIN_EXE_smith`;
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
