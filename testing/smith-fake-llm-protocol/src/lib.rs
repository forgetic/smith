//! Fake provider documents and events (domain/session.md, sections 4 and 12).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
pub mod documents;
pub mod oauth;
pub mod provider;
