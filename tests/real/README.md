# The real local-host world

The world consumes `tests/local-process`'s processes, scenarios and referee
unchanged. Hosted local and agent factories adopt independent scratch roots;
only git, rg, sh and checks are actual children. The terminal uses real pipes,
and the provider, issuer and browser use Skein's test trust over loopback TLS.
TLS fixture ports are unprivileged; nextest serializes stories using those
addresses. Missing io_uring or required tools fails the world.

The scratch directory owns its settings, durable chat/token directories,
test trust and git repository with an executable check script. Git implements
the referee's narrow head/message/files face through outside observations.
The ordinary test observer reaps each observation command before returning.

The current Skein pin is `e8bc0183785de5d56c263db40efb76cbaa58267e`, which
includes all five Skein sessions, required hosted-root and peer-batching fixes
and shared checked real-loop accounting.
Moving back to the original skein 04 tip would remove those prerequisites.

The four named stories cover sign-in/token persistence, all tool families
followed by mandatory checks and a local commit, durable history across
invocations, and equivalent hosted/colocated reports. The original adapter
controls are covered by these stories. Every construction, iteration and final
process drop uses Skein's checked real world and counting allocator. The
checked outcome stays borrowed until its final drop verifies exact release;
smith owns no generic allocator spans, host schedule or descriptor ledger.

On settlement, a complete scratch snapshot rejects unknown additions,
removals or changes outside the expected chat records, sign-in token and
commit outputs. Already durable turns and delivery records stay unchanged
across the second invocation. Newly saved turns and delivery terminals match
the shared observed facts; git changes are confined to the committed files,
index, current reference/reflogs and newly reachable loose objects. The
checkout is clean. The ring's settled descriptor/operation ledger and normal
process exits are checked, and Linux's outside task-child observations must
be empty, including zombies. Signals and containment controls remain in the
later pass.
