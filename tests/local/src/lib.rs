//! Local-host world: the real in-process agent, scripted person and durable
//! typed store, and Skein's fake provider (domain/host.md, sections 8–10).
//! Agent conversation scripts and their translation come from smith-agent-world.

pub mod referee;
mod world;

pub use world::{Cut, Store, StoreFault, World};
