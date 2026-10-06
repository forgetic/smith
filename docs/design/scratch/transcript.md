# Concrete session record codecs

Scratch proposal, outside the current domain contract and 05s plan.

The protocol owns the complete lossless VERSION2 Turn and Transcript byte
representation. This contract specifies receiving bounds and ownership; it
confers no execution authority and leaves semantic restore admission in the
domain.

The sections below incorporate independently approved codec contract revision 2.
Contract approval does not establish source implementation, integration or
runtime gate acceptance. The format is new and unshipped; all implementation
and evidence obligations below remain mandatory.

## 1. Owner, concrete API and dependency direction

Crate `smith-transcript`, protocol-only, no_std+alloc, no IO/time/randomness/state machine. Direct dependencies are skein-lib and the actual Smith record vocabulary: smith-domain-session and smith-domain-tools (the latter is required to construct/match the actual nested Call/Outcome/Path/Name types). It imports no smith-domain root, smith-host-domain, Smith provider codec, JSON or shared Client. Root/session never import this codec. Protocol/channel adapters call it and give the existing typed record to domain admission. The direct dependencies are parent-selected for this increment. There is no extra proxy schema or alternative record vocabulary. Future lower schema extraction remains separate work.

Required explicit exports:

```rust
pub enum Error { Version, Malformed, TooLarge }
pub struct Limits {
    pub turn_bytes: u32,
    pub transcript_bytes: u32,
    pub turns: u32,
    pub messages: u32,
    pub blocks: u32,
    pub entries: u32,
    pub field_bytes: u32,
    pub replay_bytes: u32,
    pub payload_bytes: u64,
    pub owned_bytes: u64,
}
pub struct Footprint {
    pub encoded: u32,
    pub owned: u64,
    pub heap: u64,
}
pub fn measure_turn(value: &record::Turn, limits: &Limits) -> Result<Footprint, Error>;
pub fn encode_turn(value: &record::Turn, limits: &Limits) -> Result<Box<[u8]>, Error>;
pub fn decode_turn(bytes: &[u8], limits: &Limits) -> Result<record::Turn, Error>;
pub fn preflight_turn(bytes: &[u8], limits: &Limits) -> Result<Footprint, Error>;
// The same four concrete functions for record::Transcript.
// The same four concrete functions named *_optional_transcript for Option<record::Transcript>.
```

`Footprint.owned` includes one actual root value cell plus every owned allocation; heap excludes that root inline cell. Nothing is resolved, repriced or counted as a new accepted completion. Functions borrowing typed inputs do not clone them. Functions borrowing raw input return a wholly owned graph after successful preflight. Exact final encode owns one box and writes directly into its exact allocation. No callback/generic protocol/trait/function pointer/glob/JSON encoder is required. Every public item, field and variant gets state/ownership/receiving/error documentation citing domain/session.md§3/12 and lib.md's bounded encoding rules.

Limits have no arbitrary generous default. `turn_bytes` caps a standalone Turn; `transcript_bytes` caps a Transcript and the complete optional wrapper when that API is used. Aggregate counters count all turns, all messages including after, all Blocks and all nested Part/Entry/Hit/u32 ambiguity-list cells. `entries` is their aggregate nested-array cap, not a count per individually small array. Every ordinary byte field obeys field_bytes; every Replay payload obeys replay_bytes. Both contribute to aggregate payload_bytes. owned_bytes bounds actual cells plus payload, including empty-cell-array costs. Caller protocol limits must be derived to admit the original maximum source world records; no smaller codec allowance silently replaces old Session receiving limits. Domain identity/shape/effect/budget admission is still independent.

## 2. Closed byte grammar

This is a new concrete format, not an existing shipped VERSION2 claim. All integers are unsigned fixed-width big endian; all counts/byte lengths are u32. No varints, padding, native layouts, architecture-sized fields, reserved extension tails, duplicate maps or alignment bytes. A byte vector is `u32 length` then exactly length bytes; empty is length0. An array is `u32 count` then that many elements in order. Option is one u8 tag:0 None,1 Some followed by its exact value. Boolean is exactly u8 0/1; all other tags are Malformed. Unknown enum discriminants are Malformed. UTF-8 is an explicit new protocol construction rule for llm Text/Refusal text, following that vocabulary's typed text sender contract. Current Domain admission does not reject every arbitrarily constructed invalid typed Text/Refusal, so this codec rule is not a claim about existing Domain validation; all other byte fields, including Returned::Text, tool input/output, problem fields, opaque and replay, stay exact arbitrary bytes. ToolCall input is never parsed as JSON here. Endpoint/dialect are numeric identities, never provider selectors/address resolvers.

Root Turn:

```
magic[4] = 53 4d 54 52             // ASCII SMTR
version:u16 = actual Turn.version // must equal record::VERSION=2
kind:u8 = 0                      // standalone Turn
endpoint:u32, dialect:u32, sequence:u32
usage.input_tokens:u64, usage.output_tokens:u64
usage.cache_read_tokens:u64, usage.cache_write_tokens:u64
spent:u64, spend_overflow:bool
messages:array(Message)
```

Root Transcript:

```
magic[4] = SMTR, version:u16 = actual Transcript.version (2), kind:u8 = 1
endpoint:u32, dialect:u32
turns:array(NestedTurn)
after:array(Message)
```

NestedTurn contains `version:u16` (2), then every field following kind in Root Turn in the same order; it has no repeated magic/kind. Its endpoint/dialect are retained exactly even when different from the enclosing Transcript; the domain then gives precise Endpoint/Dialect refusal. Its sequence may be zero/noncontiguous structurally; domain decides Malformed. Likewise empty messages/turns and wrong call/result roles are structurally representable and retain domain classification rather than being accepted histories automatically. Minimal structurally valid Root Turn is64 bytes; NestedTurn59; Root Transcript23; Message5.

Optional Transcript is `0` alone for None, or `1` then the complete Root Transcript. Some(empty Transcript) is distinct from None and includes all version/identity/count fields. Full consumption applies to the option too. The optional API is not a second layer automatically added inside a channel's already optional opaque field: channel adapter selects one representation explicitly and counts every wrapper actually retained.

Message: `role:u8` (0 User,1 Assistant), then content:array(Block). Blocks remain in exact original order.

| Block tag | Exact following fields, in order |
| --- | --- |
| 0 Opaque | bytes:vector |
| 1 Refusal | text:vector UTF-8, replay:Option(Replay) |
| 2 Text | text:vector UTF-8, replay:Option(Replay) |
| 3 ToolCall | id:vector, name:vector, input:vector, call:Decoded, replay:Option(Replay) |
| 4 ToolResult | id:vector, result:Returned |

Replay is only bytes:vector under replay_bytes, including the complete actual inner envelope. Some(empty) and None differ structurally. An inner envelope that the real Client cannot interpret stays present and receives the Client's existing Unsupported/Invalid behavior when used; this protocol codec does not strip it or pretend a malformed inner envelope is provider-usable. No phase field is fabricated beside the actual Replay bytes.

| Decoded tag | Exact payload |
| --- | --- |
| 0 Owned | call:tools::Call |
| 1 Delegated | ticket.raw():u64, effect:u8 (0 Read,1 Write) |
| 2 Invalid | problem:Problem |
| 3 Historical | none |

| Returned tag | Exact payload |
| --- | --- |
| 0 Owned | outcome:tools::Outcome |
| 1 Invalid | problem:Problem |
| 2 NotRun | none |
| 3 Text | text:vector arbitrary bytes, error:bool, replay:Option(Replay) |
| 4 Withdrawn | none |

Problem tags:0 UnknownTool,1 NotAnObject,2 Missing(field:vector),3 WrongType(field:vector),4 BadValue(field:vector),5 TooLarge. Problem field bytes are neither normalized nor rendered. The codec does not replace these variants with their human text.

Path: absolute:bool, parts:array(Part). Part tags:0 Name(name:vector),1 Current,2 Parent. Name bytes obey the actual tools::Name invariant: nonempty, neither `.` nor `..`, no slash or NUL. This is construction/structural validity, not path normalization or checkout authority. Preserve exact component order, absolute flag, Current and Parent. No joined-path surrogate.

| tools::Call tag | Exact payload |
| --- | --- |
| 0 Read | path:Path, skip:u32, lines:Option(u32) |
| 1 List | path:Path |
| 2 Search | path:Path, pattern:vector, glob:Option(vector) |
| 3 Write | path:Path, content:vector |
| 4 Edit | path:Path, old:vector, new:vector, all:bool |
| 5 Shell | command:vector, timeout:Option(u64 nanoseconds) |

Duration uses the actual as_nanos()/from_nanos() value; no unit conversion/truncation. Its cap and effect meaning belong to domain admission. Name has public as_bytes/new; preflight uses its documented borrowed validity predicate before allocating, then constructs it in decode. Parent selected one public borrowed `tools::Name::valid(&[u8]) -> bool`, reused by actual `Name::new`, allocation-free preflight and typed measurement. The codec does not maintain a second independent Name predicate. Its docs must state the exact construction invariant above, borrowed input/no allocation/no normalization, and bounds/citations; existing meaningful positive and negative invariant controls remain, with added controls proving the borrowed predicate and constructor agree, including arbitrary non-slash/non-NUL binary components. Name construction occurs only in the successful decode pass.

Entry: name:vector satisfying Name, kind:u8 (0 File,1 Directory,2 Link,3 Other). Hit: path:vector, line:u32, text:vector. Exit tags:0 Code(code:u8),1 Signal(signal:u8),2 TimedOut. Fault tags:0 Denied,1 NoSpace,2 Other.

| tools::Outcome tag | Exact payload |
| --- | --- |
| 0 Read | content:vector, skipped:u32, lines:u32, total:u32, cut:bool |
| 1 Listed | entries:array(Entry), more:u64 |
| 2 Found | hits:array(Hit), more:u64, timed_out:bool |
| 3 Written | created:bool |
| 4 Edited | replaced:u32 |
| 5 Exited | exit:Exit, head:vector, tail:vector, dropped:u64 |
| 6 NotGranted | none |
| 7 Outside | none |
| 8 ReadOnly | none |
| 9 TooLong | none |
| 10 NotFound | none |
| 11 NotFile | none |
| 12 Linked | none |
| 13 Protected | none |
| 14 NotDirectory | none |
| 15 TooLarge | size:u64 |
| 16 NotRead | none |
| 17 Stale | none |
| 18 NoMatch | none |
| 19 Ambiguous | count:u32, lines:array(u32) |
| 20 Unchanged | none |
| 21 Failed | fault:Fault |
| 22 TimedOut | none |
| 23 Cancelled | none |
| 24 Busy | none |
| 25 NulByte | none |

All of these typed fields are encoded exactly, including reported counts that the codec cannot independently verify. No sorting, deduplication, accounting repair, inferred bill, normalized path, renumbering or text rendering occurs. The grammar is finite/nonrecursive: nested containers have fixed schema depth.

## 3. Checked preflight, allocation and error contract

Decode first rejects input whose length cannot fit u32 or exceeds the selected full encoded cap, before constructing skein_lib::Reader (Reader::new asserts u32 representability). That is TooLarge. Then an allocation-free full structural walk uses a concrete Reader, checked aggregate counters and actual size_of values. It checks magic, known kind, supported version, all exact counts/tags/booleans/byte spans/UTF-8/Name construction invariants, actual fixed field widths and final consumption. No peer-declared List/Box capacity is allocated during this walk.

Error priority is explicit: initial full encoded cap/representability first; then first failing field in grammar order. A complete unsupported version field is Version; a truncated version field is Malformed. Invalid magic/kind/tag/boolean/UTF-8/name, truncated bytes/scalars/elements and any trailing byte are Malformed. A declared count/length above its receiving cap, aggregate cap violation or checked ownership/count/length/product overflow is TooLarge, checked before using it. When an in-cap count's checked minimum encoded size exceeds remaining bytes, it is Malformed. This distinction prevents a short hostile body from reserving huge decoded arrays, without changing domain refusals.

Useful minimum element checks are Message5, NestedTurn59, Block5, Part1, Entry6, Hit12 and u32-list4. Both minimum-product and exact full scan are required; the former is not a substitute for later fields/aggregates. Empty arrays still retain the parent pointer/value and any array-element cells; empty payload is not permission for unbounded wrappers. Count limits apply before allocation even if every payload is empty. No u32-to-usize conversion is assumed infallible before checked target representability. Every prospective allocation additionally obeys the per-allocation Layout/isize guards below; an aggregate u64 ownership cap is not an allocation guard. Optional None is checked and fully consumed, not treated as zero-length corrupt input.

Only after the full preflight succeeds does a second concrete Reader pass build the actual boxed graph, with exact validated capacities (Skein List or a concrete exact array builder) and moved children. No values are cloned. On this same immutable borrowed slice, a structural second-pass failure is an implementation defect, not a peer-triggered partial-allocation policy. Preflight returns actual Footprint; the decode pass checks its final cursor. No scratch graph, JSON tokens or placeholder record graph is created first. A failed preflight must allocate zero bytes, including a malformed field at the last nested element.

Measurement over an already typed value is likewise allocation-free and uses the same field/tag/order/limit rules, checked sums and every prospective decoded-graph/output allocation guard below. Typed slice counts/lengths must fit their wire u32 before those checked target conversions/products. Existing allocation of a typed input does not exempt measurement from the codec's target-representability contract. It rejects unsupported typed version with Version and invalid Text/Refusal UTF-8 with Malformed, never silently changes them. Encoding measures first, allocates one exact final Writer box, writes directly in field order, checks exact written length and returns that box. No intermediate per-Turn encoding, concatenation Vec, retained child serialized boxes or guessed slack. The implementation must share the concrete measure/write traversal without traits/generics/callbacks or a second drifting grammar. Writer/Reader helpers remain concrete; all source shapes and receiving checks get independent review.

## 4. R1: every prospective allocation must be target-representable

This is an additional independent admission requirement, not a replacement for encoded/count/payload/owned receiving caps or checked u64 graph sums. Before any decode construction allocation, the allocation-free preflight validates **every separate actual backing allocation**. Before any encode output allocation, allocation-free typed measurement validates those same prospective graph allocations and its exact final output. Encode must use that successful measured length; preflight also validates the exact complete canonical encoded length for the corresponding final Writer. Root inline values and stack parser state are not heap allocations and do not acquire fictitious Layout charges.

For each wire `count` or byte `length`, first enforce its receiving cap/aggregate counter in field order, then perform all applicable allocation checks without allocating:

1. Convert the actual count to target `usize` with `usize::try_from(count)`, returning `TooLarge` on failure. For typed measurement, convert the actual slice length to wire `u32` checked first and then use the same checked target count; never cast or assume a u32 fits usize.
2. Compute `allocation_bytes = target_count.checked_mul(size_of::<ActualCell>())`, returning `TooLarge` on target-sized product overflow. Byte boxes and the final output use the actual byte cell `u8`; no guessed/mirror layout or generous constant is allowed. Separately maintain the existing checked u64 ownership products/sums.
3. Require `isize::try_from(allocation_bytes)` to succeed, so a prospective nonempty Rust allocation never exceeds `isize::MAX` bytes. Then check `core::alloc::Layout::from_size_align(allocation_bytes, align_of::<ActualCell>())` (or equivalent standard actual-cell Layout calculation) successfully, including its alignment/padded-size representability condition. A failed Layout or isize check is `TooLarge`. No hand-written alignment/headroom or replacement cell may stand in for an actual compiled type.
4. Apply the same check to **each** allocation even when all aggregate u64 sums and wire u32 lengths fit. Only after the full remaining structural walk/full consumption succeeds may actual `List::with_capacity`, byte boxing or `Writer::new` occur. In the decode pass each construction site must visibly use a previously validated exact count; no unchecked capacity derived elsewhere is allowed.

| Prospective backing owner | Actual cell used for checked product and Layout | Required separate checks |
| --- | --- | --- |
| Transcript turns box | `record::Turn` | Every turns count -> usize, checked actual product, isize/Layout before List/box construction |
| Each Turn messages box and Transcript after box | `llm::Message` | Each individual array, as well as aggregate messages; after is not exempt |
| Every Message blocks box | `llm::Block` | Each individual array, even empty-payload blocks; aggregate blocks remains separate |
| Every owned Call Path parts box | `tools::Part` | Each Read/List/Search/Write/Edit path array independently |
| Every Listed entries box | `tools::Entry` | Each nested result array independently |
| Every Found hits box | `tools::Hit` | Each nested result array independently |
| Every Ambiguous lines box | `u32` | Each nested result array independently; count metadata remains independently preserved |
| Every ordinary owned byte box | `u8` | Each Opaque, Text/Refusal, ToolCall id/name/input, ToolResult id, Returned::Text, Problem field, Path/Entry Name, Search pattern/glob, Write content, Edit old/new, Shell command, Read content, Hit path/text and Exited head/tail vector independently |
| Every Replay bytes box | `u8` | Each Some payload independently, including Text/Refusal/ToolCall and Returned::Text Replay; no fictional Box<Replay> allocation |
| One exact final Writer backing/output box | `u8` | Complete measured output length -> u32 and usize checked, checked `count * size_of::<u8>()`, isize/Layout; applies to Turn, Transcript and complete optional Transcript including its tag |

There are no separately allocated Call/Outcome/Path/Replay/Option enum wrappers in this actual tree; their inline cells are already charged in the actual enclosing array/root. Any additional allocation introduced by implementation, including scratch storage, must be identified and guarded/priced; it cannot be hidden by this table. Exact filled List capacity -> boxed slice and exact filled Writer -> boxed bytes must not create a larger unguarded reservation. Source review must trace `skein-lib/src/list.rs::with_capacity` to actual `Vec::with_capacity`, its `into_boxed` path, every byte-box constructor, and `skein-lib/src/writer.rs::new`/`finish` to verify guards dominate allocation. Typed measure/encode cannot rely on List panic, capacity-overflow panic, allocation failure, the caller's overall budget or an encoded u32 cap to reject a target-unrepresentable allocation.

Zero count/length remains wire-visible but requires no heap allocation; multiplication yields zero and zero-sized Layout is not passed to a real allocator. Empty arrays/byte boxes retain their actual inline parent/pointer cells already charged above. For an actual zero-sized cell, a representable nonzero count also has zero backing bytes, so Vec/box uses its zero-sized convention rather than a fake positive heap allocation; conversion/counter checks still apply. The listed actual record/message/block/part/entry/hit/u32/u8 types are nonzero-sized; this zero-size rule is honest general allocator handling, not a new mirror type or omitted charge. No `alloc` call may receive a zero Layout. These representability checks prove capacity/Layout validity, not that all possible machine memory is available.

Guard-error precedence is explicit: receiving/aggregate cap, target conversion, target checked allocation product and isize/Layout failures are `TooLarge`, before construction and before the declared-count minimum-remaining-body check. An in-cap, allocation-representable count whose minimum/exact body is truncated is `Malformed`. Complete unsupported version remains `Version` in grammar order. The final Writer guard runs in allocation-free preflight/measurement, before `Writer::new`, even when many separately representable boxes produced a total output beyond the target's single-allocation bound. No full consumption or late nested corruption may allocate during preflight.

Meaningful future representability evidence must exercise the guard arithmetic without trying to exhaust memory. On each supported target, direct concrete arithmetic controls use actual compiled cell sizes/alignments and test zero, exact accepted boundary, first rejected boundary, failed conversion/product and Layout failure where reachable. An actual supported 32-bit target permits counts whose `count * size_of::<llm::Block>()` fits usize but exceeds isize::MAX; that rejected count and a complete-output byte length exceeding isize::MAX must return `TooLarge` before any allocation (ordinary constructor-only controls may call the same allocation-free concrete guard). For every listed cell, test the largest reachable valid u32 count below the target Layout/isize boundary and the next reachable count, with other caps roomy; do not build giant graphs to obtain this evidence. On a 64-bit target some boundaries cannot be reached through a wire u32 count; state that honestly, use meaningful pure target-sized guard boundary controls and independently review each actual-cell call site instead of inventing a wire overflow. Encoded u32 sum overflow, late malformed/count-product zero-allocation controls and all-owner meters are still mandatory. No actual 32-bit execution result is claimed here; target arithmetic tests and exact source review remain future proof obligations. Do not shrink source/world maxima to make guards unreachable.

## 5. Exact ownership and simultaneous memory

Let S(X) be checked u64 conversion of actual size_of::<X>(). For a Transcript graph:

```
H = turns*S(record::Turn)
  + total_messages*S(llm::Message)
  + total_blocks*S(llm::Block)
  + total_path_parts*S(tools::Part)
  + total_list_entries*S(tools::Entry)
  + total_search_hits*S(tools::Hit)
  + total_ambiguous_lines*S(u32)
  + sum(length of every separately owned byte box)
owned = S(record::Transcript) + H
```

For a standalone Turn omit the Turn array term and use S(record::Turn) as root. For Option<Transcript> use its actual root size rather than adding another fictitious Transcript allocation; None has no graph heap. Every Opaque/text/id/name/input/problem/command/output/path-name/Hit/Replay byte box is in the payload sum. Inline nested tools::Call/Outcome/Path/Replay/Option wrappers are already included in the actual Block/Part/Entry cells; they must not be omitted or double-charged. Replay has an inline wrapper with a nested byte box, not a separately allocated Box<Replay>. A List with exactly admitted filled capacity converts to Box without a larger retained backing; any real temporary/capacity overlap observed in implementation must be priced, not assumed away.

Codec limits bound H and the actual root cell independently of encoded size; raw bytes alone do not bound empty-wrapper amplification. The encoded vector length is a separate measured checked sum; zero payload and maximum wrapper-count cases are mandatory controls. Exact preflight arithmetic depends on actual compiled type layouts, not a hand-written layout mirror. Parent will measure those after source exists. Raw input is borrowed and remains caller-owned through decode. Thus decode simultaneous ownership is raw length N + decoded root/heap D, plus only fixed concrete parser/builder stack/value state. When raw input, decoded value and exact re-encoded output are all retained, the measured bound is N + D + E, not max(N,D,E); measure/write borrows do not release either existing owner. If a channel frame/raw body copy, host retained copy or queue handoff is also live, add each distinct actual owner and its wrappers once. No drop-before-process assumption may hide them.

A max API bound may be computed from the independent actual count/payload caps with checked products and sums, but exact preflight/measure values govern each entry. No old V1 unused allowance is recycled as an unproved record budget. Full/one-short controls independently exercise encoded bytes, owned cells/payload, aggregate counts, per-field and replay caps while other dimensions have room. Meter invalid preflight at zero allocation; admitted maximal decode at raw+decoded; encode at typed+exact output; three-owner roundtrip at their sum; explicit drops must return every owned allocation. Include all nested array variants, complete Replay Some, empty wrappers, maximum caller-owned plaintext/result bytes and real root transit alongside the record codec.

## 6. Domain boundaries deliberately retained

Codec success proves structural representation and receiving ownership only. Endpoint/dialect match, contiguous sequence, role/call-result pairing, one assistant per turn, yielded unresolved tails, Historical execution prohibition, waking prompt/reservation admission and semantic restore classification remain in current Session/root. Current session admission gives Version/Endpoint/Dialect/Malformed/Unresolved/TooLarge distinctly. Codec error maps only Version/Malformed/TooLarge at the protocol entrance; it does not classify endpoint mismatch, re-execute Owned/Delegated, reprice usage/spent or fabricate scopes/financial witnesses.

All four historical Usage values, cumulative spent and its sticky spend_overflow are copied exactly. No repricing, sum comparison, settlement-order inference or monotonic spend constraint is added. Restore begins new activation accounting under current source rules. Current Turn contains no Stop, activation, financial journal or usage_overflow; this codec must not synthesize any. Root must never import this codec/renderer and introduce a domain→protocol dependency cycle.

Returned::Text replay Some has mandatory positive codec coverage. The current real adapter in `smith-protocol-llm/src/prompt.rs::result_text` still returns Unsupported before Client preparation for Some because shared ToolResult has no replay field. Preserve that negative exactly. No stripped result replay, fabricated shared support or successful real provider request is claimed from codec roundtrip. Text/Refusal/ToolCall replay that the real adapter supports must also retain exact phase/id/unknown fields and pass the actual Client positive path.

## 7. Required evidence and exact future commands

Required fixture location: `crates/smith-transcript/src/golden/v2/*.bin`, explicit fixture-name manifest and explicit literal typed values. Normal tests are read-only, compare encoder output with files, decode into the exact value, require re-encoding to those bytes, and fail missing/stale files or unvisited manifest entries. Required opt-in regeneration command:

```
cargo test -p smith-transcript regenerate_record_goldens -- --ignored
cargo test -p smith-transcript golden_drift
cargo test -p smith-transcript literal_bytes
```

The ignored entry is fixture tooling only, not an ignored scenario/acceptance test. It must not edit independent literal arrays; normal gates run every meaningful control. Existing Temper commands remain unchanged:

```
cargo test -p temper-channel regenerate_goldens -- --ignored
cargo test -p temper-channel tests_v2
cargo nextest run -p temper-channel-world
```

Do not run legacy regeneration to accommodate this new Smith codec. Verify all250 legacy binary hashes and fixture manifests independently; retain default/V1 byte equality and all old V2/header/unknown-kind rules, worlds, random draw order and heap controls. Existing channel focused memory tests are demanded_body_and_decode_stay_within_exact_bound, maximum_start_arrays_and_fields_stay_within_bound, maximum_v2_start_arrays_conflicts_and_transcript_stay_within_bound and typed_v2_payload_heaps_and_malformed_counts_are_bounded. Original fuzzy_machine remains128 seeds plus4096 arbitrary-byte cases; fuzzy_versions remains64 seeds plus4096 bounded payload cases. Smith original Session/Agent seed sweeps and memory controls remain unchanged.

Independent literal byte assertions are required for magic/version/kind, big-endian u32/u64 extrema, every boolean/Option/role/block/Decoded/Returned/Problem/tools Call/Outcome/Path/Exit/Fault discriminant and positions, Some(empty)/None distinction, all four Usage fields, overflow flag and nested Turn header. Literal bytes are manually specified from this grammar and never regenerated by calling the production codec. Include at least one complete actual admitted Turn/Transcript literal, not only subcomponents; also structural empty values that domain refuses separately. The root minimum lengths64/23 are arithmetic checks, not replacements for complete literal records. Selected literal extrema distinguish every raw usage column and spent, preventing a swapped field from passing a self-roundtrip.

Malformed controls: every truncation offset of selected full records, trailing bytes including optional None+tail, wrong magic/kind, complete unsupported outer/nested version vs truncated version, unknown every enum tag, bool/Option values2/255, invalid UTF-8 only in defined text, invalid Name, impossible in-cap array products, excessive per-array/aggregate counts, last-nested-field corruption, usize/u32/u64 checked limits, every actual-cell allocation product/Layout/isize boundary, exact final Writer allocation boundary, overflow-before-allocation and encoded/owned/replay exact/full/one-short. The R1 section specifies target-reachable pure arithmetic controls without giant allocation or an unsupported 32-bit execution claim. Large declared empty arrays cannot allocate during rejected preflight. Wrong endpoint/dialect and unresolved typed classifications decode positively, then refuse through actual domain admission under the original class.

Concrete positive roundtrips must cover every nested variant and real recorded Owned IO terminal outcomes, all Opaque/Refusal/Text/ToolCall blocks in position, arbitrary Returned::Text bytes/error, Withdrawn/NotRun/Invalid, empty/Some Replay and complete phase/id/unknown-extension envelopes. Standalone Turn, Transcript turns/after and optional wrapper must all preserve full typed equality. No Debug format, provider JSON, hash equality or reduced replay-only structure is a codec proof. Byte equality is used only for frozen protocol expectations; it never becomes the recovery semantic classifier.

Actual-root integration extends `tests/agent/tests/native_root_restore.rs::actual_native_root_parks_then_restores_whole_opaque_prefix_and_real_post_tail` without replacing its native fake-peer/Client route. Capture an actual root-emitted session record; encode and decode it through this concrete codec, retain independently saved after results, start a new actual root with that value and require the exact original context/opaque/call-id/arguments/replay/phase/wake order observed at actual Client/native query. Preserve its zero-new-charge historical behavior and current independently observed new accounting. Also retain `tests/session/tests/recorded.rs` named replay/late-cancel/cap-filled/maximum actual owned-result controls and insert the codec around their actual records only when authorized. Add codec-positive result ReplaySome followed by the existing adapter Unsupported negative in `tests/agent/tests/client.rs`; do not make it a successful Client request by removing replay.

Parent gates after immutable source, with existing workflow budgets and idle serial measurement:

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo nextest run --workspace --profile fuzzy
cargo nextest run --workspace --profile measure -j 1
cargo nextest run --workspace --profile measure -j 1 --ignore-default-filter -E 'binary(/^fuzzy_/)'
```

Default≤15s and fuzzy≤60s remain unchanged. Before assigning source, parent fixes exact ownership paths/test package/dependencies; before approval it verifies actual layouts, closed enum coverage, body/count caps, zero-allocation preflight and all-owner meter bounds, exact source/fixtures hashes and full gates. Known kernel/platform failures are not waived by a protocol-only passing targeted test. No source coverage, real root positive or recovery assembly is called accepted by this proposal.

## 8. Selected requirements

1. Package is `smith-transcript`; dependencies are exactly skein-lib + smith-domain-session + smith-domain-tools for this increment. Root/session must not import it. No proxy record schema is added; future lower schema/renderer extraction remains separate work.
2. The closed SMTR/version2/kind grammar above is adopted as a **new unshipped format**, based on scoped evidence that no authoritative older full record codec was found. All 250 legacy fixtures/default byte/API evidence remain unchanged; discovery of a genuinely shipped incompatible full grammar still requires review rather than a compatibility claim.
3. Single borrowed actual `tools::Name::valid(&[u8]) -> bool` is reused by `Name::new`, preflight and typed measure. Name rules are construction invariants with meaningful existing and new positive/negative controls; no independent duplicate predicate or preflight Name allocation. Text/Refusal UTF-8 is an explicit new protocol construction rule following typed text sender contracts, not a claim that Domain already rejects every directly constructed invalid typed value. Returned::Text/binary/Opaque/Replay remain arbitrary bytes.
4. Concrete caps/API/error priority and R1 representability rules are required. Every prospective actual graph/output allocation must pass checked count conversion/product and Layout/isize representability before allocation, in addition to u64 graph sums, encoded u32 and independent receiving caps. Semantic empty/inconsistent/unresolved values remain codec-positive and retain precise existing Domain refusal classes.
5. ReplaySome is codec-positive and preserved fully; actual sharedClient Unsupported remains its unchanged negative. No metadata is silently stripped. Future Rust docs cite `domain/session.md`; Skein `lib.md` references are permitted.


Independent exact source review, fixture and ownership evidence, integration and all workflow gates are required before implementation acceptance. Contract approval does not satisfy them.
