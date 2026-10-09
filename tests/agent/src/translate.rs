//! Typed fixture wiring between the copied scripts and the agent conversation
//! (domain/session.md, section 12; domain/run.md, section 14). This test-only
//! adapter recognizes the scripts' finite arguments. It owns no production
//! provider schema adapter; migration 05s5 supplies those. Opaque host inputs
//! are supplied as bounded, attested typed values by the scripted fake. Tool results keep
//! call IDs and diagnostic bytes, allowing the provider to check transcript
//! pairing and the tests to inspect feedback on subsequent requests.

use skein_fake_llm_domain::api as provider;
use smith_domain::{llm as agent, run, tools};

#[must_use]
pub fn query(prompt: agent::Prompt) -> provider::Query {
    let mut tools = Vec::new();
    let mut names = Vec::new();
    if prompt.tools.inspect {
        names.extend([b"read".as_slice(), b"list", b"search"]);
    }
    if prompt.tools.modify {
        names.extend([b"write".as_slice(), b"edit"]);
    }
    if prompt.tools.shell {
        names.push(b"shell");
    }
    for name in names {
        tools.push(provider::ToolSpec {
            name: name.into(),
            description: b"Scripted domain tool.".as_slice().into(),
            parameters: b"{}".as_slice().into(),
        });
    }
    for served in prompt.served {
        let specification = match served {
            agent::Served::Host(tool) => {
                provider::ToolSpec { name: tool.name, description: tool.description, parameters: tool.schema }
            }
            agent::Served::Deliver => fixed(b"deliver"),
            agent::Served::Finish => fixed(b"finish"),
            agent::Served::SubAgent => fixed(b"sub_agent"),
            agent::Served::Wait => fixed(b"wait"),
        };
        tools.push(specification);
    }
    let messages = prompt
        .messages
        .into_iter()
        .map(|message| provider::Message {
            role: match message.role {
                agent::Role::User => provider::Role::User,
                agent::Role::Assistant => provider::Role::Assistant,
            },
            parts: message.content.into_iter().map(part).collect(),
        })
        .collect();
    provider::Query {
        model: prompt.model,
        system: prompt.system,
        tools: tools.into(),
        choice: provider::ToolChoice::Auto,
        messages,
        max_tokens: prompt.max_tokens,
    }
}

fn fixed(name: &[u8]) -> provider::ToolSpec {
    provider::ToolSpec {
        name: name.into(),
        description: b"Scripted domain tool.".as_slice().into(),
        parameters: b"{}".as_slice().into(),
    }
}

fn part(block: agent::Block) -> provider::Part {
    match block {
        agent::Block::Text { text, .. } | agent::Block::Refusal { text, .. } => provider::Part::Text { text },
        agent::Block::Opaque { bytes } => provider::Part::Opaque { bytes },
        agent::Block::ToolCall { id, name, input, .. } => provider::Part::ToolCall { id, name, arguments: input },
        agent::Block::ToolResult { id, result } => {
            let error = match &result {
                agent::Returned::Text { error, .. } | agent::Returned::Served { error, .. } => *error,
                agent::Returned::Withdrawn | agent::Returned::Invalid { .. } | agent::Returned::NotRun => true,
                agent::Returned::Owned { outcome } => !matches!(
                    outcome,
                    tools::Outcome::Read { .. }
                        | tools::Outcome::Listed { .. }
                        | tools::Outcome::Found { .. }
                        | tools::Outcome::Written { .. }
                        | tools::Outcome::Edited { .. }
                        | tools::Outcome::Exited { exit: tools::Exit::Code { code: 0 }, .. }
                ),
            };
            let output = match &result {
                agent::Returned::Text { text, .. }
                | agent::Returned::Served { returned: run::Returned::Answered { text, .. }, .. } => text.clone(),
                agent::Returned::Withdrawn => b"withdrawn".as_slice().into(),
                agent::Returned::Served { returned: run::Returned::DeliveryFailed { failure }, .. } => {
                    failure.diagnostic.output().into()
                }
                agent::Returned::Served { returned: run::Returned::HostAnswered(answer), .. } => answer.text().into(),
                agent::Returned::Owned { .. }
                | agent::Returned::Served { .. }
                | agent::Returned::Invalid { .. }
                | agent::Returned::NotRun => format!("{result:?}").into_bytes().into(),
            };
            provider::Part::ToolOutput { id, output, is_error: error }
        }
    }
}

#[must_use]
pub fn completion(answer: provider::Answer, grants: tools::Grants, served: &[agent::Served]) -> agent::Completion {
    let stop = match answer.finish {
        provider::Finish::Stop => agent::Stop::EndTurn,
        provider::Finish::ToolCalls => agent::Stop::ToolUse,
        provider::Finish::Length => agent::Stop::MaxTokens,
        provider::Finish::ContentFilter => agent::Stop::Refusal,
    };
    let usage = agent::Usage {
        input_tokens: answer.usage.prompt_tokens,
        output_tokens: answer.usage.completion_tokens,
        cache_read_tokens: answer.usage.cached_tokens,
        cache_write_tokens: answer.usage.cache_creation_tokens,
    };
    let content = answer
        .parts
        .into_iter()
        .map(|part| match part {
            provider::Part::Text { text } => agent::Said::Text { text, replay: None },
            provider::Part::Opaque { bytes } => agent::Said::Opaque { bytes },
            provider::Part::ToolCall { id, name, arguments } => {
                let call = decode(&name, &arguments, grants, served);
                agent::Said::ToolCall { id, name, input: arguments, call, replay: None }
            }
            provider::Part::ToolOutput { .. } => panic!("a fake completion contains no tool result"),
        })
        .collect();
    agent::Completion { content, stop, usage }
}

#[must_use]
pub fn decode(name: &[u8], arguments: &[u8], grants: tools::Grants, served: &[agent::Served]) -> agent::Decoded {
    let invalid = || agent::Decoded::Invalid { problem: agent::Problem::UnknownTool };
    for tool in served {
        match tool {
            agent::Served::Host(tool) if tool.name.as_ref() == name => {
                return run::HostInput::attested(arguments.into()).map_or_else(invalid, |input| {
                    agent::Decoded::Served { ask: run::Ask::Host { tool: name.into(), effect: tool.effect, input } }
                });
            }
            agent::Served::Host(_)
            | agent::Served::Deliver
            | agent::Served::Finish
            | agent::Served::SubAgent
            | agent::Served::Wait => {}
        }
    }
    if name == b"deliver" && served.contains(&agent::Served::Deliver) {
        let Some(value) = field(arguments, b"ticket") else {
            return invalid();
        };
        return agent::Decoded::Served {
            ask: run::Ask::Deliver {
                change: run::outcome::Change {
                    fields: Box::new([run::outcome::Field { name: b"ticket".as_slice().into(), value }]),
                },
            },
        };
    }
    if name == b"finish" && served.contains(&agent::Served::Finish) {
        return finish(arguments)
            .map_or_else(invalid, |outcome| agent::Decoded::Served { ask: run::Ask::Finish { outcome } });
    }
    if name == b"wait" && served.contains(&agent::Served::Wait) {
        let no_arguments: Vec<u8> = arguments.iter().copied().filter(|byte| !byte.is_ascii_whitespace()).collect();
        return if no_arguments == b"{}" { agent::Decoded::Served { ask: run::Ask::Wait } } else { invalid() };
    }
    if name == b"sub_agent" && served.contains(&agent::Served::SubAgent) {
        let Some(brief) = field(arguments, b"brief") else { return invalid() };
        let has = |word: &[u8]| arguments.windows(word.len()).any(|part| part == word);
        let families = run::charter::Families {
            tools: run::charter::Tools {
                inspect: has(b"\"inspect\""),
                modify: has(b"\"modify\""),
                shell: has(b"\"shell\""),
            },

            agents: has(b"\"agents\":true"),
        };
        return agent::Decoded::Served {
            ask: run::Ask::SubAgent { brief, families, llm: field(arguments, b"llm"), share: None },
        };
    }
    let (session_name, granted): (&[u8], bool) = match name {
        b"read" => (b"read", grants.inspect),
        b"list" => (b"list", grants.inspect),
        b"search" => (b"search", grants.inspect),
        b"write" => (b"write", grants.modify),
        b"edit" => (b"edit", grants.modify),
        b"shell" => (b"shell", grants.shell),
        _ => return invalid(),
    };
    if !granted {
        return invalid();
    }
    match smith_session_world::translate::decode(session_name, arguments) {
        smith_domain::session::llm::Decoded::Owned { call } => agent::Decoded::Owned { call },
        smith_domain::session::llm::Decoded::Invalid { problem } => agent::Decoded::Invalid { problem },
        smith_domain::session::llm::Decoded::Delegated { .. } => {
            unreachable!("the component fixture decoder owns only checkout tools")
        }
        smith_domain::session::llm::Decoded::Historical => {
            unreachable!("the live fixture decoder never produces a concrete-history replay marker")
        }
    }
}

fn finish(arguments: &[u8]) -> Option<run::outcome::Declared> {
    use run::outcome::{Change, Declared, DeclaredFailure, Field, Item, Report, Verdict};
    if !arguments.starts_with(b"{") || !arguments.ends_with(b"}") {
        return None;
    }
    if let Some(text) = field(arguments, b"report") {
        let fields: Box<[Field]> = match field(arguments, b"source") {
            Some(value) => Box::new([Field { name: b"source".as_slice().into(), value }]),
            None => Box::new([]),
        };
        return Some(Declared::Report(Report { text, fields }));
    }
    if let Some(reason) = field(arguments, b"failure") {
        let fields: Box<[Field]> = match field(arguments, b"cause") {
            Some(value) => Box::new([Field { name: b"cause".as_slice().into(), value }]),
            None => Box::new([]),
        };
        return Some(Declared::Failure(DeclaredFailure { reason, fields }));
    }
    let body = field(arguments, b"body")?;
    if let Some(title) = field(arguments, b"title") {
        return Some(Declared::Change(Change {
            fields: Box::new([
                smith_domain::run::outcome::Field { name: b"title".as_slice().into(), value: title },
                smith_domain::run::outcome::Field { name: b"body".as_slice().into(), value: body },
            ]),
        }));
    }
    let name = field(arguments, b"verdict")?;
    let children: Box<[Item]> = if arguments.windows(10).any(|part| part == b"\"children\"") {
        let child = Item {
            kind: field(arguments, b"kind")?,
            fields: Box::new([
                Field { name: b"path".as_slice().into(), value: field(arguments, b"path")? },
                Field { name: b"body".as_slice().into(), value: b"43".as_slice().into() },
            ]),
        };
        Box::new([child])
    } else {
        Box::new([])
    };
    Some(Declared::Verdict(Verdict { name, text: body, items: children, fields: Box::new([]) }))
}

/// The copied scripts use unescaped string fields. This recognizes that finite
/// fixture language, never arbitrary JSON or production provider documents.
///
/// Scripted-world contract: domain/run.md, sections 13 and 14; testing-strategy.md, section 7.
fn field(arguments: &[u8], name: &[u8]) -> Option<Box<[u8]>> {
    let prefix = [b"\"", name, b"\":\""].concat();
    let start = arguments.windows(prefix.len()).position(|part| part == prefix)? + prefix.len();
    let rest = &arguments[start..];
    let end = rest.iter().position(|byte| *byte == b'"')?;
    Some(rest[..end].into())
}

#[must_use]
pub fn failure(error: provider::Error) -> agent::Failure {
    match error {
        provider::Error::Overloaded => agent::Failure::Overloaded,
        provider::Error::RateLimited { retry_after } => agent::Failure::RateLimited { retry_after },
        provider::Error::Unavailable => agent::Failure::Unavailable,
        provider::Error::ContextTooLong => agent::Failure::ContextTooLong,
        provider::Error::Unauthorized => agent::Failure::Unauthorized,
        provider::Error::Exhausted { retry_after } => agent::Failure::Exhausted { retry_after },
        provider::Error::InvalidRequest => agent::Failure::Invalid,
    }
}
