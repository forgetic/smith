//! The copied smith agent's session child domain
//! (programming-model.md, section 4.5): one conversation with an LLM, driven turn by
//! turn, running the tools the LLM asks for, its own or, delegated, those its
//! opener serves. When the LLM stops calling tools the session yields to its
//! opener, which continues it with a new message or closes it; a failure, a
//! limit or its budget (turns, tokens and time, given by the opener) ends it
//! on its own.
//!
//! Sans-io: [`step`] and [`fire`] turn events into requests and change nothing
//! but the [`Domain`] they are given. Time and randomness are inputs; every
//! effect, from calling an LLM to telling the opener the session has ended, is
//! a [`Request`] that its parent, the root domain (`smith-domain`),
//! routes on, and its outcome comes back later through the parent as an
//! [`Event`].
//!
//! The session owns the tools child domain (`smith-domain-tools`), which
//! runs the LLM's calls to its own tools: it opens a kit for each session,
//! hands it the calls within its own step, and passes the file and process
//! operations the tools ask of io out as they are, and their ends back.
//!
//! The conversation is provider-neutral ([`llm`]): the protocol layer speaks
//! each provider's wire format.
//!
//! What happens is also told as content-free [`Fact`]s, kept in a bounded
//! queue the parent drains ([`Domain::pop_fact`]); what does not fit is dropped
//! and counted, and nothing the session decides depends on it.

//!
//! Copy baseline: temper `25ac2ad`, migration 05s2 (domain/session.md, section 12).
//! The temper-specific charter, push, verdict and token-budget vocabulary
//! is retained until 05s4; this copy does not yet implement the generic design.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod boundary;
mod domain;
mod facts;
mod limits;
pub mod llm;
pub mod record;
mod session;
#[cfg(test)]
mod tests;

pub use boundary::{Budget, Dimension, End, Event, Request, Spec, Yield};
pub use domain::{Domain, fire, max_out, max_to_opener, resume, step};
pub use facts::Fact;
pub use limits::{Limits, MAX_PARALLEL, worst_case};
