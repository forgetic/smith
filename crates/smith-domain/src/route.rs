//! Routing (programming-model.md, section 4.5; domain/run.md, sections 2, 3, 10 and 13;
//! domain/session.md, sections 3–6): each of the protocol's events to the child domain it is for,
//! each child domain's requests to the protocol layer or, translated, to the
//! other, and the hand-offs the ready list deferred.
//!
//! Every match is exhaustive, so a variant added to either side's vocabulary
//! breaks the build here.
//! The functions retain no independent state: they mutate the root's bounded
//! queues and child domains. They never know provider wire syntax, credential
//! secrets or the host's delivery policy. [`event`], [`deliver`] and
//! [`hand_off`] perform the boundary handoffs.

use alloc::boxed::Box;

use skein_lib::{Duration, Env, Id, Queue, ReplyTo, Token};
use smith_domain_run::{self as run, Spend};
use smith_domain_session as session;

use crate::boundary::{Event, Request};
use crate::domain::{Credential, Domain, Due, Flight, Handoff, StartContext, TurnHandoff};
use crate::limits::{self, Limits};
use crate::peer::Peer;
use crate::translate;

/// What the run reads: this iteration's times, and its own limits.
pub(crate) const fn run_env(env: &Env<Limits>) -> Env<run::Limits> {
    Env { now: env.now, wall: env.wall, limits: env.limits.run }
}

/// What the session child domain reads.
pub(crate) const fn session_env(env: &Env<Limits>) -> Env<session::Limits> {
    Env { now: env.now, wall: env.wall, limits: env.limits.session }
}

/// Hands one of the protocol's events to the child domain it is for.
pub(crate) fn event(domain: &mut Domain, env: &Env<Limits>, event: Event) {
    let event = match event {
        Event::Start { reply_to, host_run, activation, charter, workspace, grants, transcript } => {
            if !endpoints_known(domain, &charter) {
                domain.notices.push(Request::Answer {
                    to: reply_to,
                    answer: run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Endpoint)),
                });
                return;
            }
            if !takes_grants(domain, &grants, env.limits.accounts) {
                domain.notices.push(Request::Answer {
                    to: reply_to,
                    answer: run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Grants)),
                });
                return;
            }
            for grant in grants {
                granted(domain, env, grant);
            }
            return start(domain, env, reply_to, host_run, activation, charter, workspace, transcript);
        }
        Event::Grant { grant } => return granted(domain, env, grant),
        Event::Message { run, name, text } => run::Event::Message { run, name, text },
        Event::Cancel { run } => run::Event::Cancel { run },
        Event::HostReturned { relay, reply } => run::Event::HostReturned { relay, reply },
        Event::Delivered { owner, delivery } => run::Event::Delivered { owner, delivery },
        Event::Read { owner, read } => run::Event::Read { owner, read },
        Event::Probed { owner, executable } => run::Event::Probed { owner, executable },
        Event::Checked { owner, ran } => run::Event::Checked { owner, ran },
        Event::Aborted { owner } => run::Event::Aborted { owner },
        Event::Completed { owner, completion } => {
            capture(domain, env, owner, &completion);
            let ended = domain.completions.remove(&owner);
            assert!(ended.is_some(), "completed calls were emitted");
            let id = *domain.sessions.get(&owner).expect("a session lives until its call has ended");
            let conversation = domain.peers.get(id).expect("a peer lives as its session").conversation;
            let overflow = match session::preview_completion(&domain.session, owner, completion.usage) {
                Err(end) => Some(end),
                Ok(price) => priced_overflow(run::completion_overflow(
                    &domain.run,
                    conversation,
                    price,
                    translate::spend(1, completion.usage),
                )),
            };
            if let Some(end) = overflow {
                return session_step(domain, env, session::Event::Overflowed { owner, end });
            }
            let peer = domain.peers.get_mut(id).expect("a peer lives as its session");
            let before = peer.tickets();
            let completion =
                peer.completion(completion, env.limits.decoded_call_bytes.min(env.limits.session.session_bytes));
            domain.tickets = domain.tickets.saturating_add(peer.tickets()).saturating_sub(before);
            return session_step(domain, env, session::Event::Completed { owner, completion });
        }
        Event::Failed { owner, failure, evidence, detail } => {
            let grant = domain.completions.remove(&owner).expect("failed calls were emitted");
            match failure {
                crate::llm::Failure::Unauthorized => {
                    if let Some(held) = domain.grants.get_mut(&grant.account)
                        && !was_rejected(held, grant.generation)
                    {
                        held.rejected = Some(grant.generation);
                        domain.notices.push(Request::Rejected { grant });
                    }
                }
                crate::llm::Failure::Exhausted { retry_after } => {
                    domain.notices.push(Request::Exhausted { account: grant.account, retry_after });
                }
                crate::llm::Failure::Overloaded
                | crate::llm::Failure::RateLimited { .. }
                | crate::llm::Failure::Unavailable
                | crate::llm::Failure::TimedOut
                | crate::llm::Failure::ContextTooLong
                | crate::llm::Failure::Invalid
                | crate::llm::Failure::Limit
                | crate::llm::Failure::Protocol
                | crate::llm::Failure::Cancelled => {}
            }
            return session_step(domain, env, session::Event::Failed { owner, failure, evidence, detail });
        }
        Event::Cancelled { owner } => {
            let ended = domain.completions.remove(&owner);
            assert!(ended.is_some(), "cancelled calls were emitted");
            return session_step(domain, env, session::Event::Cancelled { owner });
        }
        Event::Done { owner, done } => {
            if room_for_content(domain, env, crate::facts::done_bytes(&done)) {
                domain.content.push(crate::Content::Tool { owner, done: done.clone() });
            }
            return session_step(domain, env, session::Event::Done { owner, done });
        }
    };
    run_step(domain, env, event);
}

fn priced_overflow(failure: Option<run::Failure>) -> Option<session::End> {
    match failure {
        Some(run::Failure::Budget(run::Exhausted::Overflow(run::Overflow::Spend))) => Some(session::End::PriceOverflow),
        Some(run::Failure::Budget(run::Exhausted::Overflow(run::Overflow::Usage))) => Some(session::End::UsageOverflow),
        Some(_) | None => None,
    }
}

fn endpoints_known(domain: &Domain, charter: &run::Charter) -> bool {
    if !domain.config.contains(charter.llm.endpoint) {
        return false;
    }
    for model in &charter.models {
        if !domain.config.contains(model.endpoint) {
            return false;
        }
    }
    true
}

/// Delivers a hand-off from the run that waited on the ready list.
pub(crate) fn deliver(domain: &mut Domain, env: &Env<Limits>, handoff: Handoff) {
    let event = match handoff {
        Handoff::Close { peer } => {
            let peer = domain.peers.get(peer).expect("a close is forgotten when its peer goes");
            let session = peer.session.expect("the run closes a conversation once it has started");
            session::Event::Close { session }
        }
        Handoff::Answer { owner } => {
            let flight = domain.flights.remove(&owner).expect("a call is in flight until it is answered");
            match flight.answer {
                Due::Answered { feedback, spent } => {
                    session::Event::Answered { owner, text: feedback.text, error: feedback.error, spent }
                }
                Due::Cancelled { spent } => session::Event::AnswerCancelled { owner, spent },
                Due::Waiting => unreachable!("a call is on the ready list once the run has returned it"),
            }
        }
    };
    session_step(domain, env, event);
}

#[expect(
    clippy::too_many_arguments,
    reason = "Start keeps the host-issued activation with its concrete admission fields"
)]
fn start(
    domain: &mut Domain,
    env: &Env<Limits>,
    reply_to: ReplyTo,
    host_run: Token,
    activation: u64,
    charter: run::Charter,
    workspace: Option<run::Workspace>,
    transcript: Option<session::record::Transcript>,
) {
    if domain.starts.is_full() {
        domain.notices.push(Request::Answer { to: reply_to, answer: run::Answer::Refused(run::Refusal::Busy) });
        return;
    }
    let compatible = match crate::feedback_worst_case(&env.limits.run) {
        Some(bytes) => bytes <= env.limits.session.delegated_result_bytes && env.limits.session.spend > 0,
        None => false,
    };
    if !compatible
        || env.limits.run.directories > env.limits.session.tools.repos
        || env.limits.run.directory_name_bytes > env.limits.session.tools.path_bytes
    {
        domain.notices.push(Request::Answer {
            to: reply_to,
            answer: run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Conversation)),
        });
        return;
    }
    let (transcript, refused) = if charter.resume {
        match transcript {
            Some(transcript) if history_fits(&transcript, &env.limits) => (Some(transcript), None),
            Some(_) => (None, Some(run::TranscriptRefusal::TooLarge)),
            None => (None, None),
        }
    } else {
        (None, None)
    };
    let context = StartContext { reply_to: Some(reply_to), transcript, refused };
    let id = domain.starts.insert(context).expect("reserved original parent right before work");
    run_step(
        domain,
        env,
        run::Event::Start {
            reply_to: ReplyTo::new(id.token()),
            host_run,
            activation,
            charter,
            workspace,
            transcript: Some(id.token()),
        },
    );
}

fn history_fits(transcript: &session::record::Transcript, limits: &Limits) -> bool {
    let cap = u64::from(limits.session.messages);
    if u64::try_from(transcript.turns.len()).expect("owned input count fits") > cap {
        return false;
    }
    let mut messages = u64::try_from(transcript.after.len()).expect("owned input count fits");
    for turn in &transcript.turns {
        messages = match messages.checked_add(u64::try_from(turn.messages.len()).expect("owned input count fits")) {
            Some(count) if count <= cap => count,
            Some(_) | None => return false,
        };
    }
    if messages > cap {
        return false;
    }
    match transcript.owned_bytes() {
        Some(bytes) => match limits::record_payload(limits) {
            Some(cap) => bytes <= cap,
            None => false,
        },
        None => false,
    }
}

/// Routes what the child domains emitted, and what that leads to, until both
/// have emitted all they will in this entry point.
pub(crate) fn hand_off(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    if let Some(notice) = domain.notices.pop() {
        out.push(notice);
    }
    let bound = limits::run_out(&env.limits).saturating_add(limits::session_out(&env.limits));
    for _ in 0..bound {
        // Finish the causal run translation of a child notice before taking
        // its next child request. In particular Session Turn precedes Complete;
        // its concrete root Turn must not be overtaken by that provider effect.
        if let Some(request) = domain.run_out.pop() {
            from_run(domain, env, request, out);
        } else if let Some(request) = domain.session_out.pop() {
            from_session(domain, env, request, out);
        } else {
            return;
        }
    }
    assert!(
        domain.session_out.is_empty() && domain.run_out.is_empty(),
        "an entry point's hand-offs end within its bound"
    );
}

fn run_step(domain: &mut Domain, env: &Env<Limits>, event: run::Event) {
    assert!(domain.run_out.room() >= run::MAX_OUT, "an entry point steps the run no more than its bound");
    run::step(&mut domain.run, &run_env(env), event, &mut domain.run_out);
}

fn session_step(domain: &mut Domain, env: &Env<Limits>, event: session::Event) {
    let room = session::max_out(&env.limits.session);
    assert!(domain.session_out.room() >= room, "an entry point steps the sessions no more than its bound");
    session::step(&mut domain.session, &session_env(env), event, &mut domain.session_out);
}

/// One of the sessions' requests: out to the protocol layer, or to the run.
fn from_session(domain: &mut Domain, env: &Env<Limits>, request: session::Request, out: &mut Queue<Request>) {
    let event = match request {
        session::Request::Priced { opener, spent, own_spent } => {
            run::Event::Priced { conversation: opener, own_spent, subtree_spent: spent }
        }
        session::Request::Turn { opener, turn } => {
            let id = peer(domain, opener);
            if !domain.peers.get(id).expect("main binding lives through Turn").is_main() {
                return;
            }
            let sequence = turn.sequence;
            let record =
                domain.turns.insert(TurnHandoff { turn: Some(turn) }).expect("room for every queued session turn");
            run::Event::Turn { conversation: opener, record: record.token(), sequence }
        }
        session::Request::Complete {
            owner,
            prompt,
            timeout,
            max_completion_bytes,
            max_completion_blocks,
            max_failure_bytes,
        } => {
            let pending = PendingCompletion {
                owner,
                prompt,
                timeout,
                max_completion_bytes,
                max_completion_blocks,
                max_failure_bytes,
            };
            return complete(domain, env, pending, out);
        }
        session::Request::Cancel { owner } => return out.push(Request::Cancel { owner }),
        session::Request::Io { owner, op, deadline } => return out.push(Request::Io { owner, op, deadline }),
        session::Request::CancelIo { owner } => return out.push(Request::CancelIo { owner }),
        session::Request::Opened { opener, session } => {
            let id = peer(domain, opener);
            domain.peers.get_mut(id).expect("found above").session = Some(session);
            let fresh = domain.sessions.insert(session, id).expect("a peer for every session");
            assert!(fresh.is_none(), "a session opens once");
            run::Event::Started { conversation: opener, peer: session }
        }
        session::Request::Yielded { opener, stop, text } => {
            let id = peer(domain, opener);
            let peer = domain.peers.get_mut(id).expect("found above");
            let forgotten = peer.forget_asks(&env.limits.session);
            domain.tickets = domain.tickets.saturating_sub(forgotten);
            run::Event::Yielded { conversation: opener, stop: translate::stop(stop), text }
        }
        session::Request::Used { opener, usage } => {
            run::Event::Used { conversation: opener, spend: translate::spend(1, usage) }
        }
        session::Request::Ended { opener, end, turns, usage } => {
            let id = peer(domain, opener);
            free(domain, id);
            run::Event::Ended { conversation: opener, end: translate::end(end), spend: translate::spend(turns, usage) }
        }
        session::Request::Delegate { owner, opener, call, deadline, origin } => {
            delegated(domain, owner, opener, call, deadline, origin)
        }
        session::Request::Withdraw { owner } => {
            let flight = domain.flights.get_mut(&owner).expect("a call is withdrawn while it is in flight");
            flight.withdrawn = true;
            match &flight.answer {
                Due::Waiting => {}
                // The answer won the race: the run has returned it.
                Due::Answered { .. } | Due::Cancelled { .. } => return,
            }
            let conversation = domain.peers.get(flight.peer).expect("a peer outlives its calls").conversation;
            run::Event::Withdraw { conversation, call: owner }
        }
    };
    run_step(domain, env, event);
}

/// Transfer one live ask into a pending terminal right, retaining its complete
/// historical origin. The session's expiry bounds the call; only its /// terminal releases this flight.
fn delegated(
    domain: &mut Domain,
    owner: Token,
    opener: Token,
    call: Token,
    deadline: skein_lib::Time,
    origin: session::record::Origin,
) -> run::Event {
    let id = peer(domain, opener);
    let activation = domain.peers.get(id).expect("found above").activation;
    let ask = domain.peers.get_mut(id).expect("found above").take(call);
    domain.tickets = domain.tickets.saturating_sub(1);
    let flight = Flight { peer: id, withdrawn: false, answer: Due::Waiting };
    let fresh = domain.flights.insert(owner, flight).expect("room for a batch of each session");
    assert!(fresh.is_none(), "a session names its calls in flight apart");
    run::Event::Delegated {
        conversation: opener,
        call: owner,
        ask,
        deadline,
        name: run::CallName { activation, completion: origin.sequence, position: origin.position },
    }
}

/// One of the run's requests: out to the protocol layer, or to the sessions.
fn from_run(domain: &mut Domain, env: &Env<Limits>, request: run::Request, out: &mut Queue<Request>) {
    let event = match request {
        run::Request::HostCall { host_run, relay, name, tool, effect, input, deadline } => {
            return out.push(Request::HostCall { host_run, relay, name, tool, effect, input, deadline });
        }
        run::Request::WithdrawHost { relay } => return out.push(Request::WithdrawHost { relay }),
        run::Request::Admitted { host_run, run } => return out.push(Request::Admitted { host_run, run }),
        run::Request::Answer { to, answer } => {
            let id = Id::<StartContext>::from_token(to.into_token());
            let context = domain.starts.get_mut(id).expect("root issued the start reply binding");
            let to = context.reply_to.take().expect("one actual terminal consumes the parent right");
            context.transcript = None;
            domain.starts.retire(id);
            return out.push(Request::Answer { to, answer });
        }
        run::Request::Waiting { host_run, read } => return out.push(Request::Waiting { host_run, read }),
        run::Request::Turn { host_run, record, number, read, spent } => {
            let id = Id::<TurnHandoff>::from_token(record);
            let handoff = domain.turns.get_mut(id).expect("root issued the concrete turn binding");
            let turn = handoff.turn.take().expect("one actual output takes the concrete body");
            domain.turns.retire(id);
            return out.push(Request::Turn { host_run, number, read, spent, turn });
        }
        run::Request::Checking { host_run, deadline } => return out.push(Request::Checking { host_run, deadline }),
        run::Request::Deliver { host_run, owner, change, name, deadline } => {
            return out.push(Request::Deliver { host_run, owner, change, name, deadline });
        }
        run::Request::Read { owner, at, max, deadline } => return out.push(Request::Read { owner, at, max, deadline }),
        run::Request::Probe { owner, at, deadline } => return out.push(Request::Probe { owner, at, deadline }),
        run::Request::Check { owner, program, deadline, tail } => {
            return out.push(Request::Check { owner, program, deadline, tail });
        }
        run::Request::Abort { owner } => return out.push(Request::Abort { owner }),
        run::Request::Open { conversation, opening } => {
            return open(domain, env, conversation, opening);
        }
        run::Request::Say { peer, text } => session::Event::Continue { session: peer, content: text },
        run::Request::Close { peer } => {
            // A close of a session that has ended is stale.
            if let Some(id) = domain.sessions.get(&peer) {
                domain.ready.defer(Handoff::Close { peer: *id });
            }
            return;
        }
        run::Request::Return { call, result, spent } => {
            let flight = domain.flights.get_mut(&call).expect("the run returns a call in flight");
            if flight.withdrawn && result == run::Returned::Cancelled {
                flight.answer = Due::Cancelled { spent };
            } else {
                let feedback = crate::feedback(result, env.limits.session.delegated_result_bytes)
                    .expect("compatible canonical receiving cap was checked before any effect");
                flight.answer = Due::Answered { feedback, spent };
            }
            return domain.ready.defer(Handoff::Answer { owner: call });
        }
    };
    session_step(domain, env, event);
}

/// Unsent session request moved into one concrete root admission cell. Its
/// source prompt already owns the receiving reservation; no extra copy is made
/// until the pure run gate accepts.
struct PendingCompletion {
    owner: Token,
    prompt: session::llm::Prompt,
    timeout: Duration,
    max_completion_bytes: u64,
    max_completion_blocks: u32,
    max_failure_bytes: u32,
}

fn complete(domain: &mut Domain, env: &Env<Limits>, pending: PendingCompletion, out: &mut Queue<Request>) {
    let PendingCompletion { owner, prompt, timeout, max_completion_bytes, max_completion_blocks, max_failure_bytes } =
        pending;
    let id = *domain.sessions.get(&owner).expect("a session asks for completions once it has opened");
    let conversation = domain.peers.get(id).expect("a peer lives as its session").conversation;
    // Admission precedes grant lookup, prompt cloning, lease retention
    // and every Client effect. This completion is still unsent.
    match run::completion_permit(&domain.run, conversation) {
        run::CompletionPermit::Allowed => {}
        run::CompletionPermit::Denied(exhausted) => {
            let reason = match exhausted {
                run::Exhausted::Turns => session::BudgetDenial::Turns,
                run::Exhausted::Spend => session::BudgetDenial::Spend,
                run::Exhausted::Time => unreachable!("time closes the run through its alarm"),
                run::Exhausted::Tokens(_) | run::Exhausted::Overflow(_) => {
                    unreachable!("receiving and overflow failures close the run")
                }
            };
            return session_step(domain, env, session::Event::BudgetDenied { owner, reason });
        }
        run::CompletionPermit::Closing => {
            return session_step(domain, env, session::Event::UnsentClosed { owner });
        }
    }
    let peer = domain.peers.get_mut(id).expect("a peer lives as its session");
    // What the last completion asked and the session did not dispatch,
    // it never will.
    let forgotten = peer.forget_asks(&env.limits.session);
    domain.tickets = domain.tickets.saturating_sub(forgotten);
    let account = peer.account;
    let grant = match domain.grants.get(&account) {
        Some(held) if held.expires > env.now => held.name,
        Some(_) | None => {
            return session_step(
                domain,
                env,
                session::Event::Failed {
                    owner,
                    failure: crate::llm::Failure::Unauthorized,
                    evidence: crate::llm::Evidence::Unsent,
                    detail: Box::default(),
                },
            );
        }
    };
    let prompt = peer.prompt(prompt);
    let inserted = domain.completions.insert(owner, grant).expect("one completion per session");
    assert!(inserted.is_none(), "the previous completion ended");
    out.push(Request::Complete {
        owner,
        grant,
        prompt,
        timeout,
        max_completion_bytes,
        max_completion_blocks,
        max_failure_bytes,
        decoded_call_bytes: env.limits.decoded_call_bytes,
    });
}

/// Host rate/share translation and concrete restore admission precede effects.
/// Session alone prices, while token ceilings remain receiving limits.
fn open(domain: &mut Domain, env: &Env<Limits>, conversation: Token, opening: run::Opening) {
    let activation = opening.activation;
    let account = opening.llm.account;
    let dialect = opening.llm.dialect;
    let prices = opening.llm.prices;
    let budget = opening.budget.spend;
    let transcript = match opening.transcript {
        Some(binding) => {
            let context = domain.starts.get_mut(Id::from_token(binding)).expect("root issued the restore binding");
            match context.refused.take() {
                Some(reason) => {
                    return run_step(
                        domain,
                        env,
                        run::Event::Ended {
                            conversation,
                            end: run::End::TranscriptRefused { reason },
                            spend: Spend::ZERO,
                        },
                    );
                }
                None => context.transcript.take(),
            }
        }
        None => None,
    };
    let Some((spec, offered)) = translate::spec(opening, env.limits.session.budget) else {
        // Refused at the conversations' entrance, in the run's terms.
        let ended = run::Event::Ended { conversation, end: run::End::Invalid, spend: Spend::ZERO };
        return run_step(domain, env, ended);
    };
    let peer = Peer::new(conversation, activation, account, offered, &env.limits.session);
    let id = domain.peers.insert(peer).expect("a peer for every conversation the run has");
    let fresh = domain.conversations.insert(conversation, id).expect("a peer for every conversation");
    assert!(fresh.is_none(), "the run names its conversations apart");
    let event = session::Event::Open {
        opener: conversation,
        opening: Box::new(session::record::Opening {
            spec,
            dialect,
            prices: session::record::Prices {
                input: prices.input,
                cached: prices.cached,
                output: prices.output,
                unit: prices.unit,
            },
            budget,
            transcript,
        }),
    };
    session_step(domain, env, event);
}

/// The peer whose session's opener is `conversation`.
fn peer(domain: &Domain, conversation: Token) -> Id<Peer> {
    *domain.conversations.get(&conversation).expect("a session's opener is a conversation the run opened")
}

/// Frees a peer whose session has ended, and its tickets: its calls have all
/// been answered.
fn free(domain: &mut Domain, id: Id<Peer>) {
    let peer = domain.peers.get(id).expect("a peer lives as its session");
    domain.tickets = domain.tickets.checked_sub(peer.tickets()).expect("the peers' tickets are counted");
    let conversation = peer.conversation;
    if let Some(session) = peer.session {
        domain.sessions.remove(&session);
    }
    domain.conversations.remove(&conversation);
    domain.ready.forget(Handoff::Close { peer: id });
    domain.peers.retire(id);
}

fn granted(domain: &mut Domain, env: &Env<Limits>, grant: crate::Grant) {
    if let Some(old) = domain.grants.get(&grant.name.account)
        && old.name.generation >= grant.name.generation
    {
        return;
    }
    let valid = grant.valid.as_nanos().saturating_sub(env.limits.skew.as_nanos());
    let rejected = match domain.grants.get(&grant.name.account) {
        Some(old) => old.rejected,
        None => None,
    };
    let entry = Credential { name: grant.name, expires: env.now.saturating_add(Duration::from_nanos(valid)), rejected };
    if domain.grants.contains_key(&grant.name.account) || domain.grants.len() < env.limits.accounts {
        let inserted = domain.grants.insert(grant.name.account, entry);
        assert!(inserted.is_ok(), "known account or checked room");
    }
}

fn takes_grants(domain: &Domain, grants: &[crate::Grant], most: u32) -> bool {
    if u32::try_from(grants.len()).unwrap_or(u32::MAX) > most {
        return false;
    }
    let mut adding = 0_u32;
    for (index, grant) in grants.iter().enumerate() {
        let mut seen = domain.grants.contains_key(&grant.name.account);
        for old in grants.get(..index).unwrap_or_default() {
            if old.name.account == grant.name.account {
                seen = true;
            }
        }
        if !seen {
            adding = adding.saturating_add(1);
        }
    }
    domain.grants.len().saturating_add(adding) <= most
}

fn was_rejected(credential: &Credential, generation: u64) -> bool {
    match credential.rejected {
        Some(reported) => reported >= generation,
        None => false,
    }
}

fn room_for_content(domain: &mut Domain, env: &Env<Limits>, bytes: u64) -> bool {
    if domain.content.room() == 0 || bytes > env.limits.session.session_bytes {
        domain.content_lost = domain.content_lost.saturating_add(1);
        false
    } else {
        true
    }
}

fn capture(domain: &mut Domain, env: &Env<Limits>, owner: Token, completion: &crate::llm::Completion) {
    for said in &completion.content {
        match said {
            crate::llm::Said::Text { text, .. } | crate::llm::Said::Refusal { text, .. } => {
                if room_for_content(domain, env, crate::facts::bytes(text)) {
                    domain.content.push(crate::Content::Text { owner, text: text.clone() });
                }
            }
            crate::llm::Said::ToolCall { id, name, input, call: _, .. } => {
                let bytes = crate::facts::bytes(id)
                    .saturating_add(crate::facts::bytes(name))
                    .saturating_add(crate::facts::bytes(input));
                if room_for_content(domain, env, bytes) {
                    domain.content.push(crate::Content::Call {
                        owner,
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                }
            }
            crate::llm::Said::Opaque { .. } => {}
        }
    }
    if room_for_content(domain, env, 0) {
        domain.content.push(crate::Content::Usage { owner, usage: completion.usage });
    }
}
