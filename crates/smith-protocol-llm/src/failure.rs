//! Lossless neutral failure/evidence and bounded mechanical invalid-input feedback.
//! No diagnostic wording participates in application decisions.

use alloc::boxed::Box;

use skein_lib::{Decimal, Token, Writer, bytes};
use skein_llm::{self as shared, client};
use smith_domain::{Event, llm};

use crate::{Context, Error};

/// Moves one actual shared Failed terminal into the root boundary unchanged in meaning.
/// Detail is exact under the secured receiving cap; the policy consumes/drops it
/// after observation and retains only typed class/evidence in facts and End.
/// Incorrect owner or over-cap diagnostic is a sender contract refusal.
/// The actual Client remains owned until its independent lower Closed terminal.
pub fn failed(
    context: Context,
    owner: Token,
    failure: shared::Failure,
    evidence: client::Evidence,
    detail: Box<[u8]>,
) -> Result<Event, Error> {
    if owner != context.owner {
        return Err(Error::Invalid);
    }
    if detail.len() > usize::try_from(context.receiving.max_failure_bytes).expect("u32 fits usize") {
        return Err(Error::Limit);
    }
    let failure = match failure {
        shared::Failure::Overloaded => llm::Failure::Overloaded,
        shared::Failure::RateLimited { retry_after } => llm::Failure::RateLimited { retry_after },
        shared::Failure::Unavailable => llm::Failure::Unavailable,
        shared::Failure::TimedOut => llm::Failure::TimedOut,
        shared::Failure::ContextTooLong => llm::Failure::ContextTooLong,
        shared::Failure::Invalid => llm::Failure::Invalid,
        shared::Failure::Unauthorized => llm::Failure::Unauthorized,
        shared::Failure::Exhausted { retry_after } => llm::Failure::Exhausted { retry_after },
        shared::Failure::Limit { which: _, bound: _ } => llm::Failure::Limit,
        shared::Failure::Protocol => llm::Failure::Protocol,
        shared::Failure::Cancelled => llm::Failure::Cancelled,
    };
    let evidence = match evidence {
        client::Evidence::Unsent => llm::Evidence::Unsent,
        client::Evidence::Unknown => llm::Evidence::Unknown,
        client::Evidence::Response { status: _ } => llm::Evidence::Response,
    };
    Ok(Event::Failed { owner, failure, evidence, detail })
}

/// Translates the actual shared cancellation terminal after real lower settlement.
/// A cancellation request alone never permits calling this function; already
/// won Completed/Failed terminals keep their values and consume context instead.
/// An incorrect callback owner is Invalid and confers no terminal authority.
pub fn cancelled(context: Context, owner: Token) -> Result<Event, Error> {
    if owner == context.owner { Ok(Event::Cancelled { owner }) } else { Err(Error::Invalid) }
}

/// Reports a preparation refusal as an unsent root terminal; no wire call existed.
/// Unsupported is explicitly Invalid, with its distinct bounded diagnostic.
/// Fixed local ASCII detail is clipped only to the receiving diagnostic allowance.
#[must_use]
pub fn refusal(owner: Token, error: Error, max_failure_bytes: u32) -> Event {
    let (failure, text): (llm::Failure, &[u8]) = match error {
        Error::Limit => (llm::Failure::Limit, b"shared Client preparation exceeds receiving limits"),
        Error::Invalid => (llm::Failure::Invalid, b"invalid application prompt or configured call"),
        Error::Unsupported => (llm::Failure::Invalid, b"unsupported replay or configured call"),
    };
    let length = text.len().min(usize::try_from(max_failure_bytes).expect("u32 fits usize"));
    let detail = bytes::copy_of(text.get(..length).expect("clipped fixed local diagnostic"));
    Event::Failed { owner, failure, evidence: llm::Evidence::Unsent, detail }
}

pub(crate) fn problem(problem: llm::Problem, maximum: u32) -> Result<Box<[u8]>, Error> {
    match problem {
        llm::Problem::Oversize { bytes, bound } => return call_size_problem(bytes, Some(bound), maximum),
        llm::Problem::CutOff { bytes } => return call_size_problem(bytes, None, maximum),
        llm::Problem::UnknownTool
        | llm::Problem::NotAnObject
        | llm::Problem::Missing { .. }
        | llm::Problem::WrongType { .. }
        | llm::Problem::BadValue { .. }
        | llm::Problem::TooLarge => {}
    }
    let (prefix, field): (&[u8], Option<Box<[u8]>>) = match problem {
        llm::Problem::UnknownTool => (b"unknown tool", None),
        llm::Problem::NotAnObject => (b"input is not a complete JSON object", None),
        llm::Problem::Missing { field } => (b"missing field ", Some(field)),
        llm::Problem::WrongType { field } => (b"wrong field type ", Some(field)),
        llm::Problem::BadValue { field } => (b"invalid field value ", Some(field)),
        llm::Problem::TooLarge => (b"input exceeds receiving limits", None),
        llm::Problem::Oversize { .. } | llm::Problem::CutOff { .. } => unreachable!("size notes rendered above"),
    };
    let mut length = prefix.len();
    if let Some(field) = &field {
        length = length.checked_add(2).ok_or(Error::Limit)?;
        for byte in field {
            let width = if printable(*byte) { 1_usize } else { 4 };
            length = length.checked_add(width).ok_or(Error::Limit)?;
        }
    }
    if length > usize::try_from(maximum).expect("u32 fits usize") {
        return Err(Error::Limit);
    }
    let mut output = Writer::new(length);
    output.put(prefix).expect("measured invalid-input prefix");
    if let Some(field) = field {
        output.put(b"\"").expect("measured opening quote");
        for byte in field {
            if printable(byte) {
                output.put(&[byte]).expect("measured literal field byte");
            } else {
                let high = hex(byte >> 4_u8);
                let low = hex(byte & 15_u8);
                output.put(&[b'\\', b'x', high, low]).expect("measured escaped field byte");
            }
        }
        output.put(b"\"").expect("measured closing quote");
    }
    Ok(output.finish())
}

fn printable(byte: u8) -> bool {
    (32..=126).contains(&byte) && byte != b'"' && byte != b'\\'
}

fn hex(value: u8) -> u8 {
    *b"0123456789abcdef".get(usize::from(value)).expect("four-bit escaped field byte")
}

fn call_size_problem(bytes: u64, bound: Option<u32>, maximum: u32) -> Result<Box<[u8]>, Error> {
    let size = Decimal::of(bytes);
    let bound = bound.map(u64::from).map(Decimal::of);
    let (prefix, suffix): (&[u8], &[u8]) = match bound {
        Some(_) => (b"input too large: ", b" bytes; make a smaller write or an edit"),
        None => (b"input cut off after ", b" bytes; make a smaller call and try again"),
    };
    let mut length = prefix.len().checked_add(size.as_bytes().len()).ok_or(Error::Limit)?;
    if let Some(bound) = &bound {
        length = length.checked_add(b" bytes, bound ".len()).ok_or(Error::Limit)?;
        length = length.checked_add(bound.as_bytes().len()).ok_or(Error::Limit)?;
    }
    length = length.checked_add(suffix.len()).ok_or(Error::Limit)?;
    if length > usize::try_from(maximum).expect("u32 fits usize") {
        return Err(Error::Limit);
    }
    let mut output = Writer::new(length);
    output.put(prefix).expect("measured note");
    output.put(size.as_bytes()).expect("measured size");
    if let Some(bound) = bound {
        output.put(b" bytes, bound ").expect("measured unit");
        output.put(bound.as_bytes()).expect("measured bound");
    }
    output.put(suffix).expect("measured remedy");
    Ok(output.finish())
}
