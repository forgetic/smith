# The charter and the result

Provisional, 2026-10-07, revised 2026-10-09. Two records:

- **the charter:** how a run is set up (domain/run.md, 3.1);
- **the result:** what a run declares to finish (domain/run.md, section 7).

They are a family of their own, `smith-charter`, versioned apart from the
channel. The party that decides runs writes the charter and reads the
result, and may not be the host: temper's engine writes charters that its
workers carry unread, and its version must be the one its agents read
(README.md, section 8).

## 1. In one page

- **One family, two records:** the charter, versioned, and the result,
  written in its charter's version.
- **Data, never code.** Everything a host shapes a run with is a value:
  - instructions;
  - brief sections;
  - tool families and host tools;
  - the result contract;
  - conventions;
  - a budget with a reserve to finish with;
  - the LLMs, each with its window, its output and its prices;
  - the waiting time;
  - whether to resume.

  smith interprets no word of it, beyond the names it compares
  (domain/README.md, section 5).
- **Endpoints by name.** A charter names the endpoints its LLMs run on, and
  the agent's configuration says what each name is (agent.md). A charter
  holds no address and no credential.
- **Checked twice, in two places:**
  - **the codec** checks what a schema can check: bounds, tags, text;
  - **the domain** checks the rest at the entrance: an endpoint not
    configured, a model its configuration does not declare or a window
    or output beyond what it declares or too small for the agent's
    limits, text too large for a window's host part, a host tool named
    like one of smith's or named twice, a contract that cannot be met, a
    reserve its budget cannot hold (domain/run.md, section 4).
- **A writer needs only this crate.** It depends on no domain, no channel
  and no transport.

## 2. The charter

A versioned record (skein's `codec.md`, section 3):

- **instructions:** text;
- **the brief:** a list of sections, each a title and its text, in order;
- **tools:**
  - the families granted: inspect, modify, shell, sub-agents. A
    sub-agent's share is not the charter's: the `sub_agent` call asks for
    `max_turns` and `max_seconds`, each clamped to what is left above the
    reserve, and its spend share is derived from them, never asked for
    (domain/run.md, 5.3; llm.md, section 3);
  - whether `wait` is granted;
  - whether `deliver` is granted mid-run, with a change's contract (2.1);
  - the host tools, each declared with:
    - a name;
    - a description for the LLM;
    - its input's JSON Schema, as bytes;
    - its effect, read or write;
    - its deadline, as a duration;
- **the result contract** (2.1);
- **conventions,** if any: the relative paths of the guide and the checks;
  without them, smith's defaults apply (domain/run.md, 8.1);
- **the budget** (domain/run.md, section 9):
  - turns, a generous loop guard;
  - spend, in the host's unit;
  - wall time, as a duration;
  - **the reserve,** `reserve { turns, time }`: what main keeps to finish
    with. Children and ordinary work draw only on what is left above it,
    and when that is spent the run winds down on the reserve. Its spend
    is derived from its turns at the main model's most costly completion,
    and is not a field. A writer that has no reason to choose gives the
    documented default, which is the product's presets'
    (`docs/design/shell.md`);
- **the LLMs:**
  - the main session's;
  - those sub-agents may use, each named by its model, once.

  Each LLM has:
  - an endpoint's name;
  - a model, one the agent's configuration declares for that endpoint;
  - **`window`:** its usable input tokens, the most one prompt may hold.
    The session compacts its conversation before a prompt would pass it
    (domain/session.md, section 8);
  - **`output`:** the most output tokens a completion may have, thinking
    included. It is sent where the dialect takes it, and reserved before
    each completion;
  - its prices: integer amounts per a positive number of tokens, for
    input, cached input, output and, optionally, cache writes. Without a
    cache-write rate, cache writes are priced at the input rate
    (domain/session.md, section 6).

  `window` and `output` are the effective values the host chose, at most
  what the agent's configuration declares for the model (limits.md,
  section 2.1), so the domain enforces the host's choice;
- **waiting:** how long the run may wait for a message before it parks, as a
  duration;
- **resume:** whether the main session opens from the transcript in the
  start.

### 2.1 The result contract

A contract allows any of four forms (domain/run.md, 7.1):

- **a report:** the most bytes of its text, and its required fields;
- **verdicts:** a closed list of labels. Each label has:
  - the most bytes of its text;
  - its required fields;
  - its items: the fewest and the most, and the kinds allowed, each kind
    with its required fields;
- **a change:** its required fields, and whether the checks must pass;
- **a failure:** the most bytes of its reason, and its required fields.

A required field is a name and the most bytes of its value. Names of
labels, kinds and fields are the host's, compared byte for byte.

## 3. The result

Written by the agent in its charter's version, and read by the charter's
writer (domain/run.md, 7.1):

- **its form:** report, verdict, change or failure;
- **its label,** for a verdict;
- **its text:** a report's text, a verdict's text or a failure's reason;
- **its fields,** each a name and text;
- **its items,** each a kind and its fields.

A change's result carries its fields. What the delivery made, the host
already has from the delivery's own answer (channel.md, section 3).

## 4. Text and JSON

- **Text is UTF-8,** checked when decoded: every field that reaches an LLM,
  and every name the LLM may echo.
- **A host tool's input schema** is JSON, kept as bytes and bounded. The
  agent's protocol layer checks that it is a JSON object within its bound
  before offering it to a provider, and passes it on as it came (llm.md).
  smith never reads its meaning.

## 5. Versions

The charter is pre-release, like every format (README.md, section 8): one
version at a time, and any change a reader would notice is a new one.

- **The charter's first field is its version.** The agent reads its one
  version and refuses any other as an invalid start (domain/run.md,
  section 4). Nothing is translated.
- **A charter that does not decode** in the version the agent reads is an
  invalid start too, with a reason of its own: its writer has a bug to
  fix, where another version needs a writer and an agent of the same
  release.
- **The result is written in the charter's version,** so the charter's
  writer reads what it asked for.
- **A writer writes the version of its release.** `smith check` prints the
  version an agent reads (`docs/design/shell.md`), so a writer and its
  agents that differ find out before a run.
- **Version 2 is current.** It adds each LLM's `window` and `output`, in
  place of the most output tokens, the cache-write price, and the
  budget's reserve. The result keeps its shape.

## 6. Limits

The schema's bounds are the vocabulary's ceilings. The agent's limits are
derived from its configuration's declared quantities and policy values,
at most those ceilings, which startup checks (limits.md, section 4). The
domain's `Limits` check the charter as a whole at the entrance:

- the bytes it owns;
- its sections, host tools, verdicts, items and fields;
- its budget and its waiting time, against the agent's ceilings;
- its reserve, within its budget: fewer turns and less time than the
  budget's, and a derived spend within its spend;
- each LLM's `window` and `output`, at most what the configuration
  declares for its model, and leaving at least the room the agent's
  limits were derived for (limits.md, 3.9);
- its instructions, brief and host tools, with a guide at its bound for
  each of the start's directories, within a window's host part
  (limits.md, 3.9).

A writer that keeps within the ceilings writes a charter every agent can
decode. Whether a given agent admits it is the agent's limits' decision,
and a refusal names what was too large.

## 7. What the domain is owed, and what skein owes

- **The domain is owed** a charter translated into `smith-domain-run`'s
  entities by `smith-protocol-channel`, with each endpoint's name resolved
  through the agent's configuration, and a result translated back.
- **skein owes** the codec generator and `skein-codec` (skein's
  `codec.md`).

## 8. The world

- **The codec:**
  - golden bytes for every record and variant;
  - each decoder fuzzed.
- **The domain's entrance,** from encoded charters:
  - each refusal of domain/run.md, section 4;
  - an endpoint name the configuration lacks;
  - a model the configuration does not declare, and a window or an output
    above its declaration, or leaving less room than the limits assume;
  - text past a window's host part;
  - a reserve its budget cannot hold;
  - a charter in a version the agent does not read.
- **For a host:** a writer's own tests encode every charter it writes and
  check that smith's codec and entrance admit it, as temper's agent.md,
  section 9, plans.

## 9. From temper

- **Kept:** the charter's content as temper's design changed it, and
  temper's verdicts, carried as data.
- **Gone:**
  - roles and task kinds;
  - verdict lists fixed in code;
  - a change's title and body as fields of their own, now names the host
    requires;
  - a token split for the budget, now spend in the host's unit with prices,
    and a reserve to finish with;
  - endpoints described in the start, now names the agent's configuration
    resolves.

## 10. Open questions

- **Conventions' home:** in the charter, as here, or in the agent's
  configuration with the charter overriding (domain/run.md, section 15).
- **A sub-agent's own contract,** if sub-agents get one (domain/run.md,
  section 15).
