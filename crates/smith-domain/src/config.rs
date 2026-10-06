//! The agent's configured provider endpoint names (domain/run.md, section 3.1).
//! This owns only names, never addresses or credentials. The root validates it
//! once at construction, then checks each charter before passing a start to the run.

use alloc::boxed::Box;

use smith_domain_run::charter::Endpoint;

use crate::Limits;

/// Endpoint names configured for this agent, separate from capacity limits.
#[derive(Debug)]
pub struct Config {
    /// Unique endpoint names, at most `Limits::endpoints`.
    pub endpoints: Box<[Endpoint]>,
}

impl Config {
    /// Whether the configuration fits its capacities and has unique names.
    pub(crate) fn valid(&self, limits: &Limits) -> bool {
        let Ok(count) = u32::try_from(self.endpoints.len()) else {
            return false;
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
        true
    }

    /// Whether this endpoint name was configured.
    pub(crate) fn contains(&self, endpoint: Endpoint) -> bool {
        self.endpoints.contains(&endpoint)
    }
}
