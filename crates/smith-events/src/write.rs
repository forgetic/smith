//! Sized deterministic encoding; capture removes content fields.
//! Contract: protocol/events.md, sections 5.3 and 6.

use crate::records::{
    Agent, AnswerClass, AnswerFailure, Block, Budget, Capture, CheckCompleted, CheckStarted, CommandExit, Completion,
    CompletionFailure, ConversationClosed, ConversationEnd, ConversationKind, ConversationOpened, Count, Cpu,
    Delivered, Delivery, Effect, Event, Evidence, FailureClass, Family, Field, Form, Level, Loss, Message, Mode, Model,
    Notice, NoticeKind, Outcome, PeakRss, Prompt, Record, ResponseCompleted, ResponseStarted, Role, RunCompleted,
    RunResult, RunStarted, SessionEnded, SessionStarted, Source, Status, Stop, TextDelta, ToolCompleted, ToolStarted,
    Tools, Usage, Verdict, Versions,
};
use crate::{Error, Limits};
use alloc::boxed::Box;
use skein_json::writer::Encoder;
use skein_lib::{List, Writer};

/// Encode one line, or omit live text when capture does not request it.
pub fn write(record: &Record, capture: &Capture, limits: &Limits) -> Result<Option<Box<[u8]>>, Error> {
    match &record.event {
        Event::TextDelta(_) if *capture != Capture::Everything => return Ok(None),
        Event::SessionStarted(_)
        | Event::SessionEnded(_)
        | Event::RunStarted(_)
        | Event::RunCompleted(_)
        | Event::ConversationOpened(_)
        | Event::ConversationClosed(_)
        | Event::ResponseStarted(_)
        | Event::ResponseCompleted(_)
        | Event::TextDelta(_)
        | Event::ToolStarted(_)
        | Event::ToolCompleted(_)
        | Event::CheckStarted(_)
        | Event::CheckCompleted(_)
        | Event::Notice(_) => {}
    }
    validate_record(record, limits)?;
    let capacity = crate::limits::largest_size(&record.event, capture, limits).ok_or(Error::Limits)?;
    let bounded = skein_json::writer::Limits { depth: 16, length: capacity.checked_sub(1).ok_or(Error::Limits)? };
    let mut measure = Encoder::measure(&bounded);
    encode(&mut measure, record, capture);
    let length = match measure.measured() {
        Ok(length) => length,
        Err(refusal) => return Err(map_refusal(refusal)),
    };
    let mut encoder = Encoder::write(length, &bounded);
    encode(&mut encoder, record, capture);
    let body = encoder.finish();
    let mut line = Writer::new(body.len().checked_add(1).ok_or(Error::Limits)?);
    line.put(&body).expect("measured line room");
    line.put(b"\n").expect("newline room reserved");
    Ok(Some(line.finish()))
}

fn map_refusal(refusal: skein_json::writer::Refusal) -> Error {
    match refusal {
        skein_json::writer::Refusal::Text => Error::Text,
        skein_json::writer::Refusal::TooLong
        | skein_json::writer::Refusal::TooDeep
        | skein_json::writer::Refusal::Number => Error::Limits,
    }
}

fn encode(json: &mut Encoder, record: &Record, capture: &Capture) {
    json.object_start();
    json.key(b"v");
    json.unsigned(crate::VERSION);
    json.key(b"type");
    match &record.event {
        Event::SessionStarted(_) => json.string(b"session.started"),
        Event::SessionEnded(_) => json.string(b"session.ended"),
        Event::RunStarted(_) => json.string(b"run.started"),
        Event::RunCompleted(_) => json.string(b"run.completed"),
        Event::ConversationOpened(_) => json.string(b"conversation.opened"),
        Event::ConversationClosed(_) => json.string(b"conversation.closed"),
        Event::ResponseStarted(_) => json.string(b"response.started"),
        Event::ResponseCompleted(_) => json.string(b"response.completed"),
        Event::TextDelta(_) => json.string(b"text.delta"),
        Event::ToolStarted(_) => json.string(b"tool.started"),
        Event::ToolCompleted(_) => json.string(b"tool.completed"),
        Event::CheckStarted(_) => json.string(b"check.started"),
        Event::CheckCompleted(_) => json.string(b"check.completed"),
        Event::Notice(_) => json.string(b"notice"),
    }
    json.key(b"t_ms");
    json.unsigned(record.t_ms);
    match &record.event {
        Event::SessionStarted(value) => encode_session_started(json, value, capture),
        Event::SessionEnded(value) => encode_session_ended(json, value, capture),
        Event::RunStarted(value) => encode_run_started(json, value, capture),
        Event::RunCompleted(value) => encode_run_completed(json, value, capture),
        Event::ConversationOpened(value) => encode_conversation_opened(json, value, capture),
        Event::ConversationClosed(value) => encode_conversation_closed(json, value, capture),
        Event::ResponseStarted(value) => encode_response_started(json, value, capture),
        Event::ResponseCompleted(value) => encode_response_completed(json, value, capture),
        Event::TextDelta(value) => encode_text_delta(json, value, capture),
        Event::ToolStarted(value) => encode_tool_started(json, value, capture),
        Event::ToolCompleted(value) => encode_tool_completed(json, value, capture),
        Event::CheckStarted(value) => encode_check_started(json, value, capture),
        Event::CheckCompleted(value) => encode_check_completed(json, value, capture),
        Event::Notice(value) => encode_notice(json, value, capture),
    }
    json.object_end();
}

fn write_capture(json: &mut Encoder, value: &Capture, _capture: &Capture) {
    match value {
        Capture::None => json.string(b"none"),
        Capture::Calls => json.string(b"calls"),
        Capture::Everything => json.string(b"everything"),
        Capture::Unknown(text) => json.string(text),
    }
}

fn write_delivery(json: &mut Encoder, value: &Delivery, _capture: &Capture) {
    match value {
        Delivery::Complete => json.string(b"complete"),
        Delivery::BestEffort => json.string(b"best_effort"),
        Delivery::Unknown(text) => json.string(text),
    }
}

fn write_mode(json: &mut Encoder, value: &Mode, _capture: &Capture) {
    match value {
        Mode::Agent => json.string(b"agent"),
        Mode::Exec => json.string(b"exec"),
        Mode::Chat => json.string(b"chat"),
        Mode::Unknown(text) => json.string(text),
    }
}

fn write_family(json: &mut Encoder, value: &Family, _capture: &Capture) {
    match value {
        Family::Inspect => json.string(b"inspect"),
        Family::Modify => json.string(b"modify"),
        Family::Shell => json.string(b"shell"),
        Family::SubAgents => json.string(b"sub_agents"),
        Family::Unknown(text) => json.string(text),
    }
}

fn write_form(json: &mut Encoder, value: &Form, _capture: &Capture) {
    match value {
        Form::Report => json.string(b"report"),
        Form::Verdict => json.string(b"verdict"),
        Form::Change => json.string(b"change"),
        Form::Failure => json.string(b"failure"),
        Form::Unknown(text) => json.string(text),
    }
}

fn write_status(json: &mut Encoder, value: &Status, _capture: &Capture) {
    match value {
        Status::Accepted => json.string(b"accepted"),
        Status::Parked => json.string(b"parked"),
        Status::Failed => json.string(b"failed"),
        Status::Refused => json.string(b"refused"),
        Status::Unknown(text) => json.string(text),
    }
}

fn write_conversation_kind(json: &mut Encoder, value: &ConversationKind, _capture: &Capture) {
    match value {
        ConversationKind::Main => json.string(b"main"),
        ConversationKind::Child => json.string(b"child"),
        ConversationKind::Compaction => json.string(b"compaction"),
        ConversationKind::Unknown(text) => json.string(text),
    }
}

fn write_conversation_end(json: &mut Encoder, value: &ConversationEnd, _capture: &Capture) {
    match value {
        ConversationEnd::Closed => json.string(b"closed"),
        ConversationEnd::Budget => json.string(b"budget"),
        ConversationEnd::Failed => json.string(b"failed"),
        ConversationEnd::Refused => json.string(b"refused"),
        ConversationEnd::Transcript => json.string(b"transcript"),
        ConversationEnd::Overflow => json.string(b"overflow"),
        ConversationEnd::Unknown(text) => json.string(text),
    }
}

fn write_outcome(json: &mut Encoder, value: &Outcome, _capture: &Capture) {
    match value {
        Outcome::Completed => json.string(b"completed"),
        Outcome::Failed => json.string(b"failed"),
        Outcome::Cancelled => json.string(b"cancelled"),
        Outcome::Unknown(text) => json.string(text),
    }
}

fn write_stop(json: &mut Encoder, value: &Stop, _capture: &Capture) {
    match value {
        Stop::End => json.string(b"end"),
        Stop::Tools => json.string(b"tools"),
        Stop::MaxTokens => json.string(b"max_tokens"),
        Stop::Refusal => json.string(b"refusal"),
        Stop::Unknown(text) => json.string(text),
    }
}

fn write_source(json: &mut Encoder, value: &Source, _capture: &Capture) {
    match value {
        Source::Workspace => json.string(b"workspace"),
        Source::Run => json.string(b"run"),
        Source::Host => json.string(b"host"),
        Source::Unknown(text) => json.string(text),
    }
}

fn write_effect(json: &mut Encoder, value: &Effect, _capture: &Capture) {
    match value {
        Effect::Read => json.string(b"read"),
        Effect::Write => json.string(b"write"),
        Effect::Unknown(text) => json.string(text),
    }
}

fn write_verdict(json: &mut Encoder, value: &Verdict, _capture: &Capture) {
    match value {
        Verdict::Read => json.string(b"read"),
        Verdict::Listed => json.string(b"listed"),
        Verdict::Found => json.string(b"found"),
        Verdict::Written => json.string(b"written"),
        Verdict::Edited => json.string(b"edited"),
        Verdict::Exited => json.string(b"exited"),
        Verdict::Conflict => json.string(b"conflict"),
        Verdict::Missing => json.string(b"missing"),
        Verdict::TooLarge => json.string(b"too_large"),
        Verdict::TimedOut => json.string(b"timed_out"),
        Verdict::Failed => json.string(b"failed"),
        Verdict::Cancelled => json.string(b"cancelled"),
        Verdict::Ambiguous => json.string(b"ambiguous"),
        Verdict::Result => json.string(b"result"),
        Verdict::Error => json.string(b"error"),
        Verdict::Invalid => json.string(b"invalid"),
        Verdict::NotRun => json.string(b"not_run"),
        Verdict::Withdrawn => json.string(b"withdrawn"),
        Verdict::NotGranted => json.string(b"not_granted"),
        Verdict::Outside => json.string(b"outside"),
        Verdict::ReadOnly => json.string(b"read_only"),
        Verdict::TooLong => json.string(b"too_long"),
        Verdict::NotFound => json.string(b"not_found"),
        Verdict::NotFile => json.string(b"not_file"),
        Verdict::Linked => json.string(b"linked"),
        Verdict::Protected => json.string(b"protected"),
        Verdict::NotDirectory => json.string(b"not_directory"),
        Verdict::NotRead => json.string(b"not_read"),
        Verdict::Stale => json.string(b"stale"),
        Verdict::NoMatch => json.string(b"no_match"),
        Verdict::Unchanged => json.string(b"unchanged"),
        Verdict::Busy => json.string(b"busy"),
        Verdict::NulByte => json.string(b"nul_byte"),
        Verdict::Unknown(text) => json.string(text),
    }
}

fn write_delivered(json: &mut Encoder, value: &Delivered, _capture: &Capture) {
    match value {
        Delivered::Delivered => json.string(b"delivered"),
        Delivered::Nothing => json.string(b"nothing"),
        Delivered::Refused => json.string(b"refused"),
        Delivered::Failed => json.string(b"failed"),
        Delivered::Stale => json.string(b"stale"),
        Delivered::Unknown(text) => json.string(text),
    }
}

fn write_failure_class(json: &mut Encoder, value: &FailureClass, _capture: &Capture) {
    match value {
        FailureClass::Overloaded => json.string(b"overloaded"),
        FailureClass::RateLimited => json.string(b"rate_limited"),
        FailureClass::Exhausted => json.string(b"exhausted"),
        FailureClass::Unavailable => json.string(b"unavailable"),
        FailureClass::TimedOut => json.string(b"timed_out"),
        FailureClass::ContextTooLong => json.string(b"context_too_long"),
        FailureClass::Invalid => json.string(b"invalid"),
        FailureClass::Unauthorized => json.string(b"unauthorized"),
        FailureClass::Limit => json.string(b"limit"),
        FailureClass::Protocol => json.string(b"protocol"),
        FailureClass::Cancelled => json.string(b"cancelled"),
        FailureClass::Unknown(text) => json.string(text),
    }
}

fn write_evidence(json: &mut Encoder, value: &Evidence, _capture: &Capture) {
    match value {
        Evidence::Unsent => json.string(b"unsent"),
        Evidence::MaybeSent => json.string(b"maybe_sent"),
        Evidence::Response => json.string(b"response"),
        Evidence::Unknown(text) => json.string(text),
    }
}

fn write_answer_class(json: &mut Encoder, value: &AnswerClass, _capture: &Capture) {
    match value {
        AnswerClass::Model => json.string(b"model"),
        AnswerClass::Budget => json.string(b"budget"),
        AnswerClass::Policy => json.string(b"policy"),
        AnswerClass::Cancelled => json.string(b"cancelled"),
        AnswerClass::Stale => json.string(b"stale"),
        AnswerClass::Transcript => json.string(b"transcript"),
        AnswerClass::Busy => json.string(b"busy"),
        AnswerClass::Invalid => json.string(b"invalid"),
        AnswerClass::Unknown(text) => json.string(text),
    }
}

fn write_level(json: &mut Encoder, value: &Level, _capture: &Capture) {
    match value {
        Level::Info => json.string(b"info"),
        Level::Warning => json.string(b"warning"),
        Level::Unknown(text) => json.string(text),
    }
}

fn write_notice_kind(json: &mut Encoder, value: &NoticeKind, _capture: &Capture) {
    match value {
        NoticeKind::CredentialRejected => json.string(b"credential_rejected"),
        NoticeKind::AccountExhausted => json.string(b"account_exhausted"),
        NoticeKind::Unknown(text) => json.string(text),
    }
}

fn write_role(json: &mut Encoder, value: &Role, _capture: &Capture) {
    match value {
        Role::User => json.string(b"user"),
        Role::Assistant => json.string(b"assistant"),
        Role::Unknown(text) => json.string(text),
    }
}

fn write_agent(json: &mut Encoder, value: &Agent, _capture: &Capture) {
    json.object_start();
    json.key(b"name");
    json.string(&value.name);
    json.key(b"version");
    json.string(&value.version);
    json.key(b"build");
    json.string(&value.build);
    json.object_end();
}

fn write_versions(json: &mut Encoder, value: &Versions, _capture: &Capture) {
    json.object_start();
    json.key(b"events");
    json.unsigned(value.events);
    json.key(b"channel");
    json.unsigned(value.channel);
    json.key(b"charter");
    json.unsigned(value.charter);
    json.key(b"transcript");
    json.unsigned(value.transcript);
    json.object_end();
}

fn write_loss(json: &mut Encoder, value: &Loss, _capture: &Capture) {
    json.object_start();
    json.key(b"events");
    json.unsigned(value.events);
    json.key(b"channel");
    match &value.channel {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.object_end();
}

fn write_cpu(json: &mut Encoder, value: &Cpu, _capture: &Capture) {
    json.object_start();
    json.key(b"user");
    json.unsigned(value.user);
    json.key(b"system");
    json.unsigned(value.system);
    json.key(b"children_user");
    json.unsigned(value.children_user);
    json.key(b"children_system");
    json.unsigned(value.children_system);
    json.object_end();
}

fn write_peak_rss(json: &mut Encoder, value: &PeakRss, _capture: &Capture) {
    json.object_start();
    json.key(b"self");
    json.unsigned(value.self_bytes);
    json.key(b"children");
    json.unsigned(value.children);
    json.object_end();
}

fn write_model(json: &mut Encoder, value: &Model, _capture: &Capture) {
    json.object_start();
    json.key(b"endpoint");
    json.string(&value.endpoint);
    json.key(b"model");
    json.string(&value.model);
    json.key(b"output_tokens");
    json.unsigned(value.output_tokens);
    json.key(b"effort");
    match &value.effort {
        Some(item) => {
            json.string(item);
        }
        None => json.null(),
    }
    json.object_end();
}

fn write_usage(json: &mut Encoder, value: &Usage, _capture: &Capture) {
    json.object_start();
    json.key(b"input_tokens");
    match &value.input_tokens {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"cache_read_tokens");
    match &value.cache_read_tokens {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"cache_write_tokens");
    match &value.cache_write_tokens {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"output_tokens");
    match &value.output_tokens {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"reasoning_tokens");
    match &value.reasoning_tokens {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.object_end();
}

fn write_budget(json: &mut Encoder, value: &Budget, _capture: &Capture) {
    json.object_start();
    json.key(b"turns");
    json.unsigned(value.turns);
    json.key(b"spend");
    json.unsigned(value.spend);
    json.key(b"time_ms");
    json.unsigned(value.time_ms);
    json.object_end();
}

fn write_tools(json: &mut Encoder, value: &Tools, capture: &Capture) {
    json.object_start();
    json.key(b"families");
    write_list_family(json, &value.families, capture);
    json.key(b"wait");
    json.boolean(value.wait);
    json.key(b"deliver");
    json.boolean(value.deliver);
    json.key(b"host");
    write_list_text(json, &value.host, capture);
    json.object_end();
}

fn write_completion_failure(json: &mut Encoder, value: &CompletionFailure, capture: &Capture) {
    json.object_start();
    json.key(b"class");
    write_failure_class(json, &value.class, capture);
    json.key(b"evidence");
    write_evidence(json, &value.evidence, capture);
    if let Some(item) = &value.retry_after_ms {
        json.key(b"retry_after_ms");
        json.unsigned(*item);
    }
    json.object_end();
}

fn write_answer_failure(json: &mut Encoder, value: &AnswerFailure, capture: &Capture) {
    json.object_start();
    json.key(b"class");
    write_answer_class(json, &value.class, capture);
    if let Some(item) = &value.completion {
        json.key(b"completion");
        write_completion_failure(json, item, capture);
    }
    if let Some(item) = &value.account {
        json.key(b"account");
        json.unsigned(*item);
    }
    if let Some(item) = &value.which {
        json.key(b"which");
        json.string(item);
    }
    if let Some(item) = &value.reason {
        json.key(b"reason");
        json.string(item);
    }
    json.object_end();
}

fn write_field(json: &mut Encoder, value: &Field, _capture: &Capture) {
    json.object_start();
    json.key(b"name");
    json.string(&value.name);
    json.key(b"text");
    json.string(&value.text);
    json.object_end();
}

fn write_run_result(json: &mut Encoder, value: &RunResult, capture: &Capture) {
    json.object_start();
    json.key(b"form");
    write_form(json, &value.form, capture);
    if let Some(item) = &value.label {
        json.key(b"label");
        json.string(item);
    }
    if *capture == Capture::Everything
        && let Some(item) = &value.text
    {
        json.key(b"text");
        json.string(item);
    }
    if *capture == Capture::Everything
        && let Some(item) = &value.fields
    {
        json.key(b"fields");
        write_list_field(json, item, capture);
    }
    json.object_end();
}

fn write_message(json: &mut Encoder, value: &Message, capture: &Capture) {
    json.object_start();
    json.key(b"role");
    write_role(json, &value.role, capture);
    json.key(b"blocks");
    write_list_block(json, &value.blocks, capture);
    json.object_end();
}

fn write_prompt(json: &mut Encoder, value: &Prompt, capture: &Capture) {
    json.object_start();
    if let Some(item) = &value.system {
        json.key(b"system");
        json.string(item);
    }
    if let Some(item) = &value.tools {
        json.key(b"tools");
        write_list_text(json, item, capture);
    }
    json.key(b"messages");
    write_list_message(json, &value.messages, capture);
    json.object_end();
}

fn write_completion(json: &mut Encoder, value: &Completion, capture: &Capture) {
    json.object_start();
    json.key(b"blocks");
    write_list_block(json, &value.blocks, capture);
    json.object_end();
}

fn write_command_exit(json: &mut Encoder, value: &CommandExit, _capture: &Capture) {
    json.object_start();
    json.key(b"code");
    json.signed(value.code);
    json.object_end();
}

fn write_block(json: &mut Encoder, value: &Block, _capture: &Capture) {
    json.object_start();
    json.key(b"type");
    match value {
        Block::Unknown { kind } => json.string(kind),
        Block::Text { text } => {
            json.string(b"text");
            json.key(b"text");
            json.string(text);
        }
        Block::Refusal { text } => {
            json.string(b"refusal");
            json.key(b"text");
            json.string(text);
        }
        Block::Call { call, tool, input } => {
            json.string(b"call");
            json.key(b"call");
            json.string(call);
            json.key(b"tool");
            json.string(tool);
            json.key(b"input");
            json.string(input);
        }
        Block::Oversized { call, tool, bytes } => {
            json.string(b"oversized");
            json.key(b"call");
            json.string(call);
            json.key(b"tool");
            json.string(tool);
            json.key(b"bytes");
            json.unsigned(*bytes);
        }
        Block::Cut { call, tool, input } => {
            json.string(b"cut");
            json.key(b"call");
            json.string(call);
            json.key(b"tool");
            json.string(tool);
            json.key(b"input");
            json.string(input);
        }
        Block::Result { call, error, text } => {
            json.string(b"result");
            json.key(b"call");
            json.string(call);
            json.key(b"error");
            json.boolean(*error);
            json.key(b"text");
            json.string(text);
        }
        Block::Opaque { bytes } => {
            json.string(b"opaque");
            json.key(b"bytes");
            json.unsigned(*bytes);
        }
    }
    json.object_end();
}

fn write_list_block(json: &mut Encoder, values: &List<Block>, capture: &Capture) {
    json.array_start();
    for value in values {
        write_block(json, value, capture);
    }
    json.array_end();
}

fn write_list_count(json: &mut Encoder, values: &List<Count>, _capture: &Capture) {
    json.object_start();
    for value in values {
        json.key(&value.record);
        json.unsigned(value.count);
    }
    json.object_end();
}

fn write_list_family(json: &mut Encoder, values: &List<Family>, capture: &Capture) {
    json.array_start();
    for value in values {
        write_family(json, value, capture);
    }
    json.array_end();
}

fn write_list_field(json: &mut Encoder, values: &List<Field>, capture: &Capture) {
    json.array_start();
    for value in values {
        write_field(json, value, capture);
    }
    json.array_end();
}

fn write_list_form(json: &mut Encoder, values: &List<Form>, capture: &Capture) {
    json.array_start();
    for value in values {
        write_form(json, value, capture);
    }
    json.array_end();
}

fn write_list_message(json: &mut Encoder, values: &List<Message>, capture: &Capture) {
    json.array_start();
    for value in values {
        write_message(json, value, capture);
    }
    json.array_end();
}

fn write_list_text(json: &mut Encoder, values: &List<Box<[u8]>>, _capture: &Capture) {
    json.array_start();
    for value in values {
        json.string(value);
    }
    json.array_end();
}

fn check_text(text: &[u8], capacity: u32) -> Result<(), Error> {
    if text.len() > usize::try_from(capacity).expect("u32 fits") {
        return Err(Error::Limits);
    }
    Ok(())
}

fn validate_record(record: &Record, limits: &Limits) -> Result<(), Error> {
    match &record.event {
        Event::SessionStarted(value) => validate_session_started(value, limits),
        Event::SessionEnded(value) => validate_session_ended(value, limits),
        Event::RunStarted(value) => validate_run_started(value, limits),
        Event::RunCompleted(value) => validate_run_completed(value, limits),
        Event::ConversationOpened(value) => validate_conversation_opened(value, limits),
        Event::ConversationClosed(value) => validate_conversation_closed(value, limits),
        Event::ResponseStarted(value) => validate_response_started(value, limits),
        Event::ResponseCompleted(value) => validate_response_completed(value, limits),
        Event::TextDelta(value) => validate_text_delta(value, limits),
        Event::ToolStarted(value) => validate_tool_started(value, limits),
        Event::ToolCompleted(value) => validate_tool_completed(value, limits),
        Event::CheckStarted(_) | Event::CheckCompleted(_) => Ok(()),
        Event::Notice(value) => validate_notice(value, limits),
    }
}

fn validate_capture(value: &Capture, limits: &Limits) -> Result<(), Error> {
    match value {
        Capture::None | Capture::Calls | Capture::Everything => Ok(()),
        Capture::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_delivery(value: &Delivery, limits: &Limits) -> Result<(), Error> {
    match value {
        Delivery::Complete | Delivery::BestEffort => Ok(()),
        Delivery::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_mode(value: &Mode, limits: &Limits) -> Result<(), Error> {
    match value {
        Mode::Agent | Mode::Exec | Mode::Chat => Ok(()),
        Mode::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_family(value: &Family, limits: &Limits) -> Result<(), Error> {
    match value {
        Family::Inspect | Family::Modify | Family::Shell | Family::SubAgents => Ok(()),
        Family::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_form(value: &Form, limits: &Limits) -> Result<(), Error> {
    match value {
        Form::Report | Form::Verdict | Form::Change | Form::Failure => Ok(()),
        Form::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_status(value: &Status, limits: &Limits) -> Result<(), Error> {
    match value {
        Status::Accepted | Status::Parked | Status::Failed | Status::Refused => Ok(()),
        Status::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_conversation_kind(value: &ConversationKind, limits: &Limits) -> Result<(), Error> {
    match value {
        ConversationKind::Main | ConversationKind::Child | ConversationKind::Compaction => Ok(()),
        ConversationKind::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_conversation_end(value: &ConversationEnd, limits: &Limits) -> Result<(), Error> {
    match value {
        ConversationEnd::Closed
        | ConversationEnd::Budget
        | ConversationEnd::Failed
        | ConversationEnd::Refused
        | ConversationEnd::Transcript
        | ConversationEnd::Overflow => Ok(()),
        ConversationEnd::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_outcome(value: &Outcome, limits: &Limits) -> Result<(), Error> {
    match value {
        Outcome::Completed | Outcome::Failed | Outcome::Cancelled => Ok(()),
        Outcome::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_stop(value: &Stop, limits: &Limits) -> Result<(), Error> {
    match value {
        Stop::End | Stop::Tools | Stop::MaxTokens | Stop::Refusal => Ok(()),
        Stop::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_source(value: &Source, limits: &Limits) -> Result<(), Error> {
    match value {
        Source::Workspace | Source::Run | Source::Host => Ok(()),
        Source::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_effect(value: &Effect, limits: &Limits) -> Result<(), Error> {
    match value {
        Effect::Read | Effect::Write => Ok(()),
        Effect::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_verdict(value: &Verdict, limits: &Limits) -> Result<(), Error> {
    match value {
        Verdict::Read
        | Verdict::Listed
        | Verdict::Found
        | Verdict::Written
        | Verdict::Edited
        | Verdict::Exited
        | Verdict::Conflict
        | Verdict::Missing
        | Verdict::TooLarge
        | Verdict::TimedOut
        | Verdict::Failed
        | Verdict::Cancelled
        | Verdict::Ambiguous
        | Verdict::Result
        | Verdict::Error
        | Verdict::Invalid
        | Verdict::NotRun
        | Verdict::Withdrawn
        | Verdict::NotGranted
        | Verdict::Outside
        | Verdict::ReadOnly
        | Verdict::TooLong
        | Verdict::NotFound
        | Verdict::NotFile
        | Verdict::Linked
        | Verdict::Protected
        | Verdict::NotDirectory
        | Verdict::NotRead
        | Verdict::Stale
        | Verdict::NoMatch
        | Verdict::Unchanged
        | Verdict::Busy
        | Verdict::NulByte => Ok(()),
        Verdict::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_delivered(value: &Delivered, limits: &Limits) -> Result<(), Error> {
    match value {
        Delivered::Delivered | Delivered::Nothing | Delivered::Refused | Delivered::Failed | Delivered::Stale => Ok(()),
        Delivered::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_failure_class(value: &FailureClass, limits: &Limits) -> Result<(), Error> {
    match value {
        FailureClass::Overloaded
        | FailureClass::RateLimited
        | FailureClass::Exhausted
        | FailureClass::Unavailable
        | FailureClass::TimedOut
        | FailureClass::ContextTooLong
        | FailureClass::Invalid
        | FailureClass::Unauthorized
        | FailureClass::Limit
        | FailureClass::Protocol
        | FailureClass::Cancelled => Ok(()),
        FailureClass::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_evidence(value: &Evidence, limits: &Limits) -> Result<(), Error> {
    match value {
        Evidence::Unsent | Evidence::MaybeSent | Evidence::Response => Ok(()),
        Evidence::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_answer_class(value: &AnswerClass, limits: &Limits) -> Result<(), Error> {
    match value {
        AnswerClass::Model
        | AnswerClass::Budget
        | AnswerClass::Policy
        | AnswerClass::Cancelled
        | AnswerClass::Stale
        | AnswerClass::Transcript
        | AnswerClass::Busy
        | AnswerClass::Invalid => Ok(()),
        AnswerClass::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_level(value: &Level, limits: &Limits) -> Result<(), Error> {
    match value {
        Level::Info | Level::Warning => Ok(()),
        Level::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_notice_kind(value: &NoticeKind, limits: &Limits) -> Result<(), Error> {
    match value {
        NoticeKind::CredentialRejected | NoticeKind::AccountExhausted => Ok(()),
        NoticeKind::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_role(value: &Role, limits: &Limits) -> Result<(), Error> {
    match value {
        Role::User | Role::Assistant => Ok(()),
        Role::Unknown(text) => check_text(text, limits.string),
    }
}

fn validate_agent(value: &Agent, limits: &Limits) -> Result<(), Error> {
    check_text(&value.name, limits.string)?;
    check_text(&value.version, limits.string)?;
    check_text(&value.build, limits.string)?;
    Ok(())
}

fn validate_count(value: &Count, limits: &Limits) -> Result<(), Error> {
    check_text(&value.record, limits.string)?;

    Ok(())
}

fn validate_model(value: &Model, limits: &Limits) -> Result<(), Error> {
    check_text(&value.endpoint, limits.string)?;
    check_text(&value.model, limits.string)?;

    if let Some(item) = &value.effort {
        check_text(item, limits.string)?;
    }
    Ok(())
}

fn validate_tools(value: &Tools, limits: &Limits) -> Result<(), Error> {
    validate_list_family(&value.families, limits)?;

    validate_list_text(&value.host, limits)?;
    Ok(())
}

fn validate_completion_failure(value: &CompletionFailure, limits: &Limits) -> Result<(), Error> {
    validate_failure_class(&value.class, limits)?;
    validate_evidence(&value.evidence, limits)?;
    Ok(())
}

fn validate_answer_failure(value: &AnswerFailure, limits: &Limits) -> Result<(), Error> {
    validate_answer_class(&value.class, limits)?;
    if let Some(item) = &value.completion {
        validate_completion_failure(item, limits)?;
    }
    if let Some(item) = &value.which {
        check_text(item, limits.string)?;
    }
    if let Some(item) = &value.reason {
        check_text(item, limits.string)?;
    }
    Ok(())
}

fn validate_field(value: &Field, limits: &Limits) -> Result<(), Error> {
    check_text(&value.name, limits.string)?;
    check_text(&value.text, limits.content)?;
    Ok(())
}

fn validate_run_result(value: &RunResult, limits: &Limits) -> Result<(), Error> {
    validate_form(&value.form, limits)?;
    if let Some(item) = &value.label {
        check_text(item, limits.string)?;
    }
    if let Some(item) = &value.text {
        check_text(item, limits.content)?;
    }
    if let Some(item) = &value.fields {
        validate_list_field(item, limits)?;
    }
    Ok(())
}

fn validate_message(value: &Message, limits: &Limits) -> Result<(), Error> {
    validate_role(&value.role, limits)?;
    validate_list_block(&value.blocks, limits)?;
    Ok(())
}

fn validate_prompt(value: &Prompt, limits: &Limits) -> Result<(), Error> {
    if let Some(item) = &value.system {
        check_text(item, limits.content)?;
    }
    if let Some(item) = &value.tools {
        validate_list_text(item, limits)?;
    }
    validate_list_message(&value.messages, limits)?;
    Ok(())
}

fn validate_completion(value: &Completion, limits: &Limits) -> Result<(), Error> {
    validate_list_block(&value.blocks, limits)?;
    Ok(())
}

fn validate_session_started(value: &SessionStarted, limits: &Limits) -> Result<(), Error> {
    validate_agent(&value.agent, limits)?;
    validate_mode(&value.mode, limits)?;

    check_text(&value.profile, limits.string)?;
    validate_capture(&value.capture, limits)?;
    validate_delivery(&value.delivery, limits)?;
    Ok(())
}

fn validate_session_ended(value: &SessionEnded, limits: &Limits) -> Result<(), Error> {
    validate_list_count(&value.emitted, limits)?;
    Ok(())
}

fn validate_run_started(value: &RunStarted, limits: &Limits) -> Result<(), Error> {
    validate_model(&value.main, limits)?;
    validate_tools(&value.tools, limits)?;
    validate_list_form(&value.contract, limits)?;
    Ok(())
}

fn validate_run_completed(value: &RunCompleted, limits: &Limits) -> Result<(), Error> {
    validate_status(&value.status, limits)?;
    if let Some(item) = &value.failure {
        validate_answer_failure(item, limits)?;
    }
    if let Some(item) = &value.result {
        validate_run_result(item, limits)?;
    }

    Ok(())
}

fn validate_conversation_opened(value: &ConversationOpened, limits: &Limits) -> Result<(), Error> {
    validate_conversation_kind(&value.kind, limits)?;
    if let Some(item) = &value.call {
        check_text(item, limits.string)?;
    }
    validate_model(&value.model, limits)?;
    Ok(())
}

fn validate_conversation_closed(value: &ConversationClosed, limits: &Limits) -> Result<(), Error> {
    validate_conversation_end(&value.end, limits)?;
    if let Some(item) = &value.which {
        check_text(item, limits.string)?;
    }
    if let Some(item) = &value.failure {
        validate_completion_failure(item, limits)?;
    }

    Ok(())
}

fn validate_response_started(value: &ResponseStarted, limits: &Limits) -> Result<(), Error> {
    validate_model(&value.model, limits)?;

    if let Some(item) = &value.prompt {
        validate_prompt(item, limits)?;
    }
    Ok(())
}

fn validate_response_completed(value: &ResponseCompleted, limits: &Limits) -> Result<(), Error> {
    validate_outcome(&value.outcome, limits)?;
    if let Some(item) = &value.stop {
        validate_stop(item, limits)?;
    }

    if let Some(item) = &value.failure {
        validate_completion_failure(item, limits)?;
    }
    if let Some(item) = &value.completion {
        validate_completion(item, limits)?;
    }
    Ok(())
}

fn validate_text_delta(value: &TextDelta, limits: &Limits) -> Result<(), Error> {
    check_text(&value.text, limits.content)?;
    Ok(())
}

fn validate_tool_started(value: &ToolStarted, limits: &Limits) -> Result<(), Error> {
    check_text(&value.call, limits.string)?;
    check_text(&value.tool, limits.string)?;
    validate_source(&value.source, limits)?;
    validate_effect(&value.effect, limits)?;

    if let Some(item) = &value.input {
        check_text(item, limits.content)?;
    }
    Ok(())
}

fn validate_tool_completed(value: &ToolCompleted, limits: &Limits) -> Result<(), Error> {
    check_text(&value.call, limits.string)?;
    check_text(&value.tool, limits.string)?;
    validate_verdict(&value.verdict, limits)?;

    if let Some(item) = &value.delivery {
        validate_delivered(item, limits)?;
    }
    if let Some(item) = &value.result {
        check_text(item, limits.content)?;
    }
    Ok(())
}

fn validate_notice(value: &Notice, limits: &Limits) -> Result<(), Error> {
    validate_level(&value.level, limits)?;
    validate_notice_kind(&value.kind, limits)?;

    Ok(())
}

fn validate_block(value: &Block, limits: &Limits) -> Result<(), Error> {
    match value {
        Block::Unknown { kind } => check_text(kind, limits.string),
        Block::Text { text } | Block::Refusal { text } => {
            check_text(text, limits.content)?;
            Ok(())
        }
        Block::Oversized { call, tool, bytes: _ } => {
            check_text(call, limits.string)?;
            check_text(tool, limits.string)?;

            Ok(())
        }
        Block::Call { call, tool, input } | Block::Cut { call, tool, input } => {
            check_text(call, limits.string)?;
            check_text(tool, limits.string)?;
            check_text(input, limits.content)?;
            Ok(())
        }
        Block::Result { call, error: _, text } => {
            check_text(call, limits.string)?;

            check_text(text, limits.content)?;
            Ok(())
        }
        Block::Opaque { bytes: _ } => Ok(()),
    }
}

fn validate_list_block(values: &List<Block>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        validate_block(value, limits)?;
    }
    Ok(())
}

fn validate_list_count(values: &List<Count>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        validate_count(value, limits)?;
    }
    for first in 0..values.len() {
        let first_value = values.get(first).expect("bounded counter list");
        for second in first.saturating_add(1)..values.len() {
            let second_value = values.get(second).expect("bounded counter list");
            if first_value.record == second_value.record {
                return Err(Error::Shape);
            }
        }
    }
    Ok(())
}

fn validate_list_family(values: &List<Family>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        validate_family(value, limits)?;
    }
    Ok(())
}

fn validate_list_field(values: &List<Field>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        validate_field(value, limits)?;
    }
    Ok(())
}

fn validate_list_form(values: &List<Form>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        validate_form(value, limits)?;
    }
    Ok(())
}

fn validate_list_message(values: &List<Message>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        validate_message(value, limits)?;
    }
    Ok(())
}

fn validate_list_text(values: &List<Box<[u8]>>, limits: &Limits) -> Result<(), Error> {
    if values.len() > limits.items {
        return Err(Error::Limits);
    }
    for value in values {
        check_text(value, limits.string)?;
    }
    Ok(())
}

fn encode_session_started(json: &mut Encoder, value: &SessionStarted, capture: &Capture) {
    json.key(b"wall_ms");
    json.unsigned(value.wall_ms);
    json.key(b"agent");
    write_agent(json, &value.agent, capture);
    json.key(b"mode");
    write_mode(json, &value.mode, capture);
    json.key(b"pid");
    json.unsigned(value.pid);
    json.key(b"profile");
    json.string(&value.profile);
    json.key(b"capture");
    write_capture(json, &value.capture, capture);
    json.key(b"delivery");
    write_delivery(json, &value.delivery, capture);
    json.key(b"versions");
    write_versions(json, &value.versions, capture);
}

fn encode_session_ended(json: &mut Encoder, value: &SessionEnded, capture: &Capture) {
    json.key(b"exit");
    json.signed(value.exit);
    json.key(b"answered");
    json.boolean(value.answered);
    json.key(b"teardown_ms");
    match &value.teardown_ms {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"emitted");
    write_list_count(json, &value.emitted, capture);
    json.key(b"loss");
    write_loss(json, &value.loss, capture);
    json.key(b"cpu_ms");
    write_cpu(json, &value.cpu_ms, capture);
    json.key(b"peak_rss_bytes");
    write_peak_rss(json, &value.peak_rss_bytes, capture);
}

fn encode_run_started(json: &mut Encoder, value: &RunStarted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"resumed");
    json.boolean(value.resumed);
    json.key(b"main");
    write_model(json, &value.main, capture);
    json.key(b"budget");
    write_budget(json, &value.budget, capture);
    json.key(b"tools");
    write_tools(json, &value.tools, capture);
    json.key(b"contract");
    write_list_form(json, &value.contract, capture);
}

fn encode_run_completed(json: &mut Encoder, value: &RunCompleted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"status");
    write_status(json, &value.status, capture);
    if let Some(item) = &value.failure {
        json.key(b"failure");
        write_answer_failure(json, item, capture);
    }
    if let Some(item) = &value.result {
        json.key(b"result");
        write_run_result(json, item, capture);
    }
    json.key(b"turns");
    json.unsigned(value.turns);
    json.key(b"spent");
    json.unsigned(value.spent);
    json.key(b"usage");
    write_usage(json, &value.usage, capture);
    json.key(b"duration_ms");
    json.unsigned(value.duration_ms);
}

fn encode_conversation_opened(json: &mut Encoder, value: &ConversationOpened, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"kind");
    write_conversation_kind(json, &value.kind, capture);
    if let Some(item) = &value.parent {
        json.key(b"parent");
        json.unsigned(*item);
    }
    if let Some(item) = &value.call {
        json.key(b"call");
        json.string(item);
    }
    json.key(b"model");
    write_model(json, &value.model, capture);
}

fn encode_conversation_closed(json: &mut Encoder, value: &ConversationClosed, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"end");
    write_conversation_end(json, &value.end, capture);
    if let Some(item) = &value.which {
        json.key(b"which");
        json.string(item);
    }
    if let Some(item) = &value.failure {
        json.key(b"failure");
        write_completion_failure(json, item, capture);
    }
    json.key(b"turns");
    json.unsigned(value.turns);
    json.key(b"usage");
    write_usage(json, &value.usage, capture);
    json.key(b"spent");
    json.unsigned(value.spent);
    json.key(b"duration_ms");
    json.unsigned(value.duration_ms);
}

fn encode_response_started(json: &mut Encoder, value: &ResponseStarted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"response");
    json.unsigned(value.response);
    json.key(b"turn");
    json.unsigned(value.turn);
    json.key(b"attempt");
    json.unsigned(value.attempt);
    json.key(b"model");
    write_model(json, &value.model, capture);
    json.key(b"messages");
    json.unsigned(value.messages);
    json.key(b"output_tokens");
    json.unsigned(value.output_tokens);
    if *capture == Capture::Everything
        && let Some(item) = &value.prompt
    {
        json.key(b"prompt");
        write_prompt(json, item, capture);
    }
}

fn encode_response_completed(json: &mut Encoder, value: &ResponseCompleted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"response");
    json.unsigned(value.response);
    json.key(b"outcome");
    write_outcome(json, &value.outcome, capture);
    if let Some(item) = &value.stop {
        json.key(b"stop");
        write_stop(json, item, capture);
    }
    json.key(b"blocks");
    json.unsigned(value.blocks);
    json.key(b"calls");
    json.unsigned(value.calls);
    json.key(b"invalid");
    json.unsigned(value.invalid);
    json.key(b"usage");
    write_usage(json, &value.usage, capture);
    json.key(b"spent");
    json.unsigned(value.spent);
    json.key(b"request_bytes");
    match &value.request_bytes {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"first_byte_ms");
    match &value.first_byte_ms {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"largest_gap_ms");
    match &value.largest_gap_ms {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"duration_ms");
    json.unsigned(value.duration_ms);
    if let Some(item) = &value.failure {
        json.key(b"failure");
        write_completion_failure(json, item, capture);
    }
    if let Some(item) = &value.retry_ms {
        json.key(b"retry_ms");
        json.unsigned(*item);
    }
    if *capture == Capture::Everything
        && let Some(item) = &value.completion
    {
        json.key(b"completion");
        write_completion(json, item, capture);
    }
}

fn encode_text_delta(json: &mut Encoder, value: &TextDelta, _capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"response");
    json.unsigned(value.response);
    json.key(b"block");
    json.unsigned(value.block);
    json.key(b"text");
    json.string(&value.text);
}

fn encode_tool_started(json: &mut Encoder, value: &ToolStarted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"call");
    json.string(&value.call);
    json.key(b"tool");
    json.string(&value.tool);
    json.key(b"source");
    write_source(json, &value.source, capture);
    json.key(b"effect");
    write_effect(json, &value.effect, capture);
    json.key(b"deadline_ms");
    match &value.deadline_ms {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
    json.key(b"input_bytes");
    json.unsigned(value.input_bytes);
    if *capture != Capture::None
        && let Some(item) = &value.input
    {
        json.key(b"input");
        json.string(item);
    }
}

fn encode_tool_completed(json: &mut Encoder, value: &ToolCompleted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"conversation");
    json.unsigned(value.conversation);
    json.key(b"call");
    json.string(&value.call);
    json.key(b"tool");
    json.string(&value.tool);
    json.key(b"verdict");
    write_verdict(json, &value.verdict, capture);
    if let Some(item) = &value.exit {
        json.key(b"exit");
        write_command_exit(json, item, capture);
    }
    json.key(b"bytes");
    json.unsigned(value.bytes);
    json.key(b"duration_ms");
    json.unsigned(value.duration_ms);
    if let Some(item) = &value.delivery {
        json.key(b"delivery");
        write_delivered(json, item, capture);
    }
    if *capture == Capture::Everything
        && let Some(item) = &value.result
    {
        json.key(b"result");
        json.string(item);
    }
}

fn encode_check_started(json: &mut Encoder, value: &CheckStarted, _capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"deadline_ms");
    match &value.deadline_ms {
        Some(item) => {
            json.unsigned(*item);
        }
        None => json.null(),
    }
}

fn encode_check_completed(json: &mut Encoder, value: &CheckCompleted, capture: &Capture) {
    json.key(b"run");
    json.unsigned(value.run);
    json.key(b"exit");
    write_command_exit(json, &value.exit, capture);
    json.key(b"passed");
    json.boolean(value.passed);
    json.key(b"duration_ms");
    json.unsigned(value.duration_ms);
}

fn encode_notice(json: &mut Encoder, value: &Notice, capture: &Capture) {
    json.key(b"level");
    write_level(json, &value.level, capture);
    json.key(b"kind");
    write_notice_kind(json, &value.kind, capture);
    if let Some(item) = &value.run {
        json.key(b"run");
        json.unsigned(*item);
    }
    json.key(b"account");
    json.unsigned(value.account);
    if let Some(item) = &value.wait_ms {
        json.key(b"wait_ms");
        json.unsigned(*item);
    }
}
