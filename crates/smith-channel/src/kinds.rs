//! Smith's one-version body table. It keeps no state and knows no domain;
//! [`schema`] projects the generated body bounds into Skein's channel table.
//! Contract: `protocol/channel.md`, sections 2-4.

use skein_channel::{Direction, Kind, Role, Schema, Term, Version};
use skein_lib::List;

use crate::v1;

/// A version-one body's number, sender, admission obligation and codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::partial_pub_fields,
    reason = "the codec selector is internal; number, direction and obligation are public"
)]
pub struct KindRule {
    pub kind: u16,
    pub direction: Direction,
    pub required: bool,
    body: Body,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    Start,
    Message,
    HostAnswer,
    Acknowledge,
    GrantRefresh,
    Cancel,
    Admitted,
    Call,
    Withdraw,
    Turn,
    Waiting,
    Long,
    LongDone,
    Rejected,
    Exhausted,
    Fact,
    Answer,
}

/// Why a body maximum cannot be represented in the channel's frame length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Generated bound arithmetic exceeded its checked range.
    Body,
    /// The body exceeds a frame's `u32` length.
    Frame,
}

/// The 17 kinds required by channel version one, in wire number order.
pub const V1: &[KindRule] = &[
    KindRule { kind: 0x0100, direction: Direction::FromInitiator, required: true, body: Body::Start },
    KindRule { kind: 0x0101, direction: Direction::FromInitiator, required: true, body: Body::Message },
    KindRule { kind: 0x0102, direction: Direction::FromInitiator, required: true, body: Body::HostAnswer },
    KindRule { kind: 0x0103, direction: Direction::FromInitiator, required: true, body: Body::Acknowledge },
    KindRule { kind: 0x0104, direction: Direction::FromInitiator, required: true, body: Body::GrantRefresh },
    KindRule { kind: 0x0105, direction: Direction::FromInitiator, required: true, body: Body::Cancel },
    KindRule { kind: 0x0106, direction: Direction::FromResponder, required: true, body: Body::Admitted },
    KindRule { kind: 0x0107, direction: Direction::FromResponder, required: true, body: Body::Call },
    KindRule { kind: 0x0108, direction: Direction::FromResponder, required: true, body: Body::Withdraw },
    KindRule { kind: 0x0109, direction: Direction::FromResponder, required: true, body: Body::Turn },
    KindRule { kind: 0x010a, direction: Direction::FromResponder, required: true, body: Body::Waiting },
    KindRule { kind: 0x010b, direction: Direction::FromResponder, required: true, body: Body::Long },
    KindRule { kind: 0x010c, direction: Direction::FromResponder, required: true, body: Body::LongDone },
    KindRule { kind: 0x010d, direction: Direction::FromResponder, required: true, body: Body::Rejected },
    KindRule { kind: 0x010e, direction: Direction::FromResponder, required: true, body: Body::Exhausted },
    KindRule { kind: 0x010f, direction: Direction::FromResponder, required: true, body: Body::Fact },
    KindRule { kind: 0x0110, direction: Direction::FromResponder, required: true, body: Body::Answer },
];

/// Build the version-one channel schema from receiving codec limits.
#[expect(clippy::manual_let_else, reason = "the strict step subset uses exhaustive matches")]
pub fn schema(limits: &v1::Limits) -> Result<Schema, Overflow> {
    let mut kinds = List::with_capacity(17);
    for rule in V1 {
        let largest = match rule.body {
            Body::Start => v1::Start::worst_case_bytes(limits),
            Body::Message => v1::Message::worst_case_bytes(limits),
            Body::HostAnswer => v1::HostAnswer::worst_case_bytes(limits),
            Body::Acknowledge => v1::Acknowledge::worst_case_bytes(limits),
            Body::GrantRefresh => v1::GrantRefresh::worst_case_bytes(limits),
            Body::Cancel => v1::Cancel::worst_case_bytes(limits),
            Body::Admitted => v1::Admitted::worst_case_bytes(limits),
            Body::Call => v1::Call::worst_case_bytes(limits),
            Body::Withdraw => v1::Withdraw::worst_case_bytes(limits),
            Body::Turn => v1::Turn::worst_case_bytes(limits),
            Body::Waiting => v1::Waiting::worst_case_bytes(limits),
            Body::Long => v1::Long::worst_case_bytes(limits),
            Body::LongDone => v1::LongDone::worst_case_bytes(limits),
            Body::Rejected => v1::Rejected::worst_case_bytes(limits),
            Body::Exhausted => v1::Exhausted::worst_case_bytes(limits),
            Body::Fact => v1::Fact::worst_case_bytes(limits),
            Body::Answer => v1::Answer::worst_case_bytes(limits),
        };
        let largest = largest.ok_or(Overflow::Body)?;
        let largest = match u32::try_from(largest) {
            Ok(largest) => largest,
            Err(_) => return Err(Overflow::Frame),
        };
        kinds.push(Kind { kind: rule.kind, direction: rule.direction, largest }).expect("V1 has exactly 17 kinds");
    }
    let mut versions = List::with_capacity(1);
    versions.push(Version { version: 1, kinds }).expect("one version has one slot");
    Ok(Schema { magic: *b"smth", versions })
}

/// Find the first required kind that the peer cannot receive at our sending bound.
/// Contract: `protocol/channel.md`, section 2.
#[must_use]
pub fn peer_terms_gap(schema: &Schema, role: Role, version: u16, terms: &List<Term>) -> Option<u16> {
    let sending = match role {
        Role::Initiator => Direction::FromInitiator,
        Role::Responder => Direction::FromResponder,
    };
    let table = schema.version(version)?;
    for rule in V1 {
        if rule.direction == sending && rule.required {
            let own = table.kind(rule.kind)?;
            let mut accepted = false;
            for term in terms {
                if term.kind == rule.kind && term.largest >= own.largest {
                    accepted = true;
                }
            }
            if !accepted {
                return Some(rule.kind);
            }
        }
    }
    None
}
