//! The agent shell keeps its service, startup configuration, inherited roots,
//! trace and error writer. It never knows the driving kernel or neighbours.
//! `Agent` hosts the service and `run` supplies its kernel and clock.
//! Contract: shell.md, section 2.2; protocol/agent.md, sections 4–6.

mod agent_shell;
mod limits;

pub mod config;
pub mod trace;

pub use agent_shell::{Agent, Resources, run};
pub use limits::LIMITS;
