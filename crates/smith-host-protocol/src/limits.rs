//! Host-side opening limits (protocol/channel.md, sections 2 and 7).
use core::mem::size_of;
use skein_channel::{Lower, SchemaError};
use skein_lib::{List, Queue};

use crate::{Component, OpenEvent};

/// Codec, framing and queue allowances fixed before the channel opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub bodies: smith_channel::Limits,
    pub channel: skein_channel::Limits,
}

/// A checked channel cannot be built from the supplied limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A codec body exceeds the frame range.
    Codec(smith_channel::Overflow),
    /// Skein rejected the schema or framing limits.
    Channel(SchemaError),
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
        .checked_add(Queue::<skein_channel::Event>::worst_case(8)?)?
        .checked_add(Queue::<OpenEvent>::worst_case(1)?)?
        .checked_add(Queue::<Lower>::worst_case(6)?)?;
    bytes.checked_add(u64::try_from(size_of::<Component>()).ok()?)
}
