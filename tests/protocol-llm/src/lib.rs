//! The copied agent, run, session and tools together on a scripted typed host
//! (domain/run.md, sections 13 and 14; domain/host.md, sections 2 and 11;
//! testing-strategy.md, sections 2.2, 6 and 7).
//!
//! The world owns time, seeds, a fake checkout and the real fake LLM domain.
//! Its opt-in actual-wire backend adopts prepared shared Clients and byte peers,
//! passes each actual root Complete receiving contract through the adapter, and
//! synchronizes their clocks with the root iteration. Active logical callbacks
//! and retained physical close rights have separate ownership until actual Closed
//! (scratch/client.md, sections 1, 4, 5 and 6).
//! Its separate opt-in parent-delivery bridge exposes only actual root requests
//! and accepts one actual sealed terminal through the existing shared schedule
//! and flight ledger (domain/run.md, section 8.2; domain/host.md, section 9).
//! It calls the real agent's step, fire and resume entrances under bounded
//! output pressure. Its host starts one charter, receives one answer and
//! supplies typed push replies. No engine, worker, forge, channel or agent
//! protocol is linked. Opaque host objects are attested by the real skein JSON
//! parser in the test protocol face. Whole fixture schemas are explicit caller
//! data; shared Skein owns provider grammar and transport. Arbitrary application
//! Finish decoding and deployment remain later migration work.
//!
//! A fixture translator recognizes the copied scripts, not arbitrary provider
//! documents. The independent referee sees only boundary observations. Facts
//! are checked after settlement against those observations; they never drive
//! the host. Replay and memory use skein's shared kit, with no local allocator.

mod fixture;

pub mod host_referee;

mod limits;

pub mod messages_referee;

pub mod referee;

pub mod script;

mod translate;

pub mod wire;

mod world;

pub use limits::{BUDGET, LIMITS, TIGHT};

pub use script::{JOBS, Job};

pub use world::{
    Boundary, CompletionObservation, CompletionTerminal, DeliverySubmission, HostReply, HostSchedule, Settings, World,
    delivered,
};
