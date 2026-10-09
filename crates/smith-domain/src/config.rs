//! Endpoint and model declarations retained by the root at startup. This module
//! knows no destinations or credentials. Configuration is checked before the
//! root starts; each charter is checked against its declared model quantities.
//! Contracts: domain/run.md, section 4; protocol/limits.md, section 2.1.

use alloc::boxed::Box;
use core::mem::size_of;

use smith_domain_run::{
    Invalid,
    charter::{Endpoint, Llm},
};

use crate::Limits;

/// One configured model, supplied at startup and retained until root teardown.
#[derive(Debug)]
pub struct ConfiguredModel {
    pub endpoint: Endpoint,
    pub model: Box<[u8]>,
    /// Declared usable input tokens.
    pub window: u32,
    /// Declared maximum completion tokens, thinking included.
    pub output: u32,
}

impl ConfiguredModel {
    /// Owned cell and model-name bytes for this declaration.
    #[must_use]
    pub fn worst_case(model_bytes: usize) -> Option<u64> {
        u64::try_from(size_of::<Self>()).ok()?.checked_add(u64::try_from(model_bytes).ok()?)
    }
}

/// Endpoint names and served models, supplied at startup separately from limits.
#[derive(Debug)]
pub struct Config {
    /// Unique endpoint names, at most `Limits::endpoints`.
    pub endpoints: Box<[Endpoint]>,
    /// Unique models per endpoint, within `Limits::configured_model_bytes`.
    pub models: Box<[ConfiguredModel]>,
}

impl Config {
    /// Whether declarations fit their capacities and names are unique.
    #[expect(clippy::manual_let_else, reason = "strict subset keeps checked construction exhaustive")]
    pub(crate) fn valid(&self, limits: &Limits) -> bool {
        let count = match u32::try_from(self.endpoints.len()) {
            Ok(count) => count,
            Err(_) => return false,
        };
        if count > limits.endpoints {
            return false;
        }
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            for other in self.endpoints.get(index.saturating_add(1)..).unwrap_or_default() {
                if endpoint == other {
                    return false;
                }
            }
        }
        let mut bytes = 0_u64;
        for (index, model) in self.models.iter().enumerate() {
            if model.model.is_empty() || model.window == 0 || model.output == 0 || !self.contains(model.endpoint) {
                return false;
            }
            let owned = match ConfiguredModel::worst_case(model.model.len()) {
                Some(owned) => owned,
                None => return false,
            };
            bytes = match bytes.checked_add(owned) {
                Some(bytes) => bytes,
                None => return false,
            };
            if bytes > limits.configured_model_bytes {
                return false;
            }
            for other in self.models.get(index.saturating_add(1)..).unwrap_or_default() {
                if model.endpoint == other.endpoint && model.model == other.model {
                    return false;
                }
            }
        }
        bytes <= limits.configured_model_bytes
    }

    /// Whether this endpoint name was configured.
    pub(crate) fn contains(&self, endpoint: Endpoint) -> bool {
        self.endpoints.contains(&endpoint)
    }

    /// Check a charter's effective quantities against its served model.
    pub(crate) fn check_model(&self, llm: &Llm) -> Result<(), Invalid> {
        if !self.contains(llm.endpoint) {
            return Err(Invalid::Endpoint);
        }
        for model in &self.models {
            if llm.endpoint == model.endpoint && llm.model == model.model {
                if llm.window == 0 || llm.output == 0 || llm.window > model.window || llm.output > model.output {
                    return Err(Invalid::Llm);
                }
                return Ok(());
            }
        }
        Err(Invalid::Llm)
    }
}
