# The channel

Provisional, 2026-10-07. The channel between a run and its host is smith's
application protocol on skein's framed channel (skein's `channel.md`). This
document covers:

- its opening;
- its kinds and their records;
- their order and the rules a peer can break;
- flow control;
- grants and facts.

The records' codecs are in `smith-channel`. The agent's half is
`smith-protocol-channel`, and the host's half is `smith-host-protocol`
(README.md, section 5). The charter and the result are charter.md's, and
turns' bodies are transcript.md's. What the channel carries is the contract
of domain/host.md, section 2.

## 1. In one page

- **One channel per run,** on skein's framed channel. The host is the
  initiator, since it spawns or connects, and the agent responds.
- **Down, from the host:**
  - the start;
  - messages;
  - answers to calls;
  - acknowledgements of turns;
  - grants;
  - a cancel.
- **Up, from the run:**
  - admitted;
  - calls and withdrawals;
  - turns;
  - waiting;
  - long operations;
  - notices about credentials;
  - facts;
  - the answer, its last word.
- **The host reads what it acts on,** and keeps the rest as bytes. It reads
  numbers, names, effects, deadlines and a delivery's fields. It keeps as
  bytes:
  - the charter and the result;
  - turns' bodies;
  - host tools' inputs, and the text of answers and messages.
- **Calls carry their names** (domain/run.md, 5.2): the activation, the
  completion and the position. Each is answered once.
- **Credentials travel as grants.** Their values cross the channel and stop
  at each side's protocol layer, so each domain sees an account and a
  generation only.
- **Durations, never times.** Each side turns a duration into a deadline on
  its own clock.
- **Flow control has three parts:**
  - turns are held until acknowledged, within a window the start sets;
  - facts go only while there is room;
  - each half's queue is sized from its domain's limits.

## 2. Opening

- **The magic is `smth`.** Versions start at 1. Each version is one table
  of kinds, and the records of that version's schema.
- **The credential** is empty over a spawned agent's pipes. A connected
  agent's is still open (README.md, section 11).
- **The agent accepts at once** over pipes. A daemon decides by its
  configuration (agent.md).
- **Required and optional kinds.** Each version names the kinds it
  requires each side to take. A kind added within a version is optional:
  a peer that does not take it leaves it out of its terms, and the writer,
  refused at its entrance, carries on without it.
- **Terms.** Each side checks the other's terms for every required kind it
  may send, and refuses with *limits*, naming the kind, when one is missing
  or too small. The agent's sending bounds come from its configuration, and
  must fit within what its hosts take. A host with smaller limits than its
  agents' configuration then finds out at the first opening, not in the
  middle of a run.
- **smith's refusal reasons,** from the application's range (skein's
  `channel.md`, 5.1):
  - **rules:** the peer broke the channel's rules (section 5);
  - **busy:** a connected agent is serving another run;
  - **unauthorized:** a connected agent did not accept the credential.

## 3. Down, from the host

| Kind | Record | When |
|---|---|---|
| start | the start (below) | first, once |
| message | a name; the sender's label; the text | any time after the start, in order |
| answer | a call's name, and its answer (below) | once per call, also after a cancel |
| acknowledge | a turn's number: that turn and every one before it are kept durably | as the host keeps them |
| grant | an account; a generation; how long it is valid; its value (section 6) | when the host refreshes one |
| cancel | nothing | at most once |

**The start** (domain/run.md, 3.2):

- **the activation number,** new for every start of the run;
- **the charter,** as bytes (charter.md);
- **the workspace,** if any: for each directory,
  - the name the LLM calls it;
  - where it sits on the agent's file system;
  - whether it may be written;
  - whether it is a git repository;
  - for a merge in progress, the files left in conflict;
- **the transcript,** if the run resumes: its turns' bytes, in order, as
  they were told (transcript.md);
- **the calls answered after the last turn,** each a name and an answer.
  Each names a call that the transcript's last turn holds without a
  result, and the run restores it as that call's result
  (domain/session.md, section 3). A call whose turn was never told is not
  among them: its host tells of it as text in a waking message
  (domain/run.md, section 6);
- **the grants** the charter's endpoints need;
- **the window:** how many turns, and how many bytes of them, may be
  unacknowledged (section 7). It must hold at least one turn of the largest
  size the agent may send, or the start is invalid. A host that keeps no
  turns gives a window it will never fill, and acknowledges each turn at
  once.

**An answer to a call** is one of:

- **for a host tool,** its text as a result or as an error (domain/run.md,
  5.2);
- **for a delivery,** delivered, nothing, refused, failed or stale, with
  what each carries (domain/run.md, 8.2):
  - delivered names each directory and what was made of it;
  - refused names what the LLM can fix;
  - failed carries its reason, and the tail of the host's diagnostic
    output with the bytes dropped before it;
- **busy:** the host has no room to decide it now;
- **unavailable:** the host cannot say what became of it.

## 4. Up, from the run

| Kind | Record | When |
|---|---|---|
| admitted | nothing | once, when the start is admitted |
| call | a name; the tool; its effect; its deadline, as a duration; what it asks (below) | while the run lives, each name once while in flight |
| withdraw | a call's name | once per call, at most |
| turn | its number in this activation; the spend so far; the last message read, if any; its body, as bytes (transcript.md) | numbered from one in each activation, each once |
| waiting | the last message read | when the run waits for a message |
| long | how long, at most, as a duration | when checks start |
| long done | nothing | when they end |
| rejected | an account and a generation | when a provider refused that credential |
| exhausted | an account, and how long until it may be used again | when its account ran out |
| fact | one fact (section 8) | best effort |
| answer | the turns it took; what it spent; how it ended (below) | the last word |

**What a call asks:**

- **a host tool:** its name, and the input the LLM wrote, as bytes;
- **a delivery:** the result's fields, each a name and text, which the host
  reads to do what delivering means to it.

**How a run ended** (domain/run.md, section 10):

- **accepted,** with its result, as bytes (charter.md);
- **parked;**
- **failed,** typed, so the host can act without reading prose:
  - the model, with its provider's failure class;
  - the budget, naming what ran out;
  - policy;
  - cancelled;
  - stale;
  - a transcript it could not resume, saying why;
- **refused at the entrance:** busy, or invalid, saying what. A refused
  start is answered with no admitted before it, no turns and no spend.

## 5. Order and rules

- **Down:**
  - the start comes first, once;
  - answers to calls come once per call, even after a cancel, since a
    delivery is never abandoned (domain/host.md, section 2);
  - the cancel comes at most once.
- **Up:**
  - admitted, or a refused answer, comes first;
  - turns are consecutive from one, and their spend never falls;
  - a call's name is used once while it is in flight;
  - the answer is the last word, and its turn count is the turns told in
    this activation.

  A turn's number on the channel counts this activation's turns. Its place
  in the conversation, which goes on across activations, is in its body
  (transcript.md, section 2).
- **Who checks what.**
  - **Each half checks what its codecs and the order can show:** a record
    that does not decode, a kind out of order, a number out of sequence.
  - **The domains check the rest,** as domain/host.md, section 2 lists.
  - **A charter that does not decode** is not a broken rule: the agent
    answers it as an invalid start.
- **A broken rule ends the channel.** The side that sees it refuses with
  *rules*. On the host's side, its domain hears of an agent failure.

## 6. Grants

- **Down as values:**
  - an account;
  - a generation;
  - how long it is valid;
  - the credential, with whatever skein's LLM client needs beside it, such
    as an account's id.
- **The agent's half keeps the values** in a table holding two generations
  per account, so a refresh does not cut a call in flight. It hands the
  domain only the account, the generation and when the grant lapses. The
  LLM component reads the value from the table when it starts a call
  (llm.md).
- **The host's half fills the values in** from its host's credentials, so
  the host's domain also names only accounts and generations
  (domain/host.md, section 7).
- **A value goes nowhere else:** not into a turn, a fact, a trace or a
  charter.

## 7. Flow control

- **An acknowledgement means durable.** The host acknowledges a turn once
  it would survive the host's own crash, since the agent then holds it no
  longer. A host that keeps turns less safely acknowledges at once, and
  does not resume from what it may have lost.
- **The window.** The agent's half counts turns, and their bytes, sent and
  not yet acknowledged. When the window is full, the run waits, without
  starting another completion, until an acknowledgement makes room
  (domain/host.md, section 6). A host also stops reading a run that goes
  past it. The host's watchdog is paused while the run waits for room.
- **Facts** go only while the queue's room leaves enough free for the
  window, the calls in flight and the answer. Facts that do not fit are
  dropped, counted by the domain that made them, and never hold anything
  back.
- **Each half's queue** is sized from its domain's limits:
  - **the agent's:** the window, the calls in flight, the notices, the
    answer, and a reserve for facts;
  - **the host's:** the start, the messages in flight, the answers to the
    calls in flight, the acknowledgements, the grants and a cancel.

  So a domain that keeps its own bounds is never refused by its channel.
- **Progress.** Every record the host's half reads counts as progress for
  its watchdog. A connected agent's channel adds skein's keepalive, with
  silence and stall deadlines (hosts.md). Over pipes, the process's end is
  the channel's end.

## 8. Facts

A fact is content-free (domain/run.md, section 11): what happened, and
when, as a duration since the run's start. Its kinds:

- a run admitted or ended;
- an LLM call started, retried or finished;
- a tool, or a check, started or finished;
- text arriving from the LLM, counted, never carried.

A trace with content is not a fact, and never crosses the channel: where
one is kept is the agent's configuration (agent.md).

## 9. What the domains are owed, and what skein owes

- **The domains are owed:**
  - **translation** of every record into their entities, and back, with
    small total functions: `smith-domain` on the agent's side and
    `smith-host-domain` on the host's;
  - **durations** turned into deadlines on their own clocks;
  - **grants** reduced to names.
- **skein owes:**
  - the framed channel (skein's `channel.md`);
  - codecs generated from `smith-channel`'s schema (skein's `codec.md`);
  - streams: pipes and sockets from io, and TLS.

## 10. The world

- **The codecs:**
  - golden bytes for every record;
  - each decoder fuzzed in the fuzzy suite.
- **A protocol world:** the agent's half with `smith-domain`, under a
  scripted LLM and machine, against the host's half with
  `smith-host-domain`, under a scripted host. Both are joined over a pipe
  each way, and over a socket-like stream.
- **Its stories:**
  - a run from start to answer;
  - a run resumed from a transcript, with calls answered after its last
    turn restored as their results;
  - a run parked, resumed and parked again, its turns numbered from one on
    the channel each time and without a gap in the conversation;
  - a host tool answered as the agent loses its channel, before
    the turn is told;
  - host tools answered busy and asked again under their names;
  - a delivery, and one found stale;
  - a cancel in the middle of a turn;
  - a full window holding turns back until acknowledged;
  - a window that holds one turn of the largest size, and one too small,
    refused;
  - an older peer that leaves an optional kind out of its terms;
  - grants refreshed while a call is in flight;
  - facts dropped under pressure;
  - no version in common;
  - a host's terms too small, refused at the opening;
  - an agent that exits after its last word, still read to the end.
- **Its referee:**
  - what one domain sent, the other received;
  - one answer per run;
  - every call answered once;
  - turns in order;
  - no credential's value in a turn, a fact or a domain.

## 11. From temper

- **Kept from temper's channel's agent hop,** in its second version:
  - the start, with a transcript and conflicted files;
  - numbered turns with spend and the last message read;
  - calls, withdrawals, long operations and waiting;
  - notices of rejected and exhausted accounts;
  - grants as values beside names;
  - the answer's turn count and spend.
- **New:**
  - acknowledgements on this hop, within a window;
  - admitted;
  - named messages with a sender's label;
  - deliveries answered in smith's terms;
  - typed failures;
  - smith's refusal reasons.
- **Gone:**
  - snapshots;
  - endpoints in the start, since the agent is configured with its own;
  - temper's kinds and their numbers;
  - the push, which becomes a delivery.

## 12. Open questions

- **A transcript in pieces,** if a start's frame proves too large to hold
  whole on either side.
- **A connected agent's credential** (README.md, section 11).
- **Which facts cross:** whether a host wants more than liveness from them,
  such as a call's model or a tool's name.
