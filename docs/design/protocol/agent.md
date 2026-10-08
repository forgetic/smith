# The agent process

Provisional, 2026-10-07. What the `smith` binary is when it runs an agent:

- its machine component, `smith-protocol-machine`, which carries the
  workspace's files and processes;
- its configuration;
- its facts and traces;
- the process itself: its streams, its startup and how it ends.

Its channel and its LLM calls are channel.md's and llm.md's.

## 1. In one page

- **The machine component translates the tools' and the checks'
  operations** into skein's io (domain/tools.md, section 6):
  - files loaded and stored;
  - directories scanned;
  - searches and commands run as contained trees;
  - checks run and guides read.
- **Everything runs in a view.** Each command, search and check is a
  contained tree (skein's `draft/process.md`). It sees the workspace's
  directories side by side, writable as the start says, with every git
  directory read-only.
- **Configuration is given at startup.** It gives:
  - the endpoints;
  - the limits;
  - the commands' environment;
  - where a trace goes.

  It never holds a credential.
- **Facts go to the host, and traces to a file.** Facts are content-free
  and cross the channel (channel.md, section 8). A trace, if configured,
  carries content as its capture policy allows, and stays on the agent's
  machine.
- **One process, one run.** The binary reads its channel on its standard
  input and output, runs the run it is started with, sends its answer and
  exits (domain/host.md, section 1).

## 2. The machine

| Operation | How |
|---|---|
| load a file | whole, within its bound, beneath its directory's root, with a digest of its content as its version |
| store a file | to a temporary file beside it, then renamed over it, once the file's content still has the version read; never through a symbolic link |
| scan a directory | its entries, within their bound |
| search | `rg` as a contained tree: no configuration, an empty environment, the pattern and glob after `--`, every directory read-only |
| run a command | `sh -c` as a contained tree, in the first directory, with the configured environment and the domain's deadline, its output's head and tail kept |
| run the checks | the convention's executable, in its directory, as a contained tree, the same way |
| read a guide | as text, UTF-8, cut at a character's boundary within its bound |

- **Paths** stay beneath the workspace's roots, and io refuses one that
  leaves them. A path with a `.git` part, in any case, is refused before io
  is asked (domain/tools.md, section 4).
- **A version** is a digest of a file's content. Comparing it again before
  a store catches a change made meanwhile, by another session or anything
  else.
- **Output** is captured as the domain asks: how much of the head and of
  the tail, and the count of bytes dropped between them.
- **Deadlines** are the domain's. When one passes, the machine stops the
  tree in three steps, each step's grace from the configuration, and
  answers once the tree is proved empty.

## 3. The view

- **The directories side by side,** each at the name the LLM calls it,
  under one parent. Relative paths and commands start in the first
  directory (domain/tools.md, section 2).
- **Writable as the start says.** Every git directory is read-only, a
  repository's included.
- **The rest of the machine** is read-only, with a private temporary
  directory, so a build's tools and caches work.
- **The commands' environment** is the configuration's, whole: what a build
  needs (a `PATH`, a home, a locale) and never a credential. It answers
  domain/tools.md, section 8's open question.
  A shell call's explicit variables override defaults with the same name;
  the combined environment must still fit the configured byte bound.

## 4. Configuration

What the agent is given at startup, before the loop. Charters,
transcripts and the channel's terms refer to what it holds, so that is
this document's concern. How it is given (a file, its format, where it is
kept) is the binary's, decided when the binary is built.

- **Endpoints,** each:
  - its name, which charters and transcripts use;
  - its dialect;
  - its address, as a name resolved at startup (skein's `io.md`,
    section 4);
  - its transport: TLS, with its server name and what to trust it by, the
    machine's roots by default; or plaintext, only to an address on
    loopback (skein's `llm-connection.md`, section 3);
  - the dialect's path and headers, each the dialect's own by default
    (skein's `llm.md`);
  - the account its grants are named by;
  - its options: reasoning effort, an identity profile, a cache key.
- **Limits:**
  - the domain's (runs, sessions, bytes, budget ceilings, waiting);
  - the largest start and transcript it takes;
  - the most turns it holds unacknowledged;
  - the channel's queue.

  These are also its terms on the channel (channel.md, section 2).
- **The commands' environment,** and the graces of a tree's three steps.
- **The trace:** a file to append to, if any, and its capture policy.

Under a host, the host provides it, as temper's worker would with its
machines. The local host writes its agent's from its own settings
(hosts.md).

## 5. Facts and traces

- **Facts** are pushed by the domains into a bounded queue
  (domain/run.md, section 11). This process sends them on the channel while
  there is room, and counts those it drops.
- **A trace** is written where the configuration says, one record per
  fact, with the content its capture policy allows:
  - **none:** facts only;
  - **calls:** with each tool call's name and input, and each result's
    size;
  - **everything:** with prompts and completions.

  Credentials never appear in it.
- **A trace never holds the run back.** When the file is behind, records are
  dropped and counted, as facts are.

## 6. The process

- **Streams:**
  - standard input and output carry the channel;
  - standard error is the agent's log for operators: a line when it
    starts, and one saying why, when it ends without an answer.

  Its host keeps the tail of standard error and never parses it
  (domain/host.md, section 4).
- **Startup:**
  - read the configuration;
  - resolve the endpoints' names;
  - check that the machine allows contained trees;
  - then start the loop, as the channel's responder.

  A startup that fails writes its reason to standard error and exits
  without opening the channel. Its host reports that the agent could not
  be started.
- **The service:** `smith-agent-service`'s `iterate` composes `smith-domain`
  with the three components (README.md, section 5).
- **Signals:** a termination signal is the domain's cancel. The run winds
  down and answers, as it would on the host's cancel.
- **The end:** after its answer, the agent finishes its channel, waits for
  its trees to be empty, and exits with success. It exits with failure
  only when it could not answer: startup, or a broken channel.

## 7. What the domain is owed, and what skein owes

- **The domain is owed** its operations carried out as asked, each answered
  once, with outcomes in its terms: a file's content and version, a
  command's exit and output, a tree proved empty after a deadline.
- **skein owes:**
  - io's files and roots;
  - contained trees (skein's `draft/process.md`);
  - the shell's startup (skein's `shell.md`).

## 8. The world

- **The machine component,** over skein's simulator and the fake machine:
  - files changed behind a session's back;
  - symbolic links and `.git` parts on paths;
  - commands that hang, fail and flood their output;
  - trees that ignore signals;
  - checks that pass, fail and time out.

  The referee requires that nothing is written outside the writable
  directories, and every tree is proved empty before its call is answered.
- **The process,** as a simulated world: the agent's `iterate` with a
  scripted host on its channel and a fake LLM peer. It covers startup
  refusals, a run from start to answer, a termination signal mid-turn, and
  standard error's last line.
- **Configuration:** every refusal of a malformed or oversized one.

## 9. From temper

- **Kept:** temper's agent's tools below the domain, unchanged in what they
  do, and its log for operators on standard error.
- **New:**
  - views with read-only git directories;
  - contained trees for searches and checks as well as commands;
  - the commands' environment in configuration;
  - traces.

## 10. Open questions

- **A daemon:** listening for connections and deciding on a credential
  waits on the domain (README.md, section 11).
- **The network for commands:** whether a command's tree has a network,
  which a build may need and a review may not.
- **Traces kept:** their format, rotation, and how long they are kept, as
  the binary is built.
