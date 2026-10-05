//! One budget per run (domain/run.md, section 14): turns, tokens and time, across
//! every conversation the run opens.

use skein_lib::Duration;

/// What a run may spend across all its conversations: completions, tokens of
/// each kind as providers count them, and time from its admission.
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Budget {
    /// Completion count, bounded across the enclosing run or session.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub turns: u32,
    /// Fresh input-token allowance or accepted count, as the provider reports it.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub input: u64,
    /// Output-token allowance or accepted count, as the provider reports it.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub output: u64,
    /// Cache-read token allowance supplied by the host.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub cache_read: u64,
    /// Cache-write token allowance supplied by the host.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub cache_write: u64,
    /// Monotonic wall-time allowance from admission.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub time: Duration,
}

/// What was spent: completions, and tokens of each kind as the provider counts
/// them.
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Spend {
    /// Completion count, bounded across the enclosing run or session.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub turns: u32,
    /// Fresh input-token allowance or accepted count, as the provider reports it.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub input: u64,
    /// Output-token allowance or accepted count, as the provider reports it.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub output: u64,
    /// Cache-read token usage reported by the provider.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub cache_read: u64,
    /// Cache-write token usage reported by the provider.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub cache_write: u64,
}

/// The part of a budget that ran out.
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exhausted {
    /// The completion-count allowance is exhausted.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Turns,
    /// The fresh input-token allowance is exhausted.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Input,
    /// The output-token allowance is exhausted.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Output,
    /// The cache-read token allowance is exhausted.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    CacheRead,
    /// The cache-write token allowance is exhausted.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    CacheWrite,
    /// The injected monotonic deadline is reached.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Time,
}

impl Spend {
    /// No accepted completions or token usage yet.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub const ZERO: Spend = Spend { turns: 0, input: 0, output: 0, cache_read: 0, cache_write: 0 };

    /// Adds accepted cumulative usage by dimension, saturating rather than wrapping on overflow.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub const fn saturating_add(self, other: Spend) -> Spend {
        Spend {
            turns: self.turns.saturating_add(other.turns),
            input: self.input.saturating_add(other.input),
            output: self.output.saturating_add(other.output),
            cache_read: self.cache_read.saturating_add(other.cache_read),
            cache_write: self.cache_write.saturating_add(other.cache_write),
        }
    }

    /// What this spends beyond `other`, kind by kind, or nothing of a kind it
    /// spends no more of.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub const fn saturating_sub(self, other: Spend) -> Spend {
        Spend {
            turns: self.turns.saturating_sub(other.turns),
            input: self.input.saturating_sub(other.input),
            output: self.output.saturating_sub(other.output),
            cache_read: self.cache_read.saturating_sub(other.cache_read),
            cache_write: self.cache_write.saturating_sub(other.cache_write),
        }
    }
}

impl Budget {
    /// The engine's token allowance, allocated by the agent: half input,
    /// quarter output, eighth cache reads, and the remainder cache writes.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    #[must_use]
    pub const fn from_tokens(turns: u32, tokens: u64, time: Duration) -> Budget {
        let input = tokens / 2;
        let output = tokens / 4;
        let cache_read = tokens / 8;
        let cache_write = tokens.saturating_sub(input).saturating_sub(output).saturating_sub(cache_read);
        Budget { turns, input, output, cache_read, cache_write, time }
    }

    /// Whether this budget asks for no more than `limit`, part by part.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn within(&self, limit: &Budget) -> bool {
        self.turns <= limit.turns
            && self.input <= limit.input
            && self.output <= limit.output
            && self.cache_read <= limit.cache_read
            && self.cache_write <= limit.cache_write
            && self.time <= limit.time
    }

    /// Whether this budget leaves room for any work: a turn, its input and
    /// output, and time. Caching may be given no budget.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn is_workable(&self) -> bool {
        self.turns > 0 && self.input > 0 && self.output > 0 && self.time > Duration::ZERO
    }

    /// The first part of this budget that `spent` has gone past, if any. Time
    /// is the run's alarm, not a part that is spent.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn overspent(&self, spent: Spend) -> Option<Exhausted> {
        if spent.turns > self.turns {
            Some(Exhausted::Turns)
        } else if spent.input > self.input {
            Some(Exhausted::Input)
        } else if spent.output > self.output {
            Some(Exhausted::Output)
        } else if spent.cache_read > self.cache_read {
            Some(Exhausted::CacheRead)
        } else if spent.cache_write > self.cache_write {
            Some(Exhausted::CacheWrite)
        } else {
            None
        }
    }

    /// What is left of this budget once `spent` is spent, with `time` to go.
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub(crate) fn remainder(&self, spent: Spend, time: Duration) -> Budget {
        Budget {
            turns: self.turns.saturating_sub(spent.turns),
            input: self.input.saturating_sub(spent.input),
            output: self.output.saturating_sub(spent.output),
            cache_read: self.cache_read.saturating_sub(spent.cache_read),
            cache_write: self.cache_write.saturating_sub(spent.cache_write),
            time,
        }
    }
}
