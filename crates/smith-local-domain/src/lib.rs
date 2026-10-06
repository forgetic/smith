//! Local host configuration and ownership for one chat (domain/host.md,
//! sections 8–10). The domain owns the in-process agent, its configured
//! charter and bounded output queue. It knows no terminal, file encoding,
//! provider secret or provider wire format.
//!
//! The state machine and its `step`, `fire` and `resume` entry points are
//! introduced with the first running-chat increment.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod boundary;
mod chat;
mod config;
mod credentials;
mod domain;
mod facts;
mod limits;
mod person;
mod turns;

#[cfg(test)]
mod tests;

pub use boundary::{Event, Request};
pub use config::{Config, Contract, Invalid, charter};
pub use domain::Domain;
pub use facts::Fact;
pub use limits::{Limits, worst_case};
