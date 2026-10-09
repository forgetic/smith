//! Task manifests and their offline validation; no agent or credentials are read.
//! The harness keeps task specifications, never an agent's internal state.
//! `read_task` parses strictly and `check_task` validates the frozen inputs,
//! following benchmarks.md, sections 5.2 and 14.

#![forbid(unsafe_code)]

mod check;
mod formats;
mod models;
mod suite;
mod task;

pub mod agent;

pub use agent::{Agent, Configuration, PinnedConfiguration, Provider, read_configuration};
pub use models::{ModelChoice, ModelTiers, read_model_tiers};
pub use suite::{
    Arm, CatalogueTask, Design, GuardSelection, Selection, Suite, SuiteAgent, catalogue, choose_guards, read_suite,
    validate_suite,
};

pub use check::{ManifestCounts, check_benchmark_tree, check_task, check_tree, seed_digest};
pub use task::{
    Budget, Calibration, Estimate, EventCheck, Grade, Kind, OutcomeCheck, Refusal, Task, Variant, read_task,
};

#[cfg(test)]
mod tests;
