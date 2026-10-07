//! Whole bounded application declarations and concrete conversations into Skein.
//! Parsing is generic JSON attestation, never schema-policy validation.

use alloc::boxed::Box;
use core::mem::size_of;

use skein_json::Token;
use skein_lib::{List, bytes};
use skein_llm::{self as shared, Error};
use smith_domain::{llm, tools};

use crate::types::{Context, Input, Prepared, ResultText, ToolKind, ToolSchema};
use crate::{Limits, worst_case};

/// Consumes one actual root request and prepares its one shared Client.
/// Receiving compatibility, inventory and JSON/replay admission precede wire effects.
/// The returned context owns declarations until actual terminal translation.
/// Refusal is typed Invalid/Limit/Unsupported and has no lower right to settle.
pub fn prepare(input: Input, limits: &Limits) -> Result<Prepared, Error> {
    let Input { owner, prompt, endpoint_name, endpoint, credential, application, results, receiving } = input;
    if prompt.endpoint != endpoint_name {
        return Err(Error::Invalid);
    }
    worst_case(limits, &receiving).ok_or(Error::Limit)?;
    let ceiling = prompt.max_tokens;
    let grants = prompt.tools;
    let (mut translated, served) = translate(prompt, &application, &results, limits)?;
    translated.output_ceiling(endpoint.provider, ceiling)?;
    let client = shared::client::Client::prepare(
        shared::Call { owner, endpoint, credential, prompt: translated },
        &limits.client,
    )?;
    Ok(Prepared { client, context: Context { owner, grants, served, application, receiving, limits: *limits } })
}

/// Pure application prompt translation, including whole supplied tool schemas.
/// The caller applies the shared `output_ceiling` for its configured endpoint
/// before directly preparing a Client; prepare performs that step itself.
/// Missing or conflicting application descriptors/results are Invalid, exceeded
/// bounds are Limit and incompatible replay remains Unsupported.
pub fn prompt(
    input: llm::Prompt,
    application: &[ToolSchema],
    results: &[ResultText],
    limits: &Limits,
) -> Result<shared::Prompt, Error> {
    let (translated, _served) = translate(input, application, results, limits)?;
    Ok(translated)
}

fn translate(
    input: llm::Prompt,
    application: &[ToolSchema],
    results: &[ResultText],
    limits: &Limits,
) -> Result<(shared::Prompt, Box<[llm::Served]>), Error> {
    inventory(&input, application, results, limits)?;
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
    let mut used = List::with_capacity(u32::try_from(results.len()).or(Err(Error::Limit))?);
    for _result in results {
        used.push(false).expect("one usage flag per admitted supplied result");
    }
    let mut translated = List::with_capacity(limits.client.dialect.parts);
    for (message_index, message) in messages.into_iter().enumerate() {
        let index = u32::try_from(message_index).or(Err(Error::Limit))?;
        let mut content = List::with_capacity(u32::try_from(message.content.len()).or(Err(Error::Limit))?);
        for (block_index, block) in message.content.into_iter().enumerate() {
            let position = u32::try_from(block_index).or(Err(Error::Limit))?;
            let block = translate_block(block, index, position, results, &mut used, limits)?;
            content.push(block).expect("room for each admitted source block");
        }
        let role = match message.role {
            llm::Role::User => shared::Role::User,
            llm::Role::Assistant => shared::Role::Assistant,
        };
        translated.push(shared::Message { role, content: content.into_boxed() }).expect("admitted message count");
    }
    for used in &used {
        if !*used {
            return Err(Error::Invalid);
        }
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

fn inventory(
    input: &llm::Prompt,
    application: &[ToolSchema],
    results: &[ResultText],
    limits: &Limits,
) -> Result<(), Error> {
    let maximum = usize::try_from(limits.client.dialect.parts).expect("u32 fits usize");
    if application.len() > maximum || results.len() > maximum || input.messages.len() > maximum {
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
    results_admitted(results, limits)?;
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

fn results_admitted(results: &[ResultText], limits: &Limits) -> Result<(), Error> {
    let mut total = 0_u64;
    for (index, result) in results.iter().enumerate() {
        total = total.checked_add(size(size_of::<ResultText>())?).ok_or(Error::Limit)?;
        total = total.checked_add(size(result.id.len())?).ok_or(Error::Limit)?;
        total = total.checked_add(size(result.text.len())?).ok_or(Error::Limit)?;
        for previous in results.get(..index).expect("enumerated result prefix") {
            if previous.message == result.message && previous.block == result.block {
                return Err(Error::Invalid);
            }
        }
    }
    if total > limits.result_bytes { Err(Error::Limit) } else { Ok(()) }
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

fn translate_block(
    block: llm::Block,
    message: u32,
    position: u32,
    results: &[ResultText],
    used: &mut List<bool>,
    limits: &Limits,
) -> Result<shared::Block, Error> {
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
            let (text, error) = result_text(result, &id, message, position, results, used, limits)?;
            Ok(shared::Block::ToolResult { id, text, is_error: error })
        }
    }
}

fn result_text(
    result: llm::Returned,
    id: &[u8],
    message: u32,
    block: u32,
    results: &[ResultText],
    used: &mut List<bool>,
    limits: &Limits,
) -> Result<(Box<[u8]>, bool), Error> {
    match result {
        llm::Returned::Text { text, error, replay } => match replay {
            Some(_) => Err(Error::Unsupported),
            None => Ok((text, error)),
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
        llm::Returned::Owned { outcome: _ } => {
            for (index, result) in results.iter().enumerate() {
                if result.message == message && result.block == block {
                    if result.id.as_ref() != id {
                        return Err(Error::Invalid);
                    }
                    let index = u32::try_from(index).or(Err(Error::Limit))?;
                    let used = used.get_mut(index).expect("one usage flag per supplied result");
                    if *used {
                        return Err(Error::Invalid);
                    }
                    *used = true;
                    return Ok((result.text.clone(), result.error));
                }
            }
            Err(Error::Invalid)
        }
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
