//! Cancel and terminal settlement (domain/host.md, section 9.2).
//! The slot keeps submitted rights after its answer and requests their cleanup
//! once. Gone requires every provider/parent terminal and exact turn ACK.
//! Cancel grace and explicit drop are added by the later stop increment.

#![expect(clippy::manual_let_else, reason = "checked slot lookup remains exhaustive")]

use alloc::boxed::Box;
use skein_lib::{Env, Id, List, Queue, Time};
use smith_domain as smith;
use smith_host_domain::{End, parent};

use crate::domain::{Agent, Slot, State, observe, root_step};
use crate::{FactKind, Limits, Request};

pub(crate) fn cancel(agent: &mut Agent, env: &Env<Limits>, id: Id<Slot>, out: &mut Queue<Request>) {
    let slot = match agent.slots.get_mut(id) {
        Some(slot) => slot,
        None => return,
    };
    match slot.state {
        State::Live => slot.state = State::Cancelling,
        State::Cancelling | State::Settling | State::Gone => return,
    }
    let run = slot.run;
    let client = slot.client;
    observe(agent, env.now, FactKind::Stopped { client });
    if let Some(run) = run {
        root_step(agent, env, id, smith::Event::Cancel { run }, out);
    }
}

pub(crate) fn finish(agent: &mut Agent, now: Time, id: Id<Slot>, out: &mut Queue<Request>) {
    let slot = agent.slots.get_mut(id).expect("answer belongs to a retained slot");
    match slot.state {
        State::Live | State::Cancelling => slot.state = State::Settling,
        State::Settling | State::Gone => unreachable!("a root answers exactly once"),
    }
    let mut cancel = List::with_capacity(slot.completions.capacity());
    for (owner, cancelled) in &slot.completions {
        if !cancelled {
            cancel.push(*owner).expect("one owner per completion");
        }
    }
    for owner in &cancel {
        *slot.completions.get_mut(owner).expect("retained provider right") = true;
        out.push(Request::Lower { agent: id.token(), request: smith::Request::Cancel { owner: *owner } });
    }
    let mut withdraw = List::with_capacity(slot.relays.capacity());
    for (owner, relay) in &slot.relays {
        if !relay.withdrawn {
            withdraw.push(*owner).expect("one owner per relay");
        }
    }
    for owner in &withdraw {
        slot.relays.get_mut(owner).expect("retained host right").withdrawn = true;
        out.push(Request::Parent(parent::Request::Withdrawn { client: slot.client, call: *owner }));
    }
    settle(agent, now, id, out);
}

pub(crate) fn settle(agent: &mut Agent, now: Time, id: Id<Slot>, out: &mut Queue<Request>) {
    let slot = agent.slots.get_mut(id).expect("settlement belongs to a retained slot");
    match slot.state {
        State::Settling => {
            if !slot.completions.is_empty() || !slot.relays.is_empty() || !slot.turns.is_empty() {
                return;
            }
            slot.state = State::Gone;
            let client = slot.client;
            agent.slots.retire(id);
            out.push(Request::Parent(parent::Request::Gone { client, end: End::Stopped, detail: Box::default() }));
            observe(agent, now, FactKind::Gone { client, end: End::Stopped });
        }
        State::Live | State::Cancelling | State::Gone => {}
    }
}
