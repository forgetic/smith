//! Agent identities and pinned configuration records; no login is read here.
//! `read_configuration` checks the pin and hashes its metadata and exact
//! configuration bytes (benchmarks.md, sections 5.4, 6 and 10).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Refusal;

pub mod codex;

/// The adapter selected by a suite author for each attempt.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    /// smith, observed through the face its binary offers.
    Smith,
    /// Codex, observed through JSON lines and its named rollouts.
    Codex,
    /// Claude Code, reserved for the next comparison pass.
    ClaudeCode,
}

impl Agent {
    /// The directory holding this adapter's pinned configurations.
    #[must_use]
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Smith => "smith",
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
}

/// The provider route selected by a suite author.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    /// The Codex subscription route.
    Codex,
    /// The Anthropic route.
    Anthropic,
}

/// A committed configuration pin supplied to one agent adapter.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub name: String,
    pub agent: Agent,
    pub provider: Provider,
    pub version: String,
    pub configuration: PathBuf,
}

/// The validated pin and digest carried into an attempt's identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinnedConfiguration {
    pub pin: Configuration,
    pub sha256: String,
}

/// Read a committed pin without reading a user's home or credentials.
pub fn read_configuration(file: &Path) -> Result<PinnedConfiguration, Refusal> {
    let pin: Configuration = crate::formats::read_document(file)?;
    if pin.name.is_empty() || pin.version.is_empty() {
        return Err(Refusal::new(file, "name/version", "configuration name and agent version must be pinned"));
    }
    crate::check::check_path(file, "configuration", &pin.configuration)?;
    match pin.agent {
        Agent::Smith => {}
        Agent::Codex => {
            if pin.provider != Provider::Codex {
                return Err(Refusal::new(file, "provider", "Codex requires its codex route"));
            }
        }
        Agent::ClaudeCode => {
            if pin.provider != Provider::Anthropic {
                return Err(Refusal::new(file, "provider", "Claude Code requires its anthropic route"));
            }
        }
    }
    let directory = file.parent().ok_or_else(|| Refusal::new(file, "configuration", "pin needs a directory"))?;
    let metadata = std::fs::read(file).map_err(|error| Refusal::new(file, "document", error.to_string()))?;
    let configuration = directory.join(&pin.configuration);
    let config_metadata = std::fs::symlink_metadata(&configuration)
        .map_err(|error| Refusal::new(file, "configuration", error.to_string()))?;
    if !config_metadata.is_file() || config_metadata.is_symlink() {
        return Err(Refusal::new(file, "configuration", "expected a committed regular configuration file"));
    }
    let bytes =
        std::fs::read(&configuration).map_err(|error| Refusal::new(file, "configuration", error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(u64::try_from(metadata.len()).expect("pin fits u64").to_be_bytes());
    digest.update(metadata);
    digest.update(u64::try_from(bytes.len()).expect("configuration fits u64").to_be_bytes());
    digest.update(bytes);
    Ok(PinnedConfiguration { pin, sha256: format!("{:x}", digest.finalize()) })
}
