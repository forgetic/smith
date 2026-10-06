//! Initial local host ownership (domain/host.md, sections 8 and 9). The
//! in-process agent is constructed from configured endpoint names here;
//! chat transitions and requests follow in the running-chat increment.

use alloc::boxed::Box;
use skein_lib::Queue;
use smith_domain as agent;

use crate::{Config, Invalid, Limits};

/// One local chat with its own in-process agent and bounded child output.
#[derive(Debug)]
#[expect(dead_code, reason = "the first running-chat increment consumes the constructed state")]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) agent: agent::Domain,
    pub(crate) agent_out: Queue<agent::Request>,
}

impl Domain {
    /// Build a local chat after validating the receiving configuration.
    pub fn new(config: Config, limits: &Limits, seed: u64) -> Result<Domain, Invalid> {
        config.validate(limits)?;
        let agent_config = agent::Config { endpoints: Box::from(limits.endpoints.as_ref()) };
        let agent = agent::Domain::new(&limits.agent, agent_config, seed);
        Ok(Domain { config, agent, agent_out: Queue::with_capacity(agent::max_out(&limits.agent)) })
    }
}
