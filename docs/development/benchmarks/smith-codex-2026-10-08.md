# Smith and Codex coding performance benchmark October 2026

The benchmark compares Smith local with enabled sub-agents against vanilla standalone Codex, starting from the requested reliability branch. Both agents passed the final independent tests and source reviews for the first six tasks. Smith's median times were lower on four tasks and slightly higher on two. Smith used more uncached input and more individual tool calls on every task, while Codex's much larger total input was predominantly cached. 

Across the selected matrix, Smith passed 20 of 21 attempts and Codex 21 of 21. On the harder repository task, Smith completed two of three repetitions and Codex all three. Median process time across all three attempts was 996.9 seconds for Smith versus 413.2 for Codex; Smith's last attempt reached its 1,200-second task budget. These are observations from three repetitions per task, not a general ranking of coding agents.

The initial failures exposed reproducible Smith runtime defects: premature requests before the first user task, delayed physical shutdown, missing failure diagnostics, a tool-argument limit too small for ordinary source writes, and completion deadlines too short for slower models. The local fixes include regression coverage and preserve bounded ownership and exactly one terminal per operation. The repository task then exposed a too-small encoded request envelope and the shared turn budget described below.

## Results

| Task | Model, high effort | Smith passes | Codex passes | Smith process seconds, median (range) | Codex process seconds, median (range) |
| --- | --- | ---: | ---: | ---: | ---: |
| Bounded backoff | Luna | 3/3 | 3/3 | 22.0 (21.6–27.0) | 23.6 (22.0–26.3) |
| Incremental decoder | Luna | 3/3 | 3/3 | 36.5 (29.6–51.1) | 50.0 (41.3–50.5) |
| Graph planner | Sol | 3/3 | 3/3 | 43.8 (42.5–49.3) | 51.9 (48.2–52.2) |
| Transactional edits | 6.1 Sol | 3/3 | 3/3 | 98.0 (93.9–111.5) | 96.6 (81.2–100.7) |
| Lease queue | Astra | 3/3 | 3/3 | 121.9 (103.0–125.8) | 117.5 (112.7–122.2) |
| Journal replay | Astra | 3/3 | 3/3 | 128.8 (120.7–198.3) | 142.4 (122.1–143.0) |
| Repository shell cwd | Astra | 2/3 | 3/3 | 996.9 (563.2–1200.1) | 413.2 (397.2–436.5) |

All times measure the actual agent process from start through exit. Each timing cell reports the median and range across all three valid measured attempts, including failures. Failure time measures the time consumed before stopping, rather than time to a correct solution. On the repository task alone, the successful-run median is 780.0 seconds for Smith (two successes) versus 413.2 for Codex (three); that conditional comparison excludes Smith's timeout and should be read alongside completion rates. Success requires a completed process, candidate and original tests, immutable external tests, and AI-agent source review. Formatting and repository checks are also required where the task specifies them. The fixture tasks use ordinary safe Rust. The repository task additionally requires Smith's production Rust subset and domain contracts.

Tasks 1–4 use Smith snapshot `8466afb`; tasks 5–6 use `d75190b`, which also incorporates the 300-second completion profile. The final repository-task phase uses `44e5120`, with the request/trace fixes, optional child `max_turns`, and an explicitly selected 256-turn global budget. Earlier fixture budgets remain 64. Per-attempt binary hashes preserve these distinctions. Root model effort is `high` throughout because it is the user's existing Codex default. The model ladder progresses from `gpt-6-luna` to `gpt-6-sol`, `gpt-6.1-sol`, and `gpt-6-astra`.

Values are Smith / Codex, median tokens across all selected attempts. Failed attempts, if any, retain their observed usage and are marked separately in the raw data.

| Task | Fresh input | Cached input | Total input | Output |
| --- | ---: | ---: | ---: | ---: |
| Bounded backoff | 14,677 / 13,399 | 0 / 97,536 | 17,051 / 110,935 | 1,815 / 1,929 |
| Incremental decoder | 31,157 / 19,811 | 8,192 / 148,224 | 41,397 / 164,052 | 2,963 / 4,465 |
| Graph planner | 30,731 / 11,686 | 6,656 / 155,392 | 33,035 / 167,078 | 3,305 / 4,594 |
| Transactional edits | 30,746 / 18,230 | 0 / 96,128 | 33,050 / 110,266 | 4,197 / 4,356 |
| Lease queue | 38,954 / 20,557 | 0 / 95,488 | 38,954 / 116,492 | 5,219 / 5,497 |
| Journal replay | 74,793 / 28,148 | 10,368 / 91,904 | 85,161 / 119,109 | 5,567 / 6,169 |
| Repository shell cwd | 3,515,963 / 284,378 | 868,224 / 7,262,592 | 4,396,385 / 7,579,438 | 23,984 / 23,926 |

Input totals and fresh input have different implications. Smith usage separates fresh input and cache reads; Codex input already includes cached input. The table normalizes those conventions and includes the root and all observed child conversations. Native response IDs are deduplicated across the selected thread tree, and response sums are checked against per-thread cumulative usage. Root-only CLI totals remain separate. Each column is a median independently, so median fresh and cached values need not add to median total. Output includes reasoning when reported by the provider; Smith lacks a reliable separate reasoning breakdown. Cache writes are unavailable because Smith's decoder supplies zero without observing a separate write count. Saved Smith prompts above 64 KiB may be omitted while usage facts remain intact; token completeness separately checks response counts and domain/fact/writer losses. Failed or unobserved responses retain a lower-bound label. On the repository task, median recorded fresh input is 3.516 million for Smith versus 0.284 million for Codex, about 12.4 times higher, despite Smith's lower total input. The failed Smith attempt has one canceled response without recorded usage. No monetary cost is inferred from these fields.

Values are Smith / Codex medians. CPU and RSS include tool processes.

| Task | Responses with usage | Model tool calls | Workspace operations | CPU seconds | Peak tree RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bounded backoff | 8 / 7 | 12 / 6 | 11 / 6 | 0.70 / 3.29 | 285 / 671 |
| Incremental decoder | 11 / 9 | 15 / 8 | 14 / 8 | 1.52 / 4.57 | 353 / 651 |
| Graph planner | 10 / 9 | 17 / 8 | 16 / 11 | 1.26 / 5.11 | 439 / 733 |
| Transactional edits | 9 / 6 | 15 / 5 | 14 / 8 | 1.41 / 4.19 | 426 / 737 |
| Lease queue | 8 / 6 | 14 / 5 | 13 / 7 | 1.83 / 4.10 | 432 / 676 |
| Journal replay | 13 / 6 | 18 / 5 | 17 / 7 | 2.95 / 4.41 | 384 / 719 |
| Repository shell cwd | 125 / 97 | 192 / 94 | 189 / 90 | 233.39 / 311.06 | 1370 / 1964 |

Model tool calls are not equivalent work units. Codex can execute several commands inside a JavaScript wrapper; Smith commonly issues separate read, search, and edit calls. Tool and workspace counts include observed root and child work. Native coordination calls include spawn, message and follow-up calls; their count is distinct from the number of children and from Smith's synchronous delegation calls. Workspace operations and their failures are recorded separately. The response column counts responses with recorded usage, rather than all transport attempts. Smith records retries and failed attempts; native internal provider attempts and retries are unavailable. A missing count is not treated as zero. CPU is user plus system time for the agent and its reaped tool descendants. RSS sums sampled descendants every 50 ms, can double-count shared pages, and can miss brief peaks. Early calibration samples missed descendants created by other threads and are labelled lower bounds; the final matrix uses the corrected traversal.

## Method and fairness

The session began at 2026-10-08 23:34:37 UTC with a six-hour deadline. The initial checkout and fetched `fix/interactive-reliability-2026-10-08` branch both passed their focused and fuzzy suites. All work descends from requested base `8e8393109a517a0b07ee4f914b175ff76efb6dc3` on the isolated local branch `bench/smith-codex-2026-10-08`.

The user superseded the initial exact-tool proposal with a native product comparison. Codex runs as a standalone process with its existing configuration:

```sh
codex --no-daemon exec --json -m MODEL \
  -c mcp_servers.codebase-memory-mcp.enabled=false \
  -C WORKSPACE -o final.txt -
```

Model choice and disabling codebase-memory are the behavioral overrides. Effort, sandbox, approvals, tools, provider, plugins, and child selection retain native defaults. Standalone mode and JSON output provide observation. Smith uses its production binary and direct TLS transport to the same account, with its normal workspace tools, local sub-agents enabled, and a child model choice matching the root model and effort. Preliminary custom-tool bridges and proxies are excluded from scored results. No MCP participates in either agent, exploration, or grading.

The host provides eight virtual cores and approximately 15 GiB RAM. Every final pair receives disjoint four-core CPU affinity, swapped between repetitions. Each attempt gets a fresh detached worktree, conversation, and private warmed build cache. Original source, seed and prompt hashes must match frozen manifests before inference. Normalization detaches hard-linked artifacts and makes the cache owner-writable for both agents. Models receive identical task prompts and cannot see the external graders or reference solutions. Setup builds, cache warmup, and external grading are outside the agent timer. Agent-issued build and test commands remain inside it. Affinity reduces local CPU contention but does not isolate provider scheduling, network, memory bandwidth, or filesystem caches.

The first five fixtures progressively cover bounded arithmetic, incremental parsing, graph scheduling, atomic edits, and lease lifecycle. A separately frozen journal replay task tests framing, validation priority, staging, commits, and sticky rollback. The repository task spans the real protocol, tools domain, machine boundary, checked path accounting, authority, cancellation, and documentation. Its oracle uses public boundaries rather than requiring a prescribed new enum variant.

The external grader runs only after the agent stops and retains partial source on failure. Independent AI-agent source review checks preserved original assertions, API compatibility, contract edges, meaningful new tests, dependencies, production restrictions when applicable, and verification claims. Changed original test files are review flags; legitimate appended tests are allowed. Finite tests and review do not establish an exhaustive proof. Raw logs, model and child usage, timings, resource samples, source archives, diffs, grading, and review results remain available per attempt.

Smith's configured `max_tokens=4096` is an admission reservation. The subscription adapter removes the wire output cap; neither product's subscription requests impose that configured generation ceiling. Smith has explicit run, turn, byte, and memory accounting bounds, while native Codex has its own product behavior. Its large accounting memory ceiling does not reserve or allocate that amount eagerly.

## Observed defects and fixes

| Observed limitation | Local change and evidence |
| --- | --- |
| Startup requested a completion before the user task | Prepared empty brief parks until the first task; actual startup/wait/cancel regressions. |
| Accepted tasks lingered about seven seconds | Shared Skein pool closes explicitly; physical settlement and finite nonblocking trace acknowledgement reduce measured teardown to about 5 ms. |
| Failure terminals were hard to diagnose | Typed failure remains separate from crashes; bounded stderr exposes failed child and run reasons without injecting them into model history. |
| 2 KiB tool arguments rejected ordinary source writes | Coherent 32 KiB argument/decoded-call limits and JSON escape bounds; actual TLS and exact boundary tests. |
| Whole-response timeout of 60 seconds cut off Astra | Finite 300-second profile, retaining shorter outer-budget cancellation; virtual-time transport tests and a successful 66.3-second child response. |
| Shared 64-turn budget ended delegated repository work before verification | Standard admission ceiling rises to 256; decoded-charter domain tests verify 256/257 admission, and existing harness verifies exact global accounting, smaller budgets and one terminal. |
| Model tool omitted existing child budget controls | Optional `sub_agent.max_turns` exposes the existing own-child quota, preserving omission and spend inheritance; both-provider actual schema/decoder and parent-continuation tests. |
| Encoded repository history exceeded 256 KiB | Finite 1 MiB outgoing envelope; actual saved request measures 266,534 bytes, plus exact-cap and allocator-meter tests. |
| Terminal facts disappeared after answering; burst trace storage retained excess capacity | Terminal-only local drain, active-channel backpressure guards, named loss counters, exact-sized boxed trace records and checked reserve; bounded writer and terminal regressions. |

Baseline task acceptance was followed by about 7.002 seconds of shutdown delay. After explicit shared-pool close and bounded trace acknowledgement, observed teardown in the focused retest fell to roughly 5 ms. The baseline also contained sandbox failures caused by read-only warm Cargo artifacts; correcting those artifacts for both agents confounds whole-task before/after comparisons. Physical teardown observations and deterministic lifecycle tests support the shutdown conclusion more directly.

The original 2 KiB argument profile caused both initial line-decoder attempts to abort. One partial snapshot passed 5 of 7 external tests and the other passed none. An early observer missed the first failure and left the host waiting until manually stopped; that raw 305-second wall time is explicitly invalid for performance. Improved diagnostics exposed `Model(Completion { failure: Limit, evidence: Response })`. The coherent 32 KiB profile subsequently completed all three decoder repetitions and passed all external checks. Tests exercise exact limits, one byte beyond, worst-case JSON escaping, replay, and the actual configured TLS binary.

The first Astra lease attempt completed in 448 seconds for Smith versus 141 seconds for Codex. Smith's synchronous sub-agent occupied 273 seconds and encountered four whole-response timeouts at precisely 60 seconds before the parent recovered. The 300-second bounded profile admits slower thinking while retaining the outer task deadline. Virtual-time tests cover response admission, continued streaming without extending the absolute deadline, sibling isolation, cancellation, and a shorter outer budget. A later successful journal child response took 66.313 seconds, directly demonstrating admission beyond the old limit. The post-fix lease median is 122 seconds, but those runs chose no child; the full latency improvement cannot be attributed entirely to the timeout change.

The first repository attempt failed when ordinary accumulated history exceeded the 256 KiB envelope; that next request was rejected before it was sent. Replaying its saved main context through the actual encoder produces 266,534 bytes, even without optional lifecycle schemas. A separate child history also exceeds the old bound. The 1 MiB profile raises this finite envelope; it does not compact history or promise unlimited contexts. Exact boundary, adapter-history, replay and allocator-meter tests verify admission and ownership.

With that fix, a fresh supported 64-turn attempt reached the global limit after 18 root turns, 35 turns in its first child, and 11 in its second. These counters agree with captured usage; delegation is not double-billed. Its partial source passed all 14 feature and three control checks, but formatting and the all-target build remained unfinished. Native Codex completed the paired task in 466.8 seconds with two asynchronous children and 106 total responses. Codex's root console reported 4.78 million input tokens; deduplicated root-plus-child input was 8.32 million, of which 8.05 million was cached. Root-only console totals are therefore unsuitable for the agent comparison. The earlier native repository run likewise used 116 responses across three threads. This motivates a bounded larger global turn profile rather than treating the 64-turn refusal as an accounting bug.

The next supported 128-turn trial also exhausted the aggregate budget, at 966.9 seconds. The root and its three children consumed the available budget before the parent could complete formatting and verification. The partial source passed all feature/control checks, all-target Clippy, 1,488 focused tests and 17 fuzzy tests, but it remains a scored failure. Native completed the paired task in 298.7 seconds. This exposed a separate model-interface omission: typed child budget shares already existed, while the offered `sub_agent` tool always requested the entire remainder. The final interface exposes an optional own-turn cap; the generous global ceiling remains separate from context limits and the 20-minute task timeout.

The final three repository repetitions passed twice for Smith and three times for Codex. Smith used explicit child own-turn caps in every repetition. The first used 125 completed responses with child caps of 28 and 32; the second used 71 with a child cap of 20. In both, the parent continued after a child exhausted its own quota and completed the task. Both fit below the earlier 128-turn ceiling, so these successes do not establish a causal benefit from the new 256-turn ceiling. The third chose six serial delegations with caps of 32, 40, 35, 45, 25 and 25, and expired at 1,200.1 seconds while the last child was still working. Its partial source passed formatting and 13 of 14 feature checks, but rejected an exactly-at-limit normalized path and contained a new test that failed to compile. The dependency-policy check passed after the Smith parent agent requested removal of extra dependency entries. These are unfinished candidate/model decisions, with no observed runtime crash or transport failure; this repetition remains a scored failure. Its canceled in-flight response makes recorded token usage a lower bound.

Smith can batch up to four adjacent inspect-only delegations within a completion, subject to its conversation and budget bounds. A child granted editing or shell access is classified as a write effect and runs alone, even if its brief says to review only. The parent waits for every call in its completion batch to settle before asking the model again. Codex offers asynchronous child handles, allowing parent work while children run. The observed repository delegations request inspect, modify and shell grants, so Smith serializes them. This is a documented authority/scheduling policy, rather than a transport bug or a provider parallel-call restriction; the subscription request enables parallel tool calls. Child finishes never terminate the parent benchmark; completion observation is scoped to the root run and conversation. Final acceptance additionally requires exit zero and no forced termination. If an outer harness deadline expires, cleanup captures and settles the owned process tree, including children with detached sessions. Unknown wall time, usage, and resources remain unavailable, rather than being inferred from wrapper elapsed time.

## Remaining gaps

A separate serial experiment used three planner controls and three treatments, alternating order, with the same Sol/high model, binary and four CPUs. The treatment sets a fresh opaque UUID in both endpoint `cache_key` and the `session-id` header, stable across each attempt. Median fresh input fell from 34,027 to 19,543 tokens (43%); median cache-read fraction increased from 25% to 72%. All six candidates passed tests and source review. Median wall changed from 59.8 seconds (47.8–66.0) to 57.7 seconds (55.3–63.9). Treatments made more calls (median 15 responses versus 13), so plans and context histories differ. The combined treatment supports an optional cache-affinity configuration, not a general speedup or isolated effect of either field. The native client explicitly uses the session header for ChatGPT affinity ([matching source](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/core/src/client.rs#L575)). Existing Smith endpoint fields support the configuration; [agent configuration](../../../crates/smith/README.md) gives an example.

Codex's observed input is predominantly cached, and its native tools can group commands into one model call. These differences accompany the measured token and operation gaps; the experiment does not isolate their causes. Differences in instructions, prompts, tool schemas, context management, and model sampling also matter. A low total-token count alone would conceal Smith's higher uncached usage. The next useful engineering checks are a larger repository cache-affinity experiment, clearer remaining-time guidance for delegation, and focused child scopes before attempting asynchronous child handles. An asynchronous design must retain the current write-authority exclusion and bounded ownership contracts. Additional repositories and providers would test how well these observations transfer.

The lease task exposed an unspecified token-exhaustion policy shared by the generated solutions: after the maximum token, a later claim panics before mutating its lease. Expiry implementation complexity also varies. These are review limitations outside the fixture's specified exhaustion contract, rather than hidden-test failures. Journal replay uses an in-memory FNV-1a checksum and logical buffered byte count; its results do not establish CRC or disk durability behavior.

An attempted benchmark configuration raised Smith turns from 64 to 128, above that binary's standard admission ceiling, and was refused before any inference. That setup error and its paired native control are excluded from the main comparison. The following trial restored the supported 64-turn setting. Later launches check the selected budget against the exact executable's validated capacity, including the final 256-turn profile. This was a harness configuration error, separate from the source-proven request limit.

## Validation and reproduction

The final production source `44e5120629237c5cc063461ecc41fc417c68a8b3` passes `cargo fmt --check`, all-target workspace Clippy with warnings denied, 1,494 focused tests in 3.259 seconds, and 17 fuzzy tests in 3.530 seconds. Suite times come from nextest summaries; compile-inclusive command walls are recorded separately. The release build also passes. All 34 lightweight harness checks pass. They cover timing, terminal identity, detached-process cleanup, native metric deduplication, resource normalization, binary admission and synthetic cases for scoped credential scanning and generated Cargo-cache exclusions; the exact result is retained in the artifact audit.

The shared Skein change passes formatting, all-target Clippy, 1,413 focused tests in 5.787 seconds and 80 fuzzy tests in 13.898 seconds. Required focused/fuzzy budgets remain satisfied. Initial checkout checks, intermediate exact-tip gates, focused regressions and allocator measurements are preserved rather than replaced by the final result.

The shared Skein shutdown mechanism lives on a separate isolated local branch. Smith's canonical Git dependency URL remains unchanged, but Cargo.lock pins unpublished local Skein commit `f5fca44f2c9ee92f2dc52d8dc845212ca692275f`. A remote-only checkout cannot necessarily retrieve it. The supplied Smith and Skein Git bundles and reproduction instructions preserve the exact source; no commits were published or pushed.

The sibling [artifact directory](../../../../smith-benchmark-2026-10-08/REPRODUCE.md) contains [normalized attempt metrics](../../../../smith-benchmark-2026-10-08/results.csv), [explicit main-matrix selection](../../../../smith-benchmark-2026-10-08/main-matrix-selection.json), frozen tasks and seed manifests, per-attempt prompts and harness hashes, raw traces, candidate source archives and diffs, independent grading/reviews, validation logs, source Git bundles and reproduction instructions. Historical diagnostic, setup-error and failed attempts remain separately labelled. Bundle manifests identify unpublished source pins; artifact checksums identify the portable handoff.

Three repetitions per final task provide useful engineering signals and ranges, but no robust population ranking or significance claim. Runtime versions evolve between task groups, and before/after runs include model-decision and environment changes. The final comparison retains native product differences deliberately. Results should therefore guide specific runtime and configuration work rather than be read as an isolated controller-speed or general coding-quality verdict.
