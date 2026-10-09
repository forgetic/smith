//! Task manifests and their offline validation; no agent or credentials are read.
//! The harness keeps task specifications, never an agent's internal state.
//! `read_task` parses strictly and `check_task` validates the frozen inputs,
//! following benchmarks.md, sections 5.2 and 14.

#![forbid(unsafe_code)]

mod check;
mod task;

pub use check::{check_task, check_tree, seed_digest};
pub use task::{
    Budget, Calibration, Estimate, EventCheck, Grade, Kind, OutcomeCheck, Refusal, Task, Variant, read_task,
};

#[cfg(test)]
mod tests;
