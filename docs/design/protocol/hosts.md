# Hosts

Provisional, 2026-10-07. The host's side of the protocol:

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
  - a spawned agent's pipes, from a contained tree (skein's `process.md`);
  - or a connection to a daemon.

  The half works the same on either.
- **The agent's tree is the run's containment.** The tree a host spawns the
  agent in holds everything the run starts, the agent's own trees
  included. The run is gone when that tree is proved empty
  (domain/host.md, section 4).
- **The local host is a host like any other.** Its protocol layer adds:
  - a person at a terminal;
  - configuration files;
  - transcripts in files;
  - signing in to a provider;
  - committing in place.

## 2. The host's half

- **Opening.** Once its streams exist, the half opens the channel with
  smith's magic, the versions it speaks and the credential, empty over
  pipes, and checks the agent's terms (channel.md, section 2). The run's
  start goes first after ready.
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

- **the program:** the `smith` binary, with its configuration (agent.md,
  section 4);
- **an environment** given whole, holding no credential;
- **the view:** at least the workspace's directories, writable as the start
  says, the binary and its configuration. The rest is the host's choice;
- **the pipes:** standard input and output for the channel, and standard
  error, whose tail the half keeps for the domain's report of how the agent
  ended (domain/host.md, section 4);
- **room for the agent's own trees** inside its tree, so they are held,
  stopped and proved empty with it (skein's `process.md`).

The spawn's deadline covers the opening. A tree that does not start, or
whose channel does not reach ready in time, is reported as an agent that
could not be started, with standard error's tail as its detail.

The domain's stops become the tree's three steps: the cancel goes down the
channel first, then the termination signal, then the kill. Gone means the
tree was proved empty and the channel was read to its end.

## 4. Connecting to an agent

For a connected agent (README.md, section 4):

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
  message, and a line ending in a backslash continues it.
- **The run's words are shown** as its turns arrive: the LLM's text, each
  tool it calls by name, and how each ended.
- **While the run waits,** a prompt says so.
- **Interrupting:**
  - the first interrupt is the run's cancel;
  - a second, before the run has answered, stops its tree.

### 5.2 Configuration

- **The local host's settings:**
  - endpoints and their accounts;
  - prices;
  - a budget;
  - models;
  - conventions;
  - a result contract, a report by default;
  - the commands' environment.
- **A workspace may override them** for itself. Where settings are kept,
  and in which format, is the binary's, decided when it is built.
- **The local host writes the agent's configuration** from these
  (agent.md, section 4), and the charter of each run.

### 5.3 Transcripts

- **Each chat** has a directory under the workspace's state directory.
- **Each turn** is a file of its own, numbered, written atomically. A turn
  is acknowledged once its file is stored.
- **A chat's transcript** is its turns' files, in order (transcript.md,
  section 3).
- **The calls answered after the last turn** are kept the same way, until
  the next turn makes them part of the history.

### 5.4 Signing in

- **With skein's OAuth client** (skein's `oauth.md`): the authorization
  page's address is shown at the terminal, and the redirect comes back to
  a loopback listener.
- **The refresh token** is kept in a file in the user's configuration
  directory that only the user may read. An access token is lent to the
  agent as a grant, and refreshed before it lapses (domain/host.md,
  section 7).

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

### 5.6 The agent

- **A child process, by default:** the local host spawns `smith` as any
  host does (section 3), through `smith-host-protocol`.
- **Or in one process,** for development and tests: the local host owns
  `smith-domain` and the agent's LLM and machine components (README.md,
  section 4).

## 6. What the domains are owed, and what skein owes

- **`smith-host-domain` is owed:**
  - the channel's records as its own vocabulary;
  - its processes spawned, stopped and proved gone;
  - standard error's tail for its reports.
- **`smith-local-domain` is owed:**
  - the person's lines as messages;
  - its files loaded and stored atomically;
  - its sign-ins;
  - its commits, each answered in smith's terms.
- **skein owes:**
  - the channel;
  - contained trees;
  - sockets and TLS;
  - its OAuth client and fake issuer;
  - the fake checkout, for commits in worlds.

## 7. The world

- **The host's half** in the channel's protocol world (channel.md,
  section 10).
- **Spawning,** in a simulated world with the fake machine:
  - agents that start, fail to start, miss the opening's deadline, exit
    after their last word, and ignore the cancel;
  - each step of stopping;
  - standard error's tail in the report.
- **The local host,** as domain/host.md, section 10 tells its stories, now
  with its protocol layer:
  - lines at a scripted terminal;
  - chats resumed from files across runs of the binary;
  - a sign-in against skein's fake issuer;
  - commits made in the fake checkout;
  - an interrupt in the middle of a turn.

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
