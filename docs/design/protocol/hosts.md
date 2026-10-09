# Hosts

Provisional, 2026-10-07, revised 2026-10-09. The host's side of the
protocol:

- `smith-host-protocol`, the host's half of the channel, a component that
  any host owns;
- how a host makes the streams it runs on, spawning an agent or connecting
  to one;
- `smith-local-protocol`, the protocol layer of smith's own local host.

The contract is domain/host.md's.

## 1. In one page

- **The host's half is smith's.** A host owns `smith-host-protocol` as a
  component of its protocol layer (README.md, section 6). The half:
  - runs one channel per run, as the initiator, on streams it is given;
  - translates between the channel's records and `smith-host-domain`'s
    vocabulary;
  - fills grants' values in from the host's credentials.
- **The streams are the host's to make:**
  - a spawned agent's pipes, from a contained tree (skein's `draft/process.md`);
  - or a connection to a daemon.

  The half works the same on either.
- **The agent's tree is the run's containment.** The tree a host spawns the
  agent in holds everything the run starts, the agent's own trees
  included. The run is gone when that tree is proved empty
  (domain/host.md, section 4).
- **The local host is a host like any other.** Its protocol layer adds:
  - a person at a terminal, or a prompt given once;
  - settings, which are the shell's;
  - transcripts in files;
  - signing in to a provider, or borrowing a sign-in;
  - committing in place.

  Its agent runs inline by default; spawning one stays a capability
  (section 5.6).

## 2. The host's half

- **Opening.** Once its streams exist, the half opens the channel with
  smith's magic, the one version it speaks and the credential, empty
  over pipes, and checks the agent's terms (channel.md, section 2). The
  run's start goes first after ready.
- **Translation.** The half decodes every record the agent sends and checks
  its order (channel.md, section 5), then hands `smith-host-domain` what it
  carries:
  - numbers, names, effects and deadlines, typed;
  - the turns' bodies, the result, the host tools' inputs and the delivery's
    fields, as bytes or as smith's records.

  On the way down, it encodes the domain's start, messages, answers,
  acknowledgements, grants and cancel.
- **Deadlines become durations** on the way down, and durations become
  deadlines on the host's clock on the way up.
- **Grants:** the domain names an account and a generation. The half fills
  in the value from its host's credentials as it encodes, so no domain of
  the host's holds one (domain/host.md, section 7).
- **The end.** The half reads the agent's channel to its end, so that what
  the agent said before it went is heard, then reports its terminal.

## 3. Spawning an agent

When `smith-host-domain` asks for an agent process, the host's protocol
layer spawns a contained tree:

- **the program:** an agent process, `smith-agent` as smith ships it or a
  host's own build of `smith-agent-shell`, with its configuration
  (agent.md, section 4);
- **an environment** given whole, holding no credential;
- **the view:** at least the workspace's directories, writable as the start
  says, the binary and its configuration. The rest is the host's choice;
- **the pipes:** standard input and output for the channel, and standard
  error, whose tail the half keeps for the domain's report of how the agent
  ended (domain/host.md, section 4);
- **room for the agent's own trees** inside its tree, so they are held,
  stopped and proved empty with it (skein's `draft/process.md`).

The spawn's deadline covers the opening. A tree that does not start, or
whose channel does not reach ready in time, is reported as an agent that
could not be started, with standard error's tail as its detail.

The domain's stops become the tree's three steps: the cancel goes down the
channel first, then the termination signal, then the kill. Gone means the
tree was proved empty and the channel was read to its end.

- **Exit evidence.** io's exit status goes up with the process's end,
  never dropped: `exit`, the code the agent exited with or the signal
  that ended it. The domain pairs it with the steps it took, as `forced`:
  none, terminated or killed. So an agent killed past its grace, one that
  crashed and one that exited cleanly are told apart (domain/host.md,
  section 4).
- **No signal to an agent that answered** until its exit grace has
  passed: after its answer it closes what it owns and exits by itself
  (agent.md, section 6).

## 4. Connecting to an agent, later

A connected agent waits on the domain (README.md, section 11). This is
what its host's protocol layer would do:

- **a socket** to the daemon's address, with TLS when it is remote;
- **the credential** in the opening, which the daemon's configuration
  decides on;
- **keepalive:** pings while nothing is sent, and silence and stall
  deadlines, armed by the host's protocol layer from the channel's
  queries (skein's `channel.md`, section 9);
- **a lost connection** is a channel hung up while the run is live. What
  that means for the run waits on the domain (README.md, section 11).

## 5. The local host's protocol layer

`smith-local-protocol` is the protocol layer of `smith-local-domain`
(domain/host.md, section 8).

### 5.1 The terminal

- **The person's words are messages.** A line the person enters is one
  message, and a line ending in a backslash continues it. The first line
  of a chat with no live run starts one, carried in its start; later
  lines go to the live run as they are entered (domain/host.md, section
  8).
- **The run's words are shown** as its turns arrive: the LLM's text, each
  tool it calls by name, and how each ended.
- **Notices are typed.** The local domain says what to show as notices
  of a closed set of kinds (waiting, checking, a line not delivered, a
  failure, a warning, a sign-in step), and the terminal protocol renders
  each as whole lines, ending each with a newline. Worlds observe notices
  by kind, never by their bytes.
- **Failures are shown in words,** whichever kind of agent runs: one
  line saying what failed, from its typed failure, such as the budget
  naming what ran out, or the model with its provider's class and the
  limit that fired, then its bounded detail where there is one, such as a
  spawned agent's error output's tail. A refused start says what was
  refused, and an end with no detail still says how the agent ended. The
  same words go to the operator's error output for unattended use. No
  failure is shown as a fixed message or as a type's debug text, and none
  enters a turn or a prompt.
- **Lines not delivered** are shown by name after a cancel, a failure or
  a refusal, then dropped (domain/host.md, section 8).
- **While the run waits,** having read every line sent to it, a prompt
  says so.
- **Interrupting:**
  - the first interrupt is the run's cancel;
  - a second, before the run has answered, drops the run (domain/host.md,
    9.2), or, for a spawned agent, stops its tree.
- **Headless,** with no person at the terminal: the result goes to
  standard output and the human status to standard error, or, when
  asked, the event stream (events.md) goes to standard output instead
  (`docs/design/shell.md`).

### 5.2 Configuration

- **The local host's settings are the shell's** (`docs/design/shell.md`):
  TOML, strict and bounded, layered from built-in defaults through the
  user's file, the repository's `.smith/`, overrides and flags, with
  provider presets that supply endpoints and known models' declared
  values. They cover:
  - endpoints and their accounts, each with its credential source;
  - prices;
  - a budget;
  - models;
  - conventions;
  - a result contract, a report by default;
  - the commands' environment.
- **The local host puts the effective values in each run's charter,** so
  the domain enforces what the host chose. For a spawned agent it also
  writes the agent's configuration, as generated JSON (agent.md, section
  4).
- **Its graces are derived, not guessed:** a spawned agent's exit grace
  from the agent's teardown bound (section 5.6), as skein's `shell.md`,
  section 13 has a supervisor derive it.

### 5.3 Transcripts

- **Each chat** has a directory under smith's state directory
  (`docs/design/shell.md`).
- **Each turn** is a file of its own, numbered by its place in the
  conversation, and written durably: the file and its directory are
  flushed before the turn is acknowledged (channel.md, section 7).
- **A chat's transcript** is its turns' files, in order, from the last
  turn whose record had `opens_window` (channel.md, section 4;
  transcript.md, section 3), with the chat's run identity, given in every
  start (domain/host.md, section 8). Earlier windows' files are kept for
  the person to read, and never sent back.
- **Answers the transcript lacks** are kept the same way, until a waking
  prompt has told the LLM of them and a turn has been kept after it.

### 5.4 Signing in

Each account has one credential source (domain/host.md, section 7):

- **`sign_in`, smith's own,** with skein's OAuth client and its driver
  (skein's `oauth.md`, section 6) and a provider preset where the
  provider sanctions a public client. The authorization page's address
  is shown at the terminal, and the redirect comes back to a loopback
  listener bound to 127.0.0.1. A
  redirect registered as `localhost` is accepted, and compared exactly,
  by skein. The account's id comes from the token's claims. The refresh
  token is kept in a file only the user may read; an access token is lent
  to the agent as a grant, and refreshed before it lapses.
- **`borrow`, another CLI's login,** opted into per account. Its file is
  read and never written, and only the access token and the account's id
  are taken: never the refresh token, which would rotate the other CLI's
  sign-in away. The host hands them to skein's OAuth driver as an
  access-only record (skein's `oauth.md`, sections 6.2 and 6.3), which
  the driver lends, never refreshes and never keeps; the host reads the
  file again and hands in a newer record when a grant is due. The
  driver's `expiring`, and its failure once the token has lapsed or is
  rejected, are only events: the words the person sees are smith's,
  naming the file and exactly which CLI to run again
  (`docs/design/shell.md`, section 7).
- **`env`, a named variable,** for unattended use: an access-only grant,
  never refreshed.
- **A credential's value goes nowhere else:** not into settings shown, a
  trace or an event.
- **At the end,** the local host cancels a sign-in still waiting for the
  person before it closes the OAuth driver, since a close lets work in
  progress finish (skein's `programming-model.md`, section 5.2). A
  termination signal after the answer aborts the driver with every other
  component.

### 5.5 Delivering in place

- **In each writable repository that changed,** the local host commits the
  checked tree, running git as a contained tree of its own. git's own
  directory is writable there, since the host, not the agent, writes it.
- **The commit's message** comes from the result's fields: the subject from
  the field the configuration names, `title` by default, and the body from
  the rest, in order.
- **Nothing is pushed** unless the configuration says so.
- **A plain directory's files** are kept as they are, and delivered says so.
- **A repository whose head moved** since the run started is a stale
  delivery.
- **Each commit names its delivery** in its message, by the call's name,
  and the host keeps its intent to deliver until the answer is kept. A
  delivery a crash interrupted is then found and answered when the host
  starts again.
- **An uncertain step is not a failure.** A git step whose effect is
  unknown (its deadline passed, its output overflowed, or the head could
  not be read after a commit) is settled as a crash would be: the host
  looks for the delivery's commit in the repository before answering, and
  never commits again on doubt.
  - Found: the directory is delivered, with that commit.
  - Absent, with the head unchanged: the step did nothing, and the
    delivery goes on or fails as usual.
  - Still unknown: the delivery fails with the reason *unknown*. The host
    keeps its intent, and the next start settles it and tells the run what
    landed (domain/run.md, section 6).

### 5.6 The agent

- **The inline agent, by default:** the local domain owns
  `smith-inline-agent` as its child, and `smith-local-service` composes
  it with the effects, routing its requests to `smith-protocol-llm` and
  `smith-protocol-machine` in its own protocol layer and lending the LLM
  component its credentials directly (domain/host.md, section 9). Its
  facts and content are drained every pass into the event stream
  (events.md).
- **A spawned agent, as a capability:** `smith-host-domain` with
  `smith-host-protocol` and an agent process (section 3), as smith's
  libraries offer it to any host. The local host's composition takes an
  agent of either kind, so its worlds run both, and the shipped
  `smith-agent` binary is the real-process proof. The product offers
  spawning as a setting only later, with contained trees (domain/host.md,
  section 12).
- **Grace.** A spawned agent's exit grace is derived from the agent's
  teardown bound, which io's close deadlines make (agent.md, section 6;
  skein's `shell.md`, section 13), with a margin above it; a setting
  below it is refused (limits.md, `grace-over-teardown`). The inline
  agent's cancel grace is a setting of its own, before a drop.

## 6. What the domains are owed, and what skein owes

- **`smith-host-domain` is owed:**
  - the channel's records as its own vocabulary;
  - its processes spawned, stopped and proved gone;
  - each process's exit status;
  - standard error's tail for its reports.
- **`smith-local-domain` is owed:**
  - the person's lines as messages;
  - its files loaded and stored atomically;
  - its sign-ins, and the credential sources' records;
  - its commits, each answered in smith's terms;
  - its agent's failures and end, typed, with their evidence and detail,
    whichever kind of agent runs.
- **skein owes:**
  - the channel;
  - contained trees;
  - sockets and TLS;
  - its OAuth client, with `localhost` redirects, and its fake issuer;
  - token records that hold an access token alone, and the OAuth driver
    that signs in, refreshes, keeps records and lends access-only ones
    handed in (skein's `oauth.md`, section 6);
  - append streams and durable whole-file replacement, for the event
    stream and the chat's files;
  - the fake checkout, for commits in worlds.

## 7. The world

- **The host's half** in the channel's protocol world (channel.md,
  section 10).
- **Spawning,** in a simulated world with the fake machine:
  - agents that start, fail to start, miss the opening's deadline, exit
    after their last word, and ignore the cancel;
  - each step of stopping;
  - exit evidence: an agent that exits with success, one that exits with
    failure, one that ignores the termination signal, and one that exits
    on it after its answer, each reported once with its `exit` and
    `forced`;
  - an answered agent that exits within its grace, never signalled;
  - standard error's tail in the report.
- **The local host,** as domain/host.md, section 10 tells its stories, now
  with its protocol layer:
  - lines at a scripted terminal, and notices observed by kind;
  - chats resumed from files across runs of the binary, after a
    compaction from the current window's files only;
  - a sign-in against skein's fake issuer, with a `localhost` redirect;
  - a borrowed login: a valid token lent; one the other CLI refreshed,
    read again and handed in; one nearing its expiry, then expired, each
    shown in smith's words naming its CLI, with no run started on the
    expired one; its file never written; its refresh token in no trace;
  - an account from the environment;
  - commits made in the fake checkout;
  - an interrupt in the middle of a turn, and a second that drops the
    run;
  - each failure shown in its words, in neither a turn nor a prompt;
  - each with the inline agent, and with a spawned agent, resumes,
    replays and lines crossing the answer included.

## 8. From temper

- **The worker's agent child's protocol half** becomes
  `smith-host-protocol`, without temper's names.
- **The worker's push** stays temper's, as its delivery (temper's
  `agent.md`, section 6).
- **The local host is new.**

## 9. Open questions

- **A richer terminal:** a full-screen view of a chat, and several chats at
  once.
- **MCP servers** in the local host's configuration, as its first host
  tools, once MCP is a tool source.
- **Pushing from the local host:** which remote, and what a push that
  fails means for the delivery.
