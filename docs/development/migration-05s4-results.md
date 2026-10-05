# 05s4: generic results

This first 05s4 increment starts from smith `8a959a5`, the independently
reviewed and gated 05s2 copy baseline. It implements the result contract in
[domain/run.md, section 7](../design/domain/run.md#7-results). It retains the
copied preparation, split token budget and `Push` lifecycle; subsequent
05s4 increments supply generic delivery, host tools, messages, waiting and
workspace conventions. Session history from 05e is unchanged.

The host permits report, closed-label verdict, change and declared failure
forms independently. Required fields have opaque byte names and individual
value caps. Each verdict has its own root fields, text cap, item count range
and closed item kinds; each kind has its own required fields. Extra names
and values are allowed and count in aggregate ownership. Duplicate field
names are feedback; required names and their caps must be unique, nonempty
and positive at admission.

Report text and failure reasons use explicit inclusive host minimum and
maximum lengths. A zero minimum permits empty text. The report world uses
that rule; the failure world requires a reason and corrects an empty one.
Verdict text may be empty. Change metadata has no intrinsic title or body
rule: only fixtures choose those names and interpret their meaning.

Admission refuses malformed or impossible contracts before emitting
preparation, session or IO requests. Every permitted form must have a
smallest accepted value within the aggregate result cap. That calculation
counts required text, field containers and names and nonempty values, and
required item containers and kind names. Charter accounting counts all rule
containers and declared names. Checked `outcome::owned_bytes` counts every
result/item container and payload beyond the inline declaration, including
unknown fields, for the run and root's respective ownership checks. The pure
shape judge does not enforce an aggregate cap on its callers' behalf.

Valid reports, verdicts and declared failures return accepted finish
feedback, settle sessions, then answer once. Declared failure is an accepted
result, distinct from a runtime failure. A valid change retains checks and
host `Push` before acceptance. Contract violations remain typed, bounded
feedback that the LLM can correct.

## Compatibility and evidence

This is an intentional boundary change: `Change.title/body` become generic
fields; `Verdict.body/children` become text, root fields and items; `Child`
and `Children` result types become `Item` and `ItemSpec`; shared child field
rules become per-kind `ItemRule`s. Callers must supply individual caps and
choose text minimums. Standard Rust `Result` is unchanged. No temper
package, channel, protocol, engine, task, or parked-v2 dependency is added.

Existing unit and component/world consumers are adapted to explicit
host-owned contracts. New cases cover four forms, forbidden forms, field
and item feedback, host minimums, exact minimum-fit boundaries, unknown
extra storage, and impossible admission with no effects. The run world
accepts empty reports and declared failures without checks or `Push`, checks
feedback correction and replays both. The composed agent uses the real
Report and declared Failure forms through the typed fixture decoder and
fake LLM. Its independent referee checks final results against observed
host rules and separately computes aggregate ownership; negative histories
reject missing report fields, missing required failure reasons and omitted
extra-field storage. Facts remain observations and do not steer peers.
The memory driver generates all four result forms and uses the shared heap
meter directly; the full run fixture includes field containers in its exact
byte-limit filling.

Static checks and scoped rustfmt are author-owned. Serial measurements and
all four exact-tip gates are parent-owned and pending at this source freeze;
no new runtime measurement is claimed here. Workspace limits remain
15 seconds focused and 60 seconds fuzzy. Canonical skein remains locked to
`5e52dd9cd8793094bf49c8bf75f9cffac027814f`, matching temper. Its local-cache,
unpublished-revision portability limitation remains as documented in
[README.md](../../README.md).
