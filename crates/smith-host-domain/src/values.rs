//! Bounded parent payload values (domain/host.md, sections 3, 6 and 9.4).
//! This stateless vocabulary retains Smith's typed policy and concrete history,
//! with their checked ownership. It knows neither a codec nor an agent entity;
//! constructors bound payloads, views borrow them and `into_value` moves them.
//! The agent vocabulary supplies the value semantics and ownership counts;
//! these envelopes own the common receiving contract of both capabilities.

use smith_domain::{self as agent, run};

/// A parent's bounded charter, sent in a start without encoding.
#[derive(PartialEq, Eq, Debug)]
pub struct Charter {
    value: run::Charter,
    bytes: u64,
}

impl Charter {
    /// Admit the owned policy under the receiving byte allowance.
    #[must_use]
    pub fn new(value: run::Charter, capacity: u64) -> Option<Self> {
        let bytes = value.owned_bytes()?;
        if bytes > capacity {
            return None;
        }
        Some(Self { value, bytes })
    }

    /// The typed policy, with no process or credential state.
    #[must_use]
    pub const fn value(&self) -> &run::Charter {
        &self.value
    }

    /// Move the policy to an admitted run or a protocol encoder.
    #[must_use]
    pub fn into_value(self) -> run::Charter {
        self.value
    }

    /// Checked boxed-value ownership, in bytes.
    #[must_use]
    pub const fn owned_bytes(&self) -> u64 {
        self.bytes
    }
}

/// Concrete history supplied by a parent when the run resumes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Transcript {
    value: agent::Transcript,
    bytes: u64,
}

impl Transcript {
    /// Admit the complete history under the receiving byte allowance.
    #[must_use]
    pub fn new(value: agent::Transcript, capacity: u64) -> Option<Self> {
        let bytes = value.owned_bytes()?;
        if bytes > capacity {
            return None;
        }
        Some(Self { value, bytes })
    }

    /// The concrete history kept by the parent.
    #[must_use]
    pub const fn value(&self) -> &agent::Transcript {
        &self.value
    }

    /// Move the history to the run or a protocol encoder.
    #[must_use]
    pub fn into_value(self) -> agent::Transcript {
        self.value
    }

    /// Checked boxed-value ownership, in bytes.
    #[must_use]
    pub const fn owned_bytes(&self) -> u64 {
        self.bytes
    }
}

/// One bounded concrete turn told to the parent and kept until its acknowledgement.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TurnValue {
    value: agent::Turn,
    bytes: u64,
}

impl TurnValue {
    /// Admit the message envelopes and payloads under the receiving allowance.
    #[must_use]
    pub fn new(value: agent::Turn, capacity: u64) -> Option<Self> {
        let bytes = value.owned_bytes()?;
        if bytes > capacity {
            return None;
        }
        Some(Self { value, bytes })
    }

    /// The complete concrete turn, including provider replay values.
    #[must_use]
    pub const fn value(&self) -> &agent::Turn {
        &self.value
    }

    /// Move the concrete turn to its parent or store encoder.
    #[must_use]
    pub fn into_value(self) -> agent::Turn {
        self.value
    }

    /// Checked boxed-value ownership, in bytes.
    #[must_use]
    pub const fn owned_bytes(&self) -> u64 {
        self.bytes
    }
}

/// One bounded accepted declaration told as the run's result.
#[derive(PartialEq, Eq, Debug)]
pub struct Declared {
    value: run::outcome::Declared,
    bytes: u64,
}

impl Declared {
    /// Admit the declared form and fields under the receiving allowance.
    #[must_use]
    pub fn new(value: run::outcome::Declared, capacity: u64) -> Option<Self> {
        let bytes = run::outcome::owned_bytes(&value)?;
        if bytes > capacity {
            return None;
        }
        Some(Self { value, bytes })
    }

    /// The accepted form and its values, never encoded result bytes.
    #[must_use]
    pub const fn value(&self) -> &run::outcome::Declared {
        &self.value
    }

    /// Move the declaration to its parent or protocol encoder.
    #[must_use]
    pub fn into_value(self) -> run::outcome::Declared {
        self.value
    }

    /// Checked boxed-value ownership, in bytes.
    #[must_use]
    pub const fn owned_bytes(&self) -> u64 {
        self.bytes
    }
}

/// Bounded named fields submitted to a parent's durable delivery operation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fields {
    value: run::outcome::Change,
    bytes: u64,
}

impl Fields {
    /// Admit the complete field names and values under the receiving allowance.
    #[must_use]
    pub fn new(value: run::outcome::Change, capacity: u64) -> Option<Self> {
        let bytes = value.owned_bytes()?;
        if bytes > capacity {
            return None;
        }
        Some(Self { value, bytes })
    }

    /// The delivery's uninterpreted named values.
    #[must_use]
    pub const fn value(&self) -> &run::outcome::Change {
        &self.value
    }

    /// Move the named values to the parent or protocol encoder.
    #[must_use]
    pub fn into_value(self) -> run::outcome::Change {
        self.value
    }

    /// Checked boxed-value ownership, in bytes.
    #[must_use]
    pub const fn owned_bytes(&self) -> u64 {
        self.bytes
    }
}
