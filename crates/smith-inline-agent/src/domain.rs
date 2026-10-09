//! Slot ownership and routing (domain/host.md, sections 9.1, 9.2 and 9.4).
//! Slots retain root domains and every emitted lower right through settlement.
//! Root requests keep their owners, while a separate slab generation scopes
//! callbacks. The parent owns told payloads; exact ACK metadata stays here.

#![expect(clippy::manual_let_else, reason = "checked routes remain exhaustive")]

use alloc::boxed::Box;
use skein_lib::{Env, Id, List, Map, Queue, Slab, Time, Token};
use smith_domain as smith;
use smith_host_domain::{self as host, parent};

use crate::{Completion, Fact, FactKind, Limits, Request, stop, translate};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    Live,
    Cancelling,
    Settling,
    Gone,
}

#[derive(Debug)]
pub(crate) struct Relay {
    pub(crate) name: smith::run::RelayName,
    pub(crate) withdrawn: bool,
}

#[derive(Debug)]
pub(crate) struct Slot {
    pub(crate) root: smith::Domain,
    pub(crate) run: Option<Token>,
    pub(crate) client: Token,
    pub(crate) logical_run: Token,
    pub(crate) state: State,
    pub(crate) wall: Time,
    pub(crate) completions: Map<Token, bool>,
    pub(crate) relays: Map<Token, Relay>,
    pub(crate) turns: Map<u32, u64>,
}

/// Bounded independent Smith roots, addressed through generation-safe handles.
#[derive(Debug)]
pub struct Agent {
    pub(crate) slots: Slab<Slot>,
    pub(crate) clients: Map<Token, Id<Slot>>,
    configuration: smith::Config,
    pub(crate) lower: Queue<smith::Request>,
    facts: Queue<Fact>,
    lost: u64,
    seed: u64,
}

impl Agent {
    /// Allocate routing containers from checked limits and configured model names.
    #[must_use]
    pub fn new(limits: &Limits, configuration: smith::Config, seed: u64) -> Self {
        assert!(crate::worst_case(limits).is_some(), "inline capacities are representable");
        assert!(configuration.valid(&limits.smith), "configured endpoints and models fit every root");
        Self {
            slots: Slab::with_capacity(limits.slots),
            clients: Map::with_capacity(limits.slots),
            configuration,
            lower: Queue::with_capacity(smith::max_out(&limits.smith)),
            facts: Queue::with_capacity(limits.facts),
            lost: 0,
            seed,
        }
    }

    /// Occupied slots, including retired slots until iteration-end reclaim.
    #[must_use]
    pub const fn hosted(&self) -> u32 {
        self.slots.len()
    }

    /// Whether a live root has deferred work for the owner's next pass.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        for (_, id) in &self.clients {
            let slot = self.slots.get(*id).expect("client binding names its retained slot");
            match slot.state {
                State::Live | State::Cancelling => {
                    if slot.root.is_ready() {
                        return true;
                    }
                }
                State::Settling | State::Gone => {}
            }
        }
        false
    }

    /// Earliest root or independent wall alarm; waits never pause either.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        let mut next = None;
        for (_, id) in &self.clients {
            let slot = self.slots.get(*id).expect("retained slot");
            match slot.state {
                State::Live => {
                    next = Some(earlier(next, slot.wall));
                    if let Some(at) = slot.root.next_deadline() {
                        next = Some(earlier(next, at));
                    }
                }
                State::Cancelling => {
                    if let Some(at) = slot.root.next_deadline() {
                        next = Some(earlier(next, at));
                    }
                }
                State::Settling | State::Gone => {}
            }
        }
        next
    }

    /// Whether one bounded alarm may be fired at the injected clock.
    #[must_use]
    pub fn is_due(&self, now: Time) -> bool {
        match self.next_deadline() {
            Some(at) => at <= now,
            None => false,
        }
    }

    /// Drain one native root fact by parent client before iteration-end reclaim.
    pub fn pop_fact(&mut self, client: Token) -> Option<smith::Fact> {
        let id = *self.clients.get(&client)?;
        self.slots.get_mut(id)?.root.pop_fact()
    }

    /// Drain one root content record by parent client under the owner's capture policy.
    pub fn pop_content(&mut self, client: Token) -> Option<smith::Content> {
        let id = *self.clients.get(&client)?;
        self.slots.get_mut(id)?.root.pop_content()
    }

    /// Drain one bounded inline lifecycle observation.
    pub fn pop_observation(&mut self) -> Option<Fact> {
        self.facts.pop()
    }

    /// Lost inline observations; never an admission or settlement input.
    #[must_use]
    pub const fn facts_lost(&self) -> u64 {
        self.lost
    }
}

fn earlier(current: Option<Time>, candidate: Time) -> Time {
    match current {
        Some(current) => current.min(candidate),
        None => candidate,
    }
}

pub(crate) fn observe(agent: &mut Agent, now: Time, kind: FactKind) {
    if agent.facts.room() > 0 {
        agent.facts.push(Fact { at: now, kind });
    } else {
        agent.lost = agent.lost.saturating_add(1);
    }
}

pub(crate) const fn child_env(env: &Env<Limits>) -> Env<smith::Limits> {
    Env { now: env.now, wall: env.wall, limits: env.limits.smith }
}

pub(crate) fn root_step(
    agent: &mut Agent,
    env: &Env<Limits>,
    id: Id<Slot>,
    event: smith::Event,
    out: &mut Queue<Request>,
) {
    let slot = agent.slots.get_mut(id).expect("event belongs to a retained root");
    smith::step(&mut slot.root, &child_env(env), event, &mut agent.lower);
    route(agent, env, id, out);
}

fn refused(agent: &mut Agent, now: Time, client: Token, end: host::End, out: &mut Queue<Request>) {
    out.push(Request::Parent(parent::Request::Gone { client, end, detail: Box::default() }));
    observe(agent, now, FactKind::Gone { client, end });
}

fn spawn(agent: &mut Agent, env: &Env<Limits>, client: Token, start: host::Start, out: &mut Queue<Request>) {
    if agent.slots.is_full() || agent.clients.contains_key(&client) {
        refused(agent, env.now, client, host::End::Busy, out);
        return;
    }
    if start.workspace.is_some() || !start.directories.is_empty() {
        refused(agent, env.now, client, host::End::Invalid(host::Invalid::Directories), out);
        return;
    }
    let logical_run = start.logical_run;
    let event = match translate::start(client, start, &env.limits) {
        Ok(event) => event,
        Err(invalid) => {
            refused(agent, env.now, client, host::End::Invalid(invalid), out);
            return;
        }
    };
    let wall = match env.now.checked_add(env.limits.wall_time) {
        Some(at) => at,
        None => {
            refused(agent, env.now, client, host::End::Invalid(host::Invalid::Limits), out);
            return;
        }
    };
    let slot = Slot {
        root: smith::Domain::new(&env.limits.smith, agent.configuration.clone(), agent.seed ^ client.raw()),
        run: None,
        client,
        logical_run,
        state: State::Live,
        wall,
        completions: Map::with_capacity(env.limits.smith.run.conversations),
        relays: Map::with_capacity(env.limits.smith.run.calls),
        turns: Map::with_capacity(env.limits.window.turns),
    };
    let id = agent.slots.insert(slot).expect("free slot was checked");
    assert!(
        agent.clients.insert(client, id).expect("slot bound also bounds clients").is_none(),
        "root output preserves its scoped identity and single right"
    );
    out.push(Request::Parent(parent::Request::Started { client, agent: id.token() }));
    observe(agent, env.now, FactKind::Opened { client });
    root_step(agent, env, id, event, out);
}

/// Handle one command in the shared agent parent vocabulary.
pub fn step(agent: &mut Agent, env: &Env<Limits>, event: parent::Event, out: &mut Queue<Request>) {
    match event {
        parent::Event::Spawn { client, start } => spawn(agent, env, client, start, out),
        parent::Event::Message { agent: handle, name, label, text } => {
            let id = Id::<Slot>::from_token(handle);
            let slot = match agent.slots.get(id) {
                Some(slot) => slot,
                None => return,
            };
            let run = match slot.state {
                State::Live => slot.run,
                State::Cancelling | State::Settling | State::Gone => None,
            };
            match run {
                Some(run) => root_step(agent, env, id, smith::Event::Message { run, name, label, text }, out),
                None => out.push(Request::Parent(parent::Request::MessageRefused {
                    client: slot.client,
                    name,
                    reason: host::MessageRefusal::Ending,
                })),
            }
        }
        parent::Event::Answer { agent: handle, call, reply } => {
            let id = Id::<Slot>::from_token(handle);
            let slot = match agent.slots.get_mut(id) {
                Some(slot) => slot,
                None => return,
            };
            let relay = match slot.relays.get(&call) {
                Some(relay) => relay.name,
                None => return,
            };
            let reply = match translate::host_reply(reply) {
                Some(reply) => reply,
                None => return,
            };
            slot.relays.remove(&call);
            match slot.state {
                State::Live | State::Cancelling => {
                    root_step(agent, env, id, smith::Event::HostReturned { relay, reply }, out);
                }
                State::Settling | State::Gone => {}
            }
            stop::settle(agent, env.now, id, out);
        }
        parent::Event::Acknowledge { agent: handle, turn } => {
            let id = Id::<Slot>::from_token(handle);
            let slot = match agent.slots.get_mut(id) {
                Some(slot) => slot,
                None => return,
            };
            if slot.turns.remove(&turn).is_none() {
                return;
            }
            let run = slot.run;
            match slot.state {
                State::Live | State::Cancelling => {
                    if let Some(run) = run {
                        root_step(agent, env, id, smith::Event::Acknowledge { run, turn }, out);
                    }
                }
                State::Settling | State::Gone => {}
            }
            stop::settle(agent, env.now, id, out);
        }
        parent::Event::Grant { agent: handle, grant } => {
            let id = Id::<Slot>::from_token(handle);
            let slot = match agent.slots.get(id) {
                Some(slot) => slot,
                None => return,
            };
            match slot.state {
                State::Live | State::Cancelling => {
                    root_step(agent, env, id, smith::Event::Grant { grant: translate::grant(grant) }, out);
                }
                State::Settling | State::Gone => {}
            }
        }
        parent::Event::Stop { agent: handle } => stop::cancel(agent, env, Id::<Slot>::from_token(handle), out),
    }
}

/// Return one provider terminal to the slot that emitted its request.
pub fn terminal(agent: &mut Agent, env: &Env<Limits>, handle: Token, completion: Completion, out: &mut Queue<Request>) {
    let id = Id::<Slot>::from_token(handle);
    let slot = match agent.slots.get_mut(id) {
        Some(slot) => slot,
        None => return,
    };
    let owner = match &completion {
        Completion::Completed { owner, .. } | Completion::Failed { owner, .. } | Completion::Cancelled { owner } => {
            *owner
        }
    };
    if slot.completions.remove(&owner).is_none() {
        return;
    }
    match slot.state {
        State::Live | State::Cancelling => {
            let event = match completion {
                Completion::Completed { owner, completion } => smith::Event::Completed { owner, completion },
                Completion::Failed { owner, failure, evidence, detail } => {
                    smith::Event::Failed { owner, failure, evidence, detail }
                }
                Completion::Cancelled { owner } => smith::Event::Cancelled { owner },
            };
            root_step(agent, env, id, event, out);
        }
        State::Settling | State::Gone => {}
    }
    stop::settle(agent, env.now, id, out);
}

/// Fire one due root or wall alarm with enough room for its complete fanout.
pub fn fire(agent: &mut Agent, env: &Env<Limits>, out: &mut Queue<Request>) {
    let mut due = None;
    for (_, id) in &agent.clients {
        let slot = agent.slots.get(*id).expect("retained slot");
        match slot.state {
            State::Live => {
                if slot.wall <= env.now || slot.root.is_due(env.now) {
                    due = Some(*id);
                    break;
                }
            }
            State::Cancelling => {
                if slot.root.is_due(env.now) {
                    due = Some(*id);
                    break;
                }
            }
            State::Settling | State::Gone => {}
        }
    }
    let id = match due {
        Some(id) => id,
        None => return,
    };
    let slot = agent.slots.get_mut(id).expect("due slot");
    if slot.state == State::Live && slot.wall <= env.now {
        stop::cancel(agent, env, id, out);
    } else {
        smith::fire(&mut slot.root, &child_env(env), &mut agent.lower);
        route(agent, env, id, out);
    }
}

/// Resume one deferred root handoff with room reserved for its complete fanout.
pub fn resume(agent: &mut Agent, env: &Env<Limits>, out: &mut Queue<Request>) {
    let mut ready = None;
    for (_, id) in &agent.clients {
        let slot = agent.slots.get(*id).expect("retained slot");
        match slot.state {
            State::Live | State::Cancelling => {
                if slot.root.is_ready() {
                    ready = Some(*id);
                    break;
                }
            }
            State::Settling | State::Gone => {}
        }
    }
    if let Some(id) = ready {
        let slot = agent.slots.get_mut(id).expect("ready slot");
        smith::resume(&mut slot.root, &child_env(env), &mut agent.lower);
        route(agent, env, id, out);
    }
}

/// Reclaim after outputs and native observations have been drained for this pass.
pub fn reclaim(agent: &mut Agent) {
    let mut gone = List::with_capacity(agent.clients.capacity());
    for (client, id) in &agent.clients {
        let slot = agent.slots.get_mut(*id).expect("retained slot");
        slot.root.reclaim();
        match slot.state {
            State::Gone => gone.push(*client).expect("one entry per slot"),
            State::Live | State::Cancelling | State::Settling => {}
        }
    }
    for client in &gone {
        agent.clients.remove(client);
    }
    agent.slots.reclaim();
}

#[expect(clippy::too_many_lines, reason = "every root request has one typed parent or lower route")]
pub(crate) fn route(agent: &mut Agent, env: &Env<Limits>, id: Id<Slot>, out: &mut Queue<Request>) {
    for _ in 0..smith::max_out(&env.limits.smith) {
        let request = match agent.lower.pop() {
            Some(request) => request,
            None => break,
        };
        let slot = agent.slots.get_mut(id).expect("root output belongs to its slot");
        let client = slot.client;
        match request {
            smith::Request::Admitted { host_run, run } => {
                assert_eq!(slot.logical_run, host_run, "root output preserves its scoped identity and single right");
                slot.run = Some(run);
                out.push(Request::Parent(parent::Request::Admitted { client }));
            }
            smith::Request::MessageRefused { host_run, name, reason } => {
                assert_eq!(slot.logical_run, host_run, "root output preserves its scoped identity and single right");
                out.push(Request::Parent(parent::Request::MessageRefused {
                    client,
                    name,
                    reason: translate::message_refusal(reason),
                }));
            }
            smith::Request::Turn { host_run, number, spent, read, turn, position: _ } => {
                assert_eq!(slot.logical_run, host_run, "root output preserves its scoped identity and single right");
                let body = host::TurnValue::new(turn, u64::MAX).expect("root concrete turns have checked ownership");
                assert!(
                    slot.turns
                        .insert(number, body.owned_bytes())
                        .expect("root respects the acknowledgement window")
                        .is_none(),
                    "root output preserves its scoped identity and single right"
                );
                out.push(Request::Parent(parent::Request::Turn {
                    client,
                    turn: host::Turn { number, spent: spent.units, read, body },
                }));
            }
            smith::Request::Waiting { host_run, read } => {
                assert_eq!(slot.logical_run, host_run, "root output preserves its scoped identity and single right");
                out.push(Request::Parent(parent::Request::Waiting { client, read }));
            }
            smith::Request::Answer { to, answer, read } => {
                assert_eq!(to.into_token(), client, "root output preserves its scoped identity and single right");
                out.push(Request::Parent(parent::Request::Answered {
                    client,
                    answer: translate::answer(answer, read),
                }));
                stop::finish(agent, env.now, id, out);
            }
            request @ smith::Request::Complete { owner, .. } => {
                assert!(
                    slot.completions.insert(owner, false).expect("root bounds live completions").is_none(),
                    "root output preserves its scoped identity and single right"
                );
                out.push(Request::Lower { agent: id.token(), request });
            }
            request @ smith::Request::Cancel { owner } => {
                match slot.completions.get_mut(&owner) {
                    Some(cancelled) => *cancelled = true,
                    None => unreachable!("cancel retains a live provider right"),
                }
                out.push(Request::Lower { agent: id.token(), request });
            }
            smith::Request::HostCall { host_run, relay, name, tool, effect, input, deadline } => {
                assert_eq!(slot.logical_run, host_run, "root output preserves its scoped identity and single right");
                assert!(
                    slot.relays
                        .insert(relay.owner, Relay { name: relay, withdrawn: false })
                        .expect("root bounds its host rights")
                        .is_none(),
                    "root output preserves its scoped identity and single right"
                );
                out.push(Request::Parent(parent::Request::Called {
                    client,
                    logical_run: host_run,
                    call: relay.owner,
                    name: host::CallName {
                        activation: name.activation,
                        completion: name.completion,
                        position: name.position,
                    },
                    deadline,
                    ask: host::Ask::Host { tool, effect: translate::effect(effect), body: Box::from(input.bytes()) },
                }));
            }
            smith::Request::WithdrawHost { relay } => {
                if let Some(pending) = slot.relays.get_mut(&relay.owner) {
                    assert_eq!(pending.name, relay, "root output preserves its scoped identity and single right");
                    if !pending.withdrawn {
                        pending.withdrawn = true;
                        out.push(Request::Parent(parent::Request::Withdrawn { client, call: relay.owner }));
                    }
                }
            }
            smith::Request::Rejected { grant } => out.push(Request::Parent(parent::Request::Rejected {
                client,
                account: grant.account,
                generation: grant.generation,
            })),
            smith::Request::Exhausted { account, retry_after } => {
                out.push(Request::Parent(parent::Request::Exhausted { client, account, retry_after }));
            }
            smith::Request::Checking { .. }
            | smith::Request::ChecksEnded { .. }
            | smith::Request::Deliver { .. }
            | smith::Request::Io { .. }
            | smith::Request::CancelIo { .. }
            | smith::Request::Read { .. }
            | smith::Request::Probe { .. }
            | smith::Request::Check { .. }
            | smith::Request::Abort { .. } => unreachable!("workspace starts were refused before root admission"),
        }
    }
}
