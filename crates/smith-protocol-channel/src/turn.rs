//! Concrete turns become durable transcript bytes (protocol/transcript.md,
//! sections 2–4 and 8; domain/session.md, section 3).
//!
//! This translation keeps no state and knows only configured endpoint names.
//! It never sees credential values, addresses or host policy. `encode_turn`
//! preserves provider blocks and typed tool outcomes within the codec bounds.

use alloc::boxed::Box;
use skein_lib::{List, Writer};
use smith_domain_session::{llm, record};
use smith_domain_tools as tools;
use smith_transcript as wire;

use crate::{Endpoints, Error};

/// Encode one domain turn under its configured endpoint and replay dialect.
pub fn encode_turn(
    source: &record::Turn,
    limits: &wire::v3::Limits,
    endpoints: &Endpoints,
) -> Result<Box<[u8]>, Error> {
    if source.version != record::VERSION || source.sequence == 0 {
        return Err(Error::Transcript(record::Refusal::Version));
    }
    let endpoint = match endpoints.name_of(source.endpoint.0, source.dialect) {
        Some(name) => Box::from(name),
        None => return Err(Error::Transcript(record::Refusal::Endpoint)),
    };
    let dialect = Box::from(dialect_name(source.dialect));
    let mut messages = List::with_capacity(count(source.messages.len())?);
    for message in &source.messages {
        if messages.push(encode_message(message, source.dialect, limits)?).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    let usage = wire::Usage::new(
        limits,
        wire::UsageParts {
            input: source.usage.input_tokens,
            output: source.usage.output_tokens,
            cache_read: source.usage.cache_read_tokens,
            cache_write: source.usage.cache_write_tokens,
            reasoning: source.usage.reasoning_tokens,
        },
    )?;
    let turn = wire::Turn::new(
        limits,
        wire::TurnParts { endpoint, dialect, place: source.sequence, usage, spent: source.spent, messages },
    )?;
    let Ok(length) = usize::try_from(turn.measure()) else {
        return Err(Error::ResultCapacity);
    };
    let mut writer = Writer::new(length);
    turn.encode(&mut writer)?;
    Ok(writer.finish())
}

fn encode_message(source: &llm::Message, dialect: u32, limits: &wire::v3::Limits) -> Result<wire::Message, Error> {
    let role = match source.role {
        llm::Role::User => wire::Role::User,
        llm::Role::Assistant => wire::Role::Assistant,
    };
    let mut blocks = List::with_capacity(count(source.content.len())?);
    for block in &source.content {
        if blocks.push(encode_block(block, dialect, limits)?).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    Ok(wire::Message::new(limits, wire::MessageParts { role, blocks })?)
}

fn encode_block(source: &llm::Block, dialect: u32, limits: &wire::v3::Limits) -> Result<wire::Block, Error> {
    let block = match source {
        llm::Block::Text { text, replay } => wire::Block::Text(wire::Text::new(
            limits,
            wire::TextParts { text: text.clone(), replay: encode_replay(replay.as_ref(), dialect, limits)? },
        )?),
        llm::Block::Refusal { text, replay } => wire::Block::Refusal(wire::Text::new(
            limits,
            wire::TextParts { text: text.clone(), replay: encode_replay(replay.as_ref(), dialect, limits)? },
        )?),
        llm::Block::ToolCall { id, name, input, call: _, replay } => wire::Block::Call(wire::Call::new(
            limits,
            wire::CallParts {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
                replay: encode_replay(replay.as_ref(), dialect, limits)?,
            },
        )?),
        llm::Block::ToolResult { id, result } => wire::Block::Result(wire::ToolResult::new(
            limits,
            wire::ToolResultParts { id: id.clone(), returned: encode_returned(result, limits)? },
        )?),
        llm::Block::Opaque { bytes } => wire::Block::Opaque(wire::Opaque::new(
            limits,
            wire::OpaqueParts { dialect: Box::from(dialect_name(dialect)), bytes: bytes.clone() },
        )?),
    };
    Ok(block)
}

fn encode_replay(
    source: Option<&llm::Replay>,
    dialect: u32,
    limits: &wire::v3::Limits,
) -> Result<Option<wire::Replay>, Error> {
    match source {
        Some(replay) => Ok(Some(wire::Replay::new(
            limits,
            wire::ReplayParts { dialect: Box::from(dialect_name(dialect)), bytes: replay.bytes.clone() },
        )?)),
        None => Ok(None),
    }
}

fn encode_returned(source: &llm::Returned, limits: &wire::v3::Limits) -> Result<wire::Returned, Error> {
    let returned = match source {
        llm::Returned::Owned { outcome } => wire::Returned::Outcome(wire::OwnedOutcome::new(
            limits,
            wire::OwnedOutcomeParts { value: encode_outcome(outcome, limits)? },
        )?),
        llm::Returned::Invalid { problem } => wire::Returned::Invalid(wire::Invalid::new(
            limits,
            wire::InvalidParts { problem: encode_problem(problem, limits)? },
        )?),
        llm::Returned::NotRun => wire::Returned::NotRun,
        llm::Returned::Text { text, error, replay } => {
            if replay.is_some() {
                return Err(Error::Transcript(record::Refusal::Unresolved));
            }
            wire::Returned::Said(wire::Said::new(limits, wire::SaidParts { text: text.clone(), error: *error })?)
        }
        llm::Returned::Withdrawn => wire::Returned::Withdrawn,
    };
    Ok(returned)
}

fn encode_problem(source: &llm::Problem, limits: &wire::v3::Limits) -> Result<wire::CallProblem, Error> {
    let problem = match source {
        llm::Problem::UnknownTool => wire::CallProblem::UnknownTool,
        llm::Problem::NotAnObject => wire::CallProblem::NotAnObject,
        llm::Problem::Missing { field } => wire::CallProblem::Missing(wire::FieldProblem::new(
            limits,
            wire::FieldProblemParts { field: field.clone() },
        )?),
        llm::Problem::WrongType { field } => wire::CallProblem::WrongType(wire::FieldProblem::new(
            limits,
            wire::FieldProblemParts { field: field.clone() },
        )?),
        llm::Problem::BadValue { field } => wire::CallProblem::BadValue(wire::FieldProblem::new(
            limits,
            wire::FieldProblemParts { field: field.clone() },
        )?),
        llm::Problem::TooLarge => wire::CallProblem::TooLarge,
    };
    Ok(problem)
}

#[expect(clippy::too_many_lines, reason = "every typed tool outcome has an explicit durable representation")]
fn encode_outcome(source: &tools::Outcome, limits: &wire::v3::Limits) -> Result<wire::Outcome, Error> {
    let outcome = match source {
        tools::Outcome::Read { content, skipped, lines, total, cut } => wire::Outcome::Read(wire::Read::new(
            limits,
            wire::ReadParts { content: content.clone(), skipped: *skipped, lines: *lines, total: *total, cut: *cut },
        )?),
        tools::Outcome::Listed { entries, more } => {
            let mut output = List::with_capacity(count(entries.len())?);
            for entry in entries {
                let kind = match entry.kind {
                    tools::Kind::File => wire::Kind::File,
                    tools::Kind::Directory => wire::Kind::Directory,
                    tools::Kind::Link => wire::Kind::Link,
                    tools::Kind::Other => wire::Kind::Other,
                };
                if output
                    .push(wire::Entry::new(limits, wire::EntryParts { name: Box::from(entry.name.as_bytes()), kind })?)
                    .is_err()
                {
                    return Err(Error::ResultCapacity);
                }
            }
            wire::Outcome::Listed(wire::Listed::new(limits, wire::ListedParts { entries: output, more: *more })?)
        }
        tools::Outcome::Found { hits, more, timed_out } => {
            let mut output = List::with_capacity(count(hits.len())?);
            for hit in hits {
                if output
                    .push(wire::Hit::new(
                        limits,
                        wire::HitParts { path: hit.path.clone(), line: hit.line, text: hit.text.clone() },
                    )?)
                    .is_err()
                {
                    return Err(Error::ResultCapacity);
                }
            }
            wire::Outcome::Found(wire::Found::new(
                limits,
                wire::FoundParts { hits: output, more: *more, timed_out: *timed_out },
            )?)
        }
        tools::Outcome::Written { created } => {
            wire::Outcome::Written(wire::Written::new(limits, wire::WrittenParts { created: *created })?)
        }
        tools::Outcome::Edited { replaced } => {
            wire::Outcome::Edited(wire::Edited::new(limits, wire::EditedParts { replaced: *replaced })?)
        }
        tools::Outcome::Exited { exit, head, tail, dropped } => {
            let exit = match exit {
                tools::Exit::Code { code } => {
                    wire::Exit::Code(wire::ExitCode::new(limits, wire::ExitCodeParts { code: *code })?)
                }
                tools::Exit::Signal { signal } => {
                    wire::Exit::Signal(wire::ExitSignal::new(limits, wire::ExitSignalParts { signal: *signal })?)
                }
                tools::Exit::TimedOut => wire::Exit::TimedOut,
            };
            wire::Outcome::Exited(wire::CommandEnd::new(
                limits,
                wire::CommandEndParts { exit, head: head.clone(), last: tail.clone(), dropped: *dropped },
            )?)
        }
        tools::Outcome::NotGranted => wire::Outcome::NotGranted,
        tools::Outcome::Outside => wire::Outcome::Outside,
        tools::Outcome::ReadOnly => wire::Outcome::ReadOnly,
        tools::Outcome::TooLong => wire::Outcome::TooLong,
        tools::Outcome::NotFound => wire::Outcome::NotFound,
        tools::Outcome::NotFile => wire::Outcome::NotFile,
        tools::Outcome::Linked => wire::Outcome::Linked,
        tools::Outcome::Protected => wire::Outcome::Protected,
        tools::Outcome::NotDirectory => wire::Outcome::NotDirectory,
        tools::Outcome::TooLarge { size } => {
            wire::Outcome::TooLarge(wire::TooLarge::new(limits, wire::TooLargeParts { size: *size })?)
        }
        tools::Outcome::NotRead => wire::Outcome::NotRead,
        tools::Outcome::Stale => wire::Outcome::Stale,
        tools::Outcome::NoMatch => wire::Outcome::NoMatch,
        tools::Outcome::Ambiguous { count: found, lines } => {
            let mut output = List::with_capacity(count(lines.len())?);
            for line in lines {
                if output.push(wire::LineNumber::new(limits, wire::LineNumberParts { number: *line })?).is_err() {
                    return Err(Error::ResultCapacity);
                }
            }
            wire::Outcome::Ambiguous(wire::Ambiguous::new(
                limits,
                wire::AmbiguousParts { count: *found, lines: output },
            )?)
        }
        tools::Outcome::Unchanged => wire::Outcome::Unchanged,
        tools::Outcome::Failed { fault } => {
            let fault = match fault {
                tools::Fault::Denied => wire::Fault::Denied,
                tools::Fault::NoSpace => wire::Fault::NoSpace,
                tools::Fault::Other => wire::Fault::Other,
            };
            wire::Outcome::Failed(wire::Failed::new(limits, wire::FailedParts { fault })?)
        }
        tools::Outcome::TimedOut => wire::Outcome::TimedOut,
        tools::Outcome::Cancelled => wire::Outcome::Cancelled,
        tools::Outcome::Busy => wire::Outcome::Busy,
        tools::Outcome::NulByte => wire::Outcome::NulByte,
    };
    Ok(outcome)
}

fn count(length: usize) -> Result<u32, Error> {
    match u32::try_from(length) {
        Ok(count) => Ok(count),
        Err(_) => Err(Error::ResultCapacity),
    }
}

fn dialect_name(dialect: u32) -> [u8; 8] {
    let [a, b, c, d] = dialect.to_be_bytes();
    [hex(a >> 4), hex(a & 15), hex(b >> 4), hex(b & 15), hex(c >> 4), hex(c & 15), hex(d >> 4), hex(d & 15)]
}

fn hex(value: u8) -> u8 {
    if value < 10 {
        b'0'.checked_add(value).expect("hex digit")
    } else {
        b'a'.checked_add(value.checked_sub(10).expect("hex letter")).expect("hex digit")
    }
}
