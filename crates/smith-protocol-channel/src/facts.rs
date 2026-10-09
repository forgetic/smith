//! Content-free domain observations become channel fact kinds
//! (protocol/channel.md, section 8; domain/run.md, section 11).
//!
//! This projection keeps no state, text or credential values. The component
//! owns elapsed time, output room and the count of facts it drops.

use smith_channel::FactKind;
use smith_domain::{self as domain, run, session, tools};

/// Project one domain observation to a bounded public kind and count.
pub(crate) fn project(fact: domain::Fact) -> Option<(FactKind, u64)> {
    match fact {
        domain::Fact::Run { fact } => match fact {
            run::facts::Fact::Admitted { .. } => Some((FactKind::Admitted, 1)),
            run::facts::Fact::Answered { .. } => Some((FactKind::Ended, 1)),
            run::facts::Fact::Called { .. } => Some((FactKind::ToolStarted, 1)),
            run::facts::Fact::Returned { .. } => Some((FactKind::ToolFinished, 1)),
            run::facts::Fact::CheckStarted { .. } => Some((FactKind::CheckStarted, 1)),
            run::facts::Fact::CheckFinished { .. } => Some((FactKind::CheckFinished, 1)),
            run::facts::Fact::MessageRead { .. }
            | run::facts::Fact::MessageFence { .. }
            | run::facts::Fact::MessageUnread { .. }
            | run::facts::Fact::MessageReceived { .. }
            | run::facts::Fact::MessageRefused { .. }
            | run::facts::Fact::Prepared { .. }
            | run::facts::Fact::Opened { .. }
            | run::facts::Fact::Ended { .. }
            | run::facts::Fact::Delivered { .. } => None,
        },
        domain::Fact::Session { fact } => match fact {
            session::Fact::CompletionStarted { attempt, .. } => Some((FactKind::LlmStarted, u64::from(attempt))),
            session::Fact::CompletionRetried { attempt, .. } => Some((FactKind::LlmRetried, u64::from(attempt))),
            session::Fact::CompletionAnswered { blocks, .. } => Some((FactKind::LlmFinished, u64::from(blocks))),
            session::Fact::CompletionFailed { .. } | session::Fact::CompletionCancelled { .. } => {
                Some((FactKind::LlmFinished, 0))
            }
            session::Fact::Tools { fact, .. } => match fact {
                tools::Fact::Started { .. } => Some((FactKind::ToolStarted, 1)),
                tools::Fact::Answered { .. } => Some((FactKind::ToolFinished, 1)),
                tools::Fact::Opened { .. }
                | tools::Fact::Refused { .. }
                | tools::Fact::Closing { .. }
                | tools::Fact::Closed { .. } => None,
            },
            session::Fact::Opened { .. }
            | session::Fact::DelegateStarted { .. }
            | session::Fact::DelegateAnswered { .. }
            | session::Fact::DelegateCancelled { .. }
            | session::Fact::Yielded { .. }
            | session::Fact::Used { .. }
            | session::Fact::Ended { .. } => None,
        },
    }
}
