# 05s4: host-unit prices and global completion gate

This increment follows reviewed temporary Smith checkpoint `440e87d`.
It implements domain/run.md, section 9, and domain/session.md, section 6,
following Temper's 05s4 plan. First-version contraction and protocol rendering
remain later increments. Original repositories remain read-only; this draft
is not original-main acceptance or authorization to remove copied packages.

| Increment | State | Commit | Evidence |
| --- | --- | --- | --- |
| 05s4 Budget | Gated temporary checkpoint | This increment, after `440e87d` | fmt/clippy pass; 560 focused / 2.255 s; 11 fuzzy / 3.928 s; no skips. |

The host supplies one scalar allowance plus turns and wall time, and integer
prices for every model. Session prices each completion with one combined upward
rounding. Run consumes only own-charge deltas once across sessions; inclusive
child bills serve parent Turns and local shares. Root checks the next-completion
gate before provider effects. Ordinary finite exhaustion preserves current
calls and same-turn Finish; typed arithmetic failure remains explicit, with
real Turns and durable delivery evidence preserved.

The reviewer checks documented public boundaries, explicit reexports, descriptive
parameter names, blank item separation and domain/<file>.md citations, plus no
standard Result shadowing. Strict-subset control flow, checked ownership, original
recording/fuzzy oracles and unchanged receiving/test budgets remain mandatory.

## Still open

- Contract the session's first version in the next 05s4 increment.
- Reconcile original tips and pass shared and consumer gates before original-main acceptance.
- Extract protocol rendering and complete channel, transcript and executable integration.
