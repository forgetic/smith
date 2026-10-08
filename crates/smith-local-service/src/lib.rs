//! One local chat and its spawned or colocated agent (protocol/hosts.md,
//! sections 5–6; protocol/README.md, sections 5–6). The service keeps local
//! policy, either the host kit or typed agent effects, a credential-value
//! table, and bounded routing queues. The process adapter owns terminal and
//! delivery descriptors; each lower layer has its own operation tokens.
//! It never knows OAuth provider responses or token-file paths. [`iterate`]
//! runs the up, domain and down passes; the shell settles durable store and
//! credential requests in later iterations. Exit waits for lower closes.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod process;
mod service;

#[cfg(test)]
mod tests;

pub use process::{Launch, ProcessLimits};
pub use service::{Config, Error, Limits, Service, StartValues, in_process_worst_case, iterate, worst_case};
