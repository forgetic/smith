# Shared local-process stories

`process::Proc`, `process::Launch`, `terminal::Terminal`, `process::BrowserProcess`,
`process::IssuerProcess` and `referee::Run` are independent of the simulator.
`world::Scenario` holds the terminal script, expected ending and immutable
launch configuration. The provider and issuer configuration uses Skein's
shared peers, never a local transport implementation.

For either backend, register `process::make_local` with `process::local_roots`
and `process::make_agent` with `process::agent_roots`, using `host_roots`.
The terminal starts `smith-local` with `Launch::arguments()`. Its launch,
delivery and agent-effect descriptors are independently opened and closed;
unused stderr and agent roots settle before hosted exit. A customized real
scenario can build the shared shell and wrap it with `LocalProcess::new`,
and configure its agent through the shared `smith_agent_process_world::process::configured`.

The real-loop tier sets `Launch::tls`, provides a private DER trust file in
`Launch::trust_der`, and sets `Launch::root_path` to its scratch directory.
Use `Scenario::provider()` and `IssuerProcess::configured(Transport::Tls)`
inside the harness's metered process constructors. The same browser follows
HTTPS authorization with the test root and HTTP loopback callbacks. A focused
control exercises those transports and every process heap under simulation;
TLS is kept out of seeded replay because its cryptography is nondeterministic.

The simulated adapter alone registers `git::make` with `git::roots`. Exact
git arguments select a bounded result directory in the fake filesystem
namespace. The fake checkout computes the command there; the child reads its
exit code and stdout through its own root and file descriptors, then writes
the actual pipes. No process shares the checkout or simulator. The real loop
leaves `/usr/bin/git` unregistered so the kernel executes it in the scratch
repository. The referee uses only `CheckoutRead::{head,message,files}`; a
real implementation supplies actual git observations through that same face.

The harness owns iteration, replay traces, timing, hosted admission, normal
exit and kill cleanup. `referee::Run` watches every emitted fact, terminal and
peer observation, checks the saved token before the first observed grant use,
and arms its browser, interrupt and shutdown flags without allocating any
process storage. It supplies wakeups for those actions before the loop skips
to a later deadline. An already accepted delayed provider reply may finish
after a cancelled local invocation; local terminal liveness is checked when
the invocation settles, independently of that peer's final cleanup.

Use `Memory::Checked` with Skein's `Counting` allocator: construction, every
iteration and final drop account separately for each process. Machine fixture
state and the referee remain outside those process heaps.
