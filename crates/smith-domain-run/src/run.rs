//! Run admission, main and child lifetimes, shared spend and terminal rights
//! (domain/run.md, sections 3, 7, 8 and 10). State retains the charter, prepared
//! guides/checks, conversation bindings, call ownership and one pending ending.
//! Entrances below act only through bounded queues, slabs and injected time.
//! The run knows no host policy, forge, receipt encoding, authentication or
//! provider syntax. Generic final forms are judged before effects; delivery is
//! one exclusive checked snapshot. Before submission checks may abort; after
//! submission the actual bounded host terminal remains owed. Final Change
//! landing wins shutdown; a mid-run delivery settles before the pending
//! ending answers. Sessions alone price actual own usage;
//! global scalar admission sums monotonic own deltas, while inclusive subtree
//! totals transfer only as child bills. A charge that cannot fit ends the
//! session before it is added (domain/run.md, sections 9, 10 and 14).

use alloc::boxed::Box;
use core::mem;

use skein_lib::bytes::copy_of;
use skein_lib::{Deadlines, Duration, Env, Id, Queue, ReplyTo, Slab, Time, Token};

use crate::agent::{self, Child, Means};
use crate::boundary::{
    Answer, Ask, AskRefusal, End, Failure, Fault, Invalid, Opening, Policy, Ran, Read, Refusal, Request, Returned, Stop,
};
use crate::budget::{Exhausted, Spend};
use crate::call::{Call, Calls, Withdrawal, Work};
use crate::charter::{self, Charter, Families, count};
use crate::delivery::{CallName, Delivery};
use crate::domain::Domain;
use crate::facts::{Asked, Fact};
use crate::land::{self, Settled};
use crate::limits::Limits;
use crate::outcome::{self, Change, Declared};
use crate::prepare::{self, Found, Step};
use crate::prompt;

#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) charter: Charter,
    pub(crate) workspace: Option<crate::Workspace>,
    /// What it found in its checkout as it prepared.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) found: Found,
    /// The host's name for it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) host_name: Token,
    /// Host-supplied number that distinguishes this activation's call names.
    /// Contract: domain/run.md, sections 3.2 and 8.2.
    pub(crate) activation: u64,
    /// What its conversations have spent.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    spent: Spend,
    /// Nudges given.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    nudges: u32,
    /// Outcomes `finish` refused: rejected, or not landed.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    rejected: u32,
    /// Its conversations, main included, opened and not ended.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    conversations: u32,
    /// When its budget's time runs out.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    deadline: Time,
    state: State,
    transcript: Option<Token>,
    inbox: Queue<Message>,
    offered: Option<Token>,
    read: Option<Token>,
    waiting: bool,
    turns: u32,
    sequence: Option<u32>,
}

/// Retained FIFO payload; parent names remain opaque. Payload moves into Say.
/// Contract: domain/run.md, sections 6 and 14.
#[derive(Debug)]
pub(crate) struct Message {
    name: Token,
    text: Box<[u8]>,
}

#[derive(Debug)]
enum State {
    /// Looking in its checkout: the look `step` is in flight. Its main
    /// conversation has its slot, and is not opened yet.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Preparing { reply_to: ReplyTo, main: Id<Conversation>, step: Step },
    /// It fails with `failure` once the look in flight has ended.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Stopping { reply_to: ReplyTo, main: Id<Conversation>, failure: Failure },
    /// Its main conversation is at work.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Working { reply_to: ReplyTo, main: Id<Conversation> },
    /// Actual settled main yield after wait; wall and idle timers remain distinct.
    Waiting { reply_to: ReplyTo, main: Id<Conversation>, until: Time },
    /// It has spent past its budget's `exhausted` part, and main keeps the
    /// turn in flight.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Over { reply_to: ReplyTo, main: Id<Conversation>, exhausted: Exhausted },
    /// It ends with `ending` once its main conversation has ended.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Winding { reply_to: ReplyTo, ending: Ending },
    /// Terminal: holds nothing.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Closed,
}

/// How a winding run ends.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Debug)]
enum Ending {
    Parked,
    Accepted(Declared),
    Failed(Failure),
}

/// A conversation a run opened: main, or a sub-agent.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Debug)]
pub(crate) struct Conversation {
    run: Id<Run>,
    /// The sub-agent call it serves, if it is a sub-agent.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    asker: Option<Id<Call>>,
    /// Its families of tools, and how deep it is: main is at zero, a sub-agent
    /// one deeper than its asker.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    families: Families,
    depth: u32,
    /// What it has spent, by its `Used` so far.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    spent: Spend,
    subtree_spent: u64,
    /// Its calls to the run in flight.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    calls: u32,
    phase: Phase,
}

#[derive(Debug)]
enum Phase {
    /// Not opened yet: its run is preparing.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Pending,
    /// Opened, and not started yet.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Opening,
    /// Opened, not started yet, and no longer wanted: closed once it starts.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Unwanted,
    /// Started, and addressed as `peer`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Running { peer: Token },
    /// Closed by its run, and not ended yet.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Closing,
    /// Terminal: holds nothing.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Closed,
}

/// The timers of runs and their calls, named by what they are for.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Alarm {
    /// The budget's time of the run `run` runs out.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Deadline { run: Id<Run> },
    /// Idle expiry of an actual waiting main. Contract: domain/run.md, section 6.
    Park { run: Id<Run> },
    /// The deadline of the call `call` passes.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Call { call: Id<Call> },
    /// Per-relay timeout or retry backoff, independent of caller expiry.
    /// Contract: domain/run.md, section 5.2.
    Host { call: Id<Call> },
}

// Entry points, one per event or alarm: look the conversation or run up, take
// its state out, run the cell's handler, follow the run's new state.

/// Owned Start fields moved directly from the boundary event into admission.
/// This transient bundle retains no additional copy or separate allocation.
/// Contract: domain/run.md, sections 3.2, 10 and 14.
#[derive(Debug)]
pub(crate) struct Start {
    /// Affine right to the run's one final answer (domain/run.md, section 10).
    pub(crate) reply_to: ReplyTo,

    /// Host-selected logical run identity (domain/run.md, sections 3.2 and 10).
    pub(crate) host_run: Token,
    /// Host-supplied positive activation number (domain/run.md, sections 3.2 and 8.2).
    pub(crate) activation: u64,

    /// Immutable requested contract and budget (domain/run.md, sections 3.1 and 14).
    pub(crate) charter: Charter,

    /// Optional host-owned mount metadata (domain/run.md, sections 3.2 and 14).
    pub(crate) workspace: Option<crate::Workspace>,

    /// Optional opaque history handle (domain/run.md, sections 11 and 14).
    pub(crate) transcript: Option<Token>,
}

pub(crate) fn start(domain: &mut Domain, env: &Env<Limits>, start: Start, out: &mut Queue<Request>) {
    let Start { reply_to, host_run, activation, charter, workspace, transcript } = start;
    let Domain { runs, conversations, calls: _, alarms, facts } = domain;
    // A charter that can never fit is invalid, room or not: busy invites a
    // retry.
    if activation == 0 {
        out.push(Request::Answer { to: reply_to, answer: Answer::Refused(Refusal::Invalid(Invalid::Activation)) });
        return;
    }
    if let Err(invalid) = charter::check(&charter, workspace.as_ref(), &env.limits) {
        out.push(Request::Answer { to: reply_to, answer: Answer::Refused(Refusal::Invalid(invalid)) });
        return;
    }
    if runs.is_full() || conversations.is_full() {
        out.push(Request::Answer { to: reply_to, answer: Answer::Refused(Refusal::Busy) });
        return;
    }
    let deadline = env.now.saturating_add(charter.budget.time);
    let found = Found::with_capacity(count(crate::workspace::directories(workspace.as_ref()).len()));
    // A run is stored before its main conversation, which names it, and starts
    // once main has a name too: main's slot is the run's from its admission.
    let run = Run {
        charter,
        workspace,
        found,
        host_name: host_run,
        activation,
        spent: Spend::ZERO,
        nudges: 0,
        rejected: 0,
        conversations: 1,
        deadline,
        state: State::Closed,
        transcript,
        inbox: Queue::with_capacity(env.limits.messages),
        offered: None,
        read: None,
        waiting: false,
        turns: 0,
        sequence: None,
    };
    let id = runs.insert(run).expect("checked for room above");
    facts.about(id.token());
    let admitted = runs.get(id).expect("inserted above");
    let families = crate::workspace::families(admitted.workspace.as_ref(), Families::of(&admitted.charter.grants));
    let conversation = Conversation {
        run: id,
        asker: None,
        families,
        depth: 0,
        spent: Spend::ZERO,
        subtree_spent: 0,
        calls: 0,
        phase: Phase::Pending,
    };
    let main = conversations.insert(conversation).expect("checked for room above");
    out.push(Request::Admitted { host_run, run: id.token() });
    let run = runs.get_mut(id).expect("inserted above");
    run.state = match prepare::next(&run.charter, run.workspace.as_ref(), None) {
        Some(step) => look(run, id, reply_to, main, step, env, out),
        None => open(run, conversations, reply_to, main, env.now, out),
    };
    follow(runs, alarms, id);
}

/// Retain a host-bounded named message while the run can read it; a message
/// arriving during shutdown remains unread. Input precedes idle alarms.
/// Contract: domain/run.md, sections 6 and 14.
pub(crate) fn message(
    domain: &mut Domain,
    env: &Env<Limits>,
    token: Token,
    name: Token,
    text: Box<[u8]>,
    out: &mut Queue<Request>,
) {
    let id = Id::<Run>::from_token(token);
    let Some(run) = domain.runs.get(id) else {
        return;
    };
    match run.state {
        State::Stopping { .. } | State::Winding { .. } | State::Closed => return,
        State::Preparing { .. } | State::Working { .. } | State::Waiting { .. } | State::Over { .. } => {}
    }
    assert!(
        u64::try_from(text.len()).expect("owned length fits") <= u64::from(env.limits.message_bytes),
        "host message exceeds run limit"
    );
    assert!(run.offered != Some(name) && run.read != Some(name), "host reused a message name");
    for queued in &run.inbox {
        assert!(queued.name != name, "host reused a queued message name");
    }
    assert!(run.inbox.room() > 0, "host exceeded run message count");
    domain.facts.about(token);
    let run = domain.runs.get_mut(id).expect("checked live run above");
    run.inbox.push(Message { name, text });
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Waiting { reply_to, main, until: _ } => {
            let conversation = domain.conversations.get(main).expect("waiting main remains alive");
            let peer = match conversation.phase {
                Phase::Running { peer } => peer,
                Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Closing | Phase::Closed => {
                    unreachable!("waiting requires an actual yielded running main")
                }
            };
            let message = run.inbox.pop().expect("admitted one message above");
            continue_message(run, reply_to, main, peer, message, out)
        }
        state @ (State::Preparing { .. } | State::Working { .. } | State::Over { .. }) => state,
        State::Stopping { .. } | State::Winding { .. } | State::Closed => unreachable!("checked active run"),
    };
    follow(&mut domain.runs, &mut domain.alarms, id);
}

fn continue_message(
    run: &mut Run,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    peer: Token,
    message: Message,
    out: &mut Queue<Request>,
) -> State {
    assert!(run.offered.is_none(), "the previous continuation has produced its actual turn");
    run.offered = Some(message.name);
    run.waiting = false;
    say(reply_to, main, peer, message.text, out)
}

/// Root calls this only for main; child bodies are discarded at its routing seam.
/// Actual turns may still arrive during shutdown, before the terminal.
/// Contract: domain/run.md, sections 6, 10 and 13.
pub(crate) fn turn(domain: &mut Domain, conversation: Token, record: Token, sequence: u32, out: &mut Queue<Request>) {
    let conversation = domain.conversations.get(Id::from_token(conversation)).expect("turn precedes session end");
    assert!(conversation.asker.is_none(), "only main enters the host transcript");
    let run = domain.runs.get_mut(conversation.run).expect("run outlives its actual turns");
    match run.sequence {
        Some(previous) => assert!(previous.checked_add(1) == Some(sequence), "exact historical turn prefix"),
        None => assert!(sequence > 0, "historical sequence is one-based"),
    }
    run.sequence = Some(sequence);
    run.turns = run.turns.checked_add(1).expect("activation count is bounded by historical sequence");
    if let Some(name) = run.offered.take() {
        run.read = Some(name);
    }
    domain.facts.about(conversation.run.token());
    out.push(Request::Turn { host_run: run.host_name, record, number: run.turns, read: run.read, spent: run.spent });
}

pub(crate) fn park(domain: &mut Domain, id: Id<Run>, out: &mut Queue<Request>) {
    let run = domain.runs.get_mut(id).expect("idle alarm is cancelled before retirement");
    domain.facts.about(id.token());
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Waiting { reply_to, main, until: _ } => {
            assert!(run.inbox.is_empty(), "an admitted wake cancels idle expiry");
            wind_down(&mut domain.conversations, reply_to, main, Ending::Parked, out)
        }
        State::Preparing { .. }
        | State::Stopping { .. }
        | State::Working { .. }
        | State::Over { .. }
        | State::Winding { .. }
        | State::Closed => {
            unreachable!("idle alarm belongs only to settled Waiting")
        }
    };
    follow(&mut domain.runs, &mut domain.alarms, id);
}

pub(crate) fn read(domain: &mut Domain, env: &Env<Limits>, owner: Token, read: Read, out: &mut Queue<Request>) {
    let Domain { runs, conversations, calls: _, alarms, facts } = domain;
    let id = Id::<Run>::from_token(owner);
    facts.about(owner);
    let run = runs.get_mut(id).expect("a run lives until its look in flight has ended");
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Preparing { reply_to, main, step } => {
            prepare::guide(&mut run.found, step, read, &env.limits);
            prepared(run, conversations, id, reply_to, main, step, env, out)
        }
        State::Stopping { reply_to, main, failure } => stop(run, conversations, reply_to, main, failure, out),
        State::Working { .. } | State::Waiting { .. } | State::Over { .. } | State::Winding { .. } | State::Closed => {
            unreachable!("a run reads only while it prepares")
        }
    };
    follow(runs, alarms, id);
}

pub(crate) fn probed(domain: &mut Domain, env: &Env<Limits>, owner: Token, executable: bool, out: &mut Queue<Request>) {
    let Domain { runs, conversations, calls: _, alarms, facts } = domain;
    let id = Id::<Run>::from_token(owner);
    facts.about(owner);
    let run = runs.get_mut(id).expect("a run lives until its look in flight has ended");
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Preparing { reply_to, main, step } => {
            prepare::checks(&mut run.found, step, executable);
            prepared(run, conversations, id, reply_to, main, step, env, out)
        }
        State::Stopping { reply_to, main, failure } => stop(run, conversations, reply_to, main, failure, out),
        State::Working { .. } | State::Waiting { .. } | State::Over { .. } | State::Winding { .. } | State::Closed => {
            unreachable!("a run probes only while it prepares")
        }
    };
    follow(runs, alarms, id);
}

pub(crate) fn cancel(domain: &mut Domain, run: Token, out: &mut Queue<Request>) {
    let Domain { runs, conversations, calls: _, alarms, facts } = domain;
    let id = Id::<Run>::from_token(run);
    // A cancel travels down, so it may name a run that has answered and gone.
    let Some(run) = runs.get_mut(id) else {
        return;
    };
    facts.about(id.token());
    let cancelled = Ending::Failed(Failure::Cancelled);
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Preparing { reply_to, main, step: _ } => State::Stopping { reply_to, main, failure: Failure::Cancelled },
        State::Working { reply_to, main }
        | State::Waiting { reply_to, main, until: _ }
        | State::Over { reply_to, main, exhausted: _ } => wind_down(conversations, reply_to, main, cancelled, out),
        // How the run ends is decided already.
        State::Stopping { reply_to, main, failure } => State::Stopping { reply_to, main, failure },
        State::Winding { reply_to, ending } => State::Winding { reply_to, ending },
        // It answered in this iteration, and is retired already.
        State::Closed => return,
    };
    follow(runs, alarms, id);
}

pub(crate) fn started(domain: &mut Domain, conversation: Token, peer: Token, out: &mut Queue<Request>) {
    let id = Id::<Conversation>::from_token(conversation);
    let conversation = domain.conversations.get_mut(id).expect("a conversation lives until it has ended");
    domain.facts.about(conversation.run.token());
    let phase = mem::replace(&mut conversation.phase, Phase::Closed);
    conversation.phase = match phase {
        Phase::Opening => Phase::Running { peer },
        Phase::Unwanted => closing(peer, out),
        Phase::Pending | Phase::Running { .. } | Phase::Closing | Phase::Closed => {
            unreachable!("a conversation starts once it is opened, before anything else")
        }
    };
}

pub(crate) fn yielded(
    domain: &mut Domain,
    env: &Env<Limits>,
    conversation: Token,
    stop: Stop,
    text: &[u8],
    out: &mut Queue<Request>,
) {
    let Domain { runs, conversations, calls, alarms, facts } = domain;
    let id = Id::<Conversation>::from_token(conversation);
    let conversation = conversations.get(id).expect("a conversation lives until it has ended");
    facts.about(conversation.run.token());
    let peer = match &conversation.phase {
        Phase::Running { peer } => *peer,
        // The yield crossed the run's close: there is nothing left to decide.
        Phase::Closing => return,
        Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Closed => {
            unreachable!("a conversation yields only between starting and ending")
        }
    };
    // A sub-agent that yields is done: its last message is its answer.
    if let Some(asker) = conversation.asker {
        let call = calls.get_mut(asker).expect("a sub-agent's call lives until it has ended");
        let close_child = match &mut call.work {
            Work::Child(child) => agent::yielded(child, text, stop, &env.limits),
            Work::Landing(_) | Work::Host(_) => unreachable!("a sub-agent serves a sub-agent's call"),
        };
        if let Some(child) = close_child {
            close(conversations, child, out);
        }
        return;
    }
    let run_id = conversation.run;
    let run = runs.get_mut(run_id).expect("a run lives until its conversations have ended");
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Working { reply_to, main } => {
            assert!(main == id, "a conversation with no asker is main");
            assert!(conversation.calls == 0, "a main yield follows every actual call terminal");
            match run.inbox.pop() {
                Some(message) => continue_message(run, reply_to, main, peer, message, out),
                None if run.waiting => {
                    out.push(Request::Waiting { host_run: run.host_name, read: run.read });
                    if run.charter.waiting == Duration::ZERO {
                        wind_down(conversations, reply_to, main, Ending::Parked, out)
                    } else {
                        State::Waiting { reply_to, main, until: env.now.saturating_add(run.charter.waiting) }
                    }
                }
                None => match nudge(run, stop, &env.limits) {
                    Ok(()) => say(reply_to, main, peer, prompt::nudge(stop, run.nudges, env.limits.nudges), out),
                    Err(failure) => wind_down(conversations, reply_to, main, Ending::Failed(failure), out),
                },
            }
        }
        State::Over { reply_to, main, exhausted } => {
            wind_down(conversations, reply_to, main, Ending::Failed(Failure::Budget(exhausted)), out)
        }
        State::Preparing { .. }
        | State::Stopping { .. }
        | State::Waiting { .. }
        | State::Winding { .. }
        | State::Closed => {
            unreachable!("main yields only while its run works: it is closed when the run winds down")
        }
    };
    follow(runs, alarms, run_id);
}

/// Session own prices are monotonic activation totals. Stale names are inert;
/// subtree updates do not add global spend (domain/run.md, sections 9 and 14).
#[must_use]
pub fn completion_overflow(domain: &Domain, conversation: Token, price: u64, usage: Spend) -> Option<Failure> {
    let conversation =
        domain.conversations.get(Id::from_token(conversation)).expect("provider owner has a live run conversation");
    let run = domain.runs.get(conversation.run).expect("conversation retains its run");
    if run.spent.units.checked_add(price).is_none() {
        return Some(Failure::PriceOverflow);
    }
    if run.spent.turns.checked_add(usage.turns).is_none()
        || run.spent.input.checked_add(usage.input).is_none()
        || run.spent.output.checked_add(usage.output).is_none()
        || run.spent.cache_read.checked_add(usage.cache_read).is_none()
        || run.spent.cache_write.checked_add(usage.cache_write).is_none()
    {
        return Some(Failure::UsageOverflow);
    }
    None
}

/// Session own prices are monotonic activation totals. Stale names are inert;
/// subtree updates do not add global spend (domain/run.md, sections 9 and 14).
pub(crate) fn priced(domain: &mut Domain, conversation: Token, own: u64, subtree: u64, out: &mut Queue<Request>) {
    let id = Id::<Conversation>::from_token(conversation);
    let Some(conversation) = domain.conversations.get_mut(id) else {
        return;
    };
    match conversation.phase {
        Phase::Running { .. } | Phase::Closing => {}
        Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Closed => return,
    }
    let run_id = conversation.run;
    if own < conversation.spent.units || subtree < conversation.subtree_spent {
        return;
    }
    let delta = own.checked_sub(conversation.spent.units).expect("checked monotonic own total");
    let run = domain.runs.get_mut(run_id).expect("conversation retains run");
    let Some(total) = run.spent.accumulate(Spend { units: delta, ..Spend::ZERO }) else {
        let state = mem::replace(&mut run.state, State::Closed);
        run.state = hard_stop(state, &mut domain.conversations, Failure::PriceOverflow, out);
        follow(&mut domain.runs, &mut domain.alarms, run_id);
        return;
    };
    conversation.spent.units = own;
    conversation.subtree_spent = subtree;
    run.spent = total;
    account_state(run);
    follow(&mut domain.runs, &mut domain.alarms, run_id);
}

pub(crate) fn used(domain: &mut Domain, conversation: Token, spend: Spend, out: &mut Queue<Request>) {
    let id = Id::<Conversation>::from_token(conversation);
    let Some(conversation) = domain.conversations.get_mut(id) else {
        return;
    };
    match conversation.phase {
        Phase::Running { .. } | Phase::Closing => {}
        Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Closed => return,
    }
    let run_id = conversation.run;
    let raw = Spend { units: 0, ..spend };
    let run = domain.runs.get_mut(run_id).expect("conversation retains run");
    let added = match (conversation.spent.accumulate(raw), run.spent.accumulate(raw)) {
        (Some(conversation_total), Some(run_total)) => Some((conversation_total, run_total)),
        (Some(_) | None, Some(_) | None) => None,
    };
    let Some((conversation_total, run_total)) = added else {
        let state = mem::replace(&mut run.state, State::Closed);
        run.state = hard_stop(state, &mut domain.conversations, Failure::UsageOverflow, out);
        follow(&mut domain.runs, &mut domain.alarms, run_id);
        return;
    };
    conversation.spent = conversation_total;
    run.spent = run_total;
    account_state(run);
    follow(&mut domain.runs, &mut domain.alarms, run_id);
}

fn account_state(run: &mut Run) {
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Working { reply_to, main } => match run.charter.budget.exhausted(run.spent) {
            Some(exhausted) => State::Over { reply_to, main, exhausted },
            None => State::Working { reply_to, main },
        },
        state @ (State::Preparing { .. }
        | State::Stopping { .. }
        | State::Waiting { .. }
        | State::Over { .. }
        | State::Winding { .. }
        | State::Closed) => state,
    };
}

fn hard_stop(
    state: State,
    conversations: &mut Slab<Conversation>,
    failure: Failure,
    out: &mut Queue<Request>,
) -> State {
    match state {
        State::Working { reply_to, main }
        | State::Over { reply_to, main, .. }
        | State::Waiting { reply_to, main, .. } => {
            wind_down(conversations, reply_to, main, Ending::Failed(failure), out)
        }
        State::Winding { reply_to, ending } => {
            let ending = hard_ending(ending, failure);
            State::Winding { reply_to, ending }
        }
        State::Preparing { .. } | State::Stopping { .. } | State::Closed => {
            unreachable!("actual usage is reported only by live admitted conversations")
        }
    }
}

fn hard_ending(ending: Ending, failure: Failure) -> Ending {
    match failure {
        Failure::PriceOverflow | Failure::UsageOverflow => Ending::Failed(failure),
        Failure::Receiving(_)
        | Failure::Transcript(_)
        | Failure::Model(_)
        | Failure::Budget(_)
        | Failure::Policy(_)
        | Failure::Cancelled
        | Failure::Stale => match ending {
            ending @ Ending::Accepted(Declared::Change(_)) => ending,
            Ending::Accepted(_) | Ending::Parked | Ending::Failed(_) => Ending::Failed(failure),
        },
    }
}

/// Pure completion gate; no leases, prompt copies or external effects.
pub(crate) fn completion_permit(domain: &Domain, conversation: Token) -> crate::CompletionPermit {
    use crate::CompletionPermit;
    let id = Id::<Conversation>::from_token(conversation);
    let Some(conversation) = domain.conversations.get(id) else {
        return CompletionPermit::Closing;
    };
    match conversation.phase {
        Phase::Running { .. } => {}
        Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Closing | Phase::Closed => {
            return CompletionPermit::Closing;
        }
    }
    let run = domain.runs.get(conversation.run).expect("conversation retains run");
    match &run.state {
        State::Working { .. } => match run.charter.budget.exhausted(run.spent) {
            Some(exhausted) => CompletionPermit::Denied(exhausted),
            None => CompletionPermit::Allowed,
        },
        State::Over { exhausted, .. } => CompletionPermit::Denied(*exhausted),
        State::Waiting { .. }
        | State::Winding { .. }
        | State::Preparing { .. }
        | State::Stopping { .. }
        | State::Closed => CompletionPermit::Closing,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the cell receives concrete origin independently of live callback identity"
)]
pub(crate) fn delegated(
    domain: &mut Domain,
    env: &Env<Limits>,
    conversation: Token,
    call: Token,
    name: CallName,
    ask: Ask,
    deadline: Time,
    out: &mut Queue<Request>,
) {
    let Domain { runs, conversations, calls: _, alarms: _, facts } = domain;
    let id = Id::<Conversation>::from_token(conversation);
    let conversation = conversations.get(id).expect("a conversation lives until it has ended");
    let closing = match &conversation.phase {
        Phase::Running { .. } => false,
        Phase::Closing => true,
        Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Closed => {
            unreachable!("a conversation calls only between starting and ending")
        }
    };
    let run_id = conversation.run;
    facts.about(run_id.token());
    let asked = match &ask {
        Ask::Wait => Asked::Wait,
        Ask::Host { .. } => Asked::Host,
        Ask::Deliver { .. } => Asked::Deliver,
        Ask::Finish { .. } => Asked::Finish,
        Ask::SubAgent { .. } => Asked::SubAgent,
    };
    facts.push(Fact::Called { run: run_id.token(), conversation: id.token(), call, ask: asked });
    // The call crossed its conversation's close: it would be withdrawn at once.
    if closing {
        out.push(Request::Return { spent: 0, call, result: Returned::Cancelled });
        return;
    }
    let run = runs.get_mut(run_id).expect("a run lives until its conversations have ended");
    let state = mem::replace(&mut run.state, State::Closed);
    let made = Asking { name, call, deadline };
    let next = match state {
        State::Winding { reply_to, ending } => {
            out.push(Request::Return { spent: 0, call, result: Returned::Cancelled });
            State::Winding { reply_to, ending }
        }
        State::Working { reply_to, main } => {
            let serving = Serving { reply_to, main, conversation: id, over: None, made };
            serve(domain, env, run_id, serving, ask, out)
        }
        State::Over { reply_to, main, exhausted } => {
            let serving = Serving { reply_to, main, conversation: id, over: Some(exhausted), made };
            serve(domain, env, run_id, serving, ask, out)
        }
        State::Preparing { .. } | State::Stopping { .. } | State::Waiting { .. } | State::Closed => {
            unreachable!("a run's conversation calls only while the run works or winds down")
        }
    };
    domain.runs.get_mut(run_id).expect("the served call retains its run").state = next;
    follow(&mut domain.runs, &mut domain.alarms, run_id);
}

/// Current completion-local call context; scalar crossing keeps these calls
/// stable while preventing the next completion (domain/run.md, sections 9 and 14).
struct Serving {
    reply_to: ReplyTo,
    main: Id<Conversation>,
    conversation: Id<Conversation>,
    over: Option<Exhausted>,
    made: Asking,
}

fn serve(
    domain: &mut Domain,
    env: &Env<Limits>,
    run_id: Id<Run>,
    serving: Serving,
    ask: Ask,
    out: &mut Queue<Request>,
) -> State {
    let Domain { runs, conversations, calls, alarms, facts: _ } = domain;
    let run = runs.get_mut(run_id).expect("the served call retains its run");
    let Serving { reply_to, main, conversation, over, made } = serving;
    match ask {
        Ask::Wait => {
            let result = if main == conversation && run.charter.grants.wait {
                run.waiting = true;
                Returned::Waiting
            } else {
                Returned::Refused { refusal: AskRefusal::NotGranted }
            };
            out.push(Request::Return { spent: 0, call: made.call, result });
        }
        Ask::Host { tool, effect, input } => {
            host_call(run, run_id, conversations, calls, alarms, conversation, made, tool, effect, input, env, out);
        }
        Ask::Deliver { change } => {
            assert!(main == conversation, "only main is offered delivery");
            deliver(run, run_id, conversations, calls, alarms, main, made, change, env, out);
        }
        Ask::Finish { outcome } => {
            assert!(main == conversation, "only main is offered finish");
            return finish(run, run_id, conversations, calls, alarms, reply_to, main, over, made, outcome, env, out);
        }
        Ask::SubAgent { brief, families, llm, share } => match over {
            Some(_) => {
                out.push(Request::Return {
                    spent: 0,
                    call: made.call,
                    result: Returned::Refused { refusal: AskRefusal::Over },
                });
            }
            None => {
                let wanted = Wanted { brief, families, llm, share };
                sub_agent(run, run_id, conversations, calls, alarms, conversation, made, wanted, env, out);
            }
        },
    }
    match over {
        None => State::Working { reply_to, main },
        Some(exhausted) => State::Over { reply_to, main, exhausted },
    }
}

pub(crate) fn withdraw(domain: &mut Domain, conversation: Token, call: Token, out: &mut Queue<Request>) {
    let conversation = Id::<Conversation>::from_token(conversation);
    // A withdraw of a call that has returned is stale.
    let Some(id) = domain.calls.find(conversation, call) else {
        return;
    };
    // The call is settling now: its deadline no longer matters.
    domain.alarms.cancel(Alarm::Call { call: id });
    stop_call(domain, id, Withdrawal::Withdrawn, out);
}

/// The deadline of the call `id` passed before it returned: stop what it is
/// doing.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn expired(domain: &mut Domain, id: Id<Call>, out: &mut Queue<Request>) {
    stop_call(domain, id, Withdrawal::Expired, out);
}

pub(crate) fn checked(domain: &mut Domain, env: &Env<Limits>, owner: Token, ran: Ran, out: &mut Queue<Request>) {
    let id = Id::<Call>::from_token(owner);
    let call = domain.calls.get_mut(id).expect("a call lives until it returns");
    let run = domain.runs.get(call.run).expect("a run lives until its calls have returned");
    domain.facts.about(call.run.token());
    domain.facts.push(Fact::CheckFinished { run: call.run.token(), exit: ran.exit });
    let settled = match &mut call.work {
        Work::Landing(landing) => land::checked(landing, id, call.owner, run, may_finish(&run.state), ran, env, out),
        Work::Child(_) | Work::Host(_) => unreachable!("io and the host answer only a landing's requests"),
    };
    settle(domain, id, settled, out);
}

pub(crate) fn aborted(domain: &mut Domain, owner: Token, out: &mut Queue<Request>) {
    let id = Id::<Call>::from_token(owner);
    let call = domain.calls.get_mut(id).expect("a call lives until it returns");
    domain.facts.about(call.run.token());
    let settled = match &mut call.work {
        Work::Landing(landing) => land::aborted(landing, call.owner, out),
        Work::Child(_) | Work::Host(_) => unreachable!("io and the host answer only a landing's requests"),
    };
    settle(domain, id, settled, out);
}

pub(crate) fn delivered(domain: &mut Domain, owner: Token, delivery: Delivery, out: &mut Queue<Request>) {
    let id = Id::<Call>::from_token(owner);
    let Some(call) = domain.calls.get(id) else {
        return;
    };
    if domain.calls.find(call.conversation, call.owner) != Some(id) {
        return;
    }
    let call = domain.calls.get_mut(id).expect("the named call is live");
    let run = domain.runs.get(call.run).expect("run waits for the terminal");
    domain.facts.about(call.run.token());
    domain.facts.push(Fact::Delivered { run: call.run.token(), status: delivery.status() });
    let settled = match &mut call.work {
        Work::Landing(landing) => land::delivered(landing, call.owner, delivery, run, out),
        Work::Child(_) | Work::Host(_) => unreachable!("host answers a delivery call"),
    };
    settle(domain, id, settled, out);
}

pub(crate) fn ended(domain: &mut Domain, conversation: Token, end: End, spend: Spend, out: &mut Queue<Request>) {
    let Domain { runs, conversations, calls, alarms, facts } = domain;
    let id = Id::<Conversation>::from_token(conversation);
    let Some(conversation) = conversations.get_mut(id) else {
        return;
    };
    match conversation.phase {
        Phase::Closed => return,
        Phase::Pending | Phase::Opening | Phase::Unwanted | Phase::Running { .. } | Phase::Closing => {}
    }
    assert!(conversation.calls == 0, "a conversation ends once its calls have returned");
    let phase = mem::replace(&mut conversation.phase, Phase::Closed);
    match phase {
        Phase::Opening | Phase::Unwanted | Phase::Running { .. } | Phase::Closing => {}
        Phase::Pending | Phase::Closed => unreachable!("a conversation ends once, once opened"),
    }
    // Its turns were counted as they were used; whatever its end counts beyond
    // them is counted now.
    let unaccounted = spend.unreported(conversation.spent);
    let (run_id, asker) = (conversation.run, conversation.asker);
    let bill = conversation.subtree_spent;
    facts.about(run_id.token());
    facts.push(Fact::Ended { run: run_id.token(), conversation: id.token(), end });
    conversations.retire(id);
    let run = runs.get_mut(run_id).expect("a run lives until its conversations have ended");
    run.spent = run.spent.accumulate(unaccounted).expect("terminal usage was checked before it was reported");
    let failure = match end {
        End::PriceOverflow => Some(Failure::PriceOverflow),
        End::UsageOverflow => Some(Failure::UsageOverflow),
        End::Receiving(_)
        | End::TranscriptRefused { .. }
        | End::Closed
        | End::Busy
        | End::Invalid
        | End::Fault(_)
        | End::Budget(_) => None,
    };
    if let (Some(failure), Some(_)) = (failure, asker) {
        let state = mem::replace(&mut run.state, State::Closed);
        run.state = hard_stop(state, conversations, failure, out);
    }
    run.conversations = run.conversations.checked_sub(1).expect("a run counts its conversations");
    // A sub-agent's end is its call's return.
    if let Some(asker) = asker {
        let call = calls.get_mut(asker).expect("a sub-agent's call lives until it has ended");
        let result = match &mut call.work {
            Work::Child(child) => agent::ended(child, end),
            Work::Landing(_) | Work::Host(_) => unreachable!("a sub-agent serves a sub-agent's call"),
        };
        out.push(Request::Return { spent: bill, call: call.owner, result });
        retire_call(calls, alarms, conversations, asker);
        return;
    }
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Working { reply_to, main }
        | State::Waiting { reply_to, main, until: _ }
        | State::Over { reply_to, main, exhausted: _ } => {
            assert!(main == id, "a conversation with no asker is main");
            answer(reply_to, ending(end, run.spent, run.turns), out)
        }
        State::Winding { reply_to, ending } => {
            let ending = match failure {
                Some(failure) => Ending::Failed(failure),
                None => ending,
            };
            answer(reply_to, finished(ending, run.spent, run.turns), out)
        }
        State::Preparing { .. } | State::Stopping { .. } | State::Closed => {
            unreachable!("a run's conversation ends only once the run has opened it")
        }
    };
    follow(runs, alarms, run_id);
}

pub(crate) fn deadline(domain: &mut Domain, id: Id<Run>, out: &mut Queue<Request>) {
    let Domain { runs, conversations, calls: _, alarms, facts } = domain;
    let run = runs.get_mut(id).expect("an alarm is cancelled before its run closes");
    facts.about(id.token());
    let failure = Failure::Budget(Exhausted::Time);
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match state {
        State::Preparing { reply_to, main, step: _ } => State::Stopping { reply_to, main, failure },
        State::Working { reply_to, main } | State::Waiting { reply_to, main, until: _ } => {
            wind_down(conversations, reply_to, main, Ending::Failed(failure), out)
        }
        // It was past its budget first.
        State::Over { reply_to, main, exhausted } => {
            wind_down(conversations, reply_to, main, Ending::Failed(Failure::Budget(exhausted)), out)
        }
        State::Stopping { .. } | State::Winding { .. } | State::Closed => {
            unreachable!("the deadline alarm runs only while a run prepares or works")
        }
    };
    follow(runs, alarms, id);
}

/// How deep the conversation `conversation` is, and for main what its run
/// found in its checkout (its guides, and its repositories with checks), for
/// their facts.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub(crate) fn opened(
    runs: &Slab<Run>,
    conversations: &Slab<Conversation>,
    conversation: Token,
) -> (u32, Option<(u32, u32)>) {
    let conversation = conversations.get(Id::from_token(conversation)).expect("a conversation is told of as it opens");
    if conversation.depth > 0 {
        return (conversation.depth, None);
    }
    let run = runs.get(conversation.run).expect("a run outlives its conversations");
    (0, Some((run.found.guides.len(), run.found.checks.len())))
}

/// What a run's state implies, applied after every transition: whether its
/// deadline alarm runs, and whether it is retired.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn follow(runs: &mut Slab<Run>, alarms: &mut Deadlines<Alarm>, id: Id<Run>) {
    let run = runs.get(id).expect("a run lives until it is retired");
    let (deadline, closed) = match &run.state {
        State::Preparing { .. } | State::Working { .. } | State::Waiting { .. } | State::Over { .. } => {
            (Some(run.deadline), false)
        }
        State::Stopping { .. } | State::Winding { .. } => (None, false),
        State::Closed => (None, true),
    };
    let alarm = Alarm::Deadline { run: id };
    if let Some(at) = deadline {
        alarms.arm(alarm, at).expect("the alarm table has room for an alarm per run and per call");
    } else {
        alarms.cancel(alarm);
    }
    let park = Alarm::Park { run: id };
    match run.state {
        State::Waiting { until, .. } => {
            alarms.arm(park, until).expect("one independent idle alarm per run");
        }
        State::Preparing { .. }
        | State::Stopping { .. }
        | State::Working { .. }
        | State::Over { .. }
        | State::Winding { .. }
        | State::Closed => {
            alarms.cancel(park);
        }
    }
    if closed {
        runs.retire(id);
    }
}

/// Whether a run in `state` may still finish: it works, or is over its budget
/// with main's turn in flight.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn may_finish(state: &State) -> bool {
    match state {
        State::Working { .. } | State::Over { .. } => true,
        State::Preparing { .. }
        | State::Stopping { .. }
        | State::Waiting { .. }
        | State::Winding { .. }
        | State::Closed => false,
    }
}

/// The landing of the call `id` has settled, as `settled` says: the run goes
/// on, or finishes.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn settle(domain: &mut Domain, id: Id<Call>, settled: Settled, out: &mut Queue<Request>) {
    let Domain { runs, conversations, calls, alarms, facts: _ } = domain;
    if settled == Settled::Going {
        return;
    }
    let run_id = retire_call(calls, alarms, conversations, id);
    let run = runs.get_mut(run_id).expect("a run lives until its calls have returned");
    let state = mem::replace(&mut run.state, State::Closed);
    run.state = match settled {
        Settled::MidDelivered => match state {
            State::Working { reply_to, main } => State::Working { reply_to, main },
            State::Over { reply_to, main, exhausted } => {
                wind_down(conversations, reply_to, main, Ending::Failed(Failure::Budget(exhausted)), out)
            }
            State::Winding { reply_to, ending } => State::Winding { reply_to, ending },
            State::Preparing { .. } | State::Stopping { .. } | State::Waiting { .. } | State::Closed => {
                unreachable!("delivery starts after main")
            }
        },
        Settled::Finished(change) => match state {
            State::Working { reply_to, main } | State::Over { reply_to, main, exhausted: _ } => {
                wind_down(conversations, reply_to, main, Ending::Accepted(Declared::Change(change)), out)
            }
            // The host landed the checked state, whatever the run was winding down
            // for: main is closing already.
            State::Winding { reply_to, ending: _ } => {
                State::Winding { reply_to, ending: Ending::Accepted(Declared::Change(change)) }
            }
            State::Preparing { .. } | State::Stopping { .. } | State::Waiting { .. } | State::Closed => {
                unreachable!("a run lands a change only once it has opened main, and before it answers")
            }
        },
        Settled::Refused => {
            run.rejected = run.rejected.saturating_add(1);
            match state {
                State::Over { reply_to, main, exhausted } => {
                    wind_down(conversations, reply_to, main, Ending::Failed(Failure::Budget(exhausted)), out)
                }
                state @ (State::Working { .. } | State::Winding { .. }) => state,
                State::Preparing { .. } | State::Stopping { .. } | State::Waiting { .. } | State::Closed => {
                    unreachable!("a run lands a change only once it has opened main, and before it answers")
                }
            }
        }
        Settled::Stale => match state {
            State::Working { reply_to, main } | State::Over { reply_to, main, exhausted: _ } => {
                wind_down(conversations, reply_to, main, Ending::Failed(Failure::Stale), out)
            }
            state @ State::Winding { .. } => state,
            State::Preparing { .. } | State::Stopping { .. } | State::Waiting { .. } | State::Closed => {
                unreachable!("a run lands a change only once it has opened main, and before it answers")
            }
        },
        Settled::Going | Settled::Cancelled => state,
    };
    follow(runs, alarms, run_id);
}

// Cell handlers: each takes the source state's data by value and returns the
// target state.

/// Asks for the look `step`, preparing.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn look(
    run: &Run,
    id: Id<Run>,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    step: Step,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) -> State {
    out.push(prepare::request(&run.charter, run.workspace.as_ref(), step, id.token(), env.now, &env.limits));
    State::Preparing { reply_to, main, step }
}

/// Preparing, the look `step` ended: the next one, or open main.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[expect(clippy::too_many_arguments, reason = "a cell handler takes the fields it touches")]
fn prepared(
    run: &mut Run,
    conversations: &mut Slab<Conversation>,
    id: Id<Run>,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    step: Step,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) -> State {
    match prepare::next(&run.charter, run.workspace.as_ref(), Some(step)) {
        Some(next) => look(run, id, reply_to, main, next, env, out),
        None => open(run, conversations, reply_to, main, env.now, out),
    }
}

/// Prepared: open main with what the run found, and the whole budget.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn open(
    run: &mut Run,
    conversations: &mut Slab<Conversation>,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    now: Time,
    out: &mut Queue<Request>,
) -> State {
    let conversation = conversations.get_mut(main).expect("main lives while its run prepares");
    let phase = mem::replace(&mut conversation.phase, Phase::Closed);
    conversation.phase = match phase {
        Phase::Pending => Phase::Opening,
        Phase::Opening | Phase::Unwanted | Phase::Running { .. } | Phase::Closing | Phase::Closed => {
            unreachable!("main is opened once, when its run has prepared")
        }
    };
    let mut opening = opening(
        &run.charter,
        run.activation,
        run.workspace.as_ref(),
        &run.found,
        run.spent,
        run.deadline.saturating_since(now),
    );
    opening.transcript = run.transcript.take();
    out.push(Request::Open { conversation: main.token(), opening });
    State::Working { reply_to, main }
}

/// Stopping, the look in flight ended: main was never opened, and the run
/// answers.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn stop(
    run: &Run,
    conversations: &mut Slab<Conversation>,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    failure: Failure,
    out: &mut Queue<Request>,
) -> State {
    let conversation = conversations.get_mut(main).expect("main lives while its run prepares");
    let phase = mem::replace(&mut conversation.phase, Phase::Closed);
    match phase {
        Phase::Pending => {}
        Phase::Opening | Phase::Unwanted | Phase::Running { .. } | Phase::Closing | Phase::Closed => {
            unreachable!("main is not opened while its run prepares")
        }
    }
    conversations.retire(main);
    answer(reply_to, Answer::Failed { failure, spent: run.spent, turns: run.turns }, out)
}

/// Working, or over the budget's `over` part: main called `finish` as
/// `made`. A refused outcome is returned at once; an accepted verdict ends the
/// run; a change lands, by the call's deadline, and the run goes on meanwhile.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[expect(clippy::too_many_arguments, reason = "a cell handler takes the fields it touches")]
fn finish(
    run: &mut Run,
    run_id: Id<Run>,
    conversations: &mut Slab<Conversation>,
    calls: &mut Calls,
    alarms: &mut Deadlines<Alarm>,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    over: Option<Exhausted>,
    made: Asking,
    declared: Declared,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) -> State {
    let Asking { name, call, deadline } = made;
    let conversation = conversations.get(main).expect("main lives while its run works");
    assert!(conversation.calls == 0, "a finish is a write, which a conversation runs alone");
    let max = env.limits.outcome_bytes;
    let judged = match outcome::owned_bytes(&declared) {
        Some(cost) if cost <= max => outcome::judge(&run.charter.outcome, &declared),
        Some(_) | None => Err(outcome::too_large(max)),
    };
    if let Err(problems) = judged {
        out.push(Request::Return { spent: 0, call, result: Returned::Rejected { problems } });
        run.rejected = run.rejected.saturating_add(1);
        return match over {
            None => State::Working { reply_to, main },
            Some(exhausted) => {
                wind_down(conversations, reply_to, main, Ending::Failed(Failure::Budget(exhausted)), out)
            }
        };
    }
    match declared {
        completed @ (Declared::Verdict(_) | Declared::Report(_) | Declared::Failure(_)) => {
            out.push(Request::Return { spent: 0, call, result: Returned::Accepted });
            wind_down(conversations, reply_to, main, Ending::Accepted(completed), out)
        }
        Declared::Change(change) => {
            if name.activation != run.activation || name.completion == 0 {
                out.push(Request::Return { spent: 0, call, result: Returned::Refused { refusal: AskRefusal::Name } });
                return match over {
                    None => State::Working { reply_to, main },
                    Some(exhausted) => State::Over { reply_to, main, exhausted },
                };
            }
            if deadline <= env.now {
                out.push(Request::Return { spent: 0, call, result: Returned::TimedOut });
                return match over {
                    None => State::Working { reply_to, main },
                    Some(exhausted) => State::Over { reply_to, main, exhausted },
                };
            }
            if calls.is_full() {
                out.push(Request::Return { spent: 0, call, result: Returned::Busy });
                return match over {
                    None => State::Working { reply_to, main },
                    Some(exhausted) => State::Over { reply_to, main, exhausted },
                };
            }
            let work = Work::Landing(land::landing(change, name, deadline, true));
            let id = begin_call(calls, alarms, Call { run: run_id, conversation: main, owner: call, work }, deadline);
            match &mut calls.get_mut(id).expect("inserted above").work {
                Work::Landing(landing) => land::begin(landing, id, run, env, out),
                Work::Child(_) | Work::Host(_) => unreachable!("inserted as a landing"),
            }
            let conversation = conversations.get_mut(main).expect("main lives while its run works");
            conversation.calls = conversation.calls.saturating_add(1);
            match over {
                None => State::Working { reply_to, main },
                Some(exhausted) => State::Over { reply_to, main, exhausted },
            }
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "exclusive main delivery handler takes its owned bindings")]
fn deliver(
    run: &mut Run,
    run_id: Id<Run>,
    conversations: &mut Slab<Conversation>,
    calls: &mut Calls,
    alarms: &mut Deadlines<Alarm>,
    main: Id<Conversation>,
    made: Asking,
    change: Change,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) {
    let Asking { name, call, deadline } = made;
    assert!(conversations.get(main).expect("main lives").calls == 0, "delivery is an exclusive write");
    let Some(spec) = &run.charter.grants.deliver else {
        out.push(Request::Return { spent: 0, call, result: Returned::Refused { refusal: AskRefusal::NotGranted } });
        return;
    };
    let judged = match outcome::change_bytes(&change) {
        Some(bytes) if bytes <= env.limits.outcome_bytes => outcome::judge_change(spec, &change),
        Some(_) | None => Err(outcome::too_large(env.limits.outcome_bytes)),
    };
    if let Err(problems) = judged {
        run.rejected = run.rejected.saturating_add(1);
        out.push(Request::Return { spent: 0, call, result: Returned::Rejected { problems } });
        return;
    }
    if name.activation != run.activation || name.completion == 0 {
        out.push(Request::Return { spent: 0, call, result: Returned::Refused { refusal: AskRefusal::Name } });
        return;
    }
    if deadline <= env.now {
        out.push(Request::Return { spent: 0, call, result: Returned::TimedOut });
        return;
    }
    if calls.is_full() {
        out.push(Request::Return { spent: 0, call, result: Returned::Busy });
        return;
    }
    let work = Work::Landing(land::landing(change, name, deadline, false));
    let id = begin_call(calls, alarms, Call { run: run_id, conversation: main, owner: call, work }, deadline);
    match &mut calls.get_mut(id).expect("inserted above").work {
        Work::Landing(landing) => land::begin(landing, id, run, env, out),
        Work::Child(_) | Work::Host(_) => unreachable!("inserted as delivery"),
    }
    let main = conversations.get_mut(main).expect("main lives");
    main.calls = main.calls.checked_add(1).expect("bounded exclusive call");
}

/// A call a conversation made: its token for it, and its deadline.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
struct Asking {
    name: CallName,
    call: Token,
    deadline: Time,
}

/// What a conversation asks for when it asks for a sub-agent.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
struct Wanted {
    brief: Box<[u8]>,
    families: Families,
    llm: Option<Box<[u8]>>,
    share: Option<crate::Share>,
}

/// Working: the conversation `asker` asked for a sub-agent as `made`. Open
/// it, with no more time than to the call's deadline, or return why not.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[expect(clippy::too_many_arguments, reason = "a cell handler takes the fields it touches")]
fn sub_agent(
    run: &mut Run,
    run_id: Id<Run>,
    conversations: &mut Slab<Conversation>,
    calls: &mut Calls,
    alarms: &mut Deadlines<Alarm>,
    asker: Id<Conversation>,
    made: Asking,
    wanted: Wanted,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) {
    let Asking { call, deadline, name: _ } = made;
    let asking = conversations.get(asker).expect("a conversation lives until it has ended");
    let left = run.deadline.min(deadline).saturating_since(env.now);
    let means = Means { spent: run.spent, conversations: run.conversations, left };
    let planned = agent::plan(
        &run.charter,
        means,
        asking.families,
        asking.depth,
        wanted.families,
        wanted.llm.as_deref(),
        wanted.share,
        &env.limits,
    );
    let plan = match planned {
        Ok(plan) => plan,
        Err(refusal) => {
            out.push(Request::Return { spent: 0, call, result: Returned::Refused { refusal } });
            return;
        }
    };
    if conversations.is_full() || calls.is_full() {
        out.push(Request::Return { spent: 0, call, result: Returned::Busy });
        return;
    }
    // A call is stored before its child, which names it, and holds the child
    // once the child has a name too.
    let unnamed = Call { run: run_id, conversation: asker, owner: call, work: Work::Child(Child::Closed) };
    let id = begin_call(calls, alarms, unnamed, deadline);
    let child = Conversation {
        run: run_id,
        asker: Some(id),
        families: plan.families,
        depth: plan.depth,
        spent: Spend::ZERO,
        subtree_spent: 0,
        calls: 0,
        phase: Phase::Opening,
    };
    let child = conversations.insert(child).expect("checked for room above");
    calls.get_mut(id).expect("inserted above").work = Work::Child(agent::working(child));
    let asking = conversations.get_mut(asker).expect("a conversation lives until it has ended");
    asking.calls = asking.calls.saturating_add(1);
    run.conversations = run.conversations.saturating_add(1);
    let opening = Opening {
        activation: run.activation,
        transcript: None,
        wait: false,
        host_tools: Box::new([]),
        deliver: false,
        system: prompt::child(&run.charter, run.workspace.as_ref(), &run.found, &wanted.brief, plan.families),
        prompt: copy_of(prompt::BEGIN),
        tools: plan.families.tools,
        workspace: run.workspace.clone(),
        budget: plan.budget,
        finish: false,
        families: plan.families,
        llm: plan.llm,
    };
    out.push(Request::Open { conversation: child.token(), opening });
}

/// Stores `call`, there being room, and arms its deadline.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn begin_call(calls: &mut Calls, alarms: &mut Deadlines<Alarm>, call: Call, deadline: Time) -> Id<Call> {
    let id = calls.insert(call);
    alarms.arm(Alarm::Call { call: id }, deadline).expect("the alarm table has room for an alarm per call");
    id
}

/// Stops what the call `id` is doing, for `why`; it returns once that has
/// settled.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn stop_call(domain: &mut Domain, id: Id<Call>, why: Withdrawal, out: &mut Queue<Request>) {
    let host = match &domain.calls.get(id).expect("live call").work {
        Work::Host(_) => true,
        Work::Child(_) | Work::Landing(_) => false,
    };
    if host {
        host_stop(domain, id, why, out);
        return;
    }
    let call = domain.calls.get_mut(id).expect("a call lives until it returns, and its alarm with it");
    domain.facts.about(call.run.token());
    match &mut call.work {
        Work::Host(_) => unreachable!("host stop dispatched above"),
        Work::Landing(landing) => land::withdraw(landing, id, why, out),
        Work::Child(child) => {
            if let Some(child) = agent::withdraw(child, why) {
                close(&mut domain.conversations, child, out);
            }
        }
    }
}

/// Retires the call `id`, which has returned: its alarm is cancelled, and its
/// conversation has a call fewer in flight. The run it is of.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn retire_call(
    calls: &mut Calls,
    alarms: &mut Deadlines<Alarm>,
    conversations: &mut Slab<Conversation>,
    id: Id<Call>,
) -> Id<Run> {
    let call = calls.get(id).expect("a call lives until it returns");
    let (run, conversation) = (call.run, call.conversation);
    calls.retire(id);
    alarms.cancel(Alarm::Call { call: id });
    alarms.cancel(Alarm::Host { call: id });
    let conversation = conversations.get_mut(conversation).expect("a conversation outlives its calls");
    conversation.calls = conversation.calls.checked_sub(1).expect("a conversation counts its calls");
    run
}

/// Working, an ending decided: close main, and wait for it to end.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn wind_down(
    conversations: &mut Slab<Conversation>,
    reply_to: ReplyTo,
    main: Id<Conversation>,
    ending: Ending,
    out: &mut Queue<Request>,
) -> State {
    close(conversations, main, out);
    State::Winding { reply_to, ending }
}

/// Closes the conversation `id`, which its owner closes once: at once, or
/// once it starts. A closing conversation withdraws its own calls.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn close(conversations: &mut Slab<Conversation>, id: Id<Conversation>, out: &mut Queue<Request>) {
    let conversation = conversations.get_mut(id).expect("a conversation lives until it has ended");
    let phase = mem::replace(&mut conversation.phase, Phase::Closed);
    conversation.phase = match phase {
        // It is closed once it starts, or ends refused.
        Phase::Opening => Phase::Unwanted,
        Phase::Running { peer } => closing(peer, out),
        Phase::Pending | Phase::Unwanted | Phase::Closing | Phase::Closed => {
            unreachable!("a conversation is closed once it is opened, once")
        }
    };
}

/// Working, main yielded and may be nudged: tell it to carry on.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn say(reply_to: ReplyTo, main: Id<Conversation>, peer: Token, text: Box<[u8]>, out: &mut Queue<Request>) -> State {
    out.push(Request::Say { peer, text });
    State::Working { reply_to, main }
}

fn closing(peer: Token, out: &mut Queue<Request>) -> Phase {
    out.push(Request::Close { peer });
    Phase::Closing
}

/// Answers the host: the run is done.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn answer(reply_to: ReplyTo, answer: Answer, out: &mut Queue<Request>) -> State {
    out.push(Request::Answer { to: reply_to, answer });
    State::Closed
}

// Helpers.

/// The opening of a run's main conversation, given what the run found in its
/// checkout, what it has spent and the time it has left. What it holds is
/// rendered from the charter or copied: the run keeps the charter (copy at
/// emission).
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn opening(
    charter: &Charter,
    activation: u64,
    workspace: Option<&crate::Workspace>,
    found: &Found,
    spent: Spend,
    left: Duration,
) -> Opening {
    Opening {
        activation,
        transcript: None,
        wait: charter.grants.wait,
        host_tools: charter.grants.host_tools.clone(),
        deliver: charter.grants.deliver.is_some(),
        llm: charter.llm.clone(),
        system: prompt::system(charter, workspace, found),
        prompt: copy_of(prompt::BEGIN),
        tools: crate::workspace::families(workspace, Families::of(&charter.grants)).tools,
        workspace: workspace.cloned(),
        budget: charter.budget.remainder(spent, left),
        finish: true,
        families: crate::workspace::families(workspace, Families::of(&charter.grants)),
    }
}

/// The answer of a run whose main conversation ended on its own, the run
/// having spent `spent`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn ending(end: End, spent: Spend, turns: u32) -> Answer {
    match end {
        End::PriceOverflow => Answer::Failed { failure: Failure::PriceOverflow, spent, turns },
        End::UsageOverflow => Answer::Failed { failure: Failure::UsageOverflow, spent, turns },
        End::Receiving(limit) => Answer::Failed { failure: Failure::Receiving(limit), spent, turns },
        End::TranscriptRefused { reason } => Answer::Failed { failure: Failure::Transcript(reason), spent, turns },
        End::Busy => Answer::Refused(Refusal::Busy),
        End::Invalid => Answer::Refused(Refusal::Invalid(Invalid::Conversation)),
        End::Fault(fault) => Answer::Failed { failure: Failure::Model(fault), spent, turns },
        End::Budget(exhausted) => Answer::Failed { failure: Failure::Budget(exhausted), spent, turns },
        End::Closed => unreachable!("a conversation ends closed only once its run has closed it, winding down"),
    }
}

/// The answer of a run that wound down to `ending`, having spent `spent`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn finished(ending: Ending, spent: Spend, turns: u32) -> Answer {
    match ending {
        Ending::Parked => Answer::Parked { spent, turns },
        Ending::Accepted(outcome) => Answer::Accepted { outcome, spent, turns },
        Ending::Failed(failure) => Answer::Failed { failure, spent, turns },
    }
}

/// Counts a nudge for a run whose LLM stopped without finishing for `stop`,
/// or says how the run fails instead: when its nudges are used up, or no turn
/// is left in its budget for the LLM to carry on with.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn nudge(run: &mut Run, stop: Stop, limits: &Limits) -> Result<(), Failure> {
    if run.nudges >= limits.nudges {
        return Err(unfinished(stop, run.nudges, run.rejected));
    }
    // Room to start a turn, as a conversation's ceilings have it: some
    // turns, input and output left.
    let spent = run.spent;
    let budget = &run.charter.budget;
    if spent.turns >= budget.turns {
        return Err(Failure::Budget(Exhausted::Turns));
    }
    if spent.units >= budget.spend {
        return Err(Failure::Budget(Exhausted::Spend));
    }
    run.nudges = run.nudges.saturating_add(1);
    Ok(())
}

/// How a run fails when its LLM stops without finishing, through `nudges`
/// nudges and `rejected` refused outcomes: as unfinished, or with the fault
/// its last stop shows.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
fn unfinished(stop: Stop, nudges: u32, rejected: u32) -> Failure {
    match stop {
        Stop::EndTurn => Failure::Policy(Policy::Unfinished { nudges, rejected }),
        Stop::MaxTokens => Failure::Model(Fault::Truncated),
        Stop::Refusal => Failure::Model(Fault::Refused),
        Stop::NoCalls => Failure::Model(Fault::Malformed),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "host admission checks the owning conversation and immutable call fields"
)]
fn host_call(
    run: &Run,
    run_id: Id<Run>,
    conversations: &mut Slab<Conversation>,
    calls: &mut Calls,
    alarms: &mut Deadlines<Alarm>,
    conversation: Id<Conversation>,
    made: Asking,
    tool: Box<[u8]>,
    effect: crate::HostEffect,
    input: crate::HostInput,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) {
    let Asking { name, call, deadline } = made;
    if conversations.get(conversation).expect("caller lives").asker.is_some() {
        out.push(Request::Return { spent: 0, call, result: Returned::HostRejected(crate::HostProblem::Undeclared) });
        return;
    }
    let mut timeout = None;
    for declaration in &run.charter.grants.host_tools {
        if declaration.name == tool {
            if declaration.effect != effect {
                out.push(Request::Return {
                    spent: 0,
                    call,
                    result: Returned::HostRejected(crate::HostProblem::Effect),
                });
                return;
            }
            timeout = Some(declaration.timeout.min(env.limits.host_timeout));
        }
    }
    let Some(timeout) = timeout else {
        out.push(Request::Return { spent: 0, call, result: Returned::HostRejected(crate::HostProblem::Undeclared) });
        return;
    };
    if input.bytes().len() > usize::try_from(env.limits.host_input_bytes).expect("byte cap fits") {
        out.push(Request::Return { spent: 0, call, result: Returned::HostRejected(crate::HostProblem::TooLarge) });
        return;
    }
    if name.activation != run.activation || name.completion == 0 {
        out.push(Request::Return { spent: 0, call, result: Returned::Refused { refusal: AskRefusal::Name } });
        return;
    }
    if env.now >= deadline || env.now >= run.deadline {
        out.push(Request::Return { spent: 0, call, result: Returned::TimedOut });
        return;
    }
    if calls.is_full() {
        out.push(Request::Return { spent: 0, call, result: Returned::Busy });
        return;
    }
    let relay = crate::host::Relay {
        name,
        tool,
        effect,
        input,
        timeout,
        caller_deadline: deadline.min(run.deadline),
        stopped: None,
        unknown: false,
        stage: crate::host::Stage::Closed,
    };
    let work = Work::Host(relay);
    let id = begin_call(calls, alarms, Call { run: run_id, conversation, owner: call, work }, deadline);
    match &mut calls.get_mut(id).expect("inserted above").work {
        Work::Host(relay) => host_send(relay, id, run.host_name, 1, env.now, out),
        Work::Child(_) | Work::Landing(_) => unreachable!("inserted as host relay"),
    }
    let caller = conversations.get_mut(conversation).expect("caller lives");
    caller.calls = caller.calls.checked_add(1).expect("bounded calls");
    host_follow(calls, alarms, id);
}

fn host_send(
    relay: &mut crate::host::Relay,
    id: Id<Call>,
    host_run: Token,
    attempt: u32,
    now: Time,
    out: &mut Queue<Request>,
) {
    let deadline = now.saturating_add(relay.timeout).min(relay.caller_deadline);
    assert!(now < deadline, "admission and recovery refuse expired relays");
    out.push(Request::HostCall {
        host_run,
        relay: crate::RelayName { owner: id.token(), attempt },
        name: relay.name,
        tool: relay.tool.clone(),
        effect: relay.effect,
        input: relay.input.clone(),
        deadline,
    });
    relay.stage = crate::host::Stage::Sending { attempt, deadline };
}

fn host_follow(calls: &Calls, alarms: &mut Deadlines<Alarm>, id: Id<Call>) {
    let relay = match &calls.get(id).expect("host call lives").work {
        Work::Host(relay) => relay,
        Work::Child(_) | Work::Landing(_) => unreachable!("host handler owns its call"),
    };
    let at = match relay.stage {
        crate::host::Stage::Sending { deadline, .. } => Some(deadline),
        crate::host::Stage::Backoff { at, .. } => Some(at),
        crate::host::Stage::Withdrawing { .. } | crate::host::Stage::Closed => None,
    };
    if let Some(at) = at {
        alarms.arm(Alarm::Host { call: id }, at).expect("two alarm slots per call");
    } else {
        alarms.cancel(Alarm::Host { call: id });
    }
}

fn host_return(domain: &mut Domain, id: Id<Call>, result: Returned, out: &mut Queue<Request>) {
    let call = domain.calls.get(id).expect("host call lives");
    domain.facts.about(call.run.token());
    let owner = call.owner;
    out.push(Request::Return { spent: 0, call: owner, result });
    retire_call(&mut domain.calls, &mut domain.alarms, &mut domain.conversations, id);
}

fn host_stop(domain: &mut Domain, id: Id<Call>, why: Withdrawal, out: &mut Queue<Request>) {
    let call = domain.calls.get_mut(id).expect("caller lives until relay settles");
    let relay = match &mut call.work {
        Work::Host(relay) => relay,
        Work::Child(_) | Work::Landing(_) => unreachable!("host stop dispatched by work kind"),
    };
    relay.stopped = relay.stopped.or(Some(why));
    let stage = mem::replace(&mut relay.stage, crate::host::Stage::Closed);
    let result = match stage {
        crate::host::Stage::Sending { attempt, .. } => {
            out.push(Request::WithdrawHost { relay: crate::RelayName { owner: id.token(), attempt } });
            relay.stage = crate::host::Stage::Withdrawing { attempt };
            None
        }
        crate::host::Stage::Withdrawing { attempt } => {
            relay.stage = crate::host::Stage::Withdrawing { attempt };
            None
        }
        crate::host::Stage::Backoff { .. } => {
            Some(if relay.unknown { Returned::HostUnknown } else { crate::call::stopped(why) })
        }
        crate::host::Stage::Closed => unreachable!("only a live call stops"),
    };
    if let Some(result) = result {
        host_return(domain, id, result, out);
    } else {
        host_follow(&domain.calls, &mut domain.alarms, id);
    }
}

pub(crate) fn host_alarm(domain: &mut Domain, env: &Env<Limits>, id: Id<Call>, out: &mut Queue<Request>) {
    let call = domain.calls.get_mut(id).expect("alarm belongs to live host call");
    let run = domain.runs.get(call.run).expect("run waits for calls");
    let active = host_active(&run.state);
    let relay = match &mut call.work {
        Work::Host(relay) => relay,
        Work::Child(_) | Work::Landing(_) => unreachable!("host alarm belongs to host work"),
    };
    let stage = mem::replace(&mut relay.stage, crate::host::Stage::Closed);
    let result = match stage {
        crate::host::Stage::Sending { attempt, .. } => {
            out.push(Request::WithdrawHost { relay: crate::RelayName { owner: id.token(), attempt } });
            relay.stage = crate::host::Stage::Withdrawing { attempt };
            None
        }
        crate::host::Stage::Backoff { attempt, .. } => {
            if active && relay.stopped.is_none() && env.now < relay.caller_deadline {
                host_send(relay, id, run.host_name, attempt, env.now, out);
                None
            } else {
                Some(if relay.unknown { Returned::HostUnknown } else { Returned::Busy })
            }
        }
        crate::host::Stage::Withdrawing { .. } | crate::host::Stage::Closed => unreachable!("non-timed host stage"),
    };
    if let Some(result) = result {
        host_return(domain, id, result, out);
    } else {
        host_follow(&domain.calls, &mut domain.alarms, id);
    }
}

fn host_active(state: &State) -> bool {
    match state {
        State::Working { .. } => true,
        State::Preparing { .. }
        | State::Stopping { .. }
        | State::Waiting { .. }
        | State::Over { .. }
        | State::Winding { .. }
        | State::Closed => false,
    }
}

pub(crate) fn host_returned(
    domain: &mut Domain,
    env: &Env<Limits>,
    name: crate::RelayName,
    reply: crate::HostReply,
    out: &mut Queue<Request>,
) {
    let id = Id::<Call>::from_token(name.owner);
    let Some(call) = domain.calls.get(id) else {
        return;
    };
    if domain.calls.find(call.conversation, call.owner) != Some(id) {
        return;
    }
    let relay = match &call.work {
        Work::Host(relay) => relay,
        Work::Child(_) | Work::Landing(_) => return,
    };
    let expected = match relay.stage {
        crate::host::Stage::Sending { attempt, .. } | crate::host::Stage::Withdrawing { attempt } => Some(attempt),
        crate::host::Stage::Backoff { .. } | crate::host::Stage::Closed => None,
    };
    if expected != Some(name.attempt) {
        return;
    }
    let call = domain.calls.get_mut(id).expect("named call is live");
    let run = domain.runs.get(call.run).expect("run waits for relay terminal");
    let active = host_active(&run.state);
    let relay = match &mut call.work {
        Work::Host(relay) => relay,
        Work::Child(_) | Work::Landing(_) => unreachable!("verified host work"),
    };
    relay.stage = crate::host::Stage::Closed;
    let result = match reply {
        crate::HostReply::Answered(answer) => {
            Some(if answer.text().len() <= usize::try_from(env.limits.host_reply_bytes).expect("cap fits") {
                Returned::HostAnswered(answer)
            } else {
                Returned::HostUnknown
            })
        }
        crate::HostReply::Busy => {
            let exhausted = if relay.unknown { Returned::HostUnknown } else { Returned::Busy };
            host_recover(relay, name.attempt, active, env, exhausted)
        }
        crate::HostReply::Unanswered(_) => {
            relay.unknown = true;
            host_recover(relay, name.attempt, active, env, Returned::HostUnknown)
        }
    };
    if let Some(result) = result {
        host_return(domain, id, result, out);
    } else {
        host_follow(&domain.calls, &mut domain.alarms, id);
    }
}

fn host_recover(
    relay: &mut crate::host::Relay,
    attempt: u32,
    active: bool,
    env: &Env<Limits>,
    exhausted: Returned,
) -> Option<Returned> {
    let at = env.now.saturating_add(env.limits.host_backoff);
    if !active || relay.stopped.is_some() || attempt >= env.limits.host_attempts || at >= relay.caller_deadline {
        return Some(exhausted);
    }
    relay.stage = crate::host::Stage::Backoff { attempt: attempt.checked_add(1).expect("bounded attempts"), at };
    None
}
