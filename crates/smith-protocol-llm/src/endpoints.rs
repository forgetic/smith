//! Startup endpoint names and their resolved transport, dialect and options.
//! This table retains no credential. The service gives the destinations to
//! skein's connection component and uses names only through this mapping.
//! Contract: protocol/agent.md, section 4; protocol/llm.md, sections 2 and 6.

use alloc::boxed::Box;
use core::mem::size_of;

use skein_lib::{List, Map};
use skein_llm::{Prompt, Provider};
use skein_llm_connection as connection;
use smith_domain::llm;

/// Optional provider identity prepended to the endpoint's instructions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IdentityProfile {
    /// Send only the domain's system instructions.
    Plain,
    /// Opt in to skein's archived Claude Code identity blocks.
    ClaudeCode,
}

/// A model's policy for a reasoning item past its bound (protocol/limits.md, section 2.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum OversizedReasoning {
    /// Refuse the completion with the reasoning item's typed limit; the default.
    Fail,
    /// Keep the completion without the item and emit its drop as a fact.
    Drop,
}

/// One declared model's per-call policy, supplied by startup and retained by its endpoint.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ConfiguredModel {
    pub name: Box<[u8]>,
    pub oversized_reasoning: OversizedReasoning,
}

/// One startup-resolved endpoint, named as the domain names it.
/// The address, TLS trust/name, dialect, path and headers live in destination.
#[derive(Debug)]
pub struct ConfiguredEndpoint {
    /// Numeric name used by a charter and a completion's prompt.
    pub name: llm::Endpoint,
    /// Resolved transport and LLM endpoint given to skein's connection pool.
    pub destination: connection::Endpoint,
    /// Credential account selected for calls to this endpoint.
    pub account: u32,
    /// Dialect-supported reasoning option supplied by startup configuration.
    pub reasoning_effort: Option<Box<[u8]>>,
    /// Dialect-supported cache affinity supplied by startup configuration.
    pub cache_key: Option<Box<[u8]>>,
    /// Optional provider system identity selected at startup.
    pub identity: IdentityProfile,
    /// The models whose per-call reasoning policy this endpoint serves.
    pub models: Box<[ConfiguredModel]>,
}

/// Per-name options kept after the connection component takes destinations.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct EndpointOptions {
    /// Index in skein's configured destination list.
    pub index: u32,
    /// Credential account selected for this endpoint.
    pub account: u32,
    /// Dialect that owns replay and provider-specific request rules.
    pub provider: Provider,
    /// Configured reasoning option, when supported.
    pub reasoning_effort: Option<Box<[u8]>>,
    /// Configured cache affinity, when supported.
    pub cache_key: Option<Box<[u8]>>,
    /// Optional provider system identity selected at startup.
    pub identity: IdentityProfile,
    /// The models whose per-call reasoning policy this endpoint serves.
    pub models: Box<[ConfiguredModel]>,
}

impl EndpointOptions {
    /// Select the declared model's per-call policy; an undeclared name keeps the default failure.
    #[must_use]
    pub fn drop_reasoning(&self, requested_model: &[u8]) -> bool {
        for model in &self.models {
            if model.name.as_ref() == requested_model {
                return match model.oversized_reasoning {
                    OversizedReasoning::Fail => false,
                    OversizedReasoning::Drop => true,
                };
            }
        }
        false
    }
}

/// Why startup endpoint registration cannot be admitted.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EndpointError {
    /// More configured endpoints than the component's bound.
    TooMany,
    /// Two destinations use the same numeric name.
    Duplicate,
    /// An endpoint asks for an account outside the agent's configured range.
    Account,
    /// A prompt named no configured destination.
    Unknown,
    /// The selected identity is incompatible with the dialect or bound.
    Identity,
}

/// Checked numeric names and resolved skein destinations.
#[derive(Debug)]
pub struct Endpoints {
    destinations: List<connection::Endpoint>,
    options: Map<u32, EndpointOptions>,
}

impl Endpoints {
    /// Validate names and accounts before a service starts its loop.
    pub fn new(
        configured: Box<[ConfiguredEndpoint]>,
        endpoint_limit: u32,
        account_limit: u32,
    ) -> Result<Endpoints, EndpointError> {
        if configured.len() > usize::try_from(endpoint_limit).expect("u32 fits usize") {
            return Err(EndpointError::TooMany);
        }
        let mut destinations = List::with_capacity(endpoint_limit);
        let mut options = Map::with_capacity(endpoint_limit);
        for (index, configured) in configured.into_iter().enumerate() {
            if configured.account >= account_limit {
                return Err(EndpointError::Account);
            }
            let index = u32::try_from(index).or(Err(EndpointError::TooMany))?;
            if options.contains_key(&configured.name.0) {
                return Err(EndpointError::Duplicate);
            }
            let provider = configured.destination.llm.provider;
            if configured.identity == IdentityProfile::ClaudeCode && provider != Provider::Anthropic {
                return Err(EndpointError::Identity);
            }
            destinations.push(configured.destination).or(Err(EndpointError::TooMany))?;
            let option = EndpointOptions {
                index,
                account: configured.account,
                provider,
                reasoning_effort: configured.reasoning_effort,
                cache_key: configured.cache_key,
                identity: configured.identity,
                models: configured.models,
            };
            options.insert(configured.name.0, option).or(Err(EndpointError::TooMany))?;
        }
        Ok(Endpoints { destinations, options })
    }

    /// The configured endpoint named in a domain request.
    #[must_use]
    pub fn resolve(&self, name: llm::Endpoint) -> Option<&EndpointOptions> {
        self.options.get(&name.0)
    }

    /// Apply the startup options to the neutral prompt given to skein.
    /// Its dialect, target and headers are already in the destination.
    pub fn apply(&self, name: llm::Endpoint, prompt: &mut Prompt) -> Result<u32, EndpointError> {
        let options = self.resolve(name).ok_or(EndpointError::Unknown)?;
        prompt.reasoning_effort.clone_from(&options.reasoning_effort);
        prompt.cache_key.clone_from(&options.cache_key);
        match options.identity {
            IdentityProfile::Plain => {}
            IdentityProfile::ClaudeCode => {
                prompt.instructions = skein_llm::anthropic::identity::instructions(&prompt.instructions)
                    .or(Err(EndpointError::Identity))?;
            }
        }
        Ok(options.index)
    }

    /// Borrow configured destination limits and credential ceilings for startup accounting.
    #[must_use]
    pub fn destinations(&self) -> &List<connection::Endpoint> {
        &self.destinations
    }

    /// Bytes owned by startup options beyond the map's inline cells.
    #[must_use]
    pub fn option_bytes(&self) -> Option<u64> {
        let mut held = 0_u64;
        for (_, option) in &self.options {
            for model in &option.models {
                held = held.checked_add(u64::try_from(size_of::<ConfiguredModel>()).ok()?)?;
                held = held.checked_add(u64::try_from(model.name.len()).ok()?)?;
            }
            if let Some(value) = &option.reasoning_effort {
                held = held.checked_add(u64::try_from(value.len()).ok()?)?;
            }
            if let Some(value) = &option.cache_key {
                held = held.checked_add(u64::try_from(value.len()).ok()?)?;
            }
        }
        Some(held)
    }

    /// Split names from resolved destinations for the owning component.
    #[must_use]
    pub fn into_parts(self) -> (List<connection::Endpoint>, Map<u32, EndpointOptions>) {
        (self.destinations, self.options)
    }
}
