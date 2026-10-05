# 05s4: caller conventions

This increment follows reviewed temporary Smith checkpoint `f9c9981`.
It replaces fixed guide/check paths with host-supplied bounded policy in
`Charter.conventions`, implementing domain/run.md, sections 3.1, 3.3 and 8.1.
Smith defaults to `AGENTS.md` and `.smith/check`; Temper-style callers explicitly
select `.temper/pre-pr`. Smith interprets no host workflow or provider grammar.

## Contracts and ownership

Custom paths replace both defaults unchanged. Admission refuses empty,
non-relative, malformed-component, NUL, ASCII-control, backslash or oversized
paths before effects. Each path is bounded by the existing 4,096-byte marker
vocabulary. The existing marker spelling helper is shared without changing
Marker acceptance. The host attests UTF-8 as for other charter text.

Both owning payloads count against aggregate `run_bytes`; inline Box wrappers
belong to Charter's slab storage. The selected immutable pair drives Read,
writable-only Probe, main/child prompt labels and actual exclusive Check.
Queued root openings separately price per-mount rendered path copies using
checked arithmetic. Session receiving limits still decide whether the actual
rendered opening fits after discovery; this increment does not change that
ordering or the receiving caps of existing fixtures.

## Evidence

Focused controls establish default/custom/explicit legacy selection, decoy
absence, readonly guide/no-check behavior, actual Check before Deliver, and
cancellation waiting for the real Abort terminal. Unsafe paths and 4,097-byte
paths refuse original Start before effects. Two 4,096-byte paths attain exact
aggregate admission, and an independent aggregate byte overflow is refused.
Repeated maximum guide headings exceed actual session receiving while a
same-limit default-path companion reaches Complete and settles cancellation.

The existing run memory driver keeps full aggregate charters, guide payloads,
child openings, answers and outcomes while inspecting actual maximum
Read/Probe/Check path owners and settled typed checks/delivery. Its selected
caller policy is constructed before the component Meter baseline. The root
memory driver independently prices a retained caller Charter with both maximum
paths, two mounts and three repeated path labels, full actual Prompt copies,
and two overlapping native Clients. Both physical settlement and final zero
retention are checked. Existing 12/six and 28/14 Message/Turn histories, actual
8,192-byte person text, exact receiving credit and independent message/byte
overflows remain unchanged. The native fixture calls only real Wait/text;
it adds no application Finish decoder or fabricated terminal.

Final frozen Rust source is the 17-file manifest
`conventions-freeze-6.txt`, SHA-256
`8322edb651a1a80a6c6a6492b0d5daa9d3cb6bb251d1ce725a97d0c66cc5bdb3`.
All 13 canonical Skein packages still resolve reviewed temporary `5bf93a6`;
Cargo.lock blob is `fcc918747b9b198b40b3e9981ce9fdce00b18541`.
Exact source review and final gate results are recorded in this increment's
commit message and the handoff evidence. No lint, test or budget waiver is used.

| Increment | State | Source | Evidence |
| --- | --- | --- | --- |
| 05s4 Conventions | Gated temporary draft | This increment on `f9c9981` | fmt/clippy pass; 512 default / 2.233 s; 11 fuzzy / 3.993 s; zero skips. |

Serial measurements pass 512 default tests in 20.849 seconds and eleven fuzzy
tests in 8.996 seconds, with zero skips. The serial `measure` profile has no
budget; the parallel gate above retains the 15/60-second budgets. Logs are
`smith-conventions-{fmt,clippy-6,default,fuzzy,serial-default,serial-fuzzy}.log`
in the handoff evidence.

## Still open

- Optional Start workspace, git kind and initial conflict paths.
- Separate instructions and ordered titled brief sections.
- Scalar prices/budget and first-version contraction.

The broader Smith channel/transcript/runtime/local-host and Temper integration
remain later plan increments. Original repositories are read-only in this
session; shared SDK mandatory default still fails `io_uring` setup with `EPERM`.
This temporary source is not original-main acceptance or authorization to
remove copied provider/OAuth/fake packages.
