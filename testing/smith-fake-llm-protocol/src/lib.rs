//! Bounded byte peers for the neutral fake LLM and rotating OAuth issuer
//! (domain/session.md, sections 4, 9 and 12; domain/host.md, sections 7 and 11;
//! programming-model.md, sections 4.4 and 6.3; testing-strategy.md, section 4).
//! [`provider`] retains HTTP/SSE connection progress and fake-call routing;
//! [`oauth`] retains queued issuer plans and bounded credential generations.
//! [`documents`] translates dialect documents to the fake's neutral API.
//!
//! Peer constructors receive explicit limits and seeds. Their start/up/down,
//! resume/fire and close/closed entrances consume injected transport events,
//! time or fake-domain terminals and emit bounded owner/transport records.
//! Callers reserve the declared output maxima and deliver the lower close
//! terminal even after requesting closure. There are no sockets or clocks
//! here. These peers never know agent charters, run state, authenticated host
//! authority, forge state, CI or delivery policy. Deliberate faults and delays
//! are scripted by their world, not inferred from a client's private state.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
pub mod documents;
pub mod oauth;
pub mod provider;
