# Running the local host

The local host reads JSON settings, opens a chat's saved files, obtains model
credentials and runs the agent. Its contracts are
[protocol/hosts.md, section 5](../design/protocol/hosts.md) and
[domain/host.md, sections 7–10](../design/domain/host.md).

## Commands and terminal

```sh
smith local SETTINGS.json STATE_DIR [WORKSPACE_SETTINGS.json]
smith agent CONFIG.json
```

These are positional arguments. The optional workspace settings file is
explicit; Smith does not search for it. Settings are read once at startup.
Objects merge recursively, and arrays and scalar values replace the global
value. Unknown fields and missing required fields are refused. Each input
settings file is limited to 1 MiB. Relative paths are relative to the process's
working directory.

One completed terminal line is one message. A backslash at the end continues
the message on the next line. The terminal shows the run's text, tool names,
ending and waiting prompt. The first interrupt cancels the run; a second asks
its child to stop. Closing input lets the host settle its run and lower IO
before exiting. Startup and failure reasons go to standard error.

The default launches the same executable as `smith agent STATE_DIR/agent.json`.
Set `in_process` to `true` to run the agent domain and its components in the
host process. Both placements use the shared local shell library.

## Settings

The required top-level fields are `agent`, `chat`, `instructions`, `models`,
`budget` and `waiting_seconds`. `agent` is an inline configuration object in
the format described in [the agent configuration](../../crates/smith/README.md).
It is written durably to `STATE_DIR/agent.json` for the child.

For example, this workspace override selects a chat and a change contract
while inheriting the global agent, model, budget and account settings:

```json
{
  "chat": "review",
  "instructions": "Make the requested change and explain it.",
  "directories": [
    { "name": "project", "path": ".", "writable": true, "git": true }
  ],
  "conventions": { "guide": "AGENTS.md", "checks": "checks" },
  "contract": {
    "form": "change",
    "checks_must_pass": true,
    "fields": [
      { "name": "title", "max": 256 },
      { "name": "body", "max": 4096 }
    ]
  }
}
```

| Field | Meaning and default |
| --- | --- |
| `agent` | Inline agent configuration; required. Endpoint names and account numbers resolve the local models. |
| `chat` | Required nonempty directory name beneath `STATE_DIR`; no slash, backslash, NUL, `.` or `..`. |
| `instructions` | Required instruction text for each activation. |
| `models` | Required nonempty array of priced models, described below. |
| `budget` | Required `{ "turns", "spend", "seconds" }`; turns and seconds are positive. Spend is an integer in the pricing currency's chosen unit. |
| `waiting_seconds` | Required positive waiting interval, in seconds. |
| `brief` | Ordered `{ "title", "text" }` context sections; default `[]`. |
| `directories` | Ordered `{ "name", "path", "writable", "git" }` workspace roots; default `[]`. The binary's host profile allows two directories. |
| `conventions` | Optional `{ "guide", "checks" }` paths within the workspace. |
| `contract` | `{ "form": "report", "max": N }` or the change shape above. Omitted means a report with at most 4,096 bytes. Field `max` values bound bytes. |
| `deliver` | Optional array of `{ "name", "max" }` fields for mid-run changes; these require passing checks. |
| `title_field` | Result field used as a commit subject; default `"title"`. |
| `delivery_environment` | Git child environment as `NAME=VALUE` strings; default `[]`. At most 64 entries and 4,096 bytes in total, with no NUL. |
| `in_process` | Agent placement; default `false` (spawned). |
| `token_directory` | Optional private OAuth record directory; default described below. |
| `accounts` | Account descriptors, described below; default `[]`. |
| `push` | Optional ordered array matching `directories`, each entry `null` or `{ "remote", "branch" }`. Omitted means no push. |

Each model has this shape:

```json
{
  "endpoint": "codex",
  "name": "registered-model",
  "max_tokens": 8192,
  "input_price": 10,
  "cached_price": 2,
  "output_price": 30,
  "price_unit": 1000000
}
```

`endpoint` names an endpoint in `agent.endpoints`; `name` is the provider's
model name. The three prices are integer currency units per `price_unit`
tokens, which must be positive. These example names and prices are placeholders
for the operator's model registration and prices. The settings do not discover
models or prices from the provider.

The agent configuration's `environment` is an array of `{ "name", "value" }`
for agent commands. It is separate from the host's `delivery_environment`.
Both default to the explicit configured environment rather than inheriting
the host shell's environment.

## Accounts and sign-in

Every model account needs an entry whose `number` matches the endpoint's
`account`. `account_id` is the opaque account identifier lent to the agent;
it is not a bearer token. An account can use an existing saved record without
an OAuth descriptor. To support fresh sign-in and refresh, supply `oauth`:

```json
{
  "number": 0,
  "account_id": "registered-account",
  "oauth": {
    "authorization_url": "https://issuer.example/authorize",
    "token_endpoint": "https://issuer.example/token",
    "client_id": "registered-public-client",
    "redirect_uri": "http://127.0.0.1:8123/return",
    "scope": "registered-scope",
    "address": "issuer.example:443",
    "server_name": "issuer.example",
    "json": false
  }
}
```

Replace the example registration and destinations with the issuer's values.
`address` is the token connection's startup-resolved destination;
`server_name` is its TLS name. `token_endpoint` uses HTTPS, or HTTP to a
numeric loopback issuer, and contains a path. Both issuer URLs may use HTTP
when their authority is numeric loopback; HTTP also requires `address` to
resolve to loopback. Omit `server_name` and `trust_der` for an HTTP token
endpoint, which uses plaintext. The redirect must use HTTP to a loopback address with a fixed nonzero
port and a path, without a query or fragment. `scope` defaults to an empty
string. `json` selects JSON token requests; omitted or `false` uses form
encoding. Optional `trust_der` names one DER trust certificate; omitted uses
the system trust store.

The host opens the loopback listener before showing the authorization URL.
Visit that URL in a browser; the host receives the redirect and exchanges the
code using Skein's OAuth client and PKCE. It saves the candidate token record
before lending the grant. A lent grant is refreshed 60 seconds before expiry.
An uncertain token POST is not repeated. A refused credential shows account
unavailability and starts no run.

Unless `token_directory` is set, records live in
`$XDG_CONFIG_HOME/smith/tokens`, or `$HOME/.config/smith/tokens` when
`XDG_CONFIG_HOME` is absent. The directory is private (0700); files are private
(0600) and named `<account-number>.json`. Skein's saved-token codec owns their
format. The host refuses symbolic links, nonregular records, extra hard links,
public permissions and account/filename mismatches. Replacement syncs the file,
renames it, and syncs the directory before acknowledging. Tokens are not stored
in settings or displayed on the terminal.

## Chat files and deliveries

`STATE_DIR/<chat>` contains `state`, numbered `0000000001.turn` files, and
delivery records named by activation, completion and position. Turn filenames
use the conversation position across runs, independently of each activation's
channel numbering. Turns and answers after the last turn are acknowledged only
after file sync, rename and directory sync. Restart loads those files and
preserves unknown delivery intents for reconciliation. Temporary replacements
are not conversation records. A pending `fresh` reset journal is completed
when the store reopens.

The startup reader bounds aggregate chat bytes to 64 MiB, retained records to
4,096, directory entries to 8,192 and each delivery record to 1 MiB. It refuses
malformed or unsupported records and nonregular files. The service and codecs
also impose their receiving limits; exceeding a bound produces a failure
instead of a partial transcript.

Git deliveries run `/usr/bin/git` as a child of the host. A result's configured
subject field becomes the commit subject; other fields form its body. The
message includes the stable `Smith-Delivery: activation/completion/position`
trailer. A moved head is stale. A push runs only for a configured destination
in the corresponding directory's `push` entry.

Git effects distinguish done, proved no effect and uncertain. Before answering
an uncertain effect, the host searches for the delivery trailer. It never
commits again on doubt. If inspection cannot settle it, the saved unknown
intent remains available on restart.

Plain directories have a sorted bounded tree snapshot before activation and
are compared again at delivery. A snapshot admits at most 256 nodes, paths of
4,096 bytes and files of 65,536 bytes. Omitted entries, unsupported files,
symbolic links, oversize and IO errors refuse the comparison. Original conflict
paths are checked through bounded file reads before a git delivery; deleted
paths are resolved, and a surviving marker refuses the delivery.

## Verification and later work

The [local-process world](../../tests/local-process/README.md) exercises the
same shell library with both agent placements, OAuth peers and a fake checkout.
It covers resume, commits, cancellation, credential renewal, observation-only
referees, memory, replay and seeded IO faults. Shared Store tests cut every
write and sync boundary. Serial measurements are in the
[workflow](workflow.md).

Process containment uses Skein's plain children until its contained-tree
mechanism is available. The inherited standard agent profile has a conservative
checked memory bound of roughly 354 GB, so its configured ceiling must exceed
that bound; reducing the retained limits or tightening that calculation remains
later work. The ceiling is checked accounting, not an eager allocation of that
number of bytes.
