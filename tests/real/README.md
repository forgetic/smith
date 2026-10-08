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

The current Skein pin is `6e36bce926aa468b9afed755f29ad696cb215802`, which
includes skein 04 and the later required hosted-root and peer-batching fixes.
Moving back to the original skein 04 tip would remove those prerequisites.

Per-process real-loop metering awaits Skein's shared real harness support;
smith owns no generic allocator spans, host schedule or descriptor ledger.
