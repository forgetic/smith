# Shared LLM client boundary

Scratch reference, outside the current domain contract.

## 1. Role and preparation

Smith's `smith-protocol-llm` translates the root's provider-neutral conversation
through the actual `skein-llm::client::Client`. Skein owns provider grammar,
HTTP/SSE progress, replay compatibility and wire failure classification. Smith
owns application schemas/decoding and conversation policy. The caller supplies
the exact configured endpoint and credential grant; Smith has no sign-in,
OAuth claims exchange or refresh client.

`prepare(Input, &Limits)` checks endpoint identity, static receiving compatibility,
tool/result admission and bounded prompt translation before calling Client
preparation once. It returns that actual Client and an immutable owned Context.
The caller drives shared Client entrances with injected time and lower stream
events. Preparation refusal produces no wire effect. Optional workspace,
conventions and protocol application schema definitions retain their separately
named contracts. Charter prices and run-wide budget policy belong to the run
and session domains (domain/run.md, section 9; domain/session.md, section 6).
Root checks its completion permit before it publishes a request to this adapter;
an unsent budget denial therefore constructs neither Context nor Client and
leases no credential. The adapter never prices usage or enforces host currency.

## 2. Tool inventory

Host declarations retain their complete name, description, schema, effect and
deadline allowance. The adapter parses whole schema syntax through Skein JSON,
hands its complete semantic object to the Client, and interprets no host policy
or argument-schema meaning. Every offered non-host tool requires one explicit
`ToolSchema`: its application name, description, whole schema and `ToolKind`.
Checkout descriptors must match actual family grants; Finish, Deliver,
SubAgent and Wait must match actual served descriptors. Missing, duplicate,
unoffered or conflicting descriptors refuse before preparation. No placeholder
schema or silently omitted offered tool is permitted. The fixed names are
`finish`, `deliver`, `sub_agent` and `wait`; owned checkout names are supplied
by their application descriptors.

Context retains the exact served and application declarations between consumed
prompt preparation and actual terminal translation. Context's `grants`,
`served`, `application` and `receiving` accessors expose that immutable admitted
metadata for a static caller decoder, through copied scalar data or ephemeral
borrows. No input prompt clone or additional authority is required.
Application-specific JSON
decoding remains a static caller handoff, with no application trait, callback
or new authority state.

## 3. Prompts and feedback

Messages and blocks preserve actual order, sender, original provider call IDs,
argument bytes and optional Text/Refusal/ToolCall replay. Opaque blocks are whole
Skein replay envelopes. Skein admits version, bound and configured dialect;
Smith never parses provider-owned replay fields.

Concrete V2 result text/error moves unchanged. Restored result replay is retained
by the domain; the current shared Client has no ToolResult replay field, so
result replay Some is explicitly Unsupported before preparation. No metadata
is silently discarded. A future shared representation can admit it without
changing transcript ownership. V1 run semantic results use the
root's canonical checked `feedback` helper, including exact host answer bytes
and complete typed status/evidence; the adapter has no second policy renderer.
Only owned checkout results need a caller `ResultText`, paired by actual message
and block position plus exact provider ID. Missing, repeated or unused result
entries are refused. Invalid, unstarted and withdrawn results have explicit
provider-neutral error feedback. Skein alone determines native error flags or
wire prefixes. The shared output-ceiling helper applies configured wire support;
no provider-specific branch lives in Smith.

## 4. Completed input

`completion(Context, actual_owner, Completion, resolved)` consumes the one actual completed
value. The actual callback owner must equal Context's owner before translation. All four usage counters remain u64 and stop/refusal distinctions remain
typed. Complete replay envelopes survive every block; partial display deltas
are never substitute history. The adapter checks original block count and
translated ownership under the secured receiving metadata.

A host call is decoded only against an actual retained declaration. Real Skein
JSON parsing must establish complete object syntax before constructing HostInput;
the whole original argument body is copied unchanged, without schema-policy
validation. Effect comes from that declaration. The run derives durable names
from the actual transcript position, never from supplied ResolvedCall metadata.
Unknown names and malformed arguments stay actual calls with typed Invalid
feedback and no effect. Non-host calls require caller ResolvedCall entries whose
position, name and exact input match the observed block and whose semantic
classification matches its offered ToolKind. Aggregate decoded ownership is
checked before dispatch. Preparation requires enough aggregate allowance for
one full Decoded wrapper per possible Client part; translation reserves every
actual ToolCall wrapper before decoding. Each admitted call charges its dynamic
payload beyond that reserved cell. An oversized or arithmetically invalid
classification uses its already-priced fixed TooLarge cell. Earlier fallbacks
therefore remain in the aggregate price when a later host call checks its exact
remaining payload allowance. Original provider ID/name/input/replay remain intact.

## 5. Failure and lifecycle

Actual Failed boundaries preserve the exact shared failure class, typed
Unsent/Unknown/Response evidence and complete bounded diagnostic bytes. Limit,
Protocol and unsolicited provider Cancelled are distinct nonretryable classes;
the session owns exhaustive retry categorization for all classes. Policy consumes
and drops diagnostic text after the actual receiving boundary, never interprets
its wording or stores it in transcripts. Content-free facts and final End retain
class/evidence without becoming traces. Callers can observe the full diagnostic
before handing the actual terminal to the domain.

Shared prepare Error::Limit maps to Limit; Invalid maps to Invalid; Unsupported
maps explicitly to Invalid with Unsent evidence, because the configured call
cannot be represented and no request was sent. These refusals are not wire
responses, provider truncation or authentication rejection.

Requested cancellation produces the root Cancelled terminal only from the real
shared Client cancellation terminal after actual lower settlement. A requested
Cancel cannot replace an already won Completed or Failed. The caller retains
the actual Client through HTTP Reusable before next_call, or through actual
Close/Closed before removal. Client wire attempts do not acquire application
retry policy. Submitted host deliveries retain their existing actual-terminal
ownership independently of this protocol boundary.

## 6. Bounds and ownership

Receiving carries max_completion_bytes, max_completion_blocks, aggregate
decoded_call_bytes and max_failure_bytes from the actual root request. Before
Client preparation, the conservative translated completion bound is checked:
Client answer_bytes + parts × (raw opaque_bytes + seven-byte envelope) +
parts × max(root Said cell, session Block cell) + aggregate decoded_call_bytes.
Replay's serialized escaped envelope can exceed the shared answer accounting
of token payloads, so its full raw cap is separately reserved. Decoded allowance
is never multiplied per part. It must cover parts × sizeof(Decoded) fixed
fallback cells before Client preparation; cap zero cannot admit a Client that
could return a tool call. Client detail_bytes must fit max_failure_bytes.
Overflow or incompatible configuration refuses before wire effects.

Limits.tool_bytes caps joint retained declaration wrappers and fields;
result_bytes caps supplied result wrappers/IDs/text. Counts and joint
declaration/result payloads are admitted before allocating translation containers. Other direct history bytes move into the native prompt;
shared Client preparation validates their complete receiving bounds before
wire effects. `worst_case` prices one
Client, retained Context/declarations, native prompt slots/schema and replay token wrappers plus bounded parsed payloads,
whole-document payloads before native request admission, separately capped
canonical feedback/string transit, temporary replay helpers and simultaneous raw
Client completion, translated
output and diagnostic handoff. The root separately prices its input snapshot,
semantic result/rendering transit, application decoder input, endpoint/credential
grants, outer queues, observations and replay copies. No destructor cancels an
effect or releases a live lower right.

## 7. Evidence and migration

The shared actual Client/script-domain/HTTP peer and all historical capture
ownership are reviewed and gated in Skein before consumer deletion. Smith's
root wire world must drive a Client prepared by this adapter through the shared
prepared-world constructor, observe actual full feedback and terminal chronology,
check positive histories before corruptions, and measure simultaneous ownership.
Typed application worlds retain application scripts and menus while using the
same shared neutral fake domain. No shared gate alone claims that Smith's new
consumer or copied-codec removal has been validated; implementation evidence is
recorded separately in migration-05s2a.md after the integrating parent's checks.
