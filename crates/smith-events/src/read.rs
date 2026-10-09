//! Bounded JSON-line decoding with unknown fields and records skipped.
//! Contract: protocol/events.md, section 6. No stream state is retained.

use crate::records::{
    Agent, AnswerClass, AnswerFailure, Block, Budget, Capture, CheckCompleted, CheckStarted, CommandExit, Completion,
    CompletionFailure, ConversationClosed, ConversationEnd, ConversationKind, ConversationOpened, Count, Cpu,
    Delivered, Delivery, Effect, Event, Evidence, FailureClass, Family, Field, Form, Level, Loss, Message, Mode, Model,
    Notice, NoticeKind, Outcome, PeakRss, Prompt, Record, ResponseCompleted, ResponseStarted, Role, RunCompleted,
    RunResult, RunStarted, SessionEnded, SessionStarted, Source, Status, Stop, TextDelta, ToolCompleted, ToolStarted,
    Tools, Usage, Verdict, Versions,
};
use crate::{Error, Limits};

/// A bounded line accumulator supplied chunks by a stream consumer.
#[derive(Debug)]
pub struct Reader {
    limits: Limits,
    line: List<u8>,
}

/// One read's consumed bytes and optional record; unknown records still complete a line.
#[derive(Debug)]
pub struct Line {
    pub consumed: usize,
    pub complete: bool,
    pub record: Option<Record>,
}

impl Reader {
    /// Reserve one largest line before accepting bytes from a stream.
    pub fn new(limits: &Limits) -> Result<Reader, Error> {
        let capacity = crate::largest_record(limits).ok_or(Error::Limits)?;
        Ok(Reader { limits: *limits, line: List::with_capacity(capacity) })
    }

    /// Consume through at most one newline, preserving a partial line between calls.
    pub fn feed(&mut self, input: &[u8]) -> Result<Line, Error> {
        let mut consumed = 0_usize;
        for byte in input {
            consumed = consumed.checked_add(1).ok_or(Error::Limits)?;
            if *byte == b'\n' {
                let result = read(self.line.as_slice(), &self.limits);
                self.line.clear();
                return Ok(Line { consumed, complete: true, record: result? });
            }
            if self.line.push(*byte).is_err() {
                self.line.clear();
                return Err(Error::Limits);
            }
        }
        Ok(Line { consumed, complete: false, record: None })
    }

    /// End a stream only between newline-terminated records.
    pub fn finish(&self) -> Result<(), Error> {
        if self.line.is_empty() { Ok(()) } else { Err(Error::Malformed) }
    }
}
use alloc::boxed::Box;
use skein_json::{Token, tokenizer};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Env, List, Queue, Time, Wall, bytes};

/// Read one JSON line. Unknown record types return `None`; every version is checked.
pub fn read(line: &[u8], limits: &Limits) -> Result<Option<Record>, Error> {
    let tokens = tokenize(line, limits)?;
    let tokens = tokens.as_slice();
    object(tokens)?;
    let version = unsigned(required(tokens, b"v")?)?;
    if version != crate::VERSION {
        return Err(Error::Version(version));
    }
    let kind = text(required(tokens, b"type")?, limits.string)?;
    let t_ms = unsigned(required(tokens, b"t_ms")?)?;
    let event = match kind.as_ref() {
        b"session.started" => Event::SessionStarted(read_session_started(tokens, limits)?),
        b"session.ended" => Event::SessionEnded(read_session_ended(tokens, limits)?),
        b"run.started" => Event::RunStarted(read_run_started(tokens, limits)?),
        b"run.completed" => Event::RunCompleted(read_run_completed(tokens, limits)?),
        b"conversation.opened" => Event::ConversationOpened(read_conversation_opened(tokens, limits)?),
        b"conversation.closed" => Event::ConversationClosed(read_conversation_closed(tokens, limits)?),
        b"response.started" => Event::ResponseStarted(read_response_started(tokens, limits)?),
        b"response.completed" => Event::ResponseCompleted(read_response_completed(tokens, limits)?),
        b"text.delta" => Event::TextDelta(read_text_delta(tokens, limits)?),
        b"tool.started" => Event::ToolStarted(read_tool_started(tokens, limits)?),
        b"tool.completed" => Event::ToolCompleted(read_tool_completed(tokens, limits)?),
        b"check.started" => Event::CheckStarted(read_check_started(tokens, limits)?),
        b"check.completed" => Event::CheckCompleted(read_check_completed(tokens, limits)?),
        b"notice" => Event::Notice(read_notice(tokens, limits)?),
        _ => return Ok(None),
    };
    Ok(Some(Record { t_ms, event }))
}

fn tokenize(input: &[u8], limits: &Limits) -> Result<List<Token>, Error> {
    let cap = crate::limits::largest_record(limits).ok_or(Error::Limits)?;
    if input.len() > usize::try_from(cap).expect("u32 fits") {
        return Err(Error::Limits);
    }
    let env = Env {
        now: Time::ZERO,
        wall: Wall::EPOCH,
        limits: tokenizer::Limits {
            depth: 16,
            string: limits.string.max(limits.content).max(64),
            number: 20,
            chunk: 256,
            length: cap,
        },
    };
    let mut machine = tokenizer::Tokenizer::new(&env.limits);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let Ok(token_cap) = u32::try_from(input.len()) else {
        return Err(Error::Limits);
    };
    let mut tokens = List::with_capacity(token_cap);
    let mut at: usize = 0;
    let ticks = input.len().checked_mul(4).ok_or(Error::Limits)?.checked_add(16).ok_or(Error::Limits)?;
    for _tick in 0..ticks {
        match machine.waiting() {
            tokenizer::Waiting::Next => {
                tokenizer::down(&mut machine, &env, tokenizer::Request::Next, &mut above, &mut below);
            }
            tokenizer::Waiting::Bytes => {
                let demand = below.pop().ok_or(Error::Malformed)?;
                let read = match demand {
                    Down::Demand { read, .. } => read,
                    Down::Send(_) | Down::Finish => return Err(Error::Malformed),
                };
                let remaining = input.get(at..).ok_or(Error::Malformed)?;
                let count = match read {
                    Read::Fill(count) => usize::try_from(count).expect("u32 fits"),
                    Read::Scan { until, max } => {
                        let max = usize::try_from(max).expect("u32 fits");
                        let scanned = remaining.get(..remaining.len().min(max)).ok_or(Error::Malformed)?;
                        match bytes::find(scanned, until.as_bytes()) {
                            Some(position) => position.checked_add(until.as_bytes().len()).ok_or(Error::Limits)?,
                            None => max,
                        }
                    }
                    Read::Nothing | Read::Line { .. } => return Err(Error::Malformed),
                };
                if count <= remaining.len() {
                    let delivery = bytes::copy_of(remaining.get(..count).ok_or(Error::Malformed)?);
                    at = at.checked_add(count).ok_or(Error::Limits)?;
                    tokenizer::up(&mut machine, &env, Up::Bytes(delivery), &mut above, &mut below);
                } else {
                    tokenizer::up(&mut machine, &env, Up::End, &mut above, &mut below);
                }
            }
            tokenizer::Waiting::Close | tokenizer::Waiting::Nothing => return Err(Error::Malformed),
        }
        if let Some(event) = above.pop() {
            match event {
                tokenizer::Event::Token(token) => match tokens.push(token) {
                    Ok(()) => {}
                    Err(_) => return Err(Error::Limits),
                },
                tokenizer::Event::Long(_) | tokenizer::Event::Skipped(_) => {
                    unreachable!("the event reader requests only Next tokens")
                }
                tokenizer::Event::Done => return Ok(tokens),
                tokenizer::Event::Failed(_) | tokenizer::Event::Closed => return Err(Error::Malformed),
            }
        }
    }
    Err(Error::Malformed)
}

fn object(tokens: &[Token]) -> Result<(), Error> {
    if tokens.first() != Some(&Token::ObjectStart) || tokens.last() != Some(&Token::ObjectEnd) {
        return Err(Error::Shape);
    }
    Ok(())
}

fn value_end(tokens: &[Token], start: usize) -> Result<usize, Error> {
    let mut depth = 0_u32;
    for position in start..tokens.len() {
        match tokens.get(position).ok_or(Error::Shape)? {
            Token::ObjectStart | Token::ArrayStart => depth = depth.checked_add(1).ok_or(Error::Limits)?,
            Token::ObjectEnd | Token::ArrayEnd => {
                depth = depth.checked_sub(1).ok_or(Error::Shape)?;
                if depth == 0 {
                    return position.checked_add(1).ok_or(Error::Limits);
                }
            }
            Token::Key(_) => {}
            Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null => {
                if depth == 0 {
                    return position.checked_add(1).ok_or(Error::Limits);
                }
            }
        }
    }
    Err(Error::Shape)
}

fn field<'a>(tokens: &'a [Token], name: &[u8]) -> Result<Option<&'a [Token]>, Error> {
    object(tokens)?;
    let mut at = 1_usize;
    let mut found = None;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let key = match tokens.get(at).ok_or(Error::Shape)? {
            Token::Key(key) => key,
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::String(_)
            | Token::Number(_)
            | Token::True
            | Token::False
            | Token::Null => return Err(Error::Shape),
        };
        let start = at.checked_add(1).ok_or(Error::Limits)?;
        at = value_end(tokens, start)?;
        if key.as_ref() == name {
            if found.is_some() {
                return Err(Error::Shape);
            }
            found = Some(tokens.get(start..at).ok_or(Error::Shape)?);
        }
    }
    Ok(found)
}

fn required<'a>(tokens: &'a [Token], name: &[u8]) -> Result<&'a [Token], Error> {
    field(tokens, name)?.ok_or(Error::Shape)
}

fn unsigned(tokens: &[Token]) -> Result<u64, Error> {
    match tokens {
        [Token::Number(value)] => digits(value),
        _ => Err(Error::Shape),
    }
}

fn digits(value: &[u8]) -> Result<u64, Error> {
    if value.is_empty() {
        return Err(Error::Shape);
    }
    let mut number = 0_u64;
    for byte in value {
        if !byte.is_ascii_digit() {
            return Err(Error::Shape);
        }
        number = number
            .checked_mul(10)
            .ok_or(Error::Shape)?
            .checked_add(u64::from(byte.saturating_sub(b'0')))
            .ok_or(Error::Shape)?;
    }
    Ok(number)
}

fn signed(tokens: &[Token]) -> Result<i64, Error> {
    match tokens {
        [Token::Number(value)] => {
            let negative = value.first() == Some(&b'-');
            let magnitude = if negative { digits(value.get(1..).ok_or(Error::Shape)?)? } else { digits(value)? };
            if negative && magnitude == i64::MIN.unsigned_abs() {
                return Ok(i64::MIN);
            }
            let Ok(number) = i64::try_from(magnitude) else {
                return Err(Error::Shape);
            };
            if negative { number.checked_neg().ok_or(Error::Shape) } else { Ok(number) }
        }
        _ => Err(Error::Shape),
    }
}

fn boolean(tokens: &[Token]) -> Result<bool, Error> {
    match tokens {
        [Token::True] => Ok(true),
        [Token::False] => Ok(false),
        _ => Err(Error::Shape),
    }
}

fn text(tokens: &[Token], capacity: u32) -> Result<Box<[u8]>, Error> {
    match tokens {
        [Token::String(value)] if value.len() <= usize::try_from(capacity).expect("u32 fits") => Ok(value.clone()),
        _ => Err(Error::Shape),
    }
}
fn read_capture(tokens: &[Token], limits: &Limits) -> Result<Capture, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"none" => Capture::None,
        b"calls" => Capture::Calls,
        b"everything" => Capture::Everything,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Capture::Unknown(value)
        }
    })
}

fn read_delivery(tokens: &[Token], limits: &Limits) -> Result<Delivery, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"complete" => Delivery::Complete,
        b"best_effort" => Delivery::BestEffort,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Delivery::Unknown(value)
        }
    })
}

fn read_mode(tokens: &[Token], limits: &Limits) -> Result<Mode, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"agent" => Mode::Agent,
        b"exec" => Mode::Exec,
        b"chat" => Mode::Chat,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Mode::Unknown(value)
        }
    })
}

fn read_family(tokens: &[Token], limits: &Limits) -> Result<Family, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"inspect" => Family::Inspect,
        b"modify" => Family::Modify,
        b"shell" => Family::Shell,
        b"sub_agents" => Family::SubAgents,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Family::Unknown(value)
        }
    })
}

fn read_form(tokens: &[Token], limits: &Limits) -> Result<Form, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"report" => Form::Report,
        b"verdict" => Form::Verdict,
        b"change" => Form::Change,
        b"failure" => Form::Failure,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Form::Unknown(value)
        }
    })
}

fn read_status(tokens: &[Token], limits: &Limits) -> Result<Status, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"accepted" => Status::Accepted,
        b"parked" => Status::Parked,
        b"failed" => Status::Failed,
        b"refused" => Status::Refused,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Status::Unknown(value)
        }
    })
}

fn read_conversation_kind(tokens: &[Token], limits: &Limits) -> Result<ConversationKind, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"main" => ConversationKind::Main,
        b"child" => ConversationKind::Child,
        b"compaction" => ConversationKind::Compaction,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            ConversationKind::Unknown(value)
        }
    })
}

fn read_conversation_end(tokens: &[Token], limits: &Limits) -> Result<ConversationEnd, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"closed" => ConversationEnd::Closed,
        b"budget" => ConversationEnd::Budget,
        b"failed" => ConversationEnd::Failed,
        b"refused" => ConversationEnd::Refused,
        b"transcript" => ConversationEnd::Transcript,
        b"overflow" => ConversationEnd::Overflow,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            ConversationEnd::Unknown(value)
        }
    })
}

fn read_outcome(tokens: &[Token], limits: &Limits) -> Result<Outcome, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"completed" => Outcome::Completed,
        b"failed" => Outcome::Failed,
        b"cancelled" => Outcome::Cancelled,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Outcome::Unknown(value)
        }
    })
}

fn read_stop(tokens: &[Token], limits: &Limits) -> Result<Stop, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"end" => Stop::End,
        b"tools" => Stop::Tools,
        b"max_tokens" => Stop::MaxTokens,
        b"refusal" => Stop::Refusal,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Stop::Unknown(value)
        }
    })
}

fn read_source(tokens: &[Token], limits: &Limits) -> Result<Source, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"workspace" => Source::Workspace,
        b"run" => Source::Run,
        b"host" => Source::Host,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Source::Unknown(value)
        }
    })
}

fn read_effect(tokens: &[Token], limits: &Limits) -> Result<Effect, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"read" => Effect::Read,
        b"write" => Effect::Write,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Effect::Unknown(value)
        }
    })
}

fn read_verdict(tokens: &[Token], limits: &Limits) -> Result<Verdict, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"read" => Verdict::Read,
        b"listed" => Verdict::Listed,
        b"found" => Verdict::Found,
        b"written" => Verdict::Written,
        b"edited" => Verdict::Edited,
        b"exited" => Verdict::Exited,
        b"conflict" => Verdict::Conflict,
        b"missing" => Verdict::Missing,
        b"too_large" => Verdict::TooLarge,
        b"timed_out" => Verdict::TimedOut,
        b"failed" => Verdict::Failed,
        b"cancelled" => Verdict::Cancelled,
        b"ambiguous" => Verdict::Ambiguous,
        b"result" => Verdict::Result,
        b"error" => Verdict::Error,
        b"invalid" => Verdict::Invalid,
        b"not_run" => Verdict::NotRun,
        b"withdrawn" => Verdict::Withdrawn,
        b"not_granted" => Verdict::NotGranted,
        b"outside" => Verdict::Outside,
        b"read_only" => Verdict::ReadOnly,
        b"too_long" => Verdict::TooLong,
        b"not_found" => Verdict::NotFound,
        b"not_file" => Verdict::NotFile,
        b"linked" => Verdict::Linked,
        b"protected" => Verdict::Protected,
        b"not_directory" => Verdict::NotDirectory,
        b"not_read" => Verdict::NotRead,
        b"stale" => Verdict::Stale,
        b"no_match" => Verdict::NoMatch,
        b"unchanged" => Verdict::Unchanged,
        b"busy" => Verdict::Busy,
        b"nul_byte" => Verdict::NulByte,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Verdict::Unknown(value)
        }
    })
}

fn read_delivered(tokens: &[Token], limits: &Limits) -> Result<Delivered, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"delivered" => Delivered::Delivered,
        b"nothing" => Delivered::Nothing,
        b"refused" => Delivered::Refused,
        b"failed" => Delivered::Failed,
        b"stale" => Delivered::Stale,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Delivered::Unknown(value)
        }
    })
}

fn read_failure_class(tokens: &[Token], limits: &Limits) -> Result<FailureClass, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"overloaded" => FailureClass::Overloaded,
        b"rate_limited" => FailureClass::RateLimited,
        b"exhausted" => FailureClass::Exhausted,
        b"unavailable" => FailureClass::Unavailable,
        b"timed_out" => FailureClass::TimedOut,
        b"context_too_long" => FailureClass::ContextTooLong,
        b"invalid" => FailureClass::Invalid,
        b"unauthorized" => FailureClass::Unauthorized,
        b"limit" => FailureClass::Limit,
        b"protocol" => FailureClass::Protocol,
        b"cancelled" => FailureClass::Cancelled,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            FailureClass::Unknown(value)
        }
    })
}

fn read_evidence(tokens: &[Token], limits: &Limits) -> Result<Evidence, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"unsent" => Evidence::Unsent,
        b"maybe_sent" => Evidence::MaybeSent,
        b"response" => Evidence::Response,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Evidence::Unknown(value)
        }
    })
}

fn read_answer_class(tokens: &[Token], limits: &Limits) -> Result<AnswerClass, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"model" => AnswerClass::Model,
        b"budget" => AnswerClass::Budget,
        b"policy" => AnswerClass::Policy,
        b"cancelled" => AnswerClass::Cancelled,
        b"stale" => AnswerClass::Stale,
        b"transcript" => AnswerClass::Transcript,
        b"busy" => AnswerClass::Busy,
        b"invalid" => AnswerClass::Invalid,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            AnswerClass::Unknown(value)
        }
    })
}

fn read_level(tokens: &[Token], limits: &Limits) -> Result<Level, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"info" => Level::Info,
        b"warning" => Level::Warning,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Level::Unknown(value)
        }
    })
}

fn read_notice_kind(tokens: &[Token], limits: &Limits) -> Result<NoticeKind, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"reasoning_dropped" => NoticeKind::ReasoningDropped,
        b"credential_rejected" => NoticeKind::CredentialRejected,
        b"account_exhausted" => NoticeKind::AccountExhausted,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            NoticeKind::Unknown(value)
        }
    })
}

fn read_role(tokens: &[Token], limits: &Limits) -> Result<Role, Error> {
    let value = text(tokens, limits.string.max(64))?;
    Ok(match value.as_ref() {
        b"user" => Role::User,
        b"assistant" => Role::Assistant,
        _ => {
            if value.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Role::Unknown(value)
        }
    })
}

fn read_agent(tokens: &[Token], limits: &Limits) -> Result<Agent, Error> {
    object(tokens)?;
    Ok(Agent {
        name: text(required(tokens, b"name")?, limits.string)?,
        version: text(required(tokens, b"version")?, limits.string)?,
        build: text(required(tokens, b"build")?, limits.string)?,
    })
}

fn read_versions(tokens: &[Token], _limits: &Limits) -> Result<Versions, Error> {
    object(tokens)?;
    Ok(Versions {
        events: unsigned(required(tokens, b"events")?)?,
        channel: unsigned(required(tokens, b"channel")?)?,
        charter: unsigned(required(tokens, b"charter")?)?,
        transcript: unsigned(required(tokens, b"transcript")?)?,
    })
}

fn read_loss(tokens: &[Token], _limits: &Limits) -> Result<Loss, Error> {
    object(tokens)?;
    Ok(Loss {
        events: unsigned(required(tokens, b"events")?)?,
        channel: match required(tokens, b"channel")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
    })
}

fn read_cpu(tokens: &[Token], _limits: &Limits) -> Result<Cpu, Error> {
    object(tokens)?;
    Ok(Cpu {
        user: unsigned(required(tokens, b"user")?)?,
        system: unsigned(required(tokens, b"system")?)?,
        children_user: unsigned(required(tokens, b"children_user")?)?,
        children_system: unsigned(required(tokens, b"children_system")?)?,
    })
}

fn read_peak_rss(tokens: &[Token], _limits: &Limits) -> Result<PeakRss, Error> {
    object(tokens)?;
    Ok(PeakRss {
        self_bytes: unsigned(required(tokens, b"self")?)?,
        children: unsigned(required(tokens, b"children")?)?,
    })
}

fn read_model(tokens: &[Token], limits: &Limits) -> Result<Model, Error> {
    object(tokens)?;
    Ok(Model {
        endpoint: text(required(tokens, b"endpoint")?, limits.string)?,
        model: text(required(tokens, b"model")?, limits.string)?,
        output_tokens: unsigned(required(tokens, b"output_tokens")?)?,
        effort: match required(tokens, b"effort")? {
            [Token::Null] => None,
            item => Some(text(item, limits.string)?),
        },
    })
}

fn read_usage(tokens: &[Token], _limits: &Limits) -> Result<Usage, Error> {
    object(tokens)?;
    Ok(Usage {
        input_tokens: match required(tokens, b"input_tokens")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        cache_read_tokens: match required(tokens, b"cache_read_tokens")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        cache_write_tokens: match required(tokens, b"cache_write_tokens")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        output_tokens: match required(tokens, b"output_tokens")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        reasoning_tokens: match required(tokens, b"reasoning_tokens")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
    })
}

fn read_budget(tokens: &[Token], _limits: &Limits) -> Result<Budget, Error> {
    object(tokens)?;
    Ok(Budget {
        turns: unsigned(required(tokens, b"turns")?)?,
        spend: unsigned(required(tokens, b"spend")?)?,
        time_ms: unsigned(required(tokens, b"time_ms")?)?,
    })
}

fn read_tools(tokens: &[Token], limits: &Limits) -> Result<Tools, Error> {
    object(tokens)?;
    Ok(Tools {
        families: read_list_family(required(tokens, b"families")?, limits)?,
        wait: boolean(required(tokens, b"wait")?)?,
        deliver: boolean(required(tokens, b"deliver")?)?,
        host: read_list_text(required(tokens, b"host")?, limits)?,
    })
}

fn read_completion_failure(tokens: &[Token], limits: &Limits) -> Result<CompletionFailure, Error> {
    object(tokens)?;
    Ok(CompletionFailure {
        class: read_failure_class(required(tokens, b"class")?, limits)?,
        evidence: read_evidence(required(tokens, b"evidence")?, limits)?,
        retry_after_ms: match field(tokens, b"retry_after_ms")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
    })
}

fn read_answer_failure(tokens: &[Token], limits: &Limits) -> Result<AnswerFailure, Error> {
    object(tokens)?;
    Ok(AnswerFailure {
        class: read_answer_class(required(tokens, b"class")?, limits)?,
        completion: match field(tokens, b"completion")? {
            Some(item) => Some(read_completion_failure(item, limits)?),
            None => None,
        },
        account: match field(tokens, b"account")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
        which: match field(tokens, b"which")? {
            Some(item) => Some(text(item, limits.string)?),
            None => None,
        },
        reason: match field(tokens, b"reason")? {
            Some(item) => Some(text(item, limits.string)?),
            None => None,
        },
    })
}

fn read_field(tokens: &[Token], limits: &Limits) -> Result<Field, Error> {
    object(tokens)?;
    Ok(Field {
        name: text(required(tokens, b"name")?, limits.string)?,
        text: text(required(tokens, b"text")?, limits.content)?,
    })
}

fn read_run_result(tokens: &[Token], limits: &Limits) -> Result<RunResult, Error> {
    object(tokens)?;
    Ok(RunResult {
        form: read_form(required(tokens, b"form")?, limits)?,
        label: match field(tokens, b"label")? {
            Some(item) => Some(text(item, limits.string)?),
            None => None,
        },
        text: match field(tokens, b"text")? {
            Some(item) => Some(text(item, limits.content)?),
            None => None,
        },
        fields: match field(tokens, b"fields")? {
            Some(item) => Some(read_list_field(item, limits)?),
            None => None,
        },
    })
}

fn read_message(tokens: &[Token], limits: &Limits) -> Result<Message, Error> {
    object(tokens)?;
    Ok(Message {
        role: read_role(required(tokens, b"role")?, limits)?,
        blocks: read_list_block(required(tokens, b"blocks")?, limits)?,
    })
}

fn read_prompt(tokens: &[Token], limits: &Limits) -> Result<Prompt, Error> {
    object(tokens)?;
    Ok(Prompt {
        system: match field(tokens, b"system")? {
            Some(item) => Some(text(item, limits.content)?),
            None => None,
        },
        tools: match field(tokens, b"tools")? {
            Some(item) => Some(read_list_text(item, limits)?),
            None => None,
        },
        messages: read_list_message(required(tokens, b"messages")?, limits)?,
    })
}

fn read_completion(tokens: &[Token], limits: &Limits) -> Result<Completion, Error> {
    object(tokens)?;
    Ok(Completion { blocks: read_list_block(required(tokens, b"blocks")?, limits)? })
}

fn read_command_exit(tokens: &[Token], _limits: &Limits) -> Result<CommandExit, Error> {
    object(tokens)?;
    Ok(CommandExit { code: signed(required(tokens, b"code")?)? })
}

fn read_session_started(tokens: &[Token], limits: &Limits) -> Result<SessionStarted, Error> {
    object(tokens)?;
    Ok(SessionStarted {
        wall_ms: unsigned(required(tokens, b"wall_ms")?)?,
        agent: read_agent(required(tokens, b"agent")?, limits)?,
        mode: read_mode(required(tokens, b"mode")?, limits)?,
        pid: unsigned(required(tokens, b"pid")?)?,
        profile: text(required(tokens, b"profile")?, limits.string)?,
        capture: read_capture(required(tokens, b"capture")?, limits)?,
        delivery: read_delivery(required(tokens, b"delivery")?, limits)?,
        versions: read_versions(required(tokens, b"versions")?, limits)?,
    })
}

fn read_session_ended(tokens: &[Token], limits: &Limits) -> Result<SessionEnded, Error> {
    object(tokens)?;
    Ok(SessionEnded {
        exit: signed(required(tokens, b"exit")?)?,
        answered: boolean(required(tokens, b"answered")?)?,
        teardown_ms: match required(tokens, b"teardown_ms")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        emitted: read_list_count(required(tokens, b"emitted")?, limits)?,
        loss: read_loss(required(tokens, b"loss")?, limits)?,
        cpu_ms: read_cpu(required(tokens, b"cpu_ms")?, limits)?,
        peak_rss_bytes: read_peak_rss(required(tokens, b"peak_rss_bytes")?, limits)?,
    })
}

fn read_run_started(tokens: &[Token], limits: &Limits) -> Result<RunStarted, Error> {
    object(tokens)?;
    Ok(RunStarted {
        run: unsigned(required(tokens, b"run")?)?,
        resumed: boolean(required(tokens, b"resumed")?)?,
        main: read_model(required(tokens, b"main")?, limits)?,
        budget: read_budget(required(tokens, b"budget")?, limits)?,
        tools: read_tools(required(tokens, b"tools")?, limits)?,
        contract: read_list_form(required(tokens, b"contract")?, limits)?,
    })
}

fn read_run_completed(tokens: &[Token], limits: &Limits) -> Result<RunCompleted, Error> {
    object(tokens)?;
    Ok(RunCompleted {
        run: unsigned(required(tokens, b"run")?)?,
        status: read_status(required(tokens, b"status")?, limits)?,
        failure: match field(tokens, b"failure")? {
            Some(item) => Some(read_answer_failure(item, limits)?),
            None => None,
        },
        result: match field(tokens, b"result")? {
            Some(item) => Some(read_run_result(item, limits)?),
            None => None,
        },
        turns: unsigned(required(tokens, b"turns")?)?,
        spent: unsigned(required(tokens, b"spent")?)?,
        usage: read_usage(required(tokens, b"usage")?, limits)?,
        duration_ms: unsigned(required(tokens, b"duration_ms")?)?,
    })
}

fn read_conversation_opened(tokens: &[Token], limits: &Limits) -> Result<ConversationOpened, Error> {
    object(tokens)?;
    Ok(ConversationOpened {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        kind: read_conversation_kind(required(tokens, b"kind")?, limits)?,
        parent: match field(tokens, b"parent")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
        call: match field(tokens, b"call")? {
            Some(item) => Some(text(item, limits.string)?),
            None => None,
        },
        model: read_model(required(tokens, b"model")?, limits)?,
    })
}

fn read_conversation_closed(tokens: &[Token], limits: &Limits) -> Result<ConversationClosed, Error> {
    object(tokens)?;
    Ok(ConversationClosed {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        end: read_conversation_end(required(tokens, b"end")?, limits)?,
        which: match field(tokens, b"which")? {
            Some(item) => Some(text(item, limits.string)?),
            None => None,
        },
        failure: match field(tokens, b"failure")? {
            Some(item) => Some(read_completion_failure(item, limits)?),
            None => None,
        },
        turns: unsigned(required(tokens, b"turns")?)?,
        usage: read_usage(required(tokens, b"usage")?, limits)?,
        spent: unsigned(required(tokens, b"spent")?)?,
        duration_ms: unsigned(required(tokens, b"duration_ms")?)?,
    })
}

fn read_response_started(tokens: &[Token], limits: &Limits) -> Result<ResponseStarted, Error> {
    object(tokens)?;
    Ok(ResponseStarted {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        response: unsigned(required(tokens, b"response")?)?,
        turn: unsigned(required(tokens, b"turn")?)?,
        attempt: unsigned(required(tokens, b"attempt")?)?,
        model: read_model(required(tokens, b"model")?, limits)?,
        messages: unsigned(required(tokens, b"messages")?)?,
        output_tokens: unsigned(required(tokens, b"output_tokens")?)?,
        prompt: match field(tokens, b"prompt")? {
            Some(item) => Some(read_prompt(item, limits)?),
            None => None,
        },
    })
}

fn read_response_completed(tokens: &[Token], limits: &Limits) -> Result<ResponseCompleted, Error> {
    object(tokens)?;
    Ok(ResponseCompleted {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        response: unsigned(required(tokens, b"response")?)?,
        outcome: read_outcome(required(tokens, b"outcome")?, limits)?,
        stop: match field(tokens, b"stop")? {
            Some(item) => Some(read_stop(item, limits)?),
            None => None,
        },
        blocks: unsigned(required(tokens, b"blocks")?)?,
        calls: unsigned(required(tokens, b"calls")?)?,
        invalid: unsigned(required(tokens, b"invalid")?)?,
        usage: read_usage(required(tokens, b"usage")?, limits)?,
        spent: unsigned(required(tokens, b"spent")?)?,
        request_bytes: match required(tokens, b"request_bytes")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        first_byte_ms: match required(tokens, b"first_byte_ms")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        largest_gap_ms: match required(tokens, b"largest_gap_ms")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        duration_ms: unsigned(required(tokens, b"duration_ms")?)?,
        failure: match field(tokens, b"failure")? {
            Some(item) => Some(read_completion_failure(item, limits)?),
            None => None,
        },
        retry_ms: match field(tokens, b"retry_ms")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
        completion: match field(tokens, b"completion")? {
            Some(item) => Some(read_completion(item, limits)?),
            None => None,
        },
    })
}

fn read_text_delta(tokens: &[Token], limits: &Limits) -> Result<TextDelta, Error> {
    object(tokens)?;
    Ok(TextDelta {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        response: unsigned(required(tokens, b"response")?)?,
        block: unsigned(required(tokens, b"block")?)?,
        text: text(required(tokens, b"text")?, limits.content)?,
    })
}

fn read_tool_started(tokens: &[Token], limits: &Limits) -> Result<ToolStarted, Error> {
    object(tokens)?;
    Ok(ToolStarted {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        call: text(required(tokens, b"call")?, limits.string)?,
        tool: text(required(tokens, b"tool")?, limits.string)?,
        source: read_source(required(tokens, b"source")?, limits)?,
        effect: read_effect(required(tokens, b"effect")?, limits)?,
        deadline_ms: match required(tokens, b"deadline_ms")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
        input_bytes: unsigned(required(tokens, b"input_bytes")?)?,
        input: match field(tokens, b"input")? {
            Some(item) => Some(text(item, limits.content)?),
            None => None,
        },
    })
}

fn read_tool_completed(tokens: &[Token], limits: &Limits) -> Result<ToolCompleted, Error> {
    object(tokens)?;
    Ok(ToolCompleted {
        run: unsigned(required(tokens, b"run")?)?,
        conversation: unsigned(required(tokens, b"conversation")?)?,
        call: text(required(tokens, b"call")?, limits.string)?,
        tool: text(required(tokens, b"tool")?, limits.string)?,
        verdict: read_verdict(required(tokens, b"verdict")?, limits)?,
        exit: match field(tokens, b"exit")? {
            Some(item) => Some(read_command_exit(item, limits)?),
            None => None,
        },
        bytes: unsigned(required(tokens, b"bytes")?)?,
        duration_ms: unsigned(required(tokens, b"duration_ms")?)?,
        delivery: match field(tokens, b"delivery")? {
            Some(item) => Some(read_delivered(item, limits)?),
            None => None,
        },
        result: match field(tokens, b"result")? {
            Some(item) => Some(text(item, limits.content)?),
            None => None,
        },
    })
}

fn read_check_started(tokens: &[Token], _limits: &Limits) -> Result<CheckStarted, Error> {
    object(tokens)?;
    Ok(CheckStarted {
        run: unsigned(required(tokens, b"run")?)?,
        deadline_ms: match required(tokens, b"deadline_ms")? {
            [Token::Null] => None,
            item => Some(unsigned(item)?),
        },
    })
}

fn read_check_completed(tokens: &[Token], limits: &Limits) -> Result<CheckCompleted, Error> {
    object(tokens)?;
    Ok(CheckCompleted {
        run: unsigned(required(tokens, b"run")?)?,
        exit: read_command_exit(required(tokens, b"exit")?, limits)?,
        passed: boolean(required(tokens, b"passed")?)?,
        duration_ms: unsigned(required(tokens, b"duration_ms")?)?,
    })
}

fn read_notice(tokens: &[Token], limits: &Limits) -> Result<Notice, Error> {
    object(tokens)?;
    let notice = Notice {
        level: read_level(required(tokens, b"level")?, limits)?,
        kind: read_notice_kind(required(tokens, b"kind")?, limits)?,
        run: match field(tokens, b"run")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
        account: match field(tokens, b"account")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
        bytes: match field(tokens, b"bytes")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
        wait_ms: match field(tokens, b"wait_ms")? {
            Some(item) => Some(unsigned(item)?),
            None => None,
        },
    };
    match notice.kind {
        NoticeKind::ReasoningDropped if notice.bytes.is_none() => Err(Error::Shape),
        NoticeKind::CredentialRejected | NoticeKind::AccountExhausted if notice.account.is_none() => Err(Error::Shape),
        NoticeKind::ReasoningDropped
        | NoticeKind::CredentialRejected
        | NoticeKind::AccountExhausted
        | NoticeKind::Unknown(_) => Ok(notice),
    }
}

fn read_block(tokens: &[Token], limits: &Limits) -> Result<Block, Error> {
    object(tokens)?;
    let kind = text(required(tokens, b"type")?, limits.string.max(64))?;
    Ok(match kind.as_ref() {
        b"text" => Block::Text { text: text(required(tokens, b"text")?, limits.content)? },
        b"refusal" => Block::Refusal { text: text(required(tokens, b"text")?, limits.content)? },
        b"call" => Block::Call {
            call: text(required(tokens, b"call")?, limits.string)?,
            tool: text(required(tokens, b"tool")?, limits.string)?,
            input: text(required(tokens, b"input")?, limits.content)?,
        },
        b"oversized" => Block::Oversized {
            call: text(required(tokens, b"call")?, limits.string)?,
            tool: text(required(tokens, b"tool")?, limits.string)?,
            bytes: unsigned(required(tokens, b"bytes")?)?,
        },
        b"cut" => Block::Cut {
            call: text(required(tokens, b"call")?, limits.string)?,
            tool: text(required(tokens, b"tool")?, limits.string)?,
            input: text(required(tokens, b"input")?, limits.content)?,
        },
        b"result" => Block::Result {
            call: text(required(tokens, b"call")?, limits.string)?,
            error: boolean(required(tokens, b"error")?)?,
            text: text(required(tokens, b"text")?, limits.content)?,
        },
        b"opaque" => Block::Opaque { bytes: unsigned(required(tokens, b"bytes")?)? },
        _ => {
            if kind.len() > usize::try_from(limits.string).expect("u32 fits") {
                return Err(Error::Limits);
            }
            Block::Unknown { kind }
        }
    })
}

fn read_list_block(tokens: &[Token], limits: &Limits) -> Result<List<Block>, Error> {
    if tokens.first() != Some(&Token::ArrayStart) || tokens.last() != Some(&Token::ArrayEnd) {
        return Err(Error::Shape);
    }
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let end = value_end(tokens, at)?;
        let item = tokens.get(at..end).ok_or(Error::Shape)?;
        let value = read_block(item, limits)?;
        if values.push(value).is_err() {
            return Err(Error::Limits);
        }
        at = end;
    }
    Ok(values)
}

fn read_list_count(tokens: &[Token], limits: &Limits) -> Result<List<Count>, Error> {
    object(tokens)?;
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let key = match tokens.get(at).ok_or(Error::Shape)? {
            Token::Key(key) => key,
            Token::ObjectStart
            | Token::ObjectEnd
            | Token::ArrayStart
            | Token::ArrayEnd
            | Token::String(_)
            | Token::Number(_)
            | Token::True
            | Token::False
            | Token::Null => return Err(Error::Shape),
        };
        if key.len() > usize::try_from(limits.string).expect("u32 fits") {
            return Err(Error::Limits);
        }
        for value in &values {
            let value: &Count = value;
            if value.record == *key {
                return Err(Error::Shape);
            }
        }
        let start = at.checked_add(1).ok_or(Error::Limits)?;
        at = value_end(tokens, start)?;
        let count = unsigned(tokens.get(start..at).ok_or(Error::Shape)?)?;
        if values.push(Count { record: key.clone(), count }).is_err() {
            return Err(Error::Limits);
        }
    }
    Ok(values)
}

fn read_list_family(tokens: &[Token], limits: &Limits) -> Result<List<Family>, Error> {
    if tokens.first() != Some(&Token::ArrayStart) || tokens.last() != Some(&Token::ArrayEnd) {
        return Err(Error::Shape);
    }
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let end = value_end(tokens, at)?;
        let item = tokens.get(at..end).ok_or(Error::Shape)?;
        let value = read_family(item, limits)?;
        if values.push(value).is_err() {
            return Err(Error::Limits);
        }
        at = end;
    }
    Ok(values)
}

fn read_list_field(tokens: &[Token], limits: &Limits) -> Result<List<Field>, Error> {
    if tokens.first() != Some(&Token::ArrayStart) || tokens.last() != Some(&Token::ArrayEnd) {
        return Err(Error::Shape);
    }
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let end = value_end(tokens, at)?;
        let item = tokens.get(at..end).ok_or(Error::Shape)?;
        let value = read_field(item, limits)?;
        if values.push(value).is_err() {
            return Err(Error::Limits);
        }
        at = end;
    }
    Ok(values)
}

fn read_list_form(tokens: &[Token], limits: &Limits) -> Result<List<Form>, Error> {
    if tokens.first() != Some(&Token::ArrayStart) || tokens.last() != Some(&Token::ArrayEnd) {
        return Err(Error::Shape);
    }
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let end = value_end(tokens, at)?;
        let item = tokens.get(at..end).ok_or(Error::Shape)?;
        let value = read_form(item, limits)?;
        if values.push(value).is_err() {
            return Err(Error::Limits);
        }
        at = end;
    }
    Ok(values)
}

fn read_list_message(tokens: &[Token], limits: &Limits) -> Result<List<Message>, Error> {
    if tokens.first() != Some(&Token::ArrayStart) || tokens.last() != Some(&Token::ArrayEnd) {
        return Err(Error::Shape);
    }
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let end = value_end(tokens, at)?;
        let item = tokens.get(at..end).ok_or(Error::Shape)?;
        let value = read_message(item, limits)?;
        if values.push(value).is_err() {
            return Err(Error::Limits);
        }
        at = end;
    }
    Ok(values)
}

fn read_list_text(tokens: &[Token], limits: &Limits) -> Result<List<Box<[u8]>>, Error> {
    if tokens.first() != Some(&Token::ArrayStart) || tokens.last() != Some(&Token::ArrayEnd) {
        return Err(Error::Shape);
    }
    let mut values = List::with_capacity(limits.items);
    let mut at = 1_usize;
    for _ in 0..tokens.len() {
        if at >= tokens.len().saturating_sub(1) {
            break;
        }
        let end = value_end(tokens, at)?;
        let item = tokens.get(at..end).ok_or(Error::Shape)?;
        let value = text(item, limits.string)?;
        if values.push(value).is_err() {
            return Err(Error::Limits);
        }
        at = end;
    }
    Ok(values)
}
