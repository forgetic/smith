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
  ]
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
