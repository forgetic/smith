//! Versioned attempt evidence, retaining observations and explicit gaps only.
//! `read_result` refuses other versions; `classify_smith_exit` reconciles the
//! observed answer, deadline and exit (benchmarks.md, sections 8.1 and 8.3;
//! shell.md, section 4.3). No measurement is borrowed from another scope.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Agent, Provider, Refusal, TokenRecord};

/// The one result format understood by this harness.
pub const RESULT_VERSION: u32 = 1;

/// A measurement supplied by an observer, preserving its knowledge boundary.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Measure<T> {
    /// The observer measured this value in the named scope.
    Observed { value: T },
    /// The observer measured at least this value, with these missing observations.
    LowerBound { value: T, missing: Vec<String> },
    /// The observer cannot measure it, for this reason.
    Unavailable { reason: String },
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum UncheckedMeasure<T> {
    Observed { value: T },
    LowerBound { value: T, missing: Vec<String> },
    Unavailable { reason: String },
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Measure<T> {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        match UncheckedMeasure::deserialize(decoder)? {
            UncheckedMeasure::Observed { value } => Ok(Self::Observed { value }),
            UncheckedMeasure::LowerBound { value, missing } => {
                if missing.is_empty() || missing.iter().any(|reason| reason.trim().is_empty()) {
                    return Err(serde::de::Error::custom("a lower bound must name its missing observations"));
                }
                Ok(Self::LowerBound { value, missing })
            }
            UncheckedMeasure::Unavailable { reason } => {
                if reason.trim().is_empty() {
                    return Err(serde::de::Error::custom("an unavailable measurement needs a reason"));
                }
                Ok(Self::Unavailable { reason })
            }
        }
    }
}

impl<T> Measure<T> {
    /// Retain an absent observation without manufacturing a numeric value.
    #[must_use]
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable { reason: reason.into() }
    }
}

/// The smith observation face recorded by the adapter.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SmithFace {
    /// A pre-events binary's structured Debug trace.
    Legacy,
    /// A binary's versioned event stream.
    Events,
}

/// Requested and resolved model choices recorded by the adapter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelIdentity {
    pub requested_model: String,
    pub requested_effort: String,
    pub resolved_model: Measure<String>,
    pub resolved_effort: Measure<String>,
}

/// Immutable attempt identity supplied by the runner and adapter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub run: String,
    pub attempt: String,
    pub arm: String,
    pub task: String,
    pub task_version: u32,
    pub seed_sha256: String,
    pub prompt_sha256: String,
    pub agent: Agent,
    pub agent_version: String,
    pub binary_sha256: String,
    pub smith_face: Option<SmithFace>,
    pub configuration_sha256: String,
    pub user_configuration_sha256: Measure<String>,
    pub provider: Provider,
    pub model: ModelIdentity,
    pub harness_commit: String,
    pub host: String,
    pub cpus: Vec<u32>,
    /// The runner's start in UTC wall time, never used to measure durations.
    pub started_utc: String,
}

/// Millisecond durations supplied by the runner's clock and agent's records.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub process_wall_ms: Measure<u64>,
    pub task_wall_ms: Measure<u64>,
    pub teardown_ms: Measure<u64>,
    pub grading_wall_ms: Measure<u64>,
    pub first_byte_ms: Measure<u64>,
    pub largest_gap_ms: Measure<u64>,
    pub longest_completion_ms: Measure<u64>,
    pub agent_t_ms: Measure<u64>,
}

/// Event counts supplied by the adapter, with names retained in their own namespace.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub responses: Measure<u64>,
    pub provider_attempts: Measure<u64>,
    pub retries: Measure<u64>,
    pub completions: Measure<u64>,
    pub tool_calls: BTreeMap<String, Measure<u64>>,
    pub tool_failures: BTreeMap<String, Measure<u64>>,
    pub children: Measure<u64>,
    pub conversations: Measure<u64>,
    pub compactions: Measure<u64>,
    pub message_terminals: BTreeMap<String, Measure<u64>>,
}

/// The process scope counted by the runner's resource observer.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceScope {
    /// A delegated cgroup contains and counts the whole tree.
    Cgroup,
    /// Skein's pidfd walk settles and counts the discovered tree.
    PidfdWalk,
    /// Only the agent's root process was counted.
    AgentProcess,
    /// The runner has no resource accounting for this attempt.
    Unavailable,
}

/// Scoped CPU milliseconds and peak bytes supplied by the process-tree kit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub cpu_scope: ResourceScope,
    pub cpu_ms: Measure<u64>,
    pub peak_memory_scope: ResourceScope,
    pub peak_rss_bytes: Measure<u64>,
}

/// The basis attached by the adapter to its spend figure.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SpendBasis {
    /// smith's configured notional list prices.
    Notional,
    /// The harness's recorded prices applied to usage.
    Priced,
    /// The agent's own reported cost.
    Reported,
}

/// US-dollar spend supplied by its observer, with its basis kept apart.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Spend {
    pub basis: SpendBasis,
    pub usd: Measure<f64>,
}

/// The typed failure class supplied by an adapter when no answer was accepted.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FailureReason {
    /// The provider exhausted its attempts.
    Provider,
    /// The model or its limits failed the run.
    Model,
    /// A tool prevented an answer.
    Tool,
    /// The agent parked awaiting more input.
    InputNeeded,
    /// The policy refused the work.
    Policy,
    /// State moved while the agent worked.
    Stale,
    /// A persisted conversation could not resume.
    Transcript,
    /// The person or environment cancelled the run.
    Cancelled,
    /// The agent ended without an answer or a more specific class.
    Agent,
}

/// The exhausted budget named by the agent, never inferred from exit 4 alone.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BudgetLimit {
    /// The agent consumed its allowed completions.
    Turns,
    /// The agent reached its own time limit.
    Time,
    /// The agent reached its spend ceiling.
    Spend,
}

/// One terminal classification supplied by the adapter to summaries.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "end", rename_all = "kebab-case", deny_unknown_fields)]
pub enum End {
    /// The agent answered; a forced teardown is retained as a warning.
    Completed,
    /// The agent ended without an answer for this typed reason.
    Failed { reason: FailureReason, detail: String },
    /// The agent stopped on its own named limit.
    Budget { which: BudgetLimit },
    /// Preparation never reached inference.
    Refused { setup: String },
    /// The harness's hard deadline ended the tree before an answer.
    Timeout,
    /// The harness could not interpret or complete this attempt.
    HarnessError { what: String },
}

impl End {
    /// Budgets and setup refusals are reported beside the eligible rate counts.
    #[must_use]
    pub const fn counts_in_rate(&self) -> bool {
        match self {
            Self::Budget { which: _ } | Self::Refused { setup: _ } => false,
            Self::Completed
            | Self::Failed { reason: _, detail: _ }
            | Self::Timeout
            | Self::HarnessError { what: _ } => true,
        }
    }
}

/// The process exit supplied by the runner after settlement.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Exit {
    /// The process returned this exit code.
    Code(i32),
    /// The process ended under this signal.
    Signal(i32),
}

/// The runner's strongest action to end the attempt tree.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Forced {
    /// The tree ended without a harness signal.
    No,
    /// The harness sent its termination signal.
    Terminated,
    /// The harness killed the remaining tree after the grace.
    Killed,
}

/// A named check's verdict and exact files or records supplied by its observer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CheckVerdict {
    pub check: String,
    pub passed: Measure<bool>,
    pub evidence: Vec<String>,
}

/// The terminal and scope checks supplied by the runner after tree settlement.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub end: End,
    pub exit: Measure<Exit>,
    pub forced: Forced,
    pub warnings: Vec<String>,
    pub checks: Vec<CheckVerdict>,
    /// No grader is None; an expected but unobserved grade is Some(Unavailable).
    pub grade: Option<Measure<bool>>,
    pub protected_files_changed: Measure<Vec<PathBuf>>,
    pub writes_outside_workspace: Measure<Vec<PathBuf>>,
}

/// The fraction of a named bound consumed, supplied by a passive observer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Headroom {
    pub fraction_used: Measure<f64>,
    pub warning: bool,
}

/// Passive contract evidence supplied by the adapter, unavailable where unobserved.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Health {
    pub loss_per_sink: BTreeMap<String, Measure<u64>>,
    pub headroom: BTreeMap<String, Headroom>,
    pub passive_checks: Vec<CheckVerdict>,
}

/// An archived raw file's relative path and digest supplied by the runner.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: PathBuf,
    pub sha256: String,
}

/// One versioned attempt document supplied by the runner to offline readers.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AttemptResult {
    pub version: u32,
    pub identity: Identity,
    pub timing: Timing,
    pub tokens: Vec<TokenRecord>,
    pub counts: Counts,
    pub resources: Resources,
    pub spend: Spend,
    pub outcome: Outcome,
    pub health: Health,
    pub artifacts: Vec<Artifact>,
}

/// Read a result with strict fields and the one supported format version.
pub fn read_result(file: &Path) -> Result<AttemptResult, Refusal> {
    let text = std::fs::read_to_string(file).map_err(|error| Refusal::new(file, "document", error.to_string()))?;
    parse_result(file, &text)
}

/// Parse one result without accepting a prefix or another version.
pub fn parse_result(file: &Path, text: &str) -> Result<AttemptResult, Refusal> {
    let mut decoder = serde_json::Deserializer::from_str(text);
    let result: AttemptResult = serde_path_to_error::deserialize(&mut decoder)
        .map_err(|error| Refusal::new(file, &error.path().to_string(), error.inner().to_string()))?;
    decoder.end().map_err(|error| Refusal::new(file, "document", error.to_string()))?;
    if result.version != RESULT_VERSION {
        return Err(Refusal::new(
            file,
            "version",
            format!("result version {} is not {RESULT_VERSION}", result.version),
        ));
    }
    Ok(result)
}

/// Render the supported result, refusing non-finite measurements rather than nulls.
pub fn render_result(result: &AttemptResult) -> Result<String, String> {
    if result.version != RESULT_VERSION {
        return Err("unsupported result version".into());
    }
    if !finite(&result.spend.usd) || result.health.headroom.values().any(|entry| !finite(&entry.fraction_used)) {
        return Err("non-finite measurement".into());
    }
    let text = serde_json::to_string_pretty(result).map_err(|error| error.to_string())?;
    parse_result(Path::new("result.json"), &text).map_err(|error| error.to_string())?;
    Ok(format!("{text}\n"))
}

fn finite(measure: &Measure<f64>) -> bool {
    match measure {
        Measure::Observed { value } | Measure::LowerBound { value, missing: _ } => value.is_finite(),
        Measure::Unavailable { reason: _ } => true,
    }
}

/// The adapter's terminal and warnings supplied to the outcome record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classification {
    pub end: End,
    pub warnings: Vec<String>,
}

/// Classify smith's published exits against the observed answer and deadline.
/// A budget record supplies its limit; the code alone never guesses which one.
#[must_use]
pub fn classify_smith_exit(exit: Exit, accepted: bool, forced: Forced, budget: Option<BudgetLimit>) -> Classification {
    let end = if accepted && (exit == Exit::Code(0) || forced != Forced::No) {
        End::Completed
    } else if accepted {
        End::HarnessError { what: "accepted answer disagrees with smith's exit".into() }
    } else if forced != Forced::No {
        End::Timeout
    } else {
        match exit {
            Exit::Code(0) => End::HarnessError { what: "smith exited 0 without an observed accepted answer".into() },
            Exit::Code(1) => End::Failed { reason: FailureReason::Agent, detail: "smith run failed".into() },
            Exit::Code(2) => End::Refused { setup: "usage or configuration".into() },
            Exit::Code(3) => End::Refused { setup: "credentials".into() },
            Exit::Code(4) => match budget {
                Some(which) => End::Budget { which },
                None => End::HarnessError { what: "smith exit 4 has no observed budget limit".into() },
            },
            Exit::Code(5) => End::Failed { reason: FailureReason::InputNeeded, detail: "smith needs input".into() },
            Exit::Code(130) => End::Failed { reason: FailureReason::Cancelled, detail: "smith interrupted".into() },
            Exit::Code(code) => End::HarnessError { what: format!("unknown smith exit {code}") },
            Exit::Signal(signal) => {
                End::Failed { reason: FailureReason::Agent, detail: format!("agent signal {signal}") }
            }
        }
    };
    let warnings = if accepted && forced != Forced::No {
        vec![format!("forced teardown after accepted answer: {forced:?}")]
    } else {
        Vec::new()
    };
    Classification { end, warnings }
}
