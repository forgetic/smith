//! The fake provider's state and its entry points.
//!
//! Contract: domain/session.md, sections 4 and 12; programming-model.md, sections 4.4 and 6.3.

use alloc::boxed::Box;
use core::mem;

use skein_lib::{Deadlines, Duration, Env, Id, Queue, ReplyTo, Rng, Slab, Time};

use crate::api::{Answer, Error, Query, Script};
use crate::{limits, respond};

/// The most requests an entry point emits per call.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const MAX_OUT: u32 = 1;

/// How the fake behaves, handed to every step read-only.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Config {
    /// Calls held at once. A call beyond them is refused as overloaded.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub calls: u32,
    /// Owned query bytes, including message and part arrays.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub query_bytes: u32,
    /// Owned script bytes, including their arrays, held at startup.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub script_bytes: u32,
    /// Owned bytes in each answer held until its timer fires.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub answer_bytes: u32,
    /// The time to answer is drawn from `latency_min..=latency_max`.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub latency_min: Duration,
    /// Inclusive upper bound for the fake's seeded response latency.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub latency_max: Duration,
    /// The chance, per mille, that a call fails as overloaded.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub overloaded: u32,
    /// The chance, per mille, that a call fails as rate-limited.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub rate_limited: u32,
    /// What a rate-limit failure asks the client to wait.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub retry_after: Duration,
    /// The chances, per mille, that a call fails as unavailable, as too long
    /// for the context window, or as unauthorised.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub unavailable: u32,
    /// Chance per mille that the fake refuses the context window.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub too_long: u32,
    /// Chance per mille that the fake refuses credentials.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub unauthorized: u32,
    /// The chance, per mille, that an answer is refused by the content filter.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub refused: u32,
    /// The chance, per mille, that an answer says it calls tools and calls
    /// none.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub no_calls: u32,
    /// The most tokens a final answer takes: each takes between one and this
    /// many, and is cut short at the query's `max_tokens`.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub answer_tokens: u32,
    /// The most tool calls an answer makes: each that makes some makes
    /// between one and this many.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub calls_per_answer: u32,
    /// The chance, per mille, that a tool call is malformed: it names a tool
    /// that was not offered, or its arguments are not an object or lack the
    /// path.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub malformed: u32,
    /// Rounds of tool calls after each of the client's messages before the
    /// fake answers it.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub tool_rounds: u32,
}

/// protocol -> domain
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(PartialEq, Eq, Debug)]
pub enum Event {
    /// A call: answer `query`.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Call {
        /// Single-use right to answer this call; exactly one terminal consumes it.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        reply_to: ReplyTo,
        /// Owned bounded neutral provider request, independent of agent vocabulary.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        query: Query,
    },
}

/// domain -> protocol
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// The answer to a `Call`: exactly one per call.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Reply {
        /// Single-use reply right returned to the layer that issued it.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        to: ReplyTo,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        result: Result<Answer, Error>,
    },
}

/// The fake provider's state.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Debug)]
pub struct Domain {
    calls: Slab<Call>,
    /// When each call is answered.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    timers: Deadlines<Id<Call>>,
    rng: Rng,
    /// Tool call ids issued.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    minted: u64,
    /// The conversations it plays from a script.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    scripts: Box<[Script]>,
}

/// A call being answered.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Debug)]
pub(crate) struct Call {
    state: State,
}

#[derive(Debug)]
enum State {
    /// The answer is decided, and goes out when the call's timer fires.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Thinking { reply_to: ReplyTo, result: Result<Answer, Error> },
    /// Terminal: holds nothing.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Closed,
}

impl Domain {
    /// Constructs empty bounded state under the supplied immutable limits; no IO or clocks are consulted.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn new(config: &Config, seed: u64) -> Domain {
        Domain::scripted(config, seed, Box::new([]))
    }

    /// A provider that plays the conversations `scripts` cue from them, and
    /// the others at random.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn scripted(config: &Config, seed: u64, scripts: Box<[Script]>) -> Domain {
        Domain::try_scripted(config, seed, scripts).expect("scripts fit the provider limits")
    }

    /// Refuses a script collection exceeding the configured owned-byte cap.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub fn try_scripted(config: &Config, seed: u64, scripts: Box<[Script]>) -> Result<Domain, Error> {
        if !limits::fits(limits::scripts(&scripts), config.script_bytes) {
            return Err(Error::ContextTooLong);
        }
        Ok(Domain {
            calls: Slab::with_capacity(config.calls),
            timers: Deadlines::with_capacity(config.calls),
            rng: Rng::new(seed),
            minted: 0,
            scripts,
        })
    }

    /// Calls present, answered ones included until they are reclaimed.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn calls(&self) -> u32 {
        self.calls.len()
    }

    /// Earliest injected deadline still pending, or `None` when no timed work remains.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        self.timers.next()
    }

    /// Whether injected now reaches the earliest pending deadline; no live clock is read.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn is_due(&self, now: Time) -> bool {
        match self.timers.next() {
            Some(at) => at <= now,
            None => false,
        }
    }

    /// Iteration-end reclamation of retired entries; it must follow delivery of owned outputs.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub fn reclaim(&mut self) {
        self.calls.reclaim();
    }
}

/// Handles one event, emitting at most [`MAX_OUT`] requests.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn step(domain: &mut Domain, env: &Env<Config>, event: Event, out: &mut Queue<Request>) {
    match event {
        Event::Call { reply_to, query } => call(domain, env, reply_to, &query, out),
    }
}

/// Answers the earliest call due at `env.now`, if there is one.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn fire(domain: &mut Domain, env: &Env<Config>, out: &mut Queue<Request>) {
    let Some(id) = domain.timers.expire(env.now) else {
        return;
    };
    let call = domain.calls.get_mut(id).expect("a call lives until its timer fires");
    let state = mem::replace(&mut call.state, State::Closed);
    call.state = match state {
        State::Thinking { reply_to, result } => answer(reply_to, result, out),
        State::Closed => unreachable!("a closed call has no timer"),
    };
    domain.calls.retire(id);
}

fn call(domain: &mut Domain, env: &Env<Config>, reply_to: ReplyTo, query: &Query, out: &mut Queue<Request>) {
    if domain.calls.is_full() {
        out.push(Request::Reply { to: reply_to, result: Err(Error::Overloaded) });
        return;
    }
    let config = &env.limits;
    let result = if limits::fits(limits::query(query), config.query_bytes) {
        match respond::respond(&mut domain.rng, &mut domain.minted, config, &domain.scripts, query) {
            Ok(answer) if limits::fits(limits::answer(&answer), config.answer_bytes) => Ok(answer),
            Ok(_) => Err(Error::ContextTooLong),
            Err(error) => Err(error),
        }
    } else {
        Err(Error::ContextTooLong)
    };
    let latency = domain.rng.between(config.latency_min.as_nanos(), config.latency_max.as_nanos());
    let call = Call { state: State::Thinking { reply_to, result } };
    let id = domain.calls.insert(call).expect("checked for room above");
    let at = env.now.saturating_add(Duration::from_nanos(latency));
    domain.timers.arm(id, at).expect("one timer per call fits");
}

/// Thinking, timer: the answer goes out.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
fn answer(reply_to: ReplyTo, result: Result<Answer, Error>, out: &mut Queue<Request>) -> State {
    out.push(Request::Reply { to: reply_to, result });
    State::Closed
}
