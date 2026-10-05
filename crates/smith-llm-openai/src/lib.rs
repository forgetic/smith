//! `ChatGPT` Responses documents, both sides, and an ordered answer decoder.
//!
//! Copy baseline: temper `25ac2ad`, migration 05s2 (domain/session.md, sections 4 and 12).
//! The subject-captured provider dialect is retained unchanged. Agent protocol
//! integration awaits 05s5; this codec owns no charter, host or session policy.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
mod common;
pub mod identity;
mod json;
mod request;
mod response;
pub use common::{DecodeError, Failure, Limits, ProviderError, RateLimit, Stop, Usage, classify, worst_case};
pub use json::{Collector, Json};
pub use request::{Input, Request, Role, Tool, decode_request, encode_request, measure_request};
pub use response::{
    Event, Item, MAX_OUT, Output, Part, StreamDecoder, decode_error, decode_event, encode_completion, encode_error,
    encode_event,
};

#[cfg(test)]
mod tests;
