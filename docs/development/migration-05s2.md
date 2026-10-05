# 05s2 copy baseline and world boundary

This increment copies the agent domains, provider codecs, OAuth codec,
neutral fake LLM and their component worlds from temper commit `25ac2ad`.
It implements the copy-first step of temper's
`docs/plans/next-domain/05s-smith.md`, section 05s2. It preserves the
recorded-session behavior from 05e. The generic host API in
`docs/design/domain/run.md`, section 14, is a later 05s4 change; the copied
charter, push, verdict, checkout preparation and finish vocabulary is
explicitly a temporary implementation baseline.

| Source at `25ac2ad` | smith destination | Preserved test functions |
| --- | --- | ---: |
| `crates/temper-agent-domain` | `crates/smith-domain` | 19 |
| `crates/temper-agent-domain-run` | `crates/smith-domain-run` | 58 |
| `crates/temper-agent-domain-session` | `crates/smith-domain-session` | 53 |
| `crates/temper-agent-domain-tools` | `crates/smith-domain-tools` | 46 |
| `crates/temper-llm-anthropic` | `crates/smith-llm-anthropic` | 8 |
| `crates/temper-llm-openai` | `crates/smith-llm-openai` | 13 |
| `crates/temper-oauth` | `crates/smith-oauth` | 10 |
| `testing/temper-fake-llm-domain` | `testing/smith-fake-llm-domain` | 10 |
| `testing/temper-fake-llm-protocol` | `testing/smith-fake-llm-protocol` | 0 |
| `tests/agent/tools` | `tests/tools` | 38 |
| `tests/agent/session` | `tests/session` | 46 |
| `tests/agent/run` | `tests/run` | 29 |
| `tests/fake-llm` | `tests/fake-llm` | 4 |

The copied surface contains 113 Rust files and 30 static provider fixture
resources. The fixture bytes remain identical; their new home is
`tests/provider-fixtures/{anthropic,openai}`, and only test helper paths
change. smith adds public item documentation, complete file-and-section
citations, and blank item separators. Component test bodies and stimuli
remain intact apart from renamed imports and borrowed diagnostics for the
shared memory meter. The dependency implementations are shared directly
from `skein_world::domain`, its heap module and `skein-fake-checkout`.

Two production syntax changes preserve the copied algorithms while
following `programming-model.md`, section 10.2: the fake script finder uses a
bounded suffix loop including the empty suffix, and the fake provider's
checked output-length sum uses nested `Option` matches. Other executable
copy changes are package/import renames, the fixture paths, and the memory
API adaptation. Optional trailing commas and rustfmt reorder no behavior.

## The top-level agent world

`tests/agent` composes the real agent, run, session and tools domains with
the real neutral fake LLM and shared contained checkout. A typed scripted
host supplies charters and push replies. Its referee sees starts,
completion requests and terminals, check results, push requests and
terminals, landed bytes and the host answer. It independently checks one
host answer, accepted usage, no answer before lower terminals, checked
changes and the exact landed snapshot. It does not inspect private state
to decide replies. Public domain counters only assert final quiescence.

The 15 focused simulation stories retain agent behavior for coding and
repair, review and rejected finishes, report, nested delegation, shared
budgets, stale and refused pushes, cancellation races, timeouts, capacity,
seed replay and fact loss. Seven negative referee stories reject
independently malformed observation histories. One fuzzy simulation
sweeps 120 seeds. The existing top-level memory driver is retained with
its 12 seeds, two capacity settings and 3,000 rounds per setting.

The old full-system story's forge-capacity case becomes an agent admission
capacity case; the stale-branch retry is two fresh scripted host requests.
These cases do not prove forge queuing or merge behavior. Byte channel,
agent protocol, process containment, durable engine state, Forgejo CI,
posting, merging and full-system retry remain covered by temper's legacy
worlds and migrate later in 05s5. The fixture translator recognizes the
copied finite script language; provider unit fixtures and fake byte worlds
retain codec coverage rather than treating the fixture parser as a
production protocol.

Facts never drive the peers. Drained, lossless facts are checked against
boundary observations; a focused world compares fact-retaining and
fact-dropping runs. Deterministic replay compares complete rendered
boundary traces, results and landed bytes. The memory binary uses the
shared counting allocator and meter directly; smith owns no second
allocator or generic harness implementation.

## Gate evidence

Serial measurements and exact-tip gate results are recorded in
`docs/development/workflow.md` after the parent performs the serialized
checks. No new timing is claimed by this authoring snapshot. Workspace
budgets remain 15 seconds focused and 60 seconds fuzzy.

The intended shared dependency is locally gated skein commit
`5e52dd9cd8793094bf49c8bf75f9cffac027814f`, containing the domain harness and
fake checkout. All skein dependencies must resolve that revision in the
final lock file, matching temper's adoption. It is not published upstream:
offline local validation supplies it through Cargo's canonical git cache.
A fresh machine cannot obtain that commit from the canonical URL until
publication is authorized. There is no path patch or alternate URL.
