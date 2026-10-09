//! Content-free domain observations become channel fact kinds
//! (protocol/channel.md, section 8; domain/run.md, section 11).
//!
//! This projection keeps no state, text or credential values. The component
//! owns elapsed time, output room and the count of facts it drops.

use smith_channel::FactKind;
use smith_domain::{self as domain, run, session, tools};

/// Project one domain observation to a bounded public kind and count.
pub(crate) fn project(fact: &domain::Fact) -> Option<(FactKind, u64)> {
    match fact {
        domain::Fact::Run { fact } => match &fact.kind {
            run::facts::FactKind::Admitted { .. } => Some((FactKind::Admitted, 1)),
            run::facts::FactKind::Answered { .. } => Some((FactKind::Ended, 1)),
            run::facts::FactKind::Called { .. } => Some((FactKind::ToolStarted, 1)),
            run::facts::FactKind::Returned { .. } => Some((FactKind::ToolFinished, 1)),
            run::facts::FactKind::CheckStarted { .. } => Some((FactKind::CheckStarted, 1)),
            run::facts::FactKind::CheckFinished { .. } => Some((FactKind::CheckFinished, 1)),
            run::facts::FactKind::MessageRead { .. }
            | run::facts::FactKind::MessageFence { .. }
            | run::facts::FactKind::MessageUnread { .. }
            | run::facts::FactKind::MessageReceived { .. }
            | run::facts::FactKind::MessageRefused { .. }
            | run::facts::FactKind::Prepared { .. }
            | run::facts::FactKind::Opened { .. }
            | run::facts::FactKind::Ended { .. }
            | run::facts::FactKind::Delivered { .. } => None,
        },
        domain::Fact::Session { fact } => match &fact.kind {
            session::FactKind::CompletionStarted { attempt, .. } => Some((FactKind::LlmStarted, u64::from(*attempt))),
            session::FactKind::CompletionRetried { attempt, .. } => Some((FactKind::LlmRetried, u64::from(*attempt))),
            session::FactKind::CompletionAnswered { blocks, .. } => Some((FactKind::LlmFinished, u64::from(*blocks))),
            session::FactKind::CompletionFailed { .. } | session::FactKind::CompletionCancelled { .. } => {
                Some((FactKind::LlmFinished, 0))
            }
            session::FactKind::Tools { fact, .. } => match fact {
                tools::FactKind::Started { .. } => Some((FactKind::ToolStarted, 1)),
                tools::FactKind::Answered { .. } => Some((FactKind::ToolFinished, 1)),
                tools::FactKind::Opened { .. }
                | tools::FactKind::Refused { .. }
                | tools::FactKind::Closing { .. }
                | tools::FactKind::Closed { .. } => None,
            },
            session::FactKind::Opened { .. }
            | session::FactKind::DelegateStarted { .. }
            | session::FactKind::DelegateAnswered { .. }
            | session::FactKind::DelegateCancelled { .. }
            | session::FactKind::Yielded { .. }
            | session::FactKind::Used { .. }
            | session::FactKind::Ended { .. } => None,
        },
    }
}
