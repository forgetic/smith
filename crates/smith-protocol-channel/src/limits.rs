//! Agent-side opening limits (protocol/channel.md, sections 2 and 7).
use core::mem::size_of;
use skein_channel::{Lower, SchemaError};
use skein_lib::{List, Queue};

use crate::{Component, OpenEvent};

/// Codec, framing and queue allowances fixed before the channel opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub bodies: smith_channel::Limits,
    /// This agent's bounded charter decoder allowances.
    pub charter: smith_charter::v1::Limits,
    /// This agent's bounded saved-turn decoder allowances.
    pub transcript: smith_transcript::v2::Limits,
    pub channel: skein_channel::Limits,
    /// Maximum configured endpoint names kept by this component.
    pub endpoints: u32,
    /// Maximum live host tool and delivery calls awaiting a channel answer.
    pub calls: u32,
}

/// A checked channel cannot be built from the supplied limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A codec body exceeds the frame range.
    Codec(smith_channel::Overflow),
    /// Skein rejected the schema or framing limits.
    Channel(SchemaError),
    /// Configured endpoint names exceed their count or byte allowance.
    Endpoints,
    /// The accepted result is outside the configured charter codec limits.
    Result(smith_charter::v1::Problem),
    /// A bounded result list had no room for a field or item.
    ResultCapacity,
    /// A saved answer decodes but fails the domain's sealed result bounds.
    InvalidSavedAnswer,
    /// A concrete turn cannot be written to the durable transcript format.
    Transcript(smith_domain_session::record::Refusal),
    /// A concrete turn exceeds the transcript codec bounds.
    TranscriptBody(smith_transcript::v2::Problem),
    /// The service sent admission or an answer out of order.
    Order,
    /// A live host operation name is repeated, unknown, or beyond capacity.
    Calls,
    /// The measured result did not fit its output writer.
    ResultWrite(skein_lib::Overflow),
    /// A domain answer exceeds the configured channel body limits.
    Answer(smith_channel::v1::Problem),
    /// A measured application frame could not be built.
    Frame(skein_channel::FrameError),
}

impl From<smith_channel::v1::Problem> for Error {
    fn from(problem: smith_channel::v1::Problem) -> Error {
        Error::Answer(problem)
    }
}

impl From<smith_transcript::v2::Problem> for Error {
    fn from(problem: smith_transcript::v2::Problem) -> Error {
        Error::TranscriptBody(problem)
    }
}

impl From<skein_channel::FrameError> for Error {
    fn from(problem: skein_channel::FrameError) -> Error {
        Error::Frame(problem)
    }
}

impl From<smith_charter::v1::Problem> for Error {
    fn from(problem: smith_charter::v1::Problem) -> Error {
        Error::Result(problem)
    }
}

impl From<skein_lib::Overflow> for Error {
    fn from(problem: skein_lib::Overflow) -> Error {
        Error::ResultWrite(problem)
    }
}

/// Slots reserved by the caller at each entry point during opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaxOut {
    pub to_domain: u32,
    pub below: u32,
}

/// Opening emits at most one owner event and six lower operations per step.
#[must_use]
pub const fn max_out(_limits: &Limits) -> MaxOut {
    MaxOut { to_domain: 1, below: 6 }
}

/// Price the machine, the opening event queue and caller output queues.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let schema = smith_channel::schema(&limits.bodies).ok()?;
    let mut duplicate = List::<skein_channel::Version>::worst_case(schema.versions.capacity())?;
    for version in &schema.versions {
        duplicate = duplicate.checked_add(List::<skein_channel::Kind>::worst_case(version.kinds.capacity())?)?;
    }
    let bytes = skein_channel::worst_case(&schema, &limits.channel)?
        .checked_add(duplicate)?
        .checked_add(u64::from(limits.endpoints).checked_mul(u64::try_from(size_of::<crate::Endpoint>()).ok()?)?)?
        .checked_add(u64::from(limits.endpoints).checked_mul(u64::from(smith_charter::CEILINGS.llm_endpoint))?)?
        .checked_add(smith_channel::Start::worst_case_bytes(&limits.bodies)?.checked_mul(2)?)?
        .checked_add(smith_charter::Charter::worst_case_bytes(&limits.charter)?.checked_mul(2)?)?
        .checked_add(
            smith_transcript::Turn::worst_case_heap(&limits.transcript)?
                .checked_mul(u64::from(limits.bodies.start_transcript))?,
        )?
        .checked_add(Queue::<skein_channel::Event>::worst_case(8)?)?
        .checked_add(Queue::<OpenEvent>::worst_case(1)?)?
        .checked_add(Queue::<Lower>::worst_case(6)?)?;
    bytes
        .checked_add(skein_lib::Map::<crate::component::CallKey, crate::component::CallRoute>::worst_case(
            limits.calls,
        )?)?
        .checked_add(u64::try_from(size_of::<Component>()).ok()?)
}
