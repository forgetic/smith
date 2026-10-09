# The channel

Provisional, 2026-10-07, revised 2026-10-09. The channel between a run and
its host is smith's application protocol on skein's framed channel
(skein's `channel.md`). This document covers:

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
  - the start, with the messages that triggered the run and its identity;
  - messages;
  - answers to calls;
  - acknowledgements of turns;
  - grants;
  - a cancel.
- **Up, from the run:**
  - admitted;
  - messages refused, typed;
  - calls and withdrawals;
  - turns;
  - waiting;
  - long operations;
  - notices about credentials;
  - facts;
  - the answer, its last word, with the last message read.
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
  - facts go only while the run is admitted and has not answered, and only
    with room to spare;
  - each half's queue is sized from its domain's limits.

## 2. Opening

- **The magic is `smth`.** Versions start at 1. Each version is one table
  of kinds, and the records of that version's schema. Each half speaks
  one version, its release's, and offers only that one: the channel is
  pre-release, and no half reads an older version (README.md, section
  8).
- **The credential** is empty over a spawned agent's pipes. A connected
  agent's is still open (README.md, section 11).
- **The agent accepts at once** over pipes. A daemon decides by its
  configuration (agent.md).
- **Required and optional kinds.** Each version names the kinds it
  requires each side to take, and any it leaves optional: a peer that
  does not take an optional kind leaves it out of its terms, and the
  writer, refused at its entrance, carries on without it. A kind is
  never added within a version: a new kind, required or optional, is a
  new version.
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
- **the run's identity,** if the host keeps one: 16 opaque bytes, the
  same in every start of the run, resumes included. Without one, the run
  mints its own (domain/run.md, 3.2). It reaches providers as the
  conversations' affinity (llm.md);
- **the charter,** as bytes (charter.md);
- **the workspace,** if any: for each directory,
  - the name the LLM calls it;
  - where it sits on the agent's file system;
  - whether it may be written;
  - whether it is a git repository;
  - for a merge in progress, the files left in conflict;
- **the transcript,** if the run resumes: its turns' bytes, in order, as
  they were told, from the last turn whose record had `opens_window`
  (section 4; transcript.md, section 3). Earlier windows are never sent;
- **the calls the transcript does not hold as answered,** that the host
  answered: a call whose turn was never told, or one the run withdrew and
  the host decided later. Each is a name, the tool, and the answer the
  host decided: a host tool's text, as a result or an error; a delivery's
  answer; or too large, when that answer is larger than the run takes.
  Busy, unavailable and withdrawn are not decisions, and never appear
  here. The run tells the LLM of them as text in its waking prompt, since
  they have no open place in the history to go back to (domain/run.md,
  section 6);
- **the messages** that triggered the run, if any: each a message record
  (a name, the sender's label, the text), in order, at most as many as
  the run's inbox holds, each within the run's message bound as the run
  reads it (section 5); a start whose messages do not fit is invalid.
  They enter the inbox at admission, before any message sent after the
  start (domain/host.md, section 2). A headless host sends its prompt so;
  a host that starts a run before it has a message sends none;
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
- **unavailable:** the host cannot say what became of it;
- **withdrawn:** the host settled a call the run withdrew, without giving
  its answer. Whatever the host did stands, and the run asks again under
  the same name to learn it (domain/run.md, 5.2);
- **too large:** the host's answer is larger than the run takes. The run
  tells the LLM so, as an error, and does not ask again, since the host
  would answer the same.

## 4. Up, from the run

| Kind | Record | When |
|---|---|---|
| admitted | nothing | once, when the start is admitted |
| message refused | a message's name; why: too large, the inbox full, a name in use, or the run ending | when the run does not take a message |
| call | a name; the tool; its effect; its deadline, as a duration; what it asks (below) | while the run lives, each name once while in flight |
| withdraw | a call's name | once per call, at most |
| turn | its number in this activation; the spend so far; the last message read, if any; `opens_window`; its body, as bytes (transcript.md) | numbered from one in each activation, each once |
| waiting | the last message read, if any | when the run waits for a message, or awaits its first |
| long | how long, at most, as a duration | when the checks, an LLM completion or a command start |
| long done | nothing | when one ends |
| rejected | an account and a generation | when a provider refused that credential |
| exhausted | an account, and how long until it may be used again | when its account ran out |
| fact | one fact (section 8) | while the run is admitted and has not answered, with room to spare; any other is not projected, and counted (section 7) |
| answer | the turns it took; what it spent; the last message read, if any; how it ended (below) | the last word |

**Long operations** are what the run waits on below it with deadlines of
its own, so the host's watchdog stretches over them (domain/host.md,
section 4). They may overlap: each long done ends one begun before it,
and the watchdog stays stretched while any is open, to the latest end
their spans allow.

**The last message read** is the run's fence: on a turn, on waiting, and
on the answer, where it is final. Every message sent and not covered by
the answer's fence went unread (domain/host.md, section 2). It is absent
while the run has read nothing: a run awaiting its first message waits
with no fence.

**`opens_window`** is true for a turn of kind window and for a
conversation's first turn, the two turns a transcript can start from
(transcript.md, section 3), and false for every other. A host that keeps
turns as bytes reads where the current window starts from it, without
decoding a body: resuming a run, it sends back only the turns from the
last one with `opens_window` (domain/host.md, section 6).

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
- **refused at the entrance:** busy, or invalid, saying what, such as a
  start's messages beyond the run's bounds. A refused start is answered
  with no admitted before it, no turns and no spend, and its messages are
  refused with it.

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
  - a message is refused at most once, and only before a fence covers it;
  - the last message read names a message the host sent, and never goes
    back across turns, waits and the answer;
  - a long done ends a long operation still open;
  - the answer is the last word, and its turn count is the turns told in
    this activation.

  A turn's number on the channel counts this activation's turns. Its place
  in the conversation, which goes on across activations, is in its body
  (transcript.md, section 2).
- **Who checks what.**
  - **Each half checks what its codecs and the order can show:** a record
    that does not decode, a kind out of order, a number out of sequence.
  - **The domains check the rest,** as domain/host.md, section 2 lists.
  - **The agent checks every message at ingress,** after rendering it as
    the run reads it (its label, a colon and a space, then its text)
    against the run's message bound, and checks its inbox's room and the
    name against those in use. One it cannot take is refused, typed, with
    a message refused record, and is never handed further, so nothing below
    ingress asserts on what a host sent (domain/run.md, section 6). The
    host's half and kit check with the same arithmetic first, and refuse
    at their own entrance with the same reasons (domain/host.md, section
    4), so a host that sends what its kit admits sees few refusals from
    the agent; those it sees are a message's terminal, not a broken rule.
  - **A charter that does not decode** is not a broken rule: the agent
    answers it as an invalid start.
- **A broken rule ends the channel.** The side that sees it refuses with
  *rules*. On the host's side, its domain hears of an agent failure.
- **Version 2.** One bump covers these records and rules:
  - the start's messages and the run's identity (section 3);
  - the last message read in the answer, optional on waiting,
    `opens_window` on turns, and long operations for LLM completions and
    commands (section 4);
  - messages checked at ingress after rendering, and refused typed
    (section 4 and above);
  - facts projected only while the run is admitted and has not answered,
    stamped when emitted (sections 7 and 8).

  Both halves move to it together. A peer that speaks only version 1
  shares no version with one that speaks only version 2, and the run ends
  before its start is read (section 2).

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
- **Facts are projected, never held.** The agent's facts reach the
  channel as one of their sinks; the event stream is another, beside it,
  with its own policy (events.md; agent.md, section 5). The
  channel projects a fact only while the run is admitted and has not
  answered, and only while the queue's room leaves enough free for the
  window, the calls in flight, the notices, the long operations and the
  answer. Every other fact, before admitted, after the answer or without
  room, is dropped from the projection and counted as not projected; none
  holds the run back, and none is taken from the other sinks.
- **Each half's queue** is sized from its domain's limits:
  - **the agent's:** the window, the calls in flight, a refusal for each
    message in flight, the notices, the long operations open, the answer,
    and a reserve for facts;
  - **the host's:** the start, the messages in flight, the answers to the
    calls in flight, the acknowledgements, the grants and a cancel.

  So a domain that keeps its own bounds is never refused by its channel.
- **Progress.** Every record the host's half reads counts as progress for
  its watchdog. A connected agent's channel adds skein's keepalive, with
  silence and stall deadlines (hosts.md). Over pipes, the process's end is
  the channel's end.

## 8. Facts

A fact is content-free (domain/run.md, section 11): what happened, and
when, as a duration since the run's start. The time is taken when the
fact is emitted, never when it is projected, so a fact that waited in a
queue still says when it happened. The kinds the channel projects, for
liveness:

- a run admitted or ended;
- an LLM call started, retried or finished;
- a tool, or a check, started or finished;
- text arriving from the LLM, counted, never carried.

The run's other facts (domain/run.md, section 11) reach the event stream
only.

A trace with content is not a fact, and never crosses the channel. It is
the event stream, a separate sink of the same facts with content as its
capture policy allows, under its own delivery policy and its own count of
loss (events.md). Where one is kept is the agent's configuration
(agent.md).

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
  - a start carrying messages, which the run opens on, and a start whose
    messages exceed the run's inbox, refused as invalid;
  - the run's identity kept across a park and a resume;
  - messages that cross the answer, its fence naming the last one read
    and the rest unread;
  - a message one byte over the run's bound as the run reads it, one
    beyond its inbox and one under a name in use, sent past the host's
    own checks, each refused typed, with nothing below ingress failing;
  - an LLM completion and a command that outlast the no-progress
    deadline, each reported as a long operation, and the run not stopped;
  - a run resumed from a transcript, with calls answered that the
    transcript does not hold as answered;
  - a run compacted, parked and resumed, the host sending back only the
    turns from the last with `opens_window`;
  - a run awaiting its first message, its waiting record with no fence;
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
  - a peer that leaves an optional kind out of its terms;
  - grants refreshed while a call is in flight;
  - facts dropped under pressure, and facts after the answer, each
    counted as not projected while the event stream keeps them all;
  - no version in common, a version-1 peer among them;
  - a host's terms too small, refused at the opening;
  - an agent that exits after its last word, still read to the end.
- **Its referee:**
  - what one domain sent, the other received;
  - one answer per run;
  - every message ends once: read by a fence, refused, or unread after
    the answer's fence;
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
  - named messages with a sender's label, carried in the start too, each
    with one terminal;
  - the run's identity, and the answer's fence;
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
