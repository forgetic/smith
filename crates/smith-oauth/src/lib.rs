//! Bounded OAuth refresh documents and durable token metadata codecs
//! (domain/host.md, sections 7 and 11; programming-model.md, sections 4.4 and
//! 6.3). Typed values own secret credential bytes; [`Collector`] owns a bounded
//! JSON document while it is assembled. No live refresh or account state is
//! retained by this crate, and secret values are excluded from debug output.
//!
//! Request/response encode and decode functions exchange complete documents
//! under explicit [`Limits`], returning typed errors instead of partial
//! accepted values. [`encode_record`] and [`decode_record`] check the durable
//! version and bounds without store IO. [`rotate`] preserves the old refresh
//! token when the peer supplies no replacement. [`read_claims`] extracts
//! metadata without verifying a JWT or authenticating its bearer.
//! The crate never knows live clocks, network, store commit order, account
//! selection, refresh scheduling or host credential policy.
//!
//! Copy baseline: temper `25ac2ad`, migration 05s2. Client mechanics and their
//! later shared extraction are not implemented by this codec.

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
