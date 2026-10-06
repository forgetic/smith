//! Scalar host-unit ceilings and exact actual accounting (domain/run.md,
//! sections 9, 10 and 14). Sessions price usage; the run never reprices it.

use skein_lib::Duration;

/// Host-supplied activation ceiling shared by every conversation. Zero turns,
/// spend or time refuses admission before effects. Contract: domain/run.md,
/// sections 3, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Budget {
    /// Maximum actual completions; already admitted work may cross once.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub turns: u32,

    /// Host-unit ceiling; sessions alone price actual usage against it.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub spend: u64,

    /// Monotonic activation time; expiry immediately settles cancellation.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub time: Duration,
}

/// Caller-selected scalar share, clamped to the enclosing run's remainder.
/// Zero in either dimension refuses before opening. Contract: domain/run.md,
/// sections 9 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Share {
    /// Child own Session completion allowance; delegated bills carry no turns.
    /// Contract: domain/run.md, section 14.
    pub turns: u32,

    /// Inclusive subtree host-unit allowance. Contract: domain/run.md, section 14.
    pub spend: u64,
}

/// Host-supplied model rates, interpreted only by Session. The price is the
/// ceiling of ((fresh input + cache writes)*input + cache reads*cached +
/// output*output)/unit, checked before conversion to u64. Zero rates are valid;
/// unit zero refuses the model before discovery. Contract: domain/run.md,
/// sections 3, 9 and 14; domain/session.md, section 6.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Prices {
    /// Fresh input and cache-write rate. Contract: domain/run.md, section 9.
    pub input: u64,

    /// Cache-read rate. Contract: domain/run.md, section 9.
    pub cached: u64,

    /// Output rate. Contract: domain/run.md, section 9.
    pub output: u64,

    /// Positive rate denominator. Contract: domain/run.md, section 9.
    pub unit: u32,
}

/// Actual activation accounting, independent of the budget. Raw counts freeze
/// together at the last representable prefix; units freeze independently.
/// Overflow attestations are sticky and never claim a prefix is the exact
/// final total. Contract: domain/run.md, sections 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Spend {
    /// Actual completion count. Contract: domain/run.md, sections 9 and 10.
    pub turns: u32,

    /// Actual fresh input tokens. Contract: domain/run.md, section 9.
    pub input: u64,

    /// Actual output tokens. Contract: domain/run.md, section 9.
    pub output: u64,

    /// Actual cache-read tokens. Contract: domain/run.md, section 9.
    pub cache_read: u64,

    /// Actual cache-write tokens. Contract: domain/run.md, section 9.
    pub cache_write: u64,

    /// Own completion units summed once across all conversations.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub units: u64,

    /// Sticky attestation that the exact scalar total exceeds u64.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub units_overflow: bool,

    /// Sticky attestation that at least one actual raw counter is unknown.
    /// Contract: domain/run.md, sections 9, 10 and 14.
    pub usage_overflow: bool,
}

/// Scalar or time ceiling that prevents a subsequent completion.
/// Contract: domain/run.md, sections 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exhausted {
    /// Completion ceiling reached. Contract: domain/run.md, section 9.
    Turns,

    /// Host-unit ceiling reached. Contract: domain/run.md, section 9.
    Spend,

    /// Monotonic deadline reached. Contract: domain/run.md, section 10.
    Time,
}

/// Session receiving token cap, distinct from the run's scalar ceiling.
/// Contract: domain/run.md, sections 9 and 14; domain/session.md, section 6.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ReceivingLimit {
    /// Fresh input cap. Contract: domain/session.md, section 6.
    Input,

    /// Output cap. Contract: domain/session.md, section 6.
    Output,

    /// Cache-read cap. Contract: domain/session.md, section 6.
    CacheRead,

    /// Cache-write cap. Contract: domain/session.md, section 6.
    CacheWrite,
}

impl Spend {
    /// Empty actual activation accounting. Contract: domain/run.md, section 9.
    pub const ZERO: Spend = Spend {
        turns: 0,
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        units: 0,
        units_overflow: false,
        usage_overflow: false,
    };

    /// Records actual increments. All raw counters freeze atomically on
    /// overflow; units update independently. Contract: domain/run.md, section 9.
    #[must_use]
    pub fn accumulate(mut self, increment: Spend) -> Spend {
        if !self.usage_overflow {
            let raw = self.raw_sum(increment);
            match raw {
                Some((turns, input, output, cache_read, cache_write)) => {
                    self.turns = turns;
                    self.input = input;
                    self.output = output;
                    self.cache_read = cache_read;
                    self.cache_write = cache_write;
                }
                None => self.usage_overflow = true,
            }
        }
        self.usage_overflow |= increment.usage_overflow;
        if !self.units_overflow {
            match self.units.checked_add(increment.units) {
                Some(units) => self.units = units,
                None => self.units_overflow = true,
            }
        }
        self.units_overflow |= increment.units_overflow;
        self
    }

    fn raw_sum(self, increment: Spend) -> Option<(u32, u64, u64, u64, u64)> {
        Some((
            self.turns.checked_add(increment.turns)?,
            self.input.checked_add(increment.input)?,
            self.output.checked_add(increment.output)?,
            self.cache_read.checked_add(increment.cache_read)?,
            self.cache_write.checked_add(increment.cache_write)?,
        ))
    }

    /// Known terminal raw residual only. Unknown totals produce no inferred
    /// residual and attest overflow. Units are delivered separately by Priced.
    /// Contract: domain/run.md, sections 9 and 10.
    #[must_use]
    pub fn unreported(self, reported: Spend) -> Spend {
        if self.usage_overflow || reported.usage_overflow {
            return Spend { usage_overflow: true, ..Spend::ZERO };
        }
        Spend {
            turns: self.turns.saturating_sub(reported.turns),
            input: self.input.saturating_sub(reported.input),
            output: self.output.saturating_sub(reported.output),
            cache_read: self.cache_read.saturating_sub(reported.cache_read),
            cache_write: self.cache_write.saturating_sub(reported.cache_write),
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
        } else if spent.units >= self.spend || spent.units_overflow {
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
/// Contract: domain/run.md, sections 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionPermit {
    /// Live conversation with remaining global allowance.
    /// Contract: domain/run.md, sections 9 and 14.
    Allowed,

    /// Live conversation prevented by a scalar ceiling.
    /// Contract: domain/run.md, sections 9 and 14.
    Denied(
        /// Exact scalar dimension. Contract: domain/run.md, section 9.
        Exhausted,
    ),

    /// Stale or closing conversation; settle its unsent completion as Closed.
    /// Contract: domain/run.md, sections 10 and 14.
    Closing,
}
