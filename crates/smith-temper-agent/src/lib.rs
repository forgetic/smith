//! Temper-specific provider/auth/decision core for Smith.
//!
//! This crate is Smith's initial home for the concrete `pi_agent_rust` wiring
//! that Temper is splitting out: provider selection, OAuth auth-file handling,
//! per-provider request knobs, and one-turn structured decision parsing. It does
//! not mutate Forge state and does not depend on Temper runtime crates except in
//! dev-only tests that reuse Temper's workflow-domain fixtures.

#![allow(clippy::result_large_err)]

pub mod decision;
pub mod provider;

pub use decision::{DecisionError, run_decision};
pub use provider::{
    ANTHROPIC_MODEL_ENV, AUTH_FILE_ENV, AuthChoice, CODEX_MODEL_ENV, DEFAULT_ANTHROPIC_MODEL,
    DEFAULT_CODEX_MODEL, ProviderConfig, ProviderError, default_auth_path,
};
