//! Local host for one person and one chat (domain/host.md, sections 8–10).
//! It owns the in-process agent, typed chat metadata, queued person lines,
//! credential names and turns awaiting a durable store terminal. It never
//! knows terminal IO, file encoding, provider secrets or provider syntax.
//!
//! `step`, `fire` and `resume` take the injected clock and limits. The caller
//! reserves `max_out` slots and settles each store, credential and agent IO
//! request once. `resume` first asks for the saved chat and later advances a
//! ready child. A run answer is shown only after its turns are saved.
//!
//! ```text
//! state     event or child request       next       emits
//! Loading   Loaded                       Idle       start queued line
//! Loading   intent Loaded                Loading   inspect git heads, save reconciled answer
//! Idle      Line                         Starting   SaveState, Credential
//! Starting  StateSaved + all grants      Running    agent Start
//! Running   Line                         Running    SaveState, then Message
//! Running   agent Turn                   Running    SaveTurn, Show
//! Running   agent Deliver                Running    survey, save intent, commit, save answer
//! Running   TurnSaved                    Running    release held answer
//! Running   Interrupt                    Ending     agent Cancel
//! Running   agent Answer                 Loading    Show after saves, Load
//! Idle      Interrupt or Closed          Done       Exit
//! any       StoreFailed                  Done       Show, Exit after child settles
//! ```

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod boundary;
mod chat;
mod config;
mod credentials;
mod delivery;
mod domain;
mod facts;
mod limits;
mod person;
mod turns;

#[cfg(test)]
mod tests;

pub use boundary::{
    AgentIo, ChatState, CredentialFailure, DeliveryIntent, DeliveryRecord, DeliveryState, Event, ExitStatus,
    ExternalEvent, ExternalRequest, ExternalStart, GitOp, GitResult, IntentDirectory, Request, StoreFailure,
};
pub use config::{Config, Contract, Invalid, PushTarget, charter};
pub use domain::{Domain, fire, max_out, resume, step};
pub use facts::Fact;
pub use limits::{Limits, worst_case};
