//! V2 host supervision extracted from Temper `19735a066cd485ca9d39e70ffb8ca8bd902ad55a`
//! (worker-agent behavior `e2a6a719`; domain/host.md, sections 2–7 and 10–12).
//! Keeps process proof, channel order, bounded call/turn metadata and terminal rights.
//! Charter, transcript, post-transcript answers and host policy are opaque; credentials
//! are names only. No IO, frame codec, durable policy record or agent internal state.
//! `step` and `fire` consume typed events and injected clocks; the parent reserves
//! `max_out` output slots and settles every issued operation, including during shutdown.
//! Turns move to the parent, with exact commitment ACK metadata retained here.
//! Actual deliveries survive EOF and tree cleanup; `Gone` is the separate process
//! containment terminal, after every outstanding right. Reclaim at the iteration boundary
//! (programming-model.md, sections 4.5, 5 and 6; domain/host.md, sections 4 and 6).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;

mod boundary;

pub mod channel;

mod delivery;

mod domain;

mod facts;

mod limits;

#[cfg(test)]
mod tests;

pub use boundary::{Bounce, End, Event, Fault, Invalid, Request, Signal};

pub use channel::{
    Answer, Ask, CallName, Directory, Down, Effect, Exhausted, Grant, ModelFault, Policy, Reply, RunFailure, RunResult,
    Start, Turn, Up,
};

pub use delivery::{
    Delivered, Delivery, DeliveryFailure, DeliveryReason, DeliveryRefusal, DeliveryStatus, Diagnostic, MAX_DIRECTORIES,
    Marker, Receipt,
};

pub use domain::{Domain, fire, max_out, step};

pub use facts::Fact;

pub use limits::{Limits, worst_case};
