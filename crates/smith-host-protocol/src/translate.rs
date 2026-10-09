//! Host-owned Start data becomes one checked channel frame (protocol/channel.md, section 3).
//! The host service supplies paths and credential values; the host domain never keeps them.
use alloc::boxed::Box;
use skein_channel::{Frame, frame_writer};
use skein_lib::{List, Writer};
use smith_channel as wire;
use smith_host_domain::{self as host, channel};

use crate::Error;

/// Values held by the host service beside its domain's names, in Start order.
#[derive(Debug)]
pub struct Values {
    /// One filesystem path for each directory, in domain order.
    pub paths: Box<[Box<[u8]>]>,
    /// One credential value for each grant, in domain order.
    pub credentials: Box<[Box<[u8]>]>,
}

/// Encode a domain Start and its service-owned values as the first downlink frame.
#[expect(clippy::manual_let_else, clippy::single_match, reason = "exhaustive typed Start translation without closures")]
pub fn encode_start(
    start: channel::Start,
    window: channel::Window,
    values: Values,
    limits: &wire::Limits,
    charter_limits: &smith_charter::v2::Limits,
    transcript_limits: &smith_transcript::v2::Limits,
    endpoints: &smith_protocol_channel::Endpoints,
) -> Result<Frame, Error> {
    let mut messages = List::with_capacity(limits.start_messages);
    for message in start.messages {
        let item = wire::Message::new(
            limits,
            wire::MessageParts { name: message.name.raw(), label: message.label, text: message.text },
        )?;
        if messages.push(item).is_err() {
            return Err(Error::MissingValue);
        }
    }
    let workspace = encode_workspace(&start.directories, &values.paths, limits)?;
    let mut transcript = List::with_capacity(limits.start_transcript);
    match &start.transcript {
        Some(history) => {
            for turn in &history.value().turns {
                let body = match smith_protocol_channel::encode_turn(turn, transcript_limits, endpoints) {
                    Ok(body) => body,
                    Err(_) => return Err(Error::Value),
                };
                if transcript.push(body).is_err() {
                    return Err(Error::MissingValue);
                }
            }
        }
        None => {}
    }
    let charter = match smith_protocol_channel::encode_charter(start.charter.value(), charter_limits, endpoints) {
        Ok(body) => body,
        Err(_) => return Err(Error::Value),
    };
    let mut answered = List::with_capacity(limits.start_answered);
    for call in start.answered {
        let name = wire::CallName::new(
            limits,
            wire::CallNameParts {
                activation: call.name.activation,
                completion: call.name.completion,
                position: call.name.position,
            },
        )?;
        let reply = encode_saved_reply(call.reply, limits)?;
        let item = wire::AnsweredCall::new(limits, wire::AnsweredCallParts { name, tool: call.tool, reply })?;
        if answered.push(item).is_err() {
            return Err(Error::MissingValue);
        }
    }
    if start.grants.len() != values.credentials.len() {
        return Err(Error::MissingValue);
    }
    let mut grants = List::with_capacity(limits.start_grants);
    for (grant, credential) in start.grants.into_iter().zip(values.credentials) {
        let value = wire::GrantValue::new(limits, wire::GrantValueParts { credential })?;
        let item = wire::Grant::new(
            limits,
            wire::GrantParts { account: grant.account, generation: grant.generation, valid: grant.valid, value },
        )?;
        if grants.push(item).is_err() {
            return Err(Error::MissingValue);
        }
    }
    let window = wire::Window::new(limits, wire::WindowParts { turns: window.turns, bytes: window.bytes })?;
    let record = wire::Start::new(
        limits,
        wire::StartParts {
            messages,
            activation: start.activation,
            charter,
            workspace,
            transcript,
            answered,
            grants,
            window,
        },
    )?;
    let mut body = Writer::new(usize::try_from(record.measure()).expect("measured body fits usize"));
    record.encode(&mut body)?;
    let mut frame = frame_writer(0x0100, record.measure())?;
    frame.put(&body.finish())?;
    Ok(frame.finish()?)
}

fn encode_workspace(
    directories: &[channel::Directory],
    paths: &[Box<[u8]>],
    limits: &wire::Limits,
) -> Result<Option<wire::Workspace>, Error> {
    if directories.is_empty() {
        if paths.is_empty() {
            return Ok(None);
        }
        return Err(Error::MissingValue);
    }
    if directories.len() != paths.len() {
        return Err(Error::MissingValue);
    }
    let mut items = List::with_capacity(limits.workspace_directories);
    for (directory, path) in directories.iter().zip(paths) {
        let mut conflicts = List::with_capacity(limits.directory_conflicts);
        for conflict in &directory.conflicts {
            if conflicts.push(conflict.clone()).is_err() {
                return Err(Error::MissingValue);
            }
        }
        let item = wire::Directory::new(
            limits,
            wire::DirectoryParts {
                name: directory.name.clone(),
                path: path.clone(),
                writable: directory.writable,
                git: directory.git,
                conflicts,
            },
        )?;
        if items.push(item).is_err() {
            return Err(Error::MissingValue);
        }
    }
    Ok(Some(wire::Workspace::new(limits, wire::WorkspaceParts { directories: items })?))
}

fn encode_saved_reply(reply: channel::SavedReply, limits: &wire::Limits) -> Result<wire::SavedReply, Error> {
    let result = match reply {
        channel::SavedReply::Host { error, body } => {
            wire::SavedReply::Host(wire::HostReply::new(limits, wire::HostReplyParts { error, text: body })?)
        }
        channel::SavedReply::Delivery(delivery) => {
            let value = encode_delivery(*delivery, limits)?;
            wire::SavedReply::Delivery(wire::DeliveryReply::new(limits, wire::DeliveryReplyParts { value })?)
        }
        channel::SavedReply::TooLarge => wire::SavedReply::TooLarge,
    };
    Ok(wire::SavedReply::new(limits, result)?)
}

pub(crate) fn encode_delivery(delivery: host::Delivery, limits: &wire::Limits) -> Result<wire::Delivery, Error> {
    let result = match delivery {
        host::Delivery::Delivered(delivered) => {
            let mut receipts = List::with_capacity(limits.delivered_receipts);
            for receipt in delivered.receipts() {
                let item = wire::Receipt::new(
                    limits,
                    wire::ReceiptParts { directory: receipt.directory(), text: Box::from(receipt.text()) },
                )?;
                if receipts.push(item).is_err() {
                    return Err(Error::MissingValue);
                }
            }
            wire::Delivery::Delivered(wire::Delivered::new(limits, wire::DeliveredParts { receipts })?)
        }
        host::Delivery::Nothing => wire::Delivery::Nothing,
        host::Delivery::Refused(refused) => {
            let marker = match refused.marker() {
                Some(marker) => Some(wire::Marker::new(
                    limits,
                    wire::MarkerParts { directory: marker.directory(), path: Box::from(marker.path()) },
                )?),
                None => None,
            };
            wire::Delivery::Refused(wire::DeliveryRefusal::new(
                limits,
                wire::DeliveryRefusalParts { marker, explanation: Box::from(refused.explanation()) },
            )?)
        }
        host::Delivery::Failed(failed) => {
            let reason = match failed.reason {
                host::DeliveryReason::Unreachable => wire::DeliveryReason::Unreachable,
                host::DeliveryReason::RefusedByTarget => wire::DeliveryReason::RefusedByTarget,
                host::DeliveryReason::TimedOut => wire::DeliveryReason::TimedOut,
                host::DeliveryReason::Broken => wire::DeliveryReason::Broken,
                host::DeliveryReason::TooLarge => wire::DeliveryReason::TooLarge,
                host::DeliveryReason::Missing => wire::DeliveryReason::Missing,
                host::DeliveryReason::Busy => wire::DeliveryReason::Busy,
                host::DeliveryReason::Unavailable => wire::DeliveryReason::Unavailable,
                host::DeliveryReason::Cancelled => wire::DeliveryReason::Cancelled,
                host::DeliveryReason::Unknown => wire::DeliveryReason::Unknown,
            };
            wire::Delivery::Failed(wire::DeliveryFailure::new(
                limits,
                wire::DeliveryFailureParts {
                    directory: failed.directory,
                    reason,
                    diagnostic: Box::from(failed.diagnostic.output()),
                    dropped: failed.diagnostic.cut(),
                },
            )?)
        }
        host::Delivery::Stale => wire::Delivery::Stale,
    };
    Ok(wire::Delivery::new(limits, result)?)
}

/// Move one bounded channel call into the host domain's typed operation face.
pub(crate) fn decode_ask(call: &wire::Call) -> Result<Option<channel::Ask>, Error> {
    let ask = match call.ask() {
        wire::Ask::Host(host) => {
            let effect = match call.effect() {
                wire::Effect::Read => channel::Effect::Read,
                wire::Effect::Write => channel::Effect::Write,
            };
            channel::Ask::Host { tool: Box::from(host.tool()), effect, body: Box::from(host.input()) }
        }
        wire::Ask::Deliver(deliver) => {
            if call.effect() != &wire::Effect::Write {
                return Ok(None);
            }
            let mut fields = List::with_capacity(deliver.fields().len());
            for field in deliver.fields() {
                let item =
                    smith_domain::run::outcome::Field { name: Box::from(field.name()), value: Box::from(field.text()) };
                if fields.push(item).is_err() {
                    return Err(Error::MissingValue);
                }
            }
            let value = smith_domain::run::outcome::Change { fields: fields.into_boxed() };
            channel::Ask::Deliver { fields: host::Fields::new(value, u64::MAX).ok_or(Error::Value)? }
        }
    };
    Ok(Some(ask))
}

/// Encode one host-domain terminal under the durable call name it answers.
pub(crate) fn encode_reply(
    name: channel::CallName,
    reply: channel::Reply,
    limits: &wire::Limits,
) -> Result<Frame, Error> {
    let name = wire::CallName::new(
        limits,
        wire::CallNameParts { activation: name.activation, completion: name.completion, position: name.position },
    )?;
    let reply = match reply {
        channel::Reply::Host { error, body } => {
            wire::Reply::Host(wire::HostReply::new(limits, wire::HostReplyParts { error, text: body })?)
        }
        channel::Reply::Delivery(delivery) => wire::Reply::Delivery(wire::DeliveryReply::new(
            limits,
            wire::DeliveryReplyParts { value: encode_delivery(delivery, limits)? },
        )?),
        channel::Reply::Busy => wire::Reply::Busy,
        channel::Reply::Unavailable => wire::Reply::Unavailable,
        channel::Reply::Withdrawn => wire::Reply::Withdrawn,
        channel::Reply::TooLarge => wire::Reply::TooLarge,
    };
    let record = wire::HostAnswer::new(limits, wire::HostAnswerParts { name, reply })?;
    let Ok(length) = usize::try_from(record.measure()) else {
        return Err(Error::MissingValue);
    };
    let mut body = Writer::new(length);
    record.encode(&mut body)?;
    let mut frame = frame_writer(0x0102, record.measure())?;
    frame.put(&body.finish())?;
    Ok(frame.finish()?)
}
