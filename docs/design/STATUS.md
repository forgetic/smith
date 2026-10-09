# Status

Updated 2026-10-09. How far smith's code is from its design, and which
plans take it there. The designer keeps this file (skein's
development.md, section 3.3).

## 1. Plans

| Plan | State | Takes the design to | Where |
|---|---|---|---|
| Domain completion | done 2026-10-07 | `domain/`: the run, session and tools domains conformed; the local host's domain, with transcripts, resuming, delivery in place and recovery of answered calls | `~/src/rust/plans/domain-completion-plan/ (not versioned)` |
| Protocol layer | done 2026-10-08 | `protocol/` as first designed: the codecs, both halves of the channel, the LLM and machine components, the agent process, spawning hosts, the local host's protocol layer | `~/src/rust/plans/smith-protocol-layer/ (not versioned)` |
| Testing, first pass | done 2026-10-08 | `testing.md`'s tiers: simulated worlds on skein's harness, the real loop, the binary end to end against fakes, an opt-in live suite | `~/src/rust/plans/smith-testing/ (not versioned)` |
| Reliability | active on debian-16gb-hel1-1 since 2026-10-09 | the 2026-10-09 revision of every document, with `shell.md`, `benchmarks.md`, `protocol/events.md` and `protocol/limits.md`: sessions 00–10 and 12. Merged increments are in its `STATUS.md` | `~/src/rust/plans/reliability-plan/ (not versioned)` |
| Claude Code, and smith on Anthropic | drafting | `benchmarks.md`, sections 6, 7 and 10: Claude Code as the third agent, smith's probes on Anthropic models. Reworked for the user's `~/.claude` login before it runs, after reliability | `~/src/rust/plans/reliability-plan/smith/11-claude.md (not versioned)` |

## 2. The design

Left names each section's unbuilt part and the plan that builds it. A
bare "session NN" is the reliability plan's, with its increments where
the session has merged some; "not planned" is in section 3.

| Document | Built | Left |
|---|---|---|
| `domain/README.md` | partly | 4: `smith-inline-agent` (session 03, from 03.3); the product's commands (session 10); skein's append streams, OAuth driver and durable replace adopted (sessions 02, 10); `smith-mcp` and supervised processes with a view, not planned |
| `domain/host.md` | partly | 2: the run's identity, `opens_window`, long operations' kinds (session 05, 05.7–05.9); lossless facts (session 02). 4: exit evidence, the watchdog over silence, the derived grace (session 06); the unread report (session 05); a contained tree, not planned. 6: resuming from the last window (sessions 05, 08). 7: `sign_in` on skein's driver, `borrow`, `env` (session 10). 8: lines carried and held, typed notices, "waiting" (session 05, 05.11–05.14); failures in words (session 06); headless (session 10); the chat's identity (session 08); an impossible spend refused (session 07); MCP host tools, not planned. 9: the inline agent and its stop ladder (session 03, 03.3–03.8). 12: an isolated mode, not planned |
| `domain/run.md` | partly | 3.1–3.2: the run's identity (sessions 05, 08), the reserve (session 07). 3.3: one directory hiding its name (session 09); the stable prefix, the summary instruction (session 08); `## Budget` and `## Your share` (session 07). 4, 5.3, 9: ceilings, shares, no nesting, the lease, the reserve and wind-down, the cache-write price (session 07); a brief past a window (session 04). 6: a finish crossing a message, deferred `wait` (session 05, 05.5–05.6); mid-work messages, not planned. 8.1: checks as long operations (session 05). 10: a model failure's limit (session 06; its endpoint, model and attempts are in no increment yet). 11: lossless facts and content (session 02), shares (session 07), compactions (session 08). 12: affinity, tool choice (sessions 07, 08). 5.3, 15: work beside a writing child, a child from its parent's prefix, not planned |
| `domain/session.md` | partly | 2: the affinity (session 08). 3: the stable prefix tested at the wire, window turns, resuming from one (session 08); blocks per message derived (session 04). 4: deadlines and retries by phase, waiting at a full pool, oversize and cut calls (session 04); typed limits and the kept detail (session 06). 5, 7: tool choice, a child's final turn (session 07). 6: cache writes at their own rate (sessions 07, 08). 8: admission against the window, compaction (sessions 04, 08). 11: provider-native compaction, not planned |
| `domain/tools.md` | partly | 2: one path namespace: the directory as `.`, `list /`, host aliases, resolved paths in failures (session 09). 3: `list` trees, `read` windows, `edit` lists, `shell` budgets (session 09). 4: one payload bound, stated (session 09; a call too large, session 04). 5: the environment the configuration's, process groups (session 09); containment, not planned. 8: a background-process tool, not planned |
| `protocol/README.md` | partly | 4–5: the one-process shape and `smith-inline-agent` (session 03), the service's event sink (session 02); `smith-mcp`, not planned. 8: what channel 2 (session 05), charter 2 (session 07) and transcript 3 (session 08) still add; versions in `smith check` (session 10). 9: skein's mechanisms (reliability, skein sessions 01–06); contained trees, Codex routing extras, not planned. 4, 11: a connected agent, not planned |
| `protocol/agent.md` | partly | 2: `.` at io, `list` trees, process groups (session 09). 3: the environment the configuration's (session 09); the view, not planned. 4: startup checks by name (session 04), reserved affinity headers (session 08), the sink's policy (session 02). 5: lossless facts, the stream through io, capture (session 02). 6: exit evidence, the stderr line, the teardown phase, a signal after the answer (session 06); resource usage in `session.ended` (session 02). 8: shipped times, the teardown invariant (session 06); memory at its worst case (session 04) |
| `protocol/channel.md` | partly | 3: the run's identity (session 05, 05.7); turns sent from the last window (session 08). 4: `opens_window`, long operations' kinds (session 05, 05.8–05.9). 7–8: lossless facts (session 02); the queue and terms derived (session 04). 2, 7: a connected agent's credential and refusals, not planned |
| `protocol/charter.md` | partly | 2: the cache-write price, the reserve, a child's share in its call (session 07). 5: versions in `smith check` (session 10). 6: the reserve within the budget (session 07); room for the derived limits, text within a window's host part (session 04) |
| `protocol/events.md` | partly | The codec, goldens and reader are built; nothing writes the stream. 2, 5: the stream through io, its sinks and policies, `session.ended` (session 02, 02.4–02.5); 5.3: capture, live text, response measures (session 02, 02.6). 3: records fed by sessions 05–10. 8: the conformance referee (session 02). `containment: "trees"`, not planned |
| `protocol/hosts.md` | partly | 3: exit evidence, no signal before the grace (session 06); the tree, not planned. 4: connecting later, not planned. 5.1: lines, notices, "waiting" (session 05); failures in words (session 06); the second interrupt's drop (session 03); headless (session 10). 5.2: TOML settings (session 10), the derived grace (session 06). 5.3: resuming from the last window, the chat's identity (session 08); the store over io (session 10). 5.4: skein's OAuth driver, `borrow`, `env` (session 10). 5.6: the inline agent by default (session 03) |
| `protocol/limits.md` | partly | 2.1–2.2: built; the profile at `shell.md`'s values (session 04, 04.14). 2.3: ceilings (session 07), graces (sessions 03, 06), `group_stop` (session 09), `write_deadline` (session 02). 3: derivation layer by layer, a window's room (session 04, after skein's `derive`). 4: startup checks by name (sessions 04, 06). 5: the memory pool (session 04). 6: oversize and cut blocks, waiting (session 04); compaction (session 08). 7: progress deadlines (session 04). 8: typed limits carried (session 06). 9: effective limits (sessions 04, 10) |
| `protocol/llm.md` | partly | 2.1–2.3: affinity, the stable prefix, Anthropic's breakpoints (session 08). 3: the tools' new schemas, honest descriptions (session 09); `sub_agent` shares, nesting, tool choice (session 07); deferred `wait` (session 05). 4: oversize, cut and dropped blocks (session 04). 5: UTF-8, resolved paths (session 09); results owed to a compaction (session 08); calls not run (session 07). 6: measures and live text (session 02); deadlines, retries, waiting (session 04); closing at the answer, typed limits (session 06). 8: limits derived and checked, the memory pool (session 04). 12: edit formats per dialect, not planned |
| `protocol/transcript.md` | partly | Version 3 holds optional usage only. 2–3: a turn's kind and the window turn, the affinity in the opening turn (session 08); cut and oversized calls (session 04); a call not run, with why (session 07); `opens_window` (sessions 05, 08). 4: refusals for the affinity and windows (session 08). 7: blocks per message derived, a resume holding a window (session 04) |
| `shell.md` | partly | 2.1–3.2: built, but for the inline agent (session 03) and the sink through io (session 02). 2.4: skein's private files, OAuth driver, trust roots, terminal modes (reliability, skein sessions 01, 02, 06). 4: `smith`, `exec`, `check`, `login`, exit codes (session 10; the last line's words, session 06). 5: settings in TOML (session 10). 6: presets (session 10), the profile's values (session 04), run defaults (session 07). 7: credentials (session 10). 8: the build environment (session 10); per-tree limits, not planned. 9: one process (sessions 03, 06, 10); `isolate`, not planned |
| `testing.md` | partly | 2.1, 2.6: the host world with both kinds, the kind as a parameter (session 03); fault sweeps, not planned. 2.2: real-loop signals, not planned. 2.3: `exec`, `check`, `login` and interrupt stories, live stories through `smith exec` (session 10). 2.4: benchmarks (session 00). 2.5: derivation properties for pinned constants (session 04). 5: the reacting person, message referees (session 05). 6: complete events (session 02); shipped times, the teardown invariant (session 06); replay by digests, containment checks, not planned |
| `benchmarks.md` | partly | Formats, schemas, statistics, the guard and arms are built (session 00, 00.1–00.7 and 00.9). 3.2, 6: the runner, the Codex adapter, smith's legacy and events faces, event checks (session 00); Claude Code (its own plan, drafting). 4, 5.4, 11: the smoke suite, probes, fixtures 01–02, back-fill, baselines (session 00); fixture 03, `shell-cwd`, experiments, comparisons, audits (session 12). 11.1: the `mid-work` and `build-environment` probes, not planned. 15: beyond serial runs, not planned |

## 3. Not planned

- **Contained trees** (`protocol/agent.md`, sections 3 and 7; also
  `shell.md`, 8, `domain/host.md`, 4 and 9.3, `protocol/hosts.md`, 3 and
  5.5): searches, commands, checks and agents in trees with views, git
  directories read-only, write scopes, network policy, per-tree limits
  and build profiles. Waits for a skein design and plan of their own
  (skein's `draft/process.md`); until then, plain children with process
  groups (session 09).
- **An isolated mode** (`shell.md`, section 9; `domain/host.md`,
  section 12): the product's `isolate`, the spawned agent as a
  containment boundary. Waits for contained trees.
- **Work beside a writing child** (`domain/run.md`, section 5.3): a
  read-only parent beside one writing child, then scoped write sets,
  then asynchronous children. Waits for contained trees and a document
  of its own.
- **A child from its parent's prefix** (`domain/run.md`, section 15).
  Waits for the `parallel-children` and `budget` probes to show children
  re-orienting at a material cost.
- **Messages mid-work and threshold notes** (`domain/run.md`, sections 6
  and 9.2). Waits for sessions 05 and 07; the `mid-work` probe comes
  with it.
- **A background-process tool** (`domain/tools.md`, section 8): start,
  poll, stop. Waits for process groups (session 09) and contained trees.
- **Provider-native compaction** (`domain/session.md`, section 11).
  Waits for the `compaction` experiment (session 12) to favour it.
- **Edit formats per dialect** (`protocol/llm.md`, section 12). Waits
  for the `edit-format` experiment, which session 12 may run.
- **Codex routing extras and WebSocket** (`protocol/README.md`,
  section 9). Waits for the `affinity` experiment to show a material
  gain; WebSocket is on skein's roadmap.
- **MCP as a tool source** (`protocol/README.md`, sections 5 and 11;
  `domain/run.md`, section 5.1): `smith-mcp`, and through it the local
  host's first host tools. Waits for its design.
- **A connected agent** (`protocol/hosts.md`, section 4;
  `protocol/channel.md`, sections 2 and 7): a daemon a host connects to,
  with its credential and refusals. Waits for the domain's contract:
  admission, cancellation, a lost connection and the daemon's own life.
- **The testing cleanup pass** (`testing.md`, sections 2.1 and 6;
  smith-testing's `later.md`): fault sweeps asserting each fault fell;
  `tests/protocol-llm` shrunk to a protocol world; referees for
  `tests/run`, `tests/tools` and `tests/host` beyond the rules sessions
  05, 07 and 09 add; a focused memory test for `tests/agent`; tiny
  limits; a failed write to the agent kept apart from a malformed agent.
  Waits for nothing.
- **Real-loop signals** (`testing.md`, section 2.2): the referee's real
  interrupt cancels a run, and a second stops its tree. Waits for
  process groups (session 09).
- **Replay by digests** (`testing.md`, section 6). Waits for skein's
  harness to take digests of the state.
- **Containment checks** (`testing.md`, section 6), with the
  `build-environment` probe (`benchmarks.md`, section 11.1). Wait for
  contained trees, cgroup v2 delegated to the user and unprivileged user
  namespaces.
- **Benchmarks beyond serial runs** (`benchmarks.md`, section 15):
  concurrent attempts, task kinds beyond code, a commit hook for smoke
  runs. Waits for the suites to outgrow serial runs.

## 4. Drafts

| Draft | About | Next |
|---|---|---|
| `draft/one-host.md` | smith's side of jig's one host: the inline agent | Nothing in smith: adopted into `domain/host.md`, section 9, built by session 03. jig's side is on jig's schedule |
