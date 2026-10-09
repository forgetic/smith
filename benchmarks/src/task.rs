//! Strict task documents, retaining only the manifest and no runtime state.
//! `read_task` refuses unknown keys with a file and key path; task checks are
//! closed enums, following benchmarks.md, sections 3.2 and 5.2.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A task's judgment, supplied by its author to the runner.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A forced behaviour whose checks must all pass.
    Probe,
    /// A small coding task judged by its external grader.
    Fixture,
    /// A frozen repository task judged by its external grader.
    Repository,
}

/// An agent-neutral outcome the task author asks the harness to inspect.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "check", rename_all = "kebab-case", deny_unknown_fields)]
pub enum OutcomeCheck {
    /// The named file must have exactly these UTF-8 bytes.
    FileContent { path: PathBuf, content: String },
    /// The named file must have this SHA-256 digest.
    FileDigest { path: PathBuf, sha256: String },
    /// The named workspace path must be absent.
    Absent { path: PathBuf },
    /// The named file must match the given pattern.
    Pattern { path: PathBuf, pattern: String },
    /// The named seed file must retain its original bytes.
    Protected { path: PathBuf },
}

/// A typed check of smith's records, extended when a probe names a check.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub enum EventCheck {}

/// A command the author gives the grader after the agent's tree has ended.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Grade {
    pub command: Vec<String>,
    pub deadline_seconds: u64,
    pub max_output_bytes: u64,
}

/// Limits supplied by the author to an agent as far as its interface permits.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub turns: Option<u64>,
    pub seconds: Option<u64>,
    /// A notional spend ceiling in US dollars.
    pub spend: Option<f64>,
}

/// Provisional task costs used until committed runs provide measurements.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Estimate {
    pub seconds: u64,
    pub tokens: u64,
}

/// The ten passing runs and failing control recorded by a probe's author.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    pub binary: String,
    pub passes: Vec<String>,
    pub failed_before: String,
}

/// A named prompt, settings or check change supplied by a task's author.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub name: String,
    pub prompt: Option<String>,
    #[serde(default)]
    pub smith: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub outcome: Vec<OutcomeCheck>,
    #[serde(default)]
    pub event: Vec<EventCheck>,
    #[serde(default)]
    pub waives: Vec<String>,
    pub budget: Option<Budget>,
}

/// A frozen task supplied by its author to offline checks and the runner.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub version: u32,
    pub kind: Kind,
    pub title: Option<String>,
    #[serde(default)]
    pub guards: Vec<String>,
    #[serde(default)]
    pub behaviours: Vec<String>,
    pub tier: String,
    pub effort: Option<String>,
    pub prompt: String,
    pub deadline_seconds: u64,
    pub estimate: Option<Estimate>,
    pub seed_sha256: String,
    #[serde(default)]
    pub budget: Budget,
    #[serde(default)]
    pub smith: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub setup: Vec<Vec<String>>,
    #[serde(default)]
    pub protected: Vec<PathBuf>,
    #[serde(default)]
    pub outcome: Vec<OutcomeCheck>,
    #[serde(default)]
    pub event: Vec<EventCheck>,
    #[serde(default)]
    pub grade: Vec<Grade>,
    #[serde(default)]
    pub waives: Vec<String>,
    pub repetitions: Option<u32>,
    #[serde(default)]
    pub variant: Vec<Variant>,
    pub calibration: Option<Calibration>,
}

/// A manifest or input refusal returned by the harness to its caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub file: PathBuf,
    pub key: String,
    pub reason: String,
}

impl Refusal {
    pub(crate) fn new(file: &Path, key: &str, reason: impl Into<String>) -> Self {
        Self { file: file.to_path_buf(), key: key.to_owned(), reason: reason.into() }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}: {}", self.file.display(), self.key, self.reason)
    }
}

impl std::error::Error for Refusal {}

/// Parse a task document and name the file and key path on refusal.
pub fn read_task(file: &Path) -> Result<Task, Refusal> {
    crate::formats::read_document(file)
}

#[cfg(test)]
pub(crate) fn parse_task(file: &Path, text: &str) -> Result<Task, Refusal> {
    crate::formats::parse_document(file, text)
}
