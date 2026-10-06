//! Scalar host-unit ceilings and exact accounting (domain/run.md,
//! sections 9 and 10). Sessions price usage; the run never reprices it.

use skein_lib::Duration;

/// Host-supplied activation ceiling shared by every conversation. Zero turns,
/// spend or time refuses admission before effects.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Budget {
    /// Maximum completions; already admitted work may cross once.
    pub turns: u32,

    /// Host-unit ceiling; sessions alone price usage against it.
    pub spend: u64,

    /// Monotonic activation time; expiry immediately settles cancellation.
    pub time: Duration,
}

/// Caller-selected scalar share, clamped to the enclosing run's remainder.
/// Zero in either dimension refuses before opening.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Share {
    /// Child own Session completion allowance; delegated bills carry no turns.
    pub turns: u32,

    /// Inclusive subtree host-unit allowance.
    pub spend: u64,
}

/// Host-supplied model rates, interpreted only by Session. The price is the
/// ceiling of ((fresh input + cache writes)*input + cache reads*cached +
/// output*output)/unit, checked before conversion to u64. Zero rates are valid;
/// unit zero refuses the model before discovery.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Prices {
    /// Fresh input and cache-write rate.
    pub input: u64,

    /// Cache-read rate.
    pub cached: u64,

    /// Output rate.
    pub output: u64,

    /// Positive rate denominator.
    pub unit: u32,
}

/// Exact charged activation accounting, independent of the budget. A failed
/// addition leaves every field unchanged.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Spend {
    /// Actual completion count.
    pub turns: u32,

    /// Actual fresh input tokens.
    pub input: u64,

    /// Actual output tokens.
    pub output: u64,

    /// Actual cache-read tokens.
    pub cache_read: u64,

    /// Actual cache-write tokens.
    pub cache_write: u64,

    /// Own completion units summed once across all conversations.
    pub units: u64,
}

/// Scalar or time ceiling that prevents a subsequent completion.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exhausted {
    /// Completion ceiling reached.
    Turns,

    /// Host-unit ceiling reached.
    Spend,

    /// Monotonic deadline reached.
    Time,
    /// A session exceeded its receiving token ceiling for one kind.
    Tokens(ReceivingLimit),
    /// Checked priced or raw usage arithmetic could not represent the total.
    Overflow(Overflow),
}

/// Which checked budget arithmetic overflowed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Overflow {
    /// Priced spend could not be represented.
    Spend,
    /// Raw completion usage could not be represented.
    Usage,
}

/// Session receiving token cap, distinct from the run's scalar ceiling.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ReceivingLimit {
    /// Fresh input cap.
    Input,

    /// Output cap.
    Output,

    /// Cache-read cap.
    CacheRead,

    /// Cache-write cap.
    CacheWrite,
}

impl Spend {
    /// Empty activation accounting.
    pub const ZERO: Spend = Spend { turns: 0, input: 0, output: 0, cache_read: 0, cache_write: 0, units: 0 };

    /// Add only a fully representable increment. No part of a failed addition
    /// is charged.
    #[must_use]
    pub fn accumulate(self, increment: Spend) -> Option<Spend> {
        Some(Spend {
            turns: self.turns.checked_add(increment.turns)?,
            input: self.input.checked_add(increment.input)?,
            output: self.output.checked_add(increment.output)?,
            cache_read: self.cache_read.checked_add(increment.cache_read)?,
            cache_write: self.cache_write.checked_add(increment.cache_write)?,
            units: self.units.checked_add(increment.units)?,
        })
    }

    /// Exact terminal raw residual. Units are delivered separately by Priced.
    #[must_use]
    pub fn unreported(self, reported: Spend) -> Spend {
        Spend {
            turns: self.turns.checked_sub(reported.turns).expect("reported completions belong to session"),
            input: self.input.checked_sub(reported.input).expect("reported usage belongs to session"),
            output: self.output.checked_sub(reported.output).expect("reported usage belongs to session"),
            cache_read: self.cache_read.checked_sub(reported.cache_read).expect("reported usage belongs to session"),
            cache_write: self.cache_write.checked_sub(reported.cache_write).expect("reported usage belongs to session"),
            ..Spend::ZERO
        }
    }
}

impl Budget {
    pub(crate) fn within(&self, limit: &Budget) -> bool {
        self.turns <= limit.turns && self.spend <= limit.spend && self.time <= limit.time
    }

    pub(crate) fn is_workable(&self) -> bool {
        self.turns > 0 && self.spend > 0 && self.time > Duration::ZERO
    }

    pub(crate) fn exhausted(&self, spent: Spend) -> Option<Exhausted> {
        if spent.turns >= self.turns {
            Some(Exhausted::Turns)
        } else if spent.units >= self.spend {
            Some(Exhausted::Spend)
        } else {
            None
        }
    }

    pub(crate) fn remainder(&self, spent: Spend, time: Duration) -> Budget {
        Budget { turns: self.turns.saturating_sub(spent.turns), spend: self.spend.saturating_sub(spent.units), time }
    }
}

/// Pure root admission decision before any Client effect. Closing has its
/// existing terminal right and must never be reported as scalar exhaustion.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionPermit {
    /// Live conversation with remaining global allowance.
    Allowed,

    /// Live conversation prevented by a scalar ceiling.
    Denied(
        /// Exact scalar dimension.
        Exhausted,
    ),

    /// Stale or closing conversation; settle its unsent completion as Closed.
    Closing,
}
