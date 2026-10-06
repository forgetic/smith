//! Content-free observations of the local host (domain/host.md, section 9).

use skein_lib::Token;
use smith_domain::run::CallName;

/// A bounded observation that never decides host behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fact {
    /// A durable activation was handed to the agent.
    Started { activation: u64 },
    /// A saved person line was handed to the agent.
    Message { name: Token },
    /// The agent told a turn for the active activation.
    Turn { number: u32 },
    /// The active activation reached one answer.
    Answered { activation: u64 },
    /// That answer was shown after the durable turn acknowledgements.
    Shown { activation: u64 },
    /// A saved delivery answer was handed to the child run.
    DeliveryReturned { name: CallName },
}
