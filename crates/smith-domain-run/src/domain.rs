//! The run child domain's state and its entry points (programming-model.md, sections 4.5 and 6.3).

use skein_lib::{Deadlines, Env, Queue, Slab, Time};

use crate::boundary::{Event, Request};
use crate::call::Calls;
use crate::facts::{self, Fact, Facts};
use crate::limits::Limits;
use crate::run::{self, Alarm, Conversation, Run};

/// The most requests an entry point emits per call: an admitted start names
/// the run and opens its main conversation or asks io for its first look; a
/// check goes with its notice to the worker; a call returns as main is
/// closed. The parent reserves this much room in `out` before calling it.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub const MAX_OUT: u32 = 2;

/// The run child domain's state.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Debug)]
pub struct Domain {
    pub(crate) runs: Slab<Run>,
    pub(crate) conversations: Slab<Conversation>,
    /// Calls of conversations to the run.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) calls: Calls,
    pub(crate) alarms: Deadlines<Alarm>,
    pub(crate) facts: Facts,
}

impl Domain {
    /// A domain with room for `limits`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn new(limits: &Limits) -> Domain {
        Domain {
            runs: Slab::with_capacity(limits.runs),
            conversations: Slab::with_capacity(limits.conversations),
            calls: Calls::with_capacity(limits.calls),
            alarms: Deadlines::with_capacity(limits.runs.saturating_add(limits.calls.saturating_mul(2))),
            facts: Facts::with_capacity(limits.facts),
        }
    }

    /// Runs present, closed ones included until they are reclaimed.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn runs(&self) -> u32 {
        self.runs.len()
    }

    /// Conversations present, ended ones included until they are reclaimed.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn conversations(&self) -> u32 {
        self.conversations.len()
    }

    /// When the earliest alarm falls due.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        self.alarms.next()
    }

    /// Whether an alarm is due at `now`. While one is, the loop fires the
    /// root domain, which calls [`fire`].
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn is_due(&self, now: Time) -> bool {
        match self.alarms.next() {
            Some(at) => at <= now,
            None => false,
        }
    }

    /// Calls of conversations to the run, returned ones included until they
    /// are reclaimed.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn calls(&self) -> u32 {
        self.calls.len()
    }

    /// The oldest fact not yet drained. The parent drains them at its own
    /// pace; what does not fit meanwhile is dropped and counted.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub fn pop_fact(&mut self) -> Option<Fact> {
        self.facts.pop()
    }

    /// How many facts were dropped for want of room, since the domain was
    /// made.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub fn facts_lost(&self) -> u64 {
        self.facts.lost()
    }

    /// The reclaim point: frees what closed in this iteration.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub fn reclaim(&mut self) {
        self.runs.reclaim();
        self.conversations.reclaim();
        self.calls.reclaim();
    }
}

/// Handles one event, emitting at most [`MAX_OUT`] requests.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub fn step(domain: &mut Domain, env: &Env<Limits>, event: Event, out: &mut Queue<Request>) {
    domain.facts.begin();
    let mark = out.len();
    take(domain, env, event, out);
    facts::tell(&mut domain.facts, &domain.runs, &domain.conversations, out, mark);
}

fn take(domain: &mut Domain, env: &Env<Limits>, event: Event, out: &mut Queue<Request>) {
    match event {
        Event::HostReturned { relay, reply } => run::host_returned(domain, env, relay, reply, out),
        Event::Start { reply_to, worker, charter } => run::start(domain, env, reply_to, worker, charter, out),
        Event::Cancel { run } => run::cancel(domain, run, out),
        Event::Started { conversation, peer } => run::started(domain, conversation, peer, out),
        Event::Yielded { conversation, stop, text } => run::yielded(domain, env, conversation, stop, &text, out),
        Event::Used { conversation, spend } => run::used(domain, conversation, spend, out),
        Event::Ended { conversation, end, spend } => run::ended(domain, conversation, end, spend, out),
        Event::Read { owner, read } => run::read(domain, env, owner, read, out),
        Event::Probed { owner, executable } => run::probed(domain, env, owner, executable, out),
        Event::Delegated { conversation, call, name, ask, deadline } => {
            run::delegated(domain, env, conversation, call, name, ask, deadline, out);
        }
        Event::Withdraw { conversation, call } => run::withdraw(domain, conversation, call, out),
        Event::Checked { owner, ran } => run::checked(domain, env, owner, ran, out),
        Event::Aborted { owner } => run::aborted(domain, owner, out),
        Event::Delivered { owner, push } => run::delivered(domain, owner, push, out),
    }
}

/// Fires the earliest alarm due at `env.now`, if there is one, emitting at most
/// [`MAX_OUT`] requests. A stage fires its alarms after its input events, so
/// progress that arrived in the same iteration wins over a deadline that passed
/// while the loop waited.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
pub fn fire(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    let Some(alarm) = domain.alarms.expire(env.now) else {
        return;
    };
    domain.facts.begin();
    let mark = out.len();
    match alarm {
        Alarm::Deadline { run } => run::deadline(domain, run, out),
        Alarm::Call { call } => run::expired(domain, call, out),
        Alarm::Host { call } => run::host_alarm(domain, env, call, out),
    }
    facts::tell(&mut domain.facts, &domain.runs, &domain.conversations, out, mark);
}
