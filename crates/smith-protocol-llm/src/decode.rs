//! Bounded decoding of provider-written tool inputs into the domain vocabulary.
//! No call starts here. The original bytes stay in the completion for replay;
//! invalid inputs become typed problems for the domain to answer.
//! Contract: protocol/llm.md, section 4; domain/tools.md, sections 3 and 4.

use alloc::boxed::Box;

use skein_json::Token;
use skein_lib::{Duration, List};
use skein_llm::{DocumentError, Json};
use smith_domain::{llm, run, tools};

use crate::Limits;

/// Decode one offered Smith tool. Host tools are checked and relayed by the
/// completion adapter; finish and deliver use their contract decoder.
#[must_use]
pub fn decode(
    name: &[u8],
    input: &[u8],
    grants: tools::Grants,
    served: &[llm::Served],
    limits: &Limits,
) -> llm::Decoded {
    let kind = match name {
        b"read" if grants.inspect => Kind::Read,
        b"list" if grants.inspect => Kind::List,
        b"search" if grants.inspect => Kind::Search,
        b"write" if grants.modify => Kind::Write,
        b"edit" if grants.modify => Kind::Edit,
        b"shell" if grants.shell => Kind::Shell,
        b"wait" if offered(served, Kind::Wait) => Kind::Wait,
        b"sub_agent" if offered(served, Kind::SubAgent) => Kind::SubAgent,
        _ => return llm::Decoded::Invalid { problem: llm::Problem::UnknownTool },
    };
    let document = match Json::from_bytes(input, &limits.client.dialect) {
        Ok(document) => document,
        Err(DocumentError::TooLarge { which: _, bound: _ }) => return invalid(llm::Problem::TooLarge),
        Err(DocumentError::Malformed | DocumentError::Missing | DocumentError::WrongType) => {
            return invalid(llm::Problem::NotAnObject);
        }
    };
    let tokens = document.as_tokens();
    if tokens.first() != Some(&Token::ObjectStart) {
        return invalid(llm::Problem::NotAnObject);
    }
    match call(kind, tokens) {
        Ok(value) => value,
        Err(problem) => invalid(problem),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Read,
    List,
    Search,
    Write,
    Edit,
    Shell,
    Wait,
    SubAgent,
}

fn offered(served: &[llm::Served], kind: Kind) -> bool {
    for declaration in served {
        match declaration {
            llm::Served::Wait if kind == Kind::Wait => return true,
            llm::Served::SubAgent if kind == Kind::SubAgent => return true,
            llm::Served::Wait
            | llm::Served::SubAgent
            | llm::Served::Host(_)
            | llm::Served::Deliver
            | llm::Served::Finish => {}
        }
    }
    false
}

#[expect(clippy::manual_map, reason = "strict step subset excludes closure arguments")]
fn call(kind: Kind, tokens: &[Token]) -> Result<llm::Decoded, llm::Problem> {
    let result = match kind {
        Kind::Read => llm::Decoded::Owned {
            call: tools::Call::Read {
                path: path(&required_string(tokens, b"path")?)?,
                skip: match optional_u32(tokens, b"first_line")? {
                    Some(first_line) => first_line.checked_sub(1).ok_or(bad_value(b"first_line"))?,
                    None => 0,
                },
                lines: optional_u32(tokens, b"lines")?,
            },
        },
        Kind::List => {
            llm::Decoded::Owned { call: tools::Call::List { path: path(&required_string(tokens, b"path")?)? } }
        }
        Kind::Search => {
            let spelling = match optional_string(tokens, b"path")? {
                Some(path) => path,
                None => b".".as_slice().into(),
            };
            llm::Decoded::Owned {
                call: tools::Call::Search {
                    path: path(&spelling)?,
                    pattern: required_string(tokens, b"pattern")?,
                    glob: optional_string(tokens, b"glob")?,
                },
            }
        }
        Kind::Write => llm::Decoded::Owned {
            call: tools::Call::Write {
                path: path(&required_string(tokens, b"path")?)?,
                content: required_string(tokens, b"content")?,
            },
        },
        Kind::Edit => llm::Decoded::Owned {
            call: tools::Call::Edit {
                path: path(&required_string(tokens, b"path")?)?,
                old: required_string(tokens, b"old")?,
                new: required_string(tokens, b"new")?,
                all: optional_bool(tokens, b"all")?.unwrap_or(false),
            },
        },
        Kind::Shell => {
            let seconds = optional_u32(tokens, b"timeout")?;
            let timeout = match seconds {
                Some(seconds) => Some(Duration::from_secs(u64::from(seconds))),
                None => None,
            };
            llm::Decoded::Owned { call: tools::Call::Shell { command: required_string(tokens, b"command")?, timeout } }
        }
        Kind::Wait => llm::Decoded::Served { ask: run::Ask::Wait },
        Kind::SubAgent => llm::Decoded::Served {
            ask: run::Ask::SubAgent {
                brief: required_string(tokens, b"brief")?,
                families: families(tokens)?,
                llm: optional_string(tokens, b"llm")?,
                share: None,
            },
        },
    };
    Ok(result)
}

fn invalid(problem: llm::Problem) -> llm::Decoded {
    llm::Decoded::Invalid { problem }
}

fn missing(name: &[u8]) -> llm::Problem {
    llm::Problem::Missing { field: name.into() }
}

fn wrong_type(name: &[u8]) -> llm::Problem {
    llm::Problem::WrongType { field: name.into() }
}

fn bad_value(name: &[u8]) -> llm::Problem {
    llm::Problem::BadValue { field: name.into() }
}

// The tokenizer validates every comma, colon, escape and nested value. Only
// direct object members are looked up; an unknown member's subtree is ignored.
fn field<'a>(tokens: &'a [Token], name: &[u8]) -> Option<&'a Token> {
    let mut depth = 0_u32;
    let mut wanted = false;
    for token in tokens {
        match token {
            Token::ObjectStart | Token::ArrayStart => {
                if wanted {
                    return Some(token);
                }
                depth = depth.saturating_add(1);
            }
            Token::ObjectEnd | Token::ArrayEnd => {
                depth = depth.saturating_sub(1);
            }
            Token::Key(key) if depth == 1 => {
                wanted = key.as_ref() == name;
            }
            Token::Key(_) => {}
            Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null => {
                if wanted {
                    return Some(token);
                }
            }
        }
    }
    None
}

fn required_string(tokens: &[Token], name: &[u8]) -> Result<Box<[u8]>, llm::Problem> {
    match field(tokens, name) {
        Some(Token::String(value)) => Ok(value.clone()),
        Some(_) => Err(wrong_type(name)),
        None => Err(missing(name)),
    }
}

fn optional_string(tokens: &[Token], name: &[u8]) -> Result<Option<Box<[u8]>>, llm::Problem> {
    match field(tokens, name) {
        Some(Token::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(wrong_type(name)),
        None => Ok(None),
    }
}

fn optional_bool(tokens: &[Token], name: &[u8]) -> Result<Option<bool>, llm::Problem> {
    match field(tokens, name) {
        Some(Token::True) => Ok(Some(true)),
        Some(Token::False) => Ok(Some(false)),
        Some(_) => Err(wrong_type(name)),
        None => Ok(None),
    }
}

fn optional_u32(tokens: &[Token], name: &[u8]) -> Result<Option<u32>, llm::Problem> {
    match field(tokens, name) {
        Some(Token::Number(value)) => {
            let number = decimal(value).ok_or(bad_value(name))?;
            if number == 0 {
                return Err(bad_value(name));
            }
            Ok(Some(number))
        }
        Some(_) => Err(wrong_type(name)),
        None => Ok(None),
    }
}

fn decimal(text: &[u8]) -> Option<u32> {
    if text.is_empty() {
        return None;
    }
    let mut number = 0_u32;
    for byte in text {
        if !byte.is_ascii_digit() {
            return None;
        }
        number = number.checked_mul(10)?.checked_add(u32::from(*byte).checked_sub(u32::from(b'0'))?)?;
    }
    Some(number)
}

fn path(text: &[u8]) -> Result<tools::Path, llm::Problem> {
    let absolute = text.first() == Some(&b'/');
    let capacity = u32::try_from(text.len()).or(Err(llm::Problem::TooLarge))?;
    let mut parts = List::with_capacity(capacity);
    let mut start = 0_usize;
    for (index, byte) in text.iter().enumerate() {
        if *byte == b'/' {
            append_part(&mut parts, text.get(start..index).ok_or(llm::Problem::TooLarge)?)?;
            start = index.checked_add(1).ok_or(llm::Problem::TooLarge)?;
        }
    }
    append_part(&mut parts, text.get(start..).ok_or(llm::Problem::TooLarge)?)?;
    if parts.is_empty() && !absolute {
        return Err(bad_value(b"path"));
    }
    Ok(tools::Path { absolute, parts: parts.into_boxed() })
}

fn append_part(parts: &mut List<tools::Part>, piece: &[u8]) -> Result<(), llm::Problem> {
    if piece.is_empty() {
        return Ok(());
    }
    let part = match piece {
        b"." => tools::Part::Current,
        b".." => tools::Part::Parent,
        name => {
            let name = tools::Name::new(name.into()).ok_or(bad_value(b"path"))?;
            tools::Part::Name { name }
        }
    };
    parts.push(part).or(Err(llm::Problem::TooLarge))
}

fn families(tokens: &[Token]) -> Result<run::charter::Families, llm::Problem> {
    let mut inspect = false;
    let mut modify = false;
    let mut shell = false;
    match field(tokens, b"tools") {
        Some(Token::ArrayStart) => {}
        Some(_) => return Err(wrong_type(b"tools")),
        None => return Err(missing(b"tools")),
    }
    let mut in_tools = false;
    let mut depth = 0_u32;
    for token in tokens {
        match token {
            Token::ObjectEnd => depth = depth.saturating_sub(1),
            Token::Key(name) if depth == 1 => in_tools = name.as_ref() == b"tools",
            Token::ObjectStart | Token::ArrayStart => depth = depth.saturating_add(1),
            Token::ArrayEnd => {
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    in_tools = false;
                }
            }
            Token::String(value) if in_tools && depth == 2 => match value.as_ref() {
                b"inspect" => inspect = true,
                b"modify" => modify = true,
                b"shell" => shell = true,
                _ => return Err(bad_value(b"tools")),
            },
            Token::Number(_) | Token::True | Token::False | Token::Null if in_tools && depth == 2 => {
                return Err(wrong_type(b"tools"));
            }
            Token::Key(_) | Token::String(_) | Token::Number(_) | Token::True | Token::False | Token::Null => {}
        }
    }
    Ok(run::charter::Families {
        tools: run::charter::Tools { inspect, modify, shell },
        agents: optional_bool(tokens, b"agents")?.unwrap_or(false),
    })
}
