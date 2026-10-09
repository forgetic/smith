//! Task manifests and their offline validation; no agent or credentials are read.
//! The harness keeps task specifications, never an agent's internal state.
//! `read_task` parses strictly and `check_task` validates the frozen inputs,
//! following benchmarks.md, sections 5.2 and 14.

#![forbid(unsafe_code)]

mod check;
mod formats;
mod models;
mod result;
mod stats;
mod suite;
mod summary;
mod task;
mod tokens;

pub mod agent;

pub mod guard;

pub use agent::{Agent, Configuration, PinnedConfiguration, Provider, read_configuration};
pub use models::{ModelChoice, ModelTiers, read_model_tiers};
pub use result::{
    Artifact, AttemptResult, BudgetLimit, CheckVerdict, Classification, Counts, End, Exit, FailureReason, Forced,
    Headroom, Health, Identity, Measure, ModelIdentity, Outcome, RESULT_VERSION, ResourceScope, Resources, SmithFace,
    Spend, SpendBasis, Timing, classify_smith_exit, parse_result, read_result, render_result,
};
pub use stats::{
    Comparison, Interval, ProbeVerdict, audit_sample, bootstrap_ratio, coefficient_of_variation, interleave, median,
    minimum_detectable_effect, pass_interval, probe_verdict,
};
pub use suite::{
    Arm, CatalogueTask, Design, GuardSelection, Selection, Suite, SuiteAgent, catalogue, choose_guards,
    choose_guards_with_costs, read_suite, validate_suite,
};
pub use summary::{
    ArmSummary, Baseline, MetricSummary, PassCounts, RunSummary, SummaryComparison, baseline, committed_costs,
    read_baseline, read_results, read_summary, summarise, write_summary,
};
pub use tokens::{Convention, ScopeKind, TokenCounts, TokenLedger, TokenRecord, Usage};

pub use check::{ManifestCounts, check_benchmark_tree, check_task, check_tree, seed_digest};
pub use task::{
    Budget, Calibration, Estimate, EventCheck, Grade, Kind, OutcomeCheck, Refusal, Task, Variant, read_task,
};

#[cfg(test)]
mod tests;
