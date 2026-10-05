//! Typed fixture wiring between the copied scripts and the agent conversation
//! (domain/session.md, section 12; domain/run.md, section 14). This test-only
//! adapter recognizes the scripts' finite arguments. It owns no production
//! schema or protocol parser; migration 05s5 supplies those. Tool results keep
//! call IDs and diagnostic bytes, allowing the provider to check transcript
//! pairing and the tests to inspect feedback on subsequent requests.

use smith_domain::{llm as agent, run, tools};
use smith_fake_llm_domain::api as provider;

pub(crate) fn query(prompt: agent::Prompt) -> provider::Query {
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
    for served in prompt.served {
        names.push(match served {
            agent::Served::Finish => b"finish",
            agent::Served::SubAgent => b"subagent",
        });
    }
    let tools = names
        .into_iter()
        .map(|name| provider::ToolSpec {
            name: name.into(),
            description: b"Scripted domain tool.".as_slice().into(),
            parameters: b"{}".as_slice().into(),
        })
        .collect();
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
    provider::Query { model: prompt.model, system: prompt.system, tools, messages, max_tokens: prompt.max_tokens }
}

fn part(block: agent::Block) -> provider::Part {
    match block {
        agent::Block::Text { text } => provider::Part::Text { text },
        agent::Block::Opaque { bytes } => provider::Part::Opaque { bytes },
        agent::Block::ToolCall { id, name, input } => provider::Part::ToolCall { id, name, arguments: input },
        agent::Block::ToolResult { id, result } => {
            let error = match &result {
                agent::Returned::Served { error, .. } => *error,
                agent::Returned::Invalid { .. } | agent::Returned::NotRun => true,
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
                agent::Returned::Served { returned: run::Returned::Unpushed { failure }, .. } => {
                    failure.diagnostic.output().into()
                }
                agent::Returned::Served { returned: run::Returned::Answered { text, .. }, .. } => text.clone(),
                _ => format!("{result:?}").into_bytes().into(),
            };
            provider::Part::ToolOutput { id, output, is_error: error }
        }
    }
}

pub(crate) fn completion(
    answer: provider::Answer,
    grants: tools::Grants,
    served: &[agent::Served],
) -> agent::Completion {
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
            provider::Part::Text { text } => agent::Said::Text { text },
            provider::Part::Opaque { bytes } => agent::Said::Opaque { bytes },
            provider::Part::ToolCall { id, name, arguments } => {
                let call = decode(&name, &arguments, grants, served);
                agent::Said::ToolCall { id, name, input: arguments, call }
            }
            provider::Part::ToolOutput { .. } => panic!("a fake completion contains no tool result"),
        })
        .collect();
    agent::Completion { content, stop, usage }
}

fn decode(name: &[u8], arguments: &[u8], grants: tools::Grants, served: &[agent::Served]) -> agent::Decoded {
    let invalid = || agent::Decoded::Invalid { problem: agent::Problem::UnknownTool };
    if name == b"finish" && served.contains(&agent::Served::Finish) {
        return finish(arguments)
            .map_or_else(invalid, |outcome| agent::Decoded::Served { ask: run::Ask::Finish { outcome } });
    }
    if name == b"subagent" && served.contains(&agent::Served::SubAgent) {
        let Some(brief) = field(arguments, b"brief") else { return invalid() };
        let has = |word: &[u8]| arguments.windows(word.len()).any(|part| part == word);
        let families = run::charter::Families {
            tools: run::charter::Tools {
                inspect: has(b"\"inspect\""),
                modify: has(b"\"modify\""),
                shell: has(b"\"shell\""),
            },
            forge: false,
            agents: has(b"\"agents\":true"),
        };
        return agent::Decoded::Served {
            ask: run::Ask::SubAgent { brief, families, llm: field(arguments, b"llm"), share: None },
        };
    }
    let (session_name, granted): (&[u8], bool) = match name {
        b"read" => (b"read_file", grants.inspect),
        b"list" => (b"list_dir", grants.inspect),
        b"search" => (b"search", grants.inspect),
        b"write" => (b"write_file", grants.modify),
        b"edit" => (b"edit_file", grants.modify),
        b"shell" => (b"run_shell", grants.shell),
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
    }
}

fn finish(arguments: &[u8]) -> Option<run::outcome::Declared> {
    use run::outcome::{Change, Child, Declared, Field, Verdict};
    if !arguments.starts_with(b"{") || !arguments.ends_with(b"}") {
        return None;
    }
    let body = field(arguments, b"body")?;
    if let Some(title) = field(arguments, b"title") {
        return Some(Declared::Change(Change { title, body }));
    }
    let name = field(arguments, b"verdict")?;
    let children: Box<[Child]> = if arguments.windows(10).any(|part| part == b"\"children\"") {
        let child = Child {
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
    Some(Declared::Verdict(Verdict { name, body, children }))
}

/// The copied scripts use unescaped string fields. This recognizes that finite
/// fixture language, never arbitrary JSON or production provider documents.
fn field(arguments: &[u8], name: &[u8]) -> Option<Box<[u8]>> {
    let prefix = [b"\"", name, b"\":\""].concat();
    let start = arguments.windows(prefix.len()).position(|part| part == prefix)? + prefix.len();
    let rest = &arguments[start..];
    let end = rest.iter().position(|byte| *byte == b'"')?;
    Some(rest[..end].into())
}

pub(crate) fn failure(error: provider::Error) -> agent::Failure {
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
