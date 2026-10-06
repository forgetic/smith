//! Concrete session history (domain/session.md, sections 3 and 12). Encoding these
//! versioned values into bytes belongs to the protocol layer, never the domain.

use crate::{
    Spec,
    llm::{Endpoint, Message, Usage},
};
use alloc::boxed::Box;

/// Concrete session transcript version emitted and accepted at the domain boundary.
pub const VERSION: u16 = 2;

/// Session-issued position of an opener call in its concrete logical transcript.
/// This fixed name survives ticket translation; provider ids and callback tokens
/// are not its identity. The host scopes it by the logical run, never a live slab.
///
/// Contract: domain/session.md, sections 3 and 5; domain/run.md, section 8.2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Origin {
    /// One-based accepted completion sequence, including a restored prefix.
    /// Checked exhaustion refuses the next completion's effects.
    ///
    /// Contract: domain/session.md, sections 3 and 5.
    pub sequence: u32,
    /// Zero-based assistant block position, including preceding non-call blocks.
    /// The receiving session bounds message storage and refuses unrepresentable block counts before effects.
    ///
    /// Contract: domain/session.md, sections 3 and 5.
    pub position: u32,
}

/// Charter prices per `unit` tokens. Cache writes are new input, cache reads
/// use `cached`. Each completion rounds its exact combined charge upwards.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Prices {
    /// Integer price for fresh input and cache-write tokens per unit.
    pub input: u64,
    /// Price for cache-read input tokens in the configured unit.
    pub cached: u64,
    /// Integer price for output tokens per unit.
    pub output: u64,
    /// Token denominator for checked, upward-rounded integer pricing; zero is refused.
    pub unit: u32,
}

impl Prices {
    /// Checked integer charge for provider usage, rounded upward; returns `None` for overflow or a zero unit.
    #[must_use]
    pub fn price(self, usage: Usage) -> Option<u64> {
        if self.unit == 0 {
            return None;
        }
        let input = u128::from(usage.input_tokens).checked_add(u128::from(usage.cache_write_tokens))?;
        let input = input.checked_mul(u128::from(self.input))?;
        let cached = u128::from(usage.cache_read_tokens).checked_mul(u128::from(self.cached))?;
        let output = u128::from(usage.output_tokens).checked_mul(u128::from(self.output))?;
        let sum = input.checked_add(cached)?.checked_add(output)?;
        let unit = u128::from(self.unit);
        let rounded = sum.checked_div(unit)?.checked_add(u128::from(sum.checked_rem(unit)? != 0))?;
        u64::try_from(rounded).ok()
    }
}

/// Parent-owned concrete opening, validated before any tools or provider effect.
/// `transcript: None` starts fresh under the same reservation and terminal rules.
#[derive(PartialEq, Eq, Debug)]
pub struct Opening {
    /// The original session opening, validated before replaying any history.
    pub spec: Spec,
    /// Provider dialect/schema identity, paired with the configured endpoint.
    pub dialect: u32,
    /// Immutable integer prices used to charge each accepted completion.
    pub prices: Prices,
    /// Allowance supplied at admission, checked before starting more work.
    pub budget: u64,
    /// Optional concrete history whose version, endpoint and dialect must match.
    pub transcript: Option<Transcript>,
}

/// A settled completion and its surrounding messages. Assistant call names,
/// provider ids and input bytes are concrete; delegated result bytes are too.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Turn {
    /// Concrete transcript encoding version; unsupported versions are refused.
    pub version: u16,
    /// Configured provider endpoint identity; the domain never resolves its address.
    pub endpoint: Endpoint,
    /// Configured provider dialect identity; replay requires an exact match.
    pub dialect: u32,
    /// Monotonic turn sequence within the concrete transcript.
    pub sequence: u32,
    /// Provider-reported token usage, charged exactly once when its completion ends.
    pub usage: Usage,
    /// Cumulative spend of this activation, children included.
    pub spent: u64,
    /// Oldest-first conversation messages, with provider call/result pairing preserved.
    pub messages: Box<[Message]>,
}

/// Concrete versioned session history, with provider endpoint and dialect identity checked before replay.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Transcript {
    /// Concrete transcript encoding version; unsupported versions are refused.
    pub version: u16,
    /// Configured provider endpoint identity; the domain never resolves its address.
    pub endpoint: Endpoint,
    /// Configured provider dialect identity; replay requires an exact match.
    pub dialect: u32,
    /// Settled concrete turns, oldest first, validated within the transcript byte cap.
    pub turns: Box<[Turn]>,
    /// Concrete user results committed after the last told turn.
    pub after: Box<[Message]>,
}

/// Reason admission or transcript replay was rejected before unsupported history could run.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// The durable format version is unsupported.
    Version,
    /// History names a different configured provider endpoint.
    Endpoint,
    /// History names a different configured provider dialect.
    Dialect,
    /// Input has invalid syntax or violates the codec's structural contract.
    Malformed,
    /// History contains a live ticket rather than concrete replay data.
    Unresolved,
    /// A configured ownership, count or encoded-byte cap would be exceeded.
    TooLarge,
}

impl Transcript {
    /// Complete retained history ownership, including allocated Turn and Message
    /// envelopes separately from block/payload storage. The protocol bounds
    /// input counts; the root checks this count before retaining Start context,
    /// while session performs the semantic admission before any effects.
    /// `None` means checked ownership arithmetic overflow; the caller refuses
    /// before retaining the history or starting effects.
    /// Contract: domain/session.md, sections 3 and 12; programming-model.md, section 6.3.
    #[must_use]
    pub fn owned_bytes(&self) -> Option<u64> {
        let mut bytes = u64::try_from(size_of::<Turn>()).ok()?.checked_mul(u64::try_from(self.turns.len()).ok()?)?;
        for turn in &self.turns {
            bytes = bytes.checked_add(messages_bytes(&turn.messages)?)?;
        }
        bytes.checked_add(messages_bytes(&self.after)?)
    }
}

fn messages_bytes(messages: &[Message]) -> Option<u64> {
    let mut bytes = u64::try_from(size_of::<Message>()).ok()?.checked_mul(u64::try_from(messages.len()).ok()?)?;
    for message in messages {
        bytes = bytes.checked_add(crate::session::content_cost(&message.content)?)?;
    }
    Some(bytes)
}
