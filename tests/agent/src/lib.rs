//! The copied agent, run, session and tools together on a scripted typed host
//! (domain/run.md, sections 13 and 14; domain/host.md, sections 2 and 11;
//! testing-strategy.md, sections 2.2, 6 and 7).
//!
//! The world owns time, seeds, a fake checkout and the real fake LLM domain.
//! It calls the real agent's step, fire and resume entrances under bounded
//! output pressure. Its host starts one charter, receives one answer and
//! supplies typed push replies. No engine, worker, forge, channel or agent
//! protocol is linked: schemas, byte decoding, transport and deployment
//! decisions remain in temper's legacy worlds or await migration 05s5.
//!
//! A fixture translator recognizes the copied scripts, not arbitrary provider
//! documents. The independent referee sees only boundary observations. Facts
//! are checked after settlement against those observations; they never drive
//! the host. Replay and memory use skein's shared kit, with no local allocator.

mod fixture;
mod limits;
pub mod referee;
pub mod script;
mod translate;
mod world;

pub use limits::{BUDGET, LIMITS, TIGHT};
pub use script::{JOBS, Job};
pub use world::{HostReply, Settings, World, delivered};
