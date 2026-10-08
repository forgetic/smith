//! One local chat and its supervised spawned agent (protocol/hosts.md,
//! sections 5–6; protocol/README.md, sections 5–6). The service keeps the
//! local and host domains, one ordered credential-value table, and bounded
//! queues between them. It never knows terminal file descriptors, OAuth
//! provider responses, or git implementation details. [`iterate`] routes an
//! up pass and a down pass; shell and process adapters settle the exposed
//! requests in later iterations.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod process;
mod service;

#[cfg(test)]
mod tests;

pub use process::{Launch, ProcessLimits};
pub use service::{Config, Error, Limits, Service, StartValues, iterate, worst_case};
