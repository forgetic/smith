//! Bounded codec limits, provider metadata and terminal classifications. The caller owns transport and retry state.
//! The classification and limits helpers use only supplied status, headers and time; they issue no effects.
//!
//! Contract: domain/session.md, sections 4 and 12; programming-model.md, sections 4.4, 4.5 and 6.3.

use crate::Part;
use alloc::boxed::Box;
use skein_json::{Token, tokenizer, writer};
use skein_lib::{Duration, List, Wall, bytes};

/// Immutable ownership and document caps supplied by the caller to every entry point.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Maximum encoded request-body bytes.
    pub request_bytes: u32,
    /// Maximum complete JSON document bytes admitted by the codec.
    pub document_bytes: u32,
    /// Maximum decoded JSON string bytes.
    pub string_bytes: u32,
    /// Nesting or JSON depth, bounded by the enclosing immutable limits.
    pub depth: u32,
    /// Maximum JSON tokens or scripted output tokens, as named by the enclosing record.
    pub tokens: u32,
    /// Maximum message parts, tool definitions or retained output parts.
    pub parts: u32,
    /// Maximum retained tool-call argument bytes.
    pub input_bytes: u32,
    /// Maximum retained provider-owned opaque-block bytes.
    pub opaque_bytes: u32,
    /// Maximum aggregate owned completion bytes.
    pub answer_bytes: u32,
    /// Maximum retained provider-error detail bytes.
    pub detail_bytes: u32,
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

/// A conservative per-exchange bound including one event's tokens, temporary
/// tokenizer/writer storage and a completion being handed to its owner.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let tokens = List::<Token>::worst_case(limits.tokens)?;
    let parts = List::<Part>::worst_case(limits.parts)?;
    let nested = parts.checked_mul(u64::from(limits.parts).checked_add(8)?)?;
    let documents = u64::from(limits.document_bytes).checked_mul(8)?;
    let answer = u64::from(limits.answer_bytes).checked_mul(4)?;
    tokens
        .checked_mul(4)?
        .checked_add(nested)?
        .checked_add(documents)?
        .checked_add(answer)?
        .checked_add(u64::from(limits.request_bytes))?
        .checked_add(tokenizer::worst_case(&limits.tokenizer_limits())?)?
        .checked_add(writer::worst_case(&limits.writer_limits())?)?
        .checked_add(crate::response::decoder_worst_case(limits)?)
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
}

/// Provider stop reason translated without interpreting the assistant's content.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stop {
    /// The provider ended its assistant turn.
    EndTurn,
    /// The provider asks for tool execution.
    ToolUse,
    /// The provider stopped at its requested output-token cap.
    MaxTokens,
    /// The provider declined to answer.
    Refusal,
}

/// Accepted provider token counts, passed to the session for checked budget charging.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Usage {
    /// Fresh input tokens reported by the provider.
    pub input_tokens: u64,
    /// Output tokens reported by the provider.
    pub output_tokens: u64,
    /// Input tokens served from the provider's prompt cache.
    pub cache_read_tokens: u64,
    /// Input tokens written to the provider's prompt cache.
    pub cache_write_tokens: u64,
}

impl Usage {
    /// No accepted completions or token usage yet.
    pub const ZERO: Usage = Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 };
}

/// Typed terminal classification produced from the provider's status, headers or stream failure.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Failure {
    /// The provider refused the credential.
    Unauthorized,
    /// The provider reports its account allowance spent.
    Exhausted {
        /// Optional provider cooldown, retained in the representation of this boundary.
        retry_after: Duration,
    },
    /// The provider asks the client to wait before retrying.
    RateLimited {
        /// Optional provider cooldown, retained in the representation of this boundary.
        retry_after: Duration,
    },
    /// The provider is temporarily overloaded.
    Overloaded,
    /// The provider could not be reached or returned an unusable terminal.
    Unavailable,
    /// The conversation no longer fits the provider's context window.
    ContextTooLong,
    /// The provider refused the request as unsupported or malformed.
    Invalid,
}

/// Bounded decoded provider-error metadata; it contains no local clock or retry policy.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ProviderError {
    /// Typed entry classification or byte label required by the enclosing contract.
    pub kind: Box<[u8]>,
    /// Bounded provider error description, not a control decision.
    pub message: Box<[u8]>,
    /// Optional provider reset interval in seconds.
    pub resets_in_seconds: Option<u64>,
    /// Optional absolute provider reset timestamp.
    pub resets_at: Option<u64>,
}

/// Rate-limit header metadata collected for one response; absent hints retain conservative defaults.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RateLimit {
    /// Optional provider cooldown, retained in the representation of this boundary.
    pub retry_after: Option<Duration>,
    /// Optional absolute rate-limit reset timestamp.
    pub reset: Option<u64>,
    /// Whether headers report a spent provider allowance.
    pub exhausted: bool,
}

impl RateLimit {
    /// No rate-limit headers have been observed for this response.
    pub const NONE: RateLimit = RateLimit { retry_after: None, reset: None, exhausted: false };

    /// Collects one response-header value as rate-limit metadata; unknown headers change nothing.
    pub fn observe(&mut self, name: &[u8], value: &[u8]) {
        if name.eq_ignore_ascii_case(b"retry-after") {
            self.retry_after = None;
            if let Some(n) = decimal(value) {
                self.retry_after = Some(Duration::from_secs(n));
            }
        }
        if name.eq_ignore_ascii_case(b"anthropic-ratelimit-unified-reset") {
            self.reset = decimal(value);
        }
        if name.eq_ignore_ascii_case(b"anthropic-ratelimit-unified-status") && value == b"rejected" {
            self.exhausted = true;
        }
    }

    /// Computes the provider's requested cooldown from headers and error metadata using the supplied wall time.
    #[must_use]
    pub fn delay(self, error: Option<&ProviderError>, wall: Wall) -> Duration {
        if let Some(delay) = self.retry_after {
            return delay;
        }
        if let Some(error) = error {
            if let Some(seconds) = error.resets_in_seconds {
                return Duration::from_secs(seconds);
            }
            if let Some(reset) = error.resets_at {
                return Duration::from_secs(reset.saturating_sub(wall.as_secs()));
            }
        }
        match self.reset {
            Some(reset) => Duration::from_secs(reset.saturating_sub(wall.as_secs())),
            None => Duration::ZERO,
        }
    }
}

/// Classifies one provider status and bounded error body; retry timing uses only injected metadata and wall time.
#[must_use]
pub fn classify(status: u16, error: Option<&ProviderError>, rate: RateLimit, wall: Wall) -> Failure {
    let kind = match error {
        Some(error) => error.kind.as_ref(),
        None => b"",
    };
    let delay = rate.delay(error, wall);
    if status == 401 || kind == b"authentication_error" || kind == b"invalid_api_key" {
        return Failure::Unauthorized;
    }
    if status == 403 {
        return Failure::Invalid;
    }
    if status == 429 || kind == b"rate_limit_error" || kind == b"rate_limit_exceeded" || kind == b"usage_limit_reached"
    {
        return if rate.exhausted || kind == b"usage_limit_reached" {
            Failure::Exhausted { retry_after: delay }
        } else {
            Failure::RateLimited { retry_after: delay }
        };
    }
    if status == 529 || status == 503 || kind == b"overloaded_error" {
        return Failure::Overloaded;
    }
    if status == 500 || status == 502 || status == 504 || kind == b"api_error" || kind == b"server_error" {
        return Failure::Unavailable;
    }
    let context = kind == b"context_length_exceeded"
        || kind == b"request_too_large"
        || match error {
            Some(error) => bytes::find(&error.message, b"prompt is too long").is_some(),
            None => false,
        };
    if (status == 400 || status == 413 || status == 0) && context {
        return Failure::ContextTooLong;
    }
    match status {
        400 | 404 | 413 | 422 => Failure::Invalid,
        _ => Failure::Unavailable,
    }
}

pub(crate) fn decimal(bytes: &[u8]) -> Option<u64> {
    let mut n: u64 = 0;
    if bytes.is_empty() {
        return None;
    }
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(b.wrapping_sub(b'0')))?;
    }
    Some(n)
}

pub(crate) fn append(out: &mut List<u8>, bytes: &[u8]) -> Result<(), DecodeError> {
    if bytes.len() > usize::try_from(out.room()).expect("u32 fits usize") {
        return Err(DecodeError::TooLarge);
    }
    for &b in bytes {
        if out.push(b).is_err() {
            return Err(DecodeError::TooLarge);
        }
    }
    Ok(())
}

pub(crate) fn clipped(bytes: &[u8], limit: u32) -> Box<[u8]> {
    let mut count = bytes.len().min(usize::try_from(limit).expect("u32 fits usize"));
    // Tokenizer strings are UTF-8. A prefix ends before a continuation byte,
    // so truncating diagnostics never creates a new malformed string.
    for _back in 0..4_u32 {
        match bytes.get(count) {
            Some(byte) if byte & 0xc0 == 0x80 => count = count.saturating_sub(1),
            Some(_) | None => break,
        }
    }
    bytes::copy_of(bytes.get(..count).expect("within bytes"))
}

pub(crate) fn measured(encoder: writer::Encoder) -> Result<u32, DecodeError> {
    match encoder.measured() {
        Ok(len) => Ok(len),
        Err(writer::Refusal::TooLong | writer::Refusal::TooDeep) => Err(DecodeError::TooLarge),
        Err(writer::Refusal::Text | writer::Refusal::Number) => Err(DecodeError::Malformed),
    }
}
