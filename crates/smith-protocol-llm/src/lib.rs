//! Smith's LLM component over skein's connection pool.
//!
//! It keeps endpoint names and options, two credential generations per account,
//! and each accepted call's receiving contract. It never knows credential
//! acquisition, run policy, retry decisions, or provider wire grammar.
//! [`Component::from_domain`] translates a completion under its admitted run
//! contract, [`Component::from_below`] routes io, and [`Component::fire`]
//! advances child deadlines and buffered work. The service calls
//! [`Component::reclaim`] after settlement. A Complete returns exactly one
//! Completed, Failed or Cancelled; streaming text crosses as a byte count.
//! The typed tools, their schemas, decoding and rendering live here. The pure
//! prompt and terminal translation functions also serve protocol worlds.
//! Contract: protocol/llm.md, sections 1 to 10;
//! programming-model.md, sections 4.4 and 6.3.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod boundary;
mod completion;
mod component;
mod contract;
mod decode;
mod endpoints;
mod failure;
mod grants;
mod limits;
mod prompt;
mod render;
mod tools;
mod types;

pub use boundary::{Below, BelowEvent, FromDomain, ToDomain};
pub use completion::completion;
pub use component::{Component, ComponentError, ComponentLimits, MAX_OUT, MaxOut, component_worst_case};
pub use contract::{decode_deliver, decode_finish, deliver_schema, finish_schema};
pub use decode::decode;
pub use endpoints::{
    ConfiguredEndpoint, ConfiguredModel, EndpointError, EndpointOptions, Endpoints, IdentityProfile, OversizedReasoning,
};
pub use failure::{cancelled, failed, refusal};
pub use grants::{GrantError, Grants};
pub use limits::{Limits, Receiving, completion_worst_case, render_worst_case, worst_case};
pub use prompt::{prepare_prompt, prompt};
pub use render::{CUT_MARKER_BYTES, render_outcome};
pub use tools::{schemas, schemas_for_contract};
pub use types::{Context, Error, ResolvedCall, ToolKind, ToolSchema};

#[cfg(test)]
mod tests;
