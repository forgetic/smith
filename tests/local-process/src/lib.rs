//! Local process neighbours, each owning no simulator state. Kernel binding,
//! fake-machine answers and faults belong to the world loop. Contract:
//! protocol/hosts.md, sections 5 and 7; programming-model.md, section 10.2.

pub mod oauth;

pub mod terminal;

pub mod llm;

pub mod git;

pub mod referee;

pub mod world;
pub use world::{Files, Placement, World};
