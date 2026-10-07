# Agent configuration

Run one agent with `smith agent CONFIG.json`. The JSON file is read once before
the channel on standard input and output opens. Unknown fields, missing fields,
oversized files, invalid endpoints and a worst case above `memory_bytes` stop
startup with a reason on standard error.

```json
{
  "profile": "standard",
  "memory_bytes": 1073741824,
  "grace_ms": 250,
  "endpoints": [
    {
      "name": "codex",
      "number": 1,
      "dialect": 1,
      "account": 0,
      "provider": "codex",
      "address": "chatgpt.com:443",
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
limits. `memory_bytes` is the process ceiling, and `grace_ms` is the duration
of each contained tree stop step. An endpoint may additionally set `authority`,
`target`, `headers` (an array of `{ "name", "value" }`), `reasoning_effort`,
`cache_key`, and `identity` (`plain` or `claude-code`). `trust_der` is an optional
path to one DER root certificate; without it, the system trust store is loaded.
Addresses are resolved before the channel opens. The file contains no bearer
tokens; the host sends grant values over the channel.

The optional trace appends JSON lines on a bounded writer thread. `none`
records content-free facts, `calls` also records tool-call names and inputs and
tool-result byte counts, and `everything` additionally records typed prompts,
completion text and usage. Prompt and call bytes use hex encoding so opaque bytes
survive. A prompt above 64 KiB or a call field above 16 KiB is dropped as a
whole. A full writer queue drops records and increments the count reported on
standard error when the run ends.
The trace never receives grant values.
