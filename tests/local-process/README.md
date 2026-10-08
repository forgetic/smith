# Local process world

The scripted terminal, shared `smith::local_host::Local` shell, fake LLM
HTTP/TLS peer, OAuth peer and browser implement `skein_world::Host`. Each
holds only process state and kernel queues. The binary and this world pass
file directories, input/output descriptors and the signal source into the
same shell. Its actual Store and Tokens implementations perform their file
work; the world never substitutes transcript or credential handling.

The world loop owns the simulated kernel, fake machine, process binding and
faults. It binds a spawned agent to the actual agent service, and a git child
to a simulator-free output Host after carrying out its command against
`skein-fake-checkout`. Both agent placements use the same production settings
translation and lower components.

The fake checkout has a synchronous world face rather than a hosted process
face. Its machine-to-checkout adapter belongs to the world loop. A future
real-loop world needs a process face for that peer in Skein; this session does
not add one or implement real-loop tests.

The referee accepts only observed terminal bytes, decoded provider requests
and local facts. Repository inspection uses the head/message/files interface,
answered here from the fake checkout's commit graph. Negative controls reject
early or duplicate answers, wrong terminal text, wrong commit contents and an
unconfigured push. Discarding facts changes no terminal or peer work.

The required stories cover resume across invocations, a commit with the
result fields as its message, interruption during a turn, and equivalent
spawned/colocated operation. File write/sync crash cuts are in the shared
Store's tests. The fuzzy world uses seeded kernel short IO, completion delays
and cancellation races, and compares application observations with Skein's
trace kit. Rustls uses kernel randomness for signing, so encrypted transport
record lengths are deliberately outside the replay comparison.

Authentication stories run through the shared token store in both placements:
sign-in and loopback return, expired-token refresh, and refusal. A timed story
also refreshes a lent grant while a provider response is pending, before its
original expiry. The first provider request verifies the record was already
saved. Refusal shows account unavailability and starts no run; EOF exits
cleanly, following the local domain's existing idle-state behavior. These
states are also part of the replayed seed sweep. The referee's checkout heads
are opaque bytes, compatible with future real git object names.

For the Responses dialect, Skein's fake document decoder reports its own
model token ceiling in Query; it does not attest an output-token field on the
wire. The referee checks the model and a positive peer ceiling; the agent's
budget and receiving limits retain their independent tests.
