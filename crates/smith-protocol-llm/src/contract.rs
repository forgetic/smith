//! Contract-derived finish and delivery schemas and typed result decoding.
//! The service supplies the charter's admitted rules; this module keeps no
//! result policy or host state. The run judges every decoded value itself.
//! Contract: protocol/llm.md, sections 3 and 4; protocol/charter.md, section 2.1;
//! domain/run.md, section 7.

use alloc::boxed::Box;

use skein_json::Token;
use skein_json::writer::{self, Encoder};
use skein_lib::List;
use skein_llm::{DocumentError, Json};
use smith_domain::{llm, run};

use crate::{Error, Limits};

/// A JSON Schema for each final form the admitted charter allows.
/// Host labels, item kinds and required fields are byte-exact.
pub fn finish_schema(contract: &run::outcome::OutcomeSpec, maximum: u32) -> Result<Box<[u8]>, Error> {
    let limits = writer::Limits { depth: 16, length: maximum };
    let mut measure = Encoder::measure(&limits);
    encode_finish(&mut measure, contract);
    let length = match measure.measured() {
        Ok(length) => length,
        Err(refusal) => return Err(schema_error(refusal)),
    };
    let mut write = Encoder::write(length, &limits);
    encode_finish(&mut write, contract);
    Ok(write.finish())
}

/// The mid-run deliver tool's schema, from its separate Change grant.
pub fn deliver_schema(change: &run::outcome::ChangeSpec, maximum: u32) -> Result<Box<[u8]>, Error> {
    let limits = writer::Limits { depth: 16, length: maximum };
    let mut measure = Encoder::measure(&limits);
    encode_deliver(&mut measure, change);
    let length = match measure.measured() {
        Ok(length) => length,
        Err(refusal) => return Err(schema_error(refusal)),
    };
    let mut write = Encoder::write(length, &limits);
    encode_deliver(&mut write, change);
    Ok(write.finish())
}

fn schema_error(refusal: writer::Refusal) -> Error {
    match refusal {
        writer::Refusal::Text | writer::Refusal::Number => Error::Invalid,
        writer::Refusal::TooLong | writer::Refusal::TooDeep => Error::Limit,
    }
}

fn encode_finish(json: &mut Encoder, contract: &run::outcome::OutcomeSpec) {
    json.object_start();
    json.key(b"oneOf");
    json.array_start();
    if let Some(change) = &contract.change {
        change_branch(json, b"change", change);
    }
    if !contract.verdicts.is_empty() {
        json.object_start();
        json.key(b"oneOf");
        json.array_start();
        for rule in &contract.verdicts {
            verdict_branch(json, rule);
        }
        json.array_end();
        json.object_end();
    }
    if let Some(report) = &contract.report {
        text_branch(json, b"report", b"text", report);
    }
    if let Some(failure) = &contract.failure {
        text_branch(json, b"failure", b"reason", failure);
    }
    json.array_end();
    json.object_end();
}

fn encode_deliver(json: &mut Encoder, change: &run::outcome::ChangeSpec) {
    object_type(json);
    json.key(b"properties");
    json.object_start();
    json.key(b"fields");
    fields_schema(json, &change.fields);
    json.object_end();
    required(json, &[b"fields"]);
    json.object_end();
}

fn change_branch(json: &mut Encoder, form: &[u8], change: &run::outcome::ChangeSpec) {
    object_type(json);
    json.key(b"properties");
    json.object_start();
    const_string(json, b"form", form);
    json.key(b"fields");
    fields_schema(json, &change.fields);
    json.object_end();
    required(json, &[b"form", b"fields"]);
    json.object_end();
}

fn text_branch(json: &mut Encoder, form: &[u8], text_name: &[u8], text: &run::outcome::TextSpec) {
    object_type(json);
    json.key(b"properties");
    json.object_start();
    const_string(json, b"form", form);
    bounded_string(json, text_name, text.max, false);
    json.key(b"fields");
    fields_schema(json, &text.fields);
    json.object_end();
    required(json, &[b"form", text_name, b"fields"]);
    json.object_end();
}

fn verdict_branch(json: &mut Encoder, verdict: &run::outcome::VerdictRule) {
    object_type(json);
    json.key(b"properties");
    json.object_start();
    const_string(json, b"form", b"verdict");
    const_string(json, b"label", &verdict.name);
    bounded_string(json, b"text", verdict.text_max, false);
    json.key(b"fields");
    fields_schema(json, &verdict.fields);
    json.key(b"items");
    json.object_start();
    json.key(b"type");
    json.string(b"array");
    json.key(b"minItems");
    json.unsigned(u64::from(verdict.items.min));
    json.key(b"maxItems");
    json.unsigned(u64::from(verdict.items.max));
    if !verdict.items.kinds.is_empty() {
        json.key(b"items");
        json.object_start();
        json.key(b"oneOf");
        json.array_start();
        for item in &verdict.items.kinds {
            item_branch(json, item);
        }
        json.array_end();
        json.object_end();
    }
    json.object_end();
    json.object_end();
    required(json, &[b"form", b"label", b"text", b"fields", b"items"]);
    json.object_end();
}

fn item_branch(json: &mut Encoder, item: &run::outcome::ItemRule) {
    object_type(json);
    json.key(b"properties");
    json.object_start();
    const_string(json, b"kind", &item.kind);
    json.key(b"fields");
    fields_schema(json, &item.fields);
    json.object_end();
    required(json, &[b"kind", b"fields"]);
    json.object_end();
}

fn fields_schema(json: &mut Encoder, fields: &[run::outcome::FieldRule]) {
    object_type(json);
    json.key(b"properties");
    json.object_start();
    for field in fields {
        bounded_string(json, &field.name, field.max, true);
    }
    json.object_end();
    json.key(b"required");
    json.array_start();
    for field in fields {
        json.string(&field.name);
    }
    json.array_end();
    json.object_end();
}

fn object_type(json: &mut Encoder) {
    json.object_start();
    json.key(b"type");
    json.string(b"object");
}

fn bounded_string(json: &mut Encoder, name: &[u8], maximum: u32, nonempty: bool) {
    json.key(name);
    json.object_start();
    json.key(b"type");
    json.string(b"string");
    if nonempty {
        json.key(b"minLength");
        json.unsigned(1);
    }
    json.key(b"maxLength");
    json.unsigned(u64::from(maximum));
    json.object_end();
}

fn const_string(json: &mut Encoder, name: &[u8], value: &[u8]) {
    json.key(name);
    json.object_start();
    json.key(b"const");
    json.string(value);
    json.object_end();
}

fn required(json: &mut Encoder, names: &[&[u8]]) {
    json.key(b"required");
    json.array_start();
    for name in names {
        json.string(name);
    }
    json.array_end();
}

/// Decode a provider's declared result under a bounded admitted contract.
/// Shape and aggregate policy are validated by the run after this translation.
#[must_use]
pub fn decode_finish(input: &[u8], _contract: &run::outcome::OutcomeSpec, limits: &Limits) -> llm::Decoded {
    let document = match object(input, limits) {
        Ok(document) => document,
        Err(problem) => return llm::Decoded::Invalid { problem },
    };
    let tokens = document.as_tokens();
    let outcome = match declared(tokens) {
        Ok(outcome) => outcome,
        Err(problem) => return llm::Decoded::Invalid { problem },
    };
    llm::Decoded::Served { ask: run::Ask::Finish { outcome } }
}

/// Decode the separately granted delivery's host fields.
#[must_use]
pub fn decode_deliver(input: &[u8], _change: &run::outcome::ChangeSpec, limits: &Limits) -> llm::Decoded {
    let document = match object(input, limits) {
        Ok(document) => document,
        Err(problem) => return llm::Decoded::Invalid { problem },
    };
    let fields = match required_fields(document.as_tokens()) {
        Ok(fields) => fields,
        Err(problem) => return llm::Decoded::Invalid { problem },
    };
    llm::Decoded::Served { ask: run::Ask::Deliver { change: run::outcome::Change { fields } } }
}

fn object(input: &[u8], limits: &Limits) -> Result<Json, llm::Problem> {
    let document = match Json::from_bytes(input, &limits.client.dialect) {
        Ok(document) => document,
        Err(DocumentError::TooLarge { which: _, bound: _ }) => return Err(llm::Problem::TooLarge),
        Err(DocumentError::Malformed | DocumentError::Missing | DocumentError::WrongType) => {
            return Err(llm::Problem::NotAnObject);
        }
    };
    match document.as_tokens().first() {
        Some(Token::ObjectStart) => Ok(document),
        Some(_) | None => Err(llm::Problem::NotAnObject),
    }
}

fn declared(tokens: &[Token]) -> Result<run::outcome::Declared, llm::Problem> {
    let form = required_string(tokens, b"form")?;
    let outcome = match form.as_ref() {
        b"change" => run::outcome::Declared::Change(run::outcome::Change { fields: required_fields(tokens)? }),
        b"report" => run::outcome::Declared::Report(run::outcome::Report {
            text: required_string(tokens, b"text")?,
            fields: required_fields(tokens)?,
        }),
        b"failure" => run::outcome::Declared::Failure(run::outcome::DeclaredFailure {
            reason: required_string(tokens, b"reason")?,
            fields: required_fields(tokens)?,
        }),
        b"verdict" => run::outcome::Declared::Verdict(run::outcome::Verdict {
            name: required_string(tokens, b"label")?,
            text: required_string(tokens, b"text")?,
            fields: required_fields(tokens)?,
            items: items(tokens)?,
        }),
        _ => return Err(llm::Problem::BadValue { field: b"form".as_slice().into() }),
    };
    Ok(outcome)
}

fn required_fields(tokens: &[Token]) -> Result<Box<[run::outcome::Field]>, llm::Problem> {
    let value = required_value(tokens, b"fields")?;
    if value.first() != Some(&Token::ObjectStart) {
        return Err(wrong_type(b"fields"));
    }
    let mut fields = List::with_capacity(u32::try_from(value.len()).or(Err(llm::Problem::TooLarge))?);
    let mut cursor = 1_usize;
    for _ in 0..value.len() {
        match value.get(cursor) {
            Some(Token::Key(name)) => {
                let next = cursor.checked_add(1).ok_or(llm::Problem::TooLarge)?;
                let text = match value.get(next) {
                    Some(Token::String(text)) => text.clone(),
                    Some(_) => return Err(wrong_type(name)),
                    None => return Err(llm::Problem::NotAnObject),
                };
                fields.push(run::outcome::Field { name: name.clone(), value: text }).or(Err(llm::Problem::TooLarge))?;
                cursor = next.checked_add(1).ok_or(llm::Problem::TooLarge)?;
            }
            Some(Token::ObjectEnd) => return Ok(fields.into_boxed()),
            Some(_) | None => return Err(llm::Problem::NotAnObject),
        }
    }
    Err(llm::Problem::NotAnObject)
}

fn items(tokens: &[Token]) -> Result<Box<[run::outcome::Item]>, llm::Problem> {
    let value = required_value(tokens, b"items")?;
    if value.first() != Some(&Token::ArrayStart) {
        return Err(wrong_type(b"items"));
    }
    let mut items = List::with_capacity(u32::try_from(value.len()).or(Err(llm::Problem::TooLarge))?);
    let mut cursor = 1_usize;
    for _ in 0..value.len() {
        match value.get(cursor) {
            Some(Token::ArrayEnd) => return Ok(items.into_boxed()),
            Some(Token::ObjectStart) => {
                let end = value_end(value, cursor).ok_or(llm::Problem::NotAnObject)?;
                let one = value.get(cursor..end).ok_or(llm::Problem::NotAnObject)?;
                items
                    .push(run::outcome::Item { kind: required_string(one, b"kind")?, fields: required_fields(one)? })
                    .or(Err(llm::Problem::TooLarge))?;
                cursor = end;
            }
            Some(_) => return Err(wrong_type(b"items")),
            None => return Err(llm::Problem::NotAnObject),
        }
    }
    Err(llm::Problem::NotAnObject)
}

fn required_string(tokens: &[Token], name: &[u8]) -> Result<Box<[u8]>, llm::Problem> {
    let value = required_value(tokens, name)?;
    match value {
        [Token::String(text)] => Ok(text.clone()),
        _ => Err(wrong_type(name)),
    }
}

fn required_value<'a>(tokens: &'a [Token], name: &[u8]) -> Result<&'a [Token], llm::Problem> {
    let mut cursor = 1_usize;
    for _ in 0..tokens.len() {
        match tokens.get(cursor) {
            Some(Token::Key(key)) => {
                let start = cursor.checked_add(1).ok_or(llm::Problem::TooLarge)?;
                let end = value_end(tokens, start).ok_or(llm::Problem::NotAnObject)?;
                if key.as_ref() == name {
                    return tokens.get(start..end).ok_or(llm::Problem::NotAnObject);
                }
                cursor = end;
            }
            Some(Token::ObjectEnd) => return Err(llm::Problem::Missing { field: name.into() }),
            Some(_) | None => return Err(llm::Problem::NotAnObject),
        }
    }
    Err(llm::Problem::NotAnObject)
}

fn value_end(tokens: &[Token], start: usize) -> Option<usize> {
    let first = tokens.get(start)?;
    match first {
        Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null => start.checked_add(1),
        Token::ObjectStart | Token::ArrayStart => {
            let mut depth = 0_u32;
            for (index, token) in tokens.iter().enumerate().skip(start) {
                match token {
                    Token::ObjectStart | Token::ArrayStart => depth = depth.checked_add(1)?,
                    Token::ObjectEnd | Token::ArrayEnd => {
                        depth = depth.checked_sub(1)?;
                        if depth == 0 {
                            return index.checked_add(1);
                        }
                    }
                    Token::Key(_) | Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null => {}
                }
            }
            None
        }
        Token::ObjectEnd | Token::ArrayEnd | Token::Key(_) => None,
    }
}

fn wrong_type(name: &[u8]) -> llm::Problem {
    llm::Problem::WrongType { field: name.into() }
}
