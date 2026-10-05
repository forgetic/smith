//! Bounded `ChatGPT` Responses request and answer codecs (domain/session.md,
//! sections 4, 9 and 12; programming-model.md, sections 4.4 and 6.3).
//! Request encoders own no state; JSON collectors and [`StreamDecoder`] retain
//! bounded documents, ordered items, usage and whether their one terminal
//! has been emitted. Decoding and encoding receive explicit [`Limits`].
//!
//! [`encode_request`] and [`decode_request`] exchange complete bounded request
//! documents. [`decode_event`] admits one typed provider event, then the
//! decoder's event/end entrances enforce stream order and return progress,
//! bounded parts or one completion/failure. Its caller reserves [`MAX_OUT`]
//! output slots. Classification uses only supplied status, headers and time.
//! This crate never knows agent charters, tool authority, host policy,
//! credential selection, network connections or live clocks.
//!
//! Copy baseline: temper `25ac2ad`, migration 05s2. Subject-captured provider
//! identity bytes and dialect are retained; agent protocol integration awaits
//! 05s5, and fresh deployment captures remain a separate task.

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
