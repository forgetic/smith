# Machine component world

The stories run the actual machine component and both IO adapters as one
`skein_world::Host`, using `skein_world::World`. Skein owns submission,
reaping, fake calls, scheduling, clock advancement, replay records and
process heap checks. There is no private simulator loop.

Each request runs through its terminal and complete descriptor settlement.
The component has no live request state between these runs. The scenario
retains its authoritative fake disk and absolute clock, so version conflicts,
external writes and absolute deadlines survive request boundaries. A fresh
independent root is opened for each run and closed before its outcome is
dropped. The scenario's inspection handle is outside the process heap and
closes with the scenario. Terminal copies and trace formatting happen outside
the process; the original terminal is freed by its metered process drop.

The shared expectation referee sees only domain terminals and checks their
owner and uniqueness. Story assertions read the fake checkout, never the
component's state. The focused replay control uses the shared replay kit;
the existing seven-mode, 128-seed sweep compares both terminals and complete
kernel traces. Fault sweeps remain later work.

The existing 4096-byte flood remains a component-boundary fixture, supplying
child events inside a metered application turn. Its synthetic child and
pipe tokens do not claim kernel execution. The deadline and check stories
exercise actual simulated child IO; every run closes its actual root.

The saturated component memory control remains. An additional checked world
covers file load, a maximal 128-byte store, a timed-out child, and the full
flood, metering construction, every iteration and drop. The application
bound includes the actual component and IO bounds plus a bounded two-MiB
reserve for its finite queues, request, terminal and fixture payloads.
