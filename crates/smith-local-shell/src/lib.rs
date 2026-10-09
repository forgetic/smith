//! Smith's local shell library (protocol/agent.md, section 6; protocol/hosts.md,
//! section 5). Configuration, durable chat and token files, and terminal
//! output are shared by the binary and hosted process worlds. Kernel,
//! simulator, neighbour and fault state belong to the caller.

mod local_auth;
mod local_auth_http;

pub mod local_host;
pub mod local_settings;
pub mod local_shell;
pub mod local_store;
pub mod local_tokens;

#[cfg(test)]
mod local_auth_tests;

pub use local_host::{Local, Resources};
pub use local_shell::run;
