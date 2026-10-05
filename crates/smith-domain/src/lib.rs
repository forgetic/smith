//! The copied agent root (domain/run.md, sections 2, 3, 10 and 14;
//! programming-model.md, sections 4.4 and 4.5). It owns the run and session
//! children, opaque credential names and validity, conversation bindings,
//! tool tickets, deferred handoffs and bounded observation queues.
//!
//! [`step`], [`fire`] and [`resume`] accept typed host, provider and IO events
//! with injected clocks and seed. They emit owned [`Request`] values within
//! [`max_out`]; the caller reserves output room and returns each request's
//! one typed terminal. Iteration-end reclamation follows output delivery.
//! The root translates run conversations into sessions; siblings share no
//! state or vocabulary, and tools remain beneath sessions.
//!
//! The host boundary retains the copied `worker` field names. They are opaque
//! host request identities; a worker process or byte protocol is not required
//! to call this typed domain. The domain never knows credential secrets,
//! authentication, channel bytes, forge state, CI, posting or merge policy.
//! IO owns confined paths and process effects; the host owns delivery.
//! Content-free facts may be dropped and never decide behavior. Owned content
//! observations are separately drained or discarded by the caller.
//!
//! Copy baseline: temper `25ac2ad`, migration 05s2 (domain/run.md, section 14;
//! domain/session.md, section 12). Generic results implement domain/run.md,
//! section 7 in 05s4; generic checked delivery implements section 8 and
//! opaque host declarations and settled recovery implement section 5.2. The copied
//! charter and split token-budget vocabulary remain pending subsequent 05s4
//! increments (domain/run.md, section 14).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod boundary;
mod domain;
mod facts;
mod limits;
pub mod llm;
mod peer;
mod route;
#[cfg(test)]
mod tests;
mod translate;

pub use boundary::{Event, Grant, GrantName, Request};
pub use domain::{Domain, fire, max_out, resume, step};
pub use facts::{Content, Fact};
pub use limits::{Limits, worst_case};
// The payloads are the children's: a parent may use its children's types.
pub use smith_domain_run as run;
pub use smith_domain_session as session;
pub use smith_domain_tools as tools;
