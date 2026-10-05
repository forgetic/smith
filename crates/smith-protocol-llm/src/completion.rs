//! Actual shared completions into root blocks, with immutable declaration lookup.
//! No provider ID, assistant position or declared host effect comes from a caller override.
//! Contract: domain/client.md, sections 2, 4 and 6.

use alloc::boxed::Box;
use core::mem::size_of;

use skein_lib::{List, Token, bytes};
use skein_llm::{self as shared, Error};
use smith_domain::{Event, llm, run, tools};

use crate::types::{Context, ResolvedCall, ToolKind};

/// Consumes one actual shared completed value under its retained receiving contract.
/// Full original fields, optional replay, stop and u64 usage survive. Host input
/// gets real JSON-object attestation; non-host decoding is an explicit checked
/// caller handoff. Excess decoded ownership becomes TooLarge without losing
/// original provider bytes. Invalid caller handoff or incompatible replay is a
/// typed translation refusal; an admitted actual Client replay fits its envelope.
/// Contract: domain/client.md, sections 2, 4 and 6.
pub fn completion(
    context: Context,
    owner: Token,
    completion: shared::Completion,
    resolved: Box<[ResolvedCall]>,
) -> Result<Event, Error> {
    if owner != context.owner {
        return Err(Error::Invalid);
    }
    let shared::Completion { content, stop, usage } = completion;
    let maximum = usize::try_from(context.receiving.max_completion_blocks.min(context.limits.client.dialect.parts))
        .expect("u32 fits usize");
    if content.len() > maximum || resolved.len() > content.len() {
        return Err(Error::Limit);
    }
    let mut decoded_bytes = decoded_cells(&content)?;
    if decoded_bytes > context.receiving.decoded_call_bytes {
        return Err(Error::Limit);
    }
    validate_resolved(&resolved)?;
    let count = u32::try_from(content.len()).or(Err(Error::Limit))?;
    let mut translated = List::with_capacity(count);
    let mut resolutions = List::with_capacity(u32::try_from(resolved.len()).or(Err(Error::Limit))?);
    for call in resolved {
        resolutions.push(Some(call)).expect("one owned slot per admitted resolution");
    }
    let mut held = 0_u64;
    for (position, block) in content.into_iter().enumerate() {
        let position = u32::try_from(position).or(Err(Error::Limit))?;
        let said = block_value(block, position, &context, &mut resolutions, &mut decoded_bytes)?;
        held = held.checked_add(said_bytes(&said)?).ok_or(Error::Limit)?;
        if held > context.receiving.max_completion_bytes {
            return Err(Error::Limit);
        }
        translated.push(said).expect("one translated block per actual input block");
    }
    for resolution in &resolutions {
        if resolution.is_some() {
            return Err(Error::Invalid);
        }
    }
    let stop = match stop {
        shared::Stop::EndTurn => llm::Stop::EndTurn,
        shared::Stop::ToolUse => llm::Stop::ToolUse,
        shared::Stop::MaxTokens => llm::Stop::MaxTokens,
        shared::Stop::Refusal => llm::Stop::Refusal,
    };
    let shared::Usage { input_tokens, output_tokens, cache_read_tokens, cache_write_tokens } = usage;
    let usage = llm::Usage { input_tokens, output_tokens, cache_read_tokens, cache_write_tokens };
    Ok(Event::Completed {
        owner: context.owner,
        completion: llm::Completion { content: translated.into_boxed(), stop, usage },
    })
}

fn validate_resolved(resolved: &[ResolvedCall]) -> Result<(), Error> {
    for (index, call) in resolved.iter().enumerate() {
        for previous in resolved.get(..index).expect("enumerated resolution prefix") {
            if previous.position == call.position {
                return Err(Error::Invalid);
            }
        }
        match &call.call {
            llm::Decoded::Served { ask: run::Ask::Host { .. } } => return Err(Error::Invalid),
            llm::Decoded::Served {
                ask: run::Ask::Finish { .. } | run::Ask::Deliver { .. } | run::Ask::SubAgent { .. } | run::Ask::Wait,
            }
            | llm::Decoded::Owned { .. }
            | llm::Decoded::Invalid { .. } => {}
        }
    }
    Ok(())
}

fn block_value(
    block: shared::Block,
    position: u32,
    context: &Context,
    resolved: &mut List<Option<ResolvedCall>>,
    decoded_bytes: &mut u64,
) -> Result<llm::Said, Error> {
    match block {
        shared::Block::Text { text, replay } => Ok(llm::Said::Text { text, replay: encode_replay(replay, context)? }),
        shared::Block::Refusal { text, replay } => {
            Ok(llm::Said::Refusal { text, replay: encode_replay(replay, context)? })
        }
        shared::Block::Reasoning { replay } => {
            Ok(llm::Said::Opaque { bytes: replay.to_bytes(&context.limits.client.dialect)? })
        }
        shared::Block::ToolCall { id, name, arguments, replay } => {
            let call = decode(position, &name, &arguments, context, resolved, *decoded_bytes)?;
            let call = receiving_call(call, decoded_bytes, context.receiving.decoded_call_bytes);
            Ok(llm::Said::ToolCall { id, name, input: arguments, call, replay: encode_replay(replay, context)? })
        }
        shared::Block::ToolResult { .. } => Err(Error::Invalid),
    }
}

fn encode_replay(value: Option<shared::Replay>, context: &Context) -> Result<Option<llm::Replay>, Error> {
    match value {
        Some(replay) => Ok(Some(llm::Replay { bytes: replay.to_bytes(&context.limits.client.dialect)? })),
        None => Ok(None),
    }
}

fn decode(
    position: u32,
    name: &[u8],
    input: &[u8],
    context: &Context,
    resolved: &mut List<Option<ResolvedCall>>,
    held: u64,
) -> Result<llm::Decoded, Error> {
    for declaration in &context.served {
        match declaration {
            llm::Served::Host(tool) if tool.name.as_ref() == name => {
                let remaining = context.receiving.decoded_call_bytes.checked_sub(held).ok_or(Error::Limit)?;
                return Ok(host(tool, input, context, remaining));
            }
            llm::Served::Host(_)
            | llm::Served::Deliver
            | llm::Served::Finish
            | llm::Served::SubAgent
            | llm::Served::Wait => {}
        }
    }
    let resolution = take_resolution(position, name, input, resolved)?;
    for descriptor in &context.application {
        if descriptor.name.as_ref() != name {
            continue;
        }
        let attested = match crate::prompt::object(input, &context.limits) {
            Ok(value) => value,
            Err(Error::Limit) => return Ok(llm::Decoded::Invalid { problem: llm::Problem::TooLarge }),
            Err(Error::Invalid | Error::Unsupported) => {
                return Ok(llm::Decoded::Invalid { problem: llm::Problem::NotAnObject });
            }
        };
        drop(attested);
        return match resolution {
            Some(call) if matches_kind(descriptor.kind, &call.call) => Ok(call.call),
            Some(_) | None => Err(Error::Invalid),
        };
    }
    Ok(llm::Decoded::Invalid { problem: llm::Problem::UnknownTool })
}

fn take_resolution(
    position: u32,
    name: &[u8],
    input: &[u8],
    resolved: &mut List<Option<ResolvedCall>>,
) -> Result<Option<ResolvedCall>, Error> {
    let mut found = None;
    for (index, slot) in resolved.iter().enumerate() {
        match slot {
            Some(call) if call.position == position => {
                if call.name.as_ref() != name || call.input.as_ref() != input {
                    return Err(Error::Invalid);
                }
                found = Some(u32::try_from(index).or(Err(Error::Limit))?);
                break;
            }
            Some(_) | None => {}
        }
    }
    match found {
        Some(index) => Ok(resolved.get_mut(index).expect("found actual resolution slot").take()),
        None => Ok(None),
    }
}

fn host(declaration: &run::HostTool, input: &[u8], context: &Context, remaining: u64) -> llm::Decoded {
    let attested = match crate::prompt::object(input, &context.limits) {
        Ok(value) => value,
        Err(Error::Limit) => return llm::Decoded::Invalid { problem: llm::Problem::TooLarge },
        Err(Error::Invalid | Error::Unsupported) => {
            return llm::Decoded::Invalid { problem: llm::Problem::NotAnObject };
        }
    };
    drop(attested);
    let wanted = host_wanted(declaration, input);
    match wanted {
        Some(wanted) if wanted <= remaining && input.len() <= run::HostInput::CAPACITY => {}
        Some(_) | None => return llm::Decoded::Invalid { problem: llm::Problem::TooLarge },
    }
    match run::HostInput::attested(bytes::copy_of(input)) {
        Some(input) => llm::Decoded::Served {
            ask: run::Ask::Host { tool: declaration.name.clone(), effect: declaration.effect, input },
        },
        None => llm::Decoded::Invalid { problem: llm::Problem::TooLarge },
    }
}

fn host_wanted(declaration: &run::HostTool, input: &[u8]) -> Option<u64> {
    // This call's wrapper was reserved with every actual tool-call cell
    // before decoding. Only the new declaration/input payload needs room.
    u64::try_from(declaration.name.len()).ok()?.checked_add(u64::try_from(input.len()).ok()?)
}

fn matches_kind(kind: ToolKind, call: &llm::Decoded) -> bool {
    match call {
        llm::Decoded::Invalid { .. } => true,
        llm::Decoded::Owned { call } => match kind {
            ToolKind::Owned(tool) => tools::tool(call) == tool,
            ToolKind::Finish | ToolKind::Deliver | ToolKind::SubAgent | ToolKind::Wait => false,
        },
        llm::Decoded::Served { ask } => {
            let actual = match ask {
                run::Ask::Host { .. } => return false,
                run::Ask::Finish { .. } => ToolKind::Finish,
                run::Ask::Deliver { .. } => ToolKind::Deliver,
                run::Ask::SubAgent { .. } => ToolKind::SubAgent,
                run::Ask::Wait => ToolKind::Wait,
            };
            kind == actual
        }
    }
}

fn decoded_cells(content: &[shared::Block]) -> Result<u64, Error> {
    let mut calls = 0_u64;
    for block in content {
        match block {
            shared::Block::ToolCall { .. } => calls = calls.checked_add(1).ok_or(Error::Limit)?,
            shared::Block::Text { .. }
            | shared::Block::Refusal { .. }
            | shared::Block::Reasoning { .. }
            | shared::Block::ToolResult { .. } => {}
        }
    }
    calls.checked_mul(u64::try_from(size_of::<llm::Decoded>()).or(Err(Error::Limit))?).ok_or(Error::Limit)
}

fn receiving_call(call: llm::Decoded, held: &mut u64, maximum: u64) -> llm::Decoded {
    // Every actual tool call already owns its full fixed fallback cell.
    // Admission charges only additional owned payload; an over-cap result
    // moves to the reserved fixed TooLarge cell without erasing its price.
    let extra = match call.owned_bytes() {
        Some(wanted) => match wanted.checked_sub(u64::try_from(size_of::<llm::Decoded>()).expect("size fits u64")) {
            Some(extra) => extra,
            None => return llm::Decoded::Invalid { problem: llm::Problem::TooLarge },
        },
        None => return llm::Decoded::Invalid { problem: llm::Problem::TooLarge },
    };
    let next = match held.checked_add(extra) {
        Some(next) if next <= maximum => next,
        Some(_) | None => return llm::Decoded::Invalid { problem: llm::Problem::TooLarge },
    };
    *held = next;
    call
}

fn said_bytes(said: &llm::Said) -> Result<u64, Error> {
    let mut held = u64::try_from(size_of::<llm::Said>()).or(Err(Error::Limit))?;
    match said {
        llm::Said::Text { text, replay } | llm::Said::Refusal { text, replay } => {
            held = held.checked_add(size(text.len())?).ok_or(Error::Limit)?;
            held = held.checked_add(replay_bytes(replay)?).ok_or(Error::Limit)?;
        }
        llm::Said::Opaque { bytes } => held = held.checked_add(size(bytes.len())?).ok_or(Error::Limit)?,
        llm::Said::ToolCall { id, name, input, call, replay } => {
            for field in [id, name, input] {
                held = held.checked_add(size(field.len())?).ok_or(Error::Limit)?;
            }
            let decoded = call.owned_bytes().ok_or(Error::Limit)?;
            let inline = u64::try_from(size_of::<llm::Decoded>()).or(Err(Error::Limit))?;
            held = held.checked_add(decoded.checked_sub(inline).ok_or(Error::Limit)?).ok_or(Error::Limit)?;
            held = held.checked_add(replay_bytes(replay)?).ok_or(Error::Limit)?;
        }
    }
    Ok(held)
}

fn replay_bytes(replay: &Option<llm::Replay>) -> Result<u64, Error> {
    match replay {
        Some(replay) => size(replay.bytes.len()),
        None => Ok(0),
    }
}

fn size(value: usize) -> Result<u64, Error> {
    u64::try_from(value).or(Err(Error::Limit))
}
