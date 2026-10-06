//! The agent, run, session and tools together on a scripted typed host
//! (domain/run.md, sections 13 and 14; domain/host.md, sections 2 and 11;
//! testing-strategy.md, sections 2.2, 6 and 7).
//!
//! The world keeps time, seeds, a fake checkout, a fake LLM domain, boundary
//! observations and pending host replies. It never knows provider grammar or
//! transport bytes. Its step, fire and resume entrances drive the domain under
//! bounded output pressure. The parent-delivery entrance exposes root requests
//! and accepts one sealed terminal through the shared schedule and flight ledger
//! (domain/run.md, section 8.2; domain/host.md, section 9).
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

pub mod translate;

mod world;

pub use limits::{BUDGET, LIMITS, TIGHT};

pub use script::{JOBS, Job};

pub use world::{
    Boundary, CompletionObservation, CompletionTerminal, DeliverySubmission, HostReply, HostSchedule, Settings, World,
    delivered,
};

/// Build the scripted fake provider used by agent and local-host worlds.
#[must_use]
pub fn scripted_provider(config: &skein_fake_llm_domain::Config, seed: u64) -> skein_fake_llm_domain::Domain {
    skein_fake_llm_domain::Domain::configured(config, seed, script::all(), smith_session_world::provider::menu())
        .expect("shared agent scripts fit fake provider limits")
}
