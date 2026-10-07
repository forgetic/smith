//! Live host operations become named channel records (protocol/channel.md,
//! sections 3–5; domain/run.md, sections 5 and 8).
//!
//! This module keeps no state. The component retains each live relay until its
//! one host answer; these encoders know neither host policy nor callbacks.

use alloc::boxed::Box;
use skein_channel::{Frame, frame_writer};
use skein_lib::{Duration, List, Writer};
use smith_channel as wire;
use smith_domain::run;

use crate::Error;

/// Encode a declared host tool relay without interpreting its attested input.
pub(crate) fn host_call(
    name: run::CallName,
    tool: Box<[u8]>,
    effect: run::HostEffect,
    input: run::HostInput,
    deadline: Duration,
    limits: &wire::Limits,
) -> Result<Frame, Error> {
    let ask =
        wire::Ask::Host(wire::HostAsk::new(limits, wire::HostAskParts { tool, input: Box::from(input.bytes()) })?);
    call(name, effect, ask, deadline, limits)
}

/// Encode one checked delivery request with the domain's host-named fields.
pub(crate) fn delivery_call(
    name: run::CallName,
    change: run::outcome::Change,
    deadline: Duration,
    limits: &wire::Limits,
) -> Result<Frame, Error> {
    let Ok(count) = u32::try_from(change.fields.len()) else {
        return Err(Error::ResultCapacity);
    };
    let mut fields = List::with_capacity(count);
    for field in change.fields {
        let value = wire::Field::new(limits, wire::FieldParts { name: field.name, text: field.value })?;
        if fields.push(value).is_err() {
            return Err(Error::ResultCapacity);
        }
    }
    let ask = wire::Ask::Deliver(wire::DeliverAsk::new(limits, wire::DeliverAskParts { fields })?);
    call(name, run::HostEffect::Write, ask, deadline, limits)
}

fn call(
    name: run::CallName,
    effect: run::HostEffect,
    ask: wire::Ask,
    deadline: Duration,
    limits: &wire::Limits,
) -> Result<Frame, Error> {
    let name = wire::CallName::new(
        limits,
        wire::CallNameParts { activation: name.activation, completion: name.completion, position: name.position },
    )?;
    let effect = match effect {
        run::HostEffect::Read => wire::Effect::Read,
        run::HostEffect::Write => wire::Effect::Write,
    };
    let record = wire::Call::new(limits, wire::CallParts { name, effect, deadline, ask })?;
    let Ok(length) = usize::try_from(record.measure()) else {
        return Err(Error::ResultCapacity);
    };
    let mut body = Writer::new(length);
    record.encode(&mut body)?;
    let mut frame = frame_writer(0x0107, record.measure())?;
    frame.put(&body.finish())?;
    Ok(frame.finish()?)
}

/// Ask the host to settle one relay without releasing its durable name.
pub(crate) fn withdraw(name: run::CallName, limits: &wire::Limits) -> Result<Frame, Error> {
    let name = wire::CallName::new(
        limits,
        wire::CallNameParts { activation: name.activation, completion: name.completion, position: name.position },
    )?;
    let record = wire::Withdraw::new(limits, wire::WithdrawParts { name })?;
    let Ok(length) = usize::try_from(record.measure()) else {
        return Err(Error::ResultCapacity);
    };
    let mut body = Writer::new(length);
    record.encode(&mut body)?;
    let mut frame = frame_writer(0x0108, record.measure())?;
    frame.put(&body.finish())?;
    Ok(frame.finish()?)
}

/// Interpret one host terminal for a declared tool; temporary refusals stay distinct.
pub(crate) fn host_reply(reply: &wire::Reply) -> Option<run::HostReply> {
    let value = match reply {
        wire::Reply::Host(host) => match run::HostAnswer::new(Box::from(host.text()), host.error()) {
            Some(answer) => run::HostReply::Answered(answer),
            None => run::HostReply::TooLarge,
        },
        wire::Reply::Busy => run::HostReply::Busy,
        wire::Reply::Unavailable => run::HostReply::Unanswered(run::Unanswered::Lost),
        wire::Reply::Withdrawn => run::HostReply::Withdrawn,
        wire::Reply::TooLarge => run::HostReply::TooLarge,
        wire::Reply::Delivery(_) => return None,
    };
    Some(value)
}
