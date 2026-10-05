//! OAuth refresh documents and durable token metadata (domain/host.md, sections 7 and 11).
//! All clocks and bounds are injected; this crate performs no I/O and owns no
//! provider policy or domain credentials. Its server codecs serve the fake peer.
//!
//! Copy baseline: temper `25ac2ad`, migration 05s2 (domain/host.md, sections 7 and 11).
//! The bounded OAuth codec is retained unchanged. Shared OAuth-client mechanics
//! remain an explicitly later extraction; this crate owns no host credential policy.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
mod common;
mod documents;
mod json;
mod jwt;
mod record;
pub use common::{DecodeError, Failure, Limits, classify, worst_case};
pub use documents::{
    OAuthError, RefreshRequest, TokenResponse, decode_error, decode_request, decode_response, encode_error,
    encode_request, encode_response,
};
pub use json::{Collector, Json};
pub use jwt::{Claims, read_claims};
pub use record::{AccountKind, RECORD_VERSION, RefreshState, SavedToken, decode_record, encode_record, rotate};

#[cfg(test)]
mod tests;
