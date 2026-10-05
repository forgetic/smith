//! Bounded codec limits, provider metadata and terminal classifications. The caller owns transport and retry state.
//! The classification and limits helpers use only supplied status, headers and time; they issue no effects.
//!
//! Contract: domain/host.md, sections 7 and 11; programming-model.md, sections 4.4, 4.5 and 6.3.

use alloc::boxed::Box;
use skein_json::{Token, tokenizer, writer};
use skein_lib::{Duration, List, bytes};

/// Bounds for one token exchange and its saved record (domain/host.md, sections 7 and 11).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Maximum complete JSON document bytes admitted by the codec.
    pub document_bytes: u32,
    /// Maximum decoded JSON string bytes.
    pub string_bytes: u32,
    /// Maximum credential-token bytes; tokens never appear in debug output.
    pub token_bytes: u32,
    /// Maximum OAuth client-identifier bytes.
    pub client_bytes: u32,
    /// Maximum retained provider-error detail bytes.
    pub detail_bytes: u32,
    /// Maximum durable token-record bytes.
    pub record_bytes: u32,
    /// Nesting or JSON depth, bounded by the enclosing immutable limits.
    pub depth: u32,
    /// Maximum JSON tokens or scripted output tokens, as named by the enclosing record.
    pub tokens: u32,
}

impl Limits {
    /// Projects the caller's depth and document-byte caps into the bounded JSON writer.
    #[must_use]
    pub const fn writer_limits(&self) -> writer::Limits {
        writer::Limits { depth: self.depth, length: self.document_bytes }
    }

    /// Projects the caller's token, string and nesting caps into the JSON tokenizer.
    #[must_use]
    pub const fn tokenizer_limits(&self) -> tokenizer::Limits {
        tokenizer::Limits {
            depth: self.depth,
            string: self.string_bytes,
            number: 32,
            chunk: 256,
            length: self.document_bytes,
        }
    }
}

/// Document admission failure; the codec returns no partial accepted document.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DecodeError {
    /// Input has invalid syntax or violates the codec's structural contract.
    Malformed,
    /// A required member is absent.
    Missing,
    /// A member has a JSON type the contract does not permit.
    WrongType,
    /// A configured ownership, count or encoded-byte cap would be exceeded.
    TooLarge,
    /// The durable format version is unsupported.
    Version,
}

/// The accounts-domain failures, without secret-bearing transport details.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Failure {
    /// The provider could not be reached or returned an unusable terminal.
    Unavailable,
    /// The injected operation deadline won the race.
    TimedOut,
    /// The provider asks the client to wait before retrying.
    RateLimited {
        /// Optional provider cooldown, retained in the representation of this boundary.
        retry_after: Duration,
    },
    /// The entrance or operation was refused with the enclosing typed reason.
    Refused,
}

/// Classifies an answered refresh. Server failures are ambiguous after sending;
/// only a transport that proves nothing was sent may use `Unavailable`.
#[must_use]
pub fn classify(status: u16, error: Option<&crate::OAuthError>, retry_after: Duration) -> Failure {
    let code = match error {
        Some(error) => error.code.as_ref(),
        None => b"",
    };
    if status == 429 || code == b"rate_limit_exceeded" || code == b"slow_down" {
        return Failure::RateLimited { retry_after };
    }
    if code == b"invalid_grant" || code == b"invalid_client" || code == b"unauthorized_client" {
        return Failure::Refused;
    }
    if status >= 500 || code == b"server_error" || code == b"temporarily_unavailable" {
        return Failure::TimedOut;
    }
    match status {
        400 | 401 | 403 | 404 | 405 | 422 => Failure::Refused,
        _ => Failure::TimedOut,
    }
}

/// Includes an exchange, tokenized JWT payload, two saved generations and a
/// candidate/record being handed to the durability owner. Checked on startup.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    List::<Token>::worst_case(limits.tokens)?
        .checked_mul(3)?
        .checked_add(u64::from(limits.document_bytes).checked_mul(6)?)?
        .checked_add(u64::from(limits.token_bytes).checked_mul(12)?)?
        .checked_add(u64::from(limits.client_bytes))?
        .checked_add(u64::from(limits.detail_bytes).checked_mul(2)?)?
        .checked_add(u64::from(limits.record_bytes).checked_mul(2)?)?
        .checked_add(crate::json::validation_worst_case(limits)?)?
        .checked_add(tokenizer::worst_case(&limits.tokenizer_limits())?)?
        .checked_add(writer::worst_case(&limits.writer_limits())?)
}

pub(crate) fn measured(encoder: writer::Encoder) -> Result<u32, DecodeError> {
    match encoder.measured() {
        Ok(len) => Ok(len),
        Err(writer::Refusal::TooLong | writer::Refusal::TooDeep) => Err(DecodeError::TooLarge),
        Err(writer::Refusal::Text | writer::Refusal::Number) => Err(DecodeError::Malformed),
    }
}

pub(crate) fn bounded(input: &[u8], cap: u32) -> Result<(), DecodeError> {
    if input.len() > usize::try_from(cap).expect("u32 fits usize") {
        return Err(DecodeError::TooLarge);
    }
    if input.is_empty() {
        return Err(DecodeError::Malformed);
    }
    Ok(())
}

pub(crate) fn bearer(input: &[u8], limits: &Limits) -> Result<(), DecodeError> {
    bounded(input, limits.token_bytes)?;
    let mut padding = false;
    let mut body = false;
    for &byte in input {
        if byte == b'=' {
            if !body {
                return Err(DecodeError::Malformed);
            }
            padding = true;
        } else if padding || !(byte.is_ascii_alphanumeric() || b"-._~+/".contains(&byte)) {
            return Err(DecodeError::Malformed);
        } else {
            body = true;
        }
    }
    Ok(())
}

pub(crate) fn text(input: &[u8]) -> Result<(), DecodeError> {
    let limits = writer::Limits { depth: 1, length: u32::MAX };
    let mut measure = writer::Encoder::measure(&limits);
    measure.string(input);
    let _length = measured(measure)?;
    Ok(())
}

pub(crate) fn clipped(input: &[u8], cap: u32) -> Box<[u8]> {
    let mut count = input.len().min(usize::try_from(cap).expect("u32 fits usize"));
    for _back in 0..4_u32 {
        match input.get(count) {
            Some(byte) if byte & 0xc0 == 0x80 => count = count.saturating_sub(1),
            Some(_) | None => break,
        }
    }
    bytes::copy_of(input.get(..count).expect("within input"))
}
