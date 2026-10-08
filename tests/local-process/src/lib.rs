//! Reusable local host, terminal, browser, peer configuration and outside
//! referee (testing.md, sections 2.1, 2.2 and 5). Processes keep only their IO
//! and public observations; Skein owns the loop, hosting, replay and heaps.

pub mod oauth;

pub mod terminal;

pub mod llm;

pub mod git;

pub mod referee;

pub mod process;

pub mod world;
pub use world::{Authentication, Files, Placement, World};
