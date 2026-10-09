//! Whole bounded application declarations and concrete conversations into Skein.
//! Parsing is generic JSON attestation, never schema-policy validation.

use alloc::boxed::Box;

use skein_json::Token;
use skein_lib::{List, bytes};
use skein_llm::{self as shared, Error};
use smith_domain::{llm, run, tools};

use crate::types::{Context, ToolKind, ToolSchema};
use crate::{Limits, worst_case};

/// Translate an admitted prompt and retain its declarations for one terminal.
/// The component and protocol worlds use this same preparation before skein
/// creates the client; receiving compatibility is checked before any effect.
pub fn prepare_prompt(
    owner: skein_lib::Token,
    prompt: llm::Prompt,
    application: Box<[ToolSchema]>,
    receiving: crate::Receiving,
    limits: &Limits,
) -> Result<(shared::Prompt, Context), Error> {
    worst_case(limits, &receiving).ok_or(Error::Limit)?;
    let grants = prompt.tools;
    let (translated, served) = translate(prompt, &application, limits)?;
    Ok((translated, Context { owner, grants, served, application, receiving, limits: *limits }))
}

/// Prepare the translated prompt and retained context for the connection
/// component, which prepares the sole shared Client when it admits Start.
pub(crate) fn prepare_component(
    owner: skein_lib::Token,
    prompt: llm::Prompt,
    outcome: &run::outcome::OutcomeSpec,
    deliver: Option<&run::outcome::ChangeSpec>,
    receiving: crate::Receiving,
    limits: &Limits,
) -> Result<(shared::Prompt, Context), Error> {
    worst_case(limits, &receiving).ok_or(Error::Limit)?;
    let application =
        crate::tools::schemas_for_contract(&prompt, outcome, deliver, limits.client.dialect.document_bytes)?;
    let application = crate::tools::with_shell_deadlines(application, limits.shell_default, limits.shell_maximum);
    prepare_prompt(owner, prompt, application, receiving, limits)
}

/// Pure application prompt translation, including whole supplied tool schemas.
/// The caller applies the shared `output_ceiling` for its configured endpoint
/// before directly preparing a Client.
/// Missing or conflicting application descriptors are Invalid, exceeded
/// bounds are Limit and incompatible replay remains Unsupported.
pub fn prompt(input: llm::Prompt, application: &[ToolSchema], limits: &Limits) -> Result<shared::Prompt, Error> {
    let (translated, _served) = translate(input, application, limits)?;
    Ok(translated)
}

fn translate(
    input: llm::Prompt,
    application: &[ToolSchema],
    limits: &Limits,
) -> Result<(shared::Prompt, Box<[llm::Served]>), Error> {
    inventory(&input, application, limits)?;
    let llm::Prompt { endpoint: _, model, system, tools: _, served, messages, max_tokens } = input;
    let mut offered = List::with_capacity(limits.client.dialect.parts);
    for descriptor in &served {
        match descriptor {
            llm::Served::Host(tool) => {
                let schema = object(&tool.schema, limits)?;
                offered
                    .push(shared::Tool { name: tool.name.clone(), description: tool.description.clone(), schema })
                    .or(Err(Error::Limit))?;
            }
            llm::Served::Deliver | llm::Served::Finish | llm::Served::SubAgent | llm::Served::Wait => {}
        }
    }
    for descriptor in application {
        let schema = object(&descriptor.schema, limits)?;
        offered
            .push(shared::Tool { name: descriptor.name.clone(), description: descriptor.description.clone(), schema })
            .or(Err(Error::Limit))?;
    }
    let mut translated = List::with_capacity(limits.client.dialect.parts);
    for message in messages {
        let mut content = List::with_capacity(u32::try_from(message.content.len()).or(Err(Error::Limit))?);
        for block in message.content {
            let block = translate_block(block, limits)?;
            content.push(block).expect("room for each admitted source block");
        }
        let role = match message.role {
            llm::Role::User => shared::Role::User,
            llm::Role::Assistant => shared::Role::Assistant,
        };
        translated.push(shared::Message { role, content: content.into_boxed() }).expect("admitted message count");
    }
    Ok((
        shared::Prompt {
            model,
            instructions: system,
            tools: offered.into_boxed(),
            messages: translated.into_boxed(),
            reasoning_effort: None,
            cache_key: None,
            max_output_tokens: Some(max_tokens),
        },
        served,
    ))
}

fn inventory(input: &llm::Prompt, application: &[ToolSchema], limits: &Limits) -> Result<(), Error> {
    let maximum = usize::try_from(limits.client.dialect.parts).expect("u32 fits usize");
    if application.len() > maximum || input.messages.len() > maximum {
        return Err(Error::Limit);
    }
    let mut count = application.len();
    let mut declaration_bytes = 0_u64;
    for (index, descriptor) in input.served.iter().enumerate() {
        served_unique(descriptor, input.served.get(..index).expect("enumerated served prefix"))?;
        declaration_bytes = declaration_bytes.checked_add(size(size_of::<llm::Served>())?).ok_or(Error::Limit)?;
        match descriptor {
            llm::Served::Host(tool) => {
                count = count.checked_add(1).ok_or(Error::Limit)?;
                for field in [&tool.name, &tool.description, &tool.schema] {
                    declaration_bytes = declaration_bytes.checked_add(size(field.len())?).ok_or(Error::Limit)?;
                }
                for other in application {
                    if other.name == tool.name {
                        return Err(Error::Invalid);
                    }
                }
            }
            llm::Served::Deliver | llm::Served::Finish | llm::Served::SubAgent | llm::Served::Wait => {}
        }
    }
    if count > maximum || input.served.len() > maximum {
        return Err(Error::Limit);
    }
    for (index, descriptor) in application.iter().enumerate() {
        if !offered(descriptor.kind, input) || descriptor.name.is_empty() || !fixed_name(descriptor) {
            return Err(Error::Invalid);
        }
        for previous in application.get(..index).expect("enumerated application prefix") {
            if previous.kind == descriptor.kind || previous.name == descriptor.name {
                return Err(Error::Invalid);
            }
        }
        declaration_bytes = declaration_bytes.checked_add(size(size_of::<ToolSchema>())?).ok_or(Error::Limit)?;
        for field in [&descriptor.name, &descriptor.description, &descriptor.schema] {
            declaration_bytes = declaration_bytes.checked_add(size(field.len())?).ok_or(Error::Limit)?;
        }
    }
    if declaration_bytes > limits.tool_bytes {
        return Err(Error::Limit);
    }
    required(input, application)?;
    history_admitted(&input.messages, limits)
}

fn served_unique(descriptor: &llm::Served, previous: &[llm::Served]) -> Result<(), Error> {
    for prior in previous {
        let repeated = match descriptor {
            llm::Served::Host(tool) => match prior {
                llm::Served::Host(other) => tool.name == other.name,
                llm::Served::Finish | llm::Served::Deliver | llm::Served::SubAgent | llm::Served::Wait => false,
            },
            llm::Served::Finish => match prior {
                llm::Served::Finish => true,
                llm::Served::Host(_) | llm::Served::Deliver | llm::Served::SubAgent | llm::Served::Wait => false,
            },
            llm::Served::Deliver => match prior {
                llm::Served::Deliver => true,
                llm::Served::Host(_) | llm::Served::Finish | llm::Served::SubAgent | llm::Served::Wait => false,
            },
            llm::Served::SubAgent => match prior {
                llm::Served::SubAgent => true,
                llm::Served::Host(_) | llm::Served::Finish | llm::Served::Deliver | llm::Served::Wait => false,
            },
            llm::Served::Wait => match prior {
                llm::Served::Wait => true,
                llm::Served::Host(_) | llm::Served::Finish | llm::Served::Deliver | llm::Served::SubAgent => false,
            },
        };
        if repeated {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}

fn required(input: &llm::Prompt, application: &[ToolSchema]) -> Result<(), Error> {
    for tool in [
        tools::Tool::Read,
        tools::Tool::List,
        tools::Tool::Search,
        tools::Tool::Write,
        tools::Tool::Edit,
        tools::Tool::Shell,
    ] {
        let kind = ToolKind::Owned(tool);
        if offered(kind, input) && !has(kind, application) {
            return Err(Error::Invalid);
        }
    }
    for descriptor in &input.served {
        let kind = match descriptor {
            llm::Served::Host(_) => continue,
            llm::Served::Deliver => ToolKind::Deliver,
            llm::Served::Finish => ToolKind::Finish,
            llm::Served::SubAgent => ToolKind::SubAgent,
            llm::Served::Wait => ToolKind::Wait,
        };
        if !has(kind, application) {
            return Err(Error::Invalid);
        }
    }
    Ok(())
}

fn has(kind: ToolKind, application: &[ToolSchema]) -> bool {
    for descriptor in application {
        if descriptor.kind == kind {
            return true;
        }
    }
    false
}

fn fixed_name(descriptor: &ToolSchema) -> bool {
    match descriptor.kind {
        ToolKind::Owned(tool) => match tool {
            tools::Tool::Read => descriptor.name.as_ref() == b"read",
            tools::Tool::List => descriptor.name.as_ref() == b"list",
            tools::Tool::Search => descriptor.name.as_ref() == b"search",
            tools::Tool::Write => descriptor.name.as_ref() == b"write",
            tools::Tool::Edit => descriptor.name.as_ref() == b"edit",
            tools::Tool::Shell => descriptor.name.as_ref() == b"shell",
        },
        ToolKind::Finish => descriptor.name.as_ref() == b"finish",
        ToolKind::Deliver => descriptor.name.as_ref() == b"deliver",
        ToolKind::SubAgent => descriptor.name.as_ref() == b"sub_agent",
        ToolKind::Wait => descriptor.name.as_ref() == b"wait",
    }
}

fn offered(kind: ToolKind, input: &llm::Prompt) -> bool {
    match kind {
        ToolKind::Owned(tool) => match tool {
            tools::Tool::Read | tools::Tool::List | tools::Tool::Search => input.tools.inspect,
            tools::Tool::Write | tools::Tool::Edit => input.tools.modify,
            tools::Tool::Shell => input.tools.shell,
        },
        ToolKind::Finish | ToolKind::Deliver | ToolKind::SubAgent | ToolKind::Wait => {
            for descriptor in &input.served {
                let same = match descriptor {
                    llm::Served::Host(_) => false,
                    llm::Served::Finish => kind == ToolKind::Finish,
                    llm::Served::Deliver => kind == ToolKind::Deliver,
                    llm::Served::SubAgent => kind == ToolKind::SubAgent,
                    llm::Served::Wait => kind == ToolKind::Wait,
                };
                if same {
                    return true;
                }
            }
            false
        }
    }
}

fn history_admitted(messages: &[llm::Message], limits: &Limits) -> Result<(), Error> {
    let maximum = usize::try_from(limits.client.dialect.parts).expect("u32 fits usize");
    let mut blocks = 0_usize;
    for message in messages {
        blocks = blocks.checked_add(message.content.len()).ok_or(Error::Limit)?;
        if blocks > maximum {
            return Err(Error::Limit);
        }
    }
    Ok(())
}

fn translate_block(block: llm::Block, limits: &Limits) -> Result<shared::Block, Error> {
    match block {
        llm::Block::Text { text, replay } => Ok(shared::Block::Text { text, replay: replay_value(replay, limits)? }),
        llm::Block::Refusal { text, replay } => {
            Ok(shared::Block::Refusal { text, replay: replay_value(replay, limits)? })
        }
        llm::Block::Opaque { bytes } => {
            Ok(shared::Block::Reasoning { replay: shared::Replay::from_bytes(&bytes, &limits.client.dialect)? })
        }
        llm::Block::ToolCall { id, name, input, replay } => {
            Ok(shared::Block::ToolCall { id, name, arguments: input, replay: replay_value(replay, limits)? })
        }
        llm::Block::ToolResult { id, result } => {
            let (text, error) = result_text(result, limits)?;
            Ok(shared::Block::ToolResult { id, text, is_error: error })
        }
    }
}

fn result_text(result: llm::Returned, limits: &Limits) -> Result<(Box<[u8]>, bool), Error> {
    match result {
        llm::Returned::Text { text, error, replay } => match replay {
            Some(_) => Err(Error::Unsupported),
            None => Ok((crate::render::cut_text(text, limits.rendered_result), error)),
        },
        llm::Returned::Withdrawn => Ok((bytes::copy_of(b"withdrawn"), true)),
        llm::Returned::NotRun => Ok((bytes::copy_of(b"not run"), true)),
        llm::Returned::Invalid { problem } => {
            Ok((crate::failure::problem(problem, limits.client.dialect.string_bytes)?, true))
        }
        llm::Returned::Served { returned, error } => {
            let value = smith_domain::feedback(returned, u64::from(limits.client.dialect.string_bytes))
                .or(Err(Error::Limit))?;
            if value.error != error {
                return Err(Error::Invalid);
            }
            Ok((value.text, value.error))
        }
        llm::Returned::Owned { outcome } => crate::render::render_outcome(&outcome, limits.rendered_result),
    }
}

fn replay_value(replay: Option<llm::Replay>, limits: &Limits) -> Result<Option<shared::Replay>, Error> {
    match replay {
        Some(replay) => Ok(Some(shared::Replay::from_bytes(&replay.bytes, &limits.client.dialect)?)),
        None => Ok(None),
    }
}

pub(crate) fn object(data: &[u8], limits: &Limits) -> Result<shared::Json, Error> {
    let value = match shared::Json::from_bytes(data, &limits.client.dialect) {
        Ok(value) => value,
        Err(error) => return Err(shared::document_error(error)),
    };
    if value.as_tokens().first() != Some(&Token::ObjectStart) {
        return Err(Error::Invalid);
    }
    Ok(value)
}

fn size(value: usize) -> Result<u64, Error> {
    u64::try_from(value).or(Err(Error::Limit))
}
