# The charter and the result

Provisional, 2026-10-07. Two records:

- **the charter:** how a run is set up (domain/run.md, 3.1);
- **the result:** what a run declares to finish (domain/run.md, section 7).

They are a family of their own, `smith-charter`, versioned apart from the
channel. The party that decides runs writes the charter and reads the
result, and may not be the host: temper's engine writes charters that its
workers carry unread, possibly with another release of smith than its
agents run (README.md, section 8).

## 1. In one page

- **One family, two records:** the charter, versioned, and the result,
  written in its charter's version.
- **Data, never code.** Everything a host shapes a run with is a value:
  - instructions;
  - brief sections;
  - tool families and host tools;
  - the result contract;
  - conventions;
  - a budget with prices;
  - the LLMs;
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
    configured, a host tool named like one of smith's or named twice, a
    contract that cannot be met (domain/run.md, section 4).
- **A writer needs only this crate.** It depends on no domain, no channel
  and no transport.

## 2. The charter

A versioned record (skein's `codec.md`, section 3):

- **instructions:** text;
- **the brief:** a list of sections, each a title and its text, in order;
- **tools:**
  - the families granted: inspect, modify, shell, sub-agents;
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
- **the budget:**
  - turns;
  - spend, in the host's unit;
  - wall time, as a duration;
- **the LLMs:**
  - the main session's;
  - those sub-agents may use, each named by its model, once.

  Each LLM has:
  - an endpoint's name;
  - a model;
  - the most output tokens a completion may have;
  - its prices: integer amounts for input, cached input and output, per a
    positive number of tokens (domain/run.md, section 9);
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

- **The charter's first field is its version.** The agent reads the
  versions in its range, translates an older charter into its current
  domain's terms, and refuses any other version as an invalid start
  (domain/run.md, section 4).
- **The result is written in the charter's version,** so the charter's
  writer reads what it asked for, whatever release its agents run.
- **A writer writes its current version.** How long agents keep reading old
  versions, and how a writer learns which versions its agents read, is
  README.md's open question.

## 6. Limits

The schema's bounds are the vocabulary's ceilings. The agent's
configuration sets its own limits, at most those ceilings, and the domain's
`Limits` check the charter as a whole at the entrance:

- the bytes it owns;
- its sections, host tools, verdicts, items and fields;
- its budget and its waiting time.

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
  - each decoder fuzzed;
  - an older version's golden charter translated and admitted by the
    current agent.
- **The domain's entrance,** from encoded charters:
  - each refusal of domain/run.md, section 4;
  - an endpoint name the configuration lacks;
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
  - a token split for the budget, now spend in the host's unit with prices;
  - endpoints described in the start, now names the agent's configuration
    resolves.

## 10. Open questions

- **Conventions' home:** in the charter, as here, or in the agent's
  configuration with the charter overriding (domain/run.md, section 15).
- **A sub-agent's own contract,** if sub-agents get one (domain/run.md,
  section 15).
