//! Bounded OAuth request, response and error documents. Credential values are secret; no token store or transport is owned here.
//! Encoding and decoding accept caller-supplied limits and return a complete value or a typed refusal.
//!
//! Contract: domain/host.md, sections 7 and 11; programming-model.md, sections 4.4, 4.5 and 6.3.

use crate::{DecodeError, Json, Limits, common, json};
use alloc::boxed::Box;
use skein_json::writer::Encoder;
use skein_lib::bytes;

/// Typed OAuth refresh body supplied by the client; secret credentials are excluded from `Debug`.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash)]
#[expect(missing_debug_implementations, reason = "credential values must never occur in traces")]
pub struct RefreshRequest {
    /// OAuth client identifier; bounded before a refresh request is encoded.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub client_id: Box<[u8]>,
    /// Secret refresh credential; never included in debug output or observations.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub refresh_token: Box<[u8]>,
}

/// Typed OAuth refresh terminal; credentials remain secret and omitted refresh tokens retain the previous value.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash)]
#[expect(missing_debug_implementations, reason = "credential values must never occur in traces")]
pub struct TokenResponse {
    /// Secret access credential; never included in debug output or observations.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub access_token: Box<[u8]>,
    /// Secret refresh credential; never included in debug output or observations.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub refresh_token: Option<Box<[u8]>>,
    /// Provider-reported token lifetime in seconds.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub expires_in: u64,
}

/// Bounded OAuth error body; classification is returned separately from diagnostic text.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OAuthError {
    /// Typed or byte-valued terminal classification in the enclosing boundary.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub code: Box<[u8]>,
    /// Bounded diagnostic text retained for this failure.
    ///
    /// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
    pub detail: Box<[u8]>,
}

/// Validates the typed request before allocating its bounded encoded body; errors return no partial request.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
pub fn encode_request(request: &RefreshRequest, limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    validate_request(request, limits)?;
    let mut measure = Encoder::measure(&limits.writer_limits());
    write_request(&mut measure, request);
    let len = common::measured(measure)?;
    let mut out = Encoder::write(len, &limits.writer_limits());
    write_request(&mut out, request);
    Ok(out.finish())
}

fn write_request(out: &mut Encoder, request: &RefreshRequest) {
    out.object_start();
    out.key(b"grant_type");
    out.string(b"refresh_token");
    out.key(b"client_id");
    out.string(&request.client_id);
    out.key(b"refresh_token");
    out.string(&request.refresh_token);
    out.object_end();
}

fn validate_request(request: &RefreshRequest, limits: &Limits) -> Result<(), DecodeError> {
    common::bounded(&request.client_id, limits.client_bytes)?;
    common::bounded(&request.refresh_token, limits.token_bytes)?;
    common::text(&request.refresh_token)
}

/// Decodes one complete bounded request document for the fake peer; rejects malformed or oversized members.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
pub fn decode_request(value: &Json, limits: &Limits) -> Result<RefreshRequest, DecodeError> {
    value.admit(limits)?;
    let tokens = value.as_tokens();
    if json::text_ref(json::value_at(tokens, json::required(tokens, b"grant_type")?)?)? != b"refresh_token" {
        return Err(DecodeError::Malformed);
    }
    let client = json::text_ref(json::value_at(tokens, json::required(tokens, b"client_id")?)?)?;
    let refresh = json::text_ref(json::value_at(tokens, json::required(tokens, b"refresh_token")?)?)?;
    common::bounded(client, limits.client_bytes)?;
    common::bounded(refresh, limits.token_bytes)?;
    Ok(RefreshRequest { client_id: bytes::copy_of(client), refresh_token: bytes::copy_of(refresh) })
}

/// Encodes the typed OAuth token response within limits; secret values remain excluded from diagnostics.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
pub fn encode_response(response: &TokenResponse, limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    validate_response(response, limits)?;
    let mut measure = Encoder::measure(&limits.writer_limits());
    write_response(&mut measure, response);
    let len = common::measured(measure)?;
    let mut out = Encoder::write(len, &limits.writer_limits());
    write_response(&mut out, response);
    Ok(out.finish())
}

fn write_response(out: &mut Encoder, response: &TokenResponse) {
    out.object_start();
    out.key(b"access_token");
    out.string(&response.access_token);
    out.key(b"token_type");
    out.string(b"Bearer");
    if let Some(refresh) = &response.refresh_token {
        out.key(b"refresh_token");
        out.string(refresh);
    }
    out.key(b"expires_in");
    out.unsigned(response.expires_in);
    out.object_end();
}

pub(crate) fn validate_response(response: &TokenResponse, limits: &Limits) -> Result<(), DecodeError> {
    common::bearer(&response.access_token, limits)?;
    if let Some(refresh) = &response.refresh_token {
        common::bounded(refresh, limits.token_bytes)?;
        common::text(refresh)?;
    }
    if response.expires_in == 0 {
        return Err(DecodeError::Malformed);
    }
    Ok(())
}

/// Decodes one bounded OAuth terminal, preserving optional refresh rotation and rejecting malformed lifetimes.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
pub fn decode_response(value: &Json, limits: &Limits) -> Result<TokenResponse, DecodeError> {
    value.admit(limits)?;
    let tokens = value.as_tokens();
    if let Some(token_type) = json::optional_at(tokens, json::field(tokens, b"token_type")?)?
        && !json::text_ref(token_type)?.eq_ignore_ascii_case(b"Bearer")
    {
        return Err(DecodeError::WrongType);
    }
    let access = json::text_ref(json::value_at(tokens, json::required(tokens, b"access_token")?)?)?;
    common::bearer(access, limits)?;
    let refresh_token = match json::optional_at(tokens, json::field(tokens, b"refresh_token")?)? {
        Some(value) => {
            let refresh = json::text_ref(value)?;
            common::bounded(refresh, limits.token_bytes)?;
            Some(bytes::copy_of(refresh))
        }
        None => None,
    };
    let response = TokenResponse {
        access_token: bytes::copy_of(access),
        refresh_token,
        expires_in: json::unsigned(json::value_at(tokens, json::required(tokens, b"expires_in")?)?)?,
    };
    validate_response(&response, limits)?;
    Ok(response)
}

/// Encodes a bounded typed peer-error body; no network or retry policy is executed.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
pub fn encode_error(error: &OAuthError, limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    common::bounded(&error.code, limits.detail_bytes)?;
    if error.detail.len() > usize::try_from(limits.detail_bytes).expect("u32 fits usize") {
        return Err(DecodeError::TooLarge);
    }
    let mut measure = Encoder::measure(&limits.writer_limits());
    write_error(&mut measure, error);
    let len = common::measured(measure)?;
    let mut out = Encoder::write(len, &limits.writer_limits());
    write_error(&mut out, error);
    Ok(out.finish())
}

fn write_error(out: &mut Encoder, error: &OAuthError) {
    out.object_start();
    out.key(b"error");
    out.string(&error.code);
    out.key(b"error_description");
    out.string(&error.detail);
    out.object_end();
}

/// Decodes bounded peer-error metadata; syntax, type and byte-cap failures remain typed.
///
/// Contract: domain/host.md, sections 7 and 11; programming-model.md, section 4.4.
pub fn decode_error(value: &Json, limits: &Limits) -> Result<OAuthError, DecodeError> {
    value.admit(limits)?;
    let tokens = value.as_tokens();
    let code = json::text_ref(json::value_at(tokens, json::required(tokens, b"error")?)?)?;
    common::bounded(code, limits.detail_bytes)?;
    let detail = match json::optional_at(tokens, json::field(tokens, b"error_description")?)? {
        Some(value) => common::clipped(json::text_ref(value)?, limits.detail_bytes),
        None => bytes::copy_of(b""),
    };
    Ok(OAuthError { code: bytes::copy_of(code), detail })
}
