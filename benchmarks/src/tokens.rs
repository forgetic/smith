//! Per-response usage retains its convention and conversation scope.
//! `TokenLedger` deduplicates immutable response ids; `records` emits one record
//! per scope and a total that names every gap (benchmarks.md, section 8.2).
//! Root-only usage never stands in for child, compaction or helper usage.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Measure;

/// The fixed normalisation selected by an adapter's recorded format.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Convention {
    /// Input includes cached reads, and output includes reasoning.
    Codex,
    /// Input excludes cache reads and writes; output includes thinking.
    ClaudeCode,
    /// Events report fresh input and cache writes separately.
    SmithEvents,
}

/// The conversation role supplied by the observer for one scope.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ScopeKind {
    /// The root conversation of the attempt.
    Root,
    /// A child conversation linked to its parent's scope id.
    Child { parent: String },
    /// A compaction of its parent's conversation.
    Compaction { parent: String },
    /// A helper model operating for its parent's conversation.
    Helper { parent: String, model: String },
    /// The total over every declared scope.
    Total,
}

/// Normalised token measurements supplied by a usage observer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TokenCounts {
    pub fresh: Measure<u64>,
    pub cache_read: Measure<u64>,
    pub cache_write: Measure<u64>,
    /// Includes reasoning or thinking where the provider includes it.
    pub output: Measure<u64>,
    pub reasoning: Measure<u64>,
}

impl TokenCounts {
    fn unavailable(reason: &str) -> Self {
        Self {
            fresh: Measure::unavailable(reason),
            cache_read: Measure::unavailable(reason),
            cache_write: Measure::unavailable(reason),
            output: Measure::unavailable(reason),
            reasoning: Measure::unavailable(reason),
        }
    }

    fn add(&self, other: &Self) -> Result<Self, String> {
        Ok(Self {
            fresh: add_measure(&self.fresh, &other.fresh)?,
            cache_read: add_measure(&self.cache_read, &other.cache_read)?,
            cache_write: add_measure(&self.cache_write, &other.cache_write)?,
            output: add_measure(&self.output, &other.output)?,
            reasoning: add_measure(&self.reasoning, &other.reasoning)?,
        })
    }
}

/// One scope's token evidence supplied by the adapter to an attempt result.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TokenRecord {
    pub scope: String,
    pub role: ScopeKind,
    pub convention: Convention,
    pub usage: TokenCounts,
}

/// A provider's immutable response usage supplied to the ledger by an observer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Usage {
    /// Codex input includes cached input and output includes reasoning.
    Codex { input: u64, cached_input: u64, output: u64 },
    /// Claude's fresh input and writes are added; cache reads stay separate.
    ClaudeCode { input: u64, cache_read: u64, cache_write: u64, output: u64 },
    /// smith's event fields remain unavailable wherever the event reports null.
    SmithEvents {
        input: Option<u64>,
        cache_read: Option<u64>,
        cache_write: Option<u64>,
        output: Option<u64>,
        reasoning: Option<u64>,
    },
}

impl Usage {
    /// Identify the mapping before any records are added to a scope.
    #[must_use]
    pub const fn convention(&self) -> Convention {
        match self {
            Self::Codex { input: _, cached_input: _, output: _ } => Convention::Codex,
            Self::ClaudeCode { input: _, cache_read: _, cache_write: _, output: _ } => Convention::ClaudeCode,
            Self::SmithEvents { input: _, cache_read: _, cache_write: _, output: _, reasoning: _ } => {
                Convention::SmithEvents
            }
        }
    }

    /// Apply the convention, rejecting impossible or overflowing provider values.
    pub fn normalise(&self) -> Result<TokenCounts, String> {
        match *self {
            Self::Codex { input, cached_input, output } => Ok(TokenCounts {
                fresh: observed(input.checked_sub(cached_input).ok_or("cached input exceeds input")?),
                cache_read: observed(cached_input),
                cache_write: Measure::unavailable("Codex does not report cache writes separately"),
                output: observed(output),
                reasoning: Measure::unavailable("Codex reasoning is included in output, not reported separately"),
            }),
            Self::ClaudeCode { input, cache_read, cache_write, output } => Ok(TokenCounts {
                fresh: observed(input.checked_add(cache_write).ok_or("fresh token overflow")?),
                cache_read: observed(cache_read),
                cache_write: observed(cache_write),
                output: observed(output),
                reasoning: Measure::unavailable("Claude Code thinking is included in output, not reported separately"),
            }),
            Self::SmithEvents { input, cache_read, cache_write, output, reasoning } => Ok(TokenCounts {
                fresh: add_measure(&optional(input, "input_tokens"), &optional(cache_write, "cache_write_tokens"))?,
                cache_read: optional(cache_read, "cache_read_tokens"),
                cache_write: optional(cache_write, "cache_write_tokens"),
                output: optional(output, "output_tokens"),
                reasoning: optional(reasoning, "reasoning_tokens"),
            }),
        }
    }
}

fn observed(value: u64) -> Measure<u64> {
    Measure::Observed { value }
}

fn optional(value: Option<u64>, field: &str) -> Measure<u64> {
    value.map_or_else(|| Measure::unavailable(format!("smith {field} is null")), observed)
}

fn parts(measure: &Measure<u64>) -> (Option<u64>, Vec<String>) {
    match measure {
        Measure::Observed { value } => (Some(*value), Vec::new()),
        Measure::LowerBound { value, missing } => (Some(*value), missing.clone()),
        Measure::Unavailable { reason } => (None, vec![reason.clone()]),
    }
}

fn add_measure(left: &Measure<u64>, right: &Measure<u64>) -> Result<Measure<u64>, String> {
    let (left_value, mut missing) = parts(left);
    let (right_value, right_missing) = parts(right);
    missing.extend(right_missing);
    missing.sort();
    missing.dedup();
    let value = match (left_value, right_value) {
        (Some(left), Some(right)) => Some(left.checked_add(right).ok_or("token sum overflow")?),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    Ok(match value {
        Some(value) if missing.is_empty() => observed(value),
        Some(value) => Measure::LowerBound { value, missing },
        None => Measure::unavailable(missing.join("; ")),
    })
}

fn scoped(measure: &Measure<u64>, scope: &str) -> Measure<u64> {
    match measure {
        Measure::Observed { value } => observed(*value),
        Measure::LowerBound { value, missing } => Measure::LowerBound {
            value: *value,
            missing: missing.iter().map(|reason| format!("{scope}: {reason}")).collect(),
        },
        Measure::Unavailable { reason } => Measure::unavailable(format!("{scope}: {reason}")),
    }
}

fn scoped_counts(usage: &TokenCounts, scope: &str) -> TokenCounts {
    TokenCounts {
        fresh: scoped(&usage.fresh, scope),
        cache_read: scoped(&usage.cache_read, scope),
        cache_write: scoped(&usage.cache_write, scope),
        output: scoped(&usage.output, scope),
        reasoning: scoped(&usage.reasoning, scope),
    }
}

#[derive(Clone, Debug)]
struct Scope {
    role: ScopeKind,
    convention: Convention,
    unavailable: String,
    responses: BTreeMap<String, TokenCounts>,
}

/// Attempt-local usage supplied by adapters, deduplicated across all scopes.
#[derive(Clone, Debug, Default)]
pub struct TokenLedger {
    scopes: BTreeMap<String, Scope>,
    responses: BTreeMap<String, Usage>,
}

impl TokenLedger {
    /// Declare a scope, retaining why its usage is unavailable until observed.
    pub fn declare(
        &mut self,
        id: &str,
        role: ScopeKind,
        convention: Convention,
        unavailable: &str,
    ) -> Result<(), String> {
        if id.is_empty()
            || id == "total"
            || unavailable.is_empty()
            || role == ScopeKind::Total
            || self.scopes.contains_key(id)
        {
            return Err("scope needs a unique nonempty id, a non-total role and an unavailable reason".into());
        }
        if role == ScopeKind::Root && self.scopes.values().any(|scope| scope.role == ScopeKind::Root) {
            return Err("attempt has more than one root scope".into());
        }
        self.scopes
            .insert(id.into(), Scope { role, convention, unavailable: unavailable.into(), responses: BTreeMap::new() });
        Ok(())
    }

    /// Count an immutable response once, returning false for an identical repeat.
    /// Conflicting repeats and convention changes fail rather than replace usage.
    pub fn record(&mut self, scope: &str, response: &str, usage: Usage) -> Result<bool, String> {
        let target = self.scopes.get_mut(scope).ok_or_else(|| format!("unknown token scope {scope}"))?;
        if response.is_empty() || target.convention != usage.convention() {
            return Err("response needs an id and its scope's convention".into());
        }
        if let Some(previous) = self.responses.get(response) {
            if previous != &usage {
                return Err(format!("conflicting repeated usage for response {response}"));
            }
            return Ok(false);
        }
        let normalised = scoped_counts(&usage.normalise()?, response);
        target.responses.insert(response.into(), normalised);
        self.responses.insert(response.into(), usage);
        Ok(true)
    }

    /// Emit each scope and its total, requiring a root and one convention.
    pub fn records(&self) -> Result<Vec<TokenRecord>, String> {
        let mut records = Vec::new();
        for (id, scope) in &self.scopes {
            let mut responses = scope.responses.values();
            let mut usage = responses.next().cloned().unwrap_or_else(|| TokenCounts::unavailable(&scope.unavailable));
            for response in responses {
                usage = usage.add(response)?;
            }
            records.push(TokenRecord {
                scope: id.clone(),
                role: scope.role.clone(),
                convention: scope.convention,
                usage,
            });
        }
        let root =
            records.iter().find(|record| record.role == ScopeKind::Root).ok_or("token total needs a root scope")?;
        let convention = root.convention;
        if records.iter().any(|record| record.convention != convention) {
            return Err("a token total cannot mix conventions".into());
        }
        let mut total = scoped_counts(&root.usage, &root.scope);
        for record in &records {
            if record.role != ScopeKind::Root {
                total = total.add(&scoped_counts(&record.usage, &record.scope))?;
            }
        }
        // A missing root makes this metric unavailable even when a child reports it.
        for (root_metric, total_metric) in [
            (&root.usage.fresh, &mut total.fresh),
            (&root.usage.cache_read, &mut total.cache_read),
            (&root.usage.cache_write, &mut total.cache_write),
            (&root.usage.output, &mut total.output),
            (&root.usage.reasoning, &mut total.reasoning),
        ] {
            if matches!(root_metric, Measure::Unavailable { .. }) {
                *total_metric = scoped(root_metric, &root.scope);
            }
        }
        records.push(TokenRecord { scope: "total".into(), role: ScopeKind::Total, convention, usage: total });
        Ok(records)
    }
}
