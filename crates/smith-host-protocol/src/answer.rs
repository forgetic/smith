//! Turn the wire last word into the host domain's typed result while leaving
//! accepted outcome bytes opaque (protocol/channel.md, sections 4 and 5;
//! domain/host.md, sections 2 and 6).
//!
//! This module keeps no state, never learns policy or credential values, and
//! enters through `decode_answer` after the channel body decoder succeeds.

use alloc::boxed::Box;
use smith_channel as wire;
use smith_host_domain::channel as host;

use crate::Error;

pub(crate) fn decode_answer(record: &wire::Answer) -> Result<host::Answer, Error> {
    let result = match record.result() {
        wire::RunResult::Refused(refused) => {
            let refusal = match refused.reason() {
                wire::StartRefusal::Busy => host::Refusal::Busy,
                wire::StartRefusal::Invalid(invalid) => host::Refusal::Invalid(invalid_start(invalid.value())),
            };
            host::RunResult::Refused { refusal }
        }
        wire::RunResult::Accepted(accepted) => host::RunResult::Accepted { outcome: Box::from(accepted.result()) },
        wire::RunResult::Parked => host::RunResult::Parked,
        wire::RunResult::Failed(failed) => host::RunResult::Failed { failure: failure(failed.reason())? },
    };
    Ok(host::Answer { turns: record.turns(), spent: record.spent(), result })
}

fn invalid_start(invalid: &wire::InvalidStart) -> host::RunInvalid {
    match invalid {
        wire::InvalidStart::CharterVersion => host::RunInvalid::CharterVersion,
        wire::InvalidStart::MalformedCharter => host::RunInvalid::MalformedCharter,
        wire::InvalidStart::Activation => host::RunInvalid::Activation,
        wire::InvalidStart::Window => host::RunInvalid::Window,
        wire::InvalidStart::Conventions => host::RunInvalid::Conventions,
        wire::InvalidStart::TooLarge => host::RunInvalid::TooLarge,
        wire::InvalidStart::Workspace => host::RunInvalid::Workspace,
        wire::InvalidStart::Grants => host::RunInvalid::Grants,
        wire::InvalidStart::Outcome => host::RunInvalid::Outcome,
        wire::InvalidStart::Budget => host::RunInvalid::Budget,
        wire::InvalidStart::Llm => host::RunInvalid::Llm,
        wire::InvalidStart::Conversation => host::RunInvalid::Conversation,
        wire::InvalidStart::Endpoint => host::RunInvalid::Endpoint,
    }
}

fn failure(source: &wire::RunFailure) -> Result<host::RunFailure, Error> {
    let result = match source {
        wire::RunFailure::Transcript(record) => {
            let reason = match record.value() {
                wire::TranscriptRefusal::Version => host::TranscriptRefusal::Version,
                wire::TranscriptRefusal::Endpoint => host::TranscriptRefusal::Endpoint,
                wire::TranscriptRefusal::Dialect => host::TranscriptRefusal::Dialect,
                wire::TranscriptRefusal::Malformed => host::TranscriptRefusal::Malformed,
                wire::TranscriptRefusal::Unresolved => host::TranscriptRefusal::Unresolved,
                wire::TranscriptRefusal::TooLarge => host::TranscriptRefusal::TooLarge,
            };
            host::RunFailure::Transcript(reason)
        }
        wire::RunFailure::Model(record) => host::RunFailure::Model(model_fault(record.value())?),
        wire::RunFailure::Budget(record) => host::RunFailure::Budget(budget(record.value())),
        wire::RunFailure::Policy(record) => {
            host::RunFailure::Policy(host::Policy::Unfinished { nudges: record.nudges(), rejected: record.rejected() })
        }
        wire::RunFailure::Cancelled => host::RunFailure::Cancelled,
        wire::RunFailure::Stale => host::RunFailure::Stale,
    };
    Ok(result)
}

fn model_fault(source: &wire::ModelFault) -> Result<host::ModelFault, Error> {
    let result = match source {
        wire::ModelFault::Completion(record) => {
            let failure = match record.failure() {
                wire::CompletionFailure::Limit => host::CompletionFailure::Limit,
                wire::CompletionFailure::Protocol => host::CompletionFailure::Protocol,
                wire::CompletionFailure::Cancelled => host::CompletionFailure::Cancelled,
                wire::CompletionFailure::Overloaded => host::CompletionFailure::Overloaded,
                wire::CompletionFailure::Unavailable => host::CompletionFailure::Unavailable,
                wire::CompletionFailure::TimedOut => host::CompletionFailure::TimedOut,
                wire::CompletionFailure::ContextTooLong => host::CompletionFailure::ContextTooLong,
                wire::CompletionFailure::Invalid => host::CompletionFailure::Invalid,
                wire::CompletionFailure::Unauthorized => host::CompletionFailure::Unauthorized,
                wire::CompletionFailure::RateLimited => host::CompletionFailure::RateLimited {
                    retry_after: (*record.retry_after()).ok_or(Error::InvalidAnswer)?,
                },
                wire::CompletionFailure::Exhausted => host::CompletionFailure::Exhausted {
                    retry_after: (*record.retry_after()).ok_or(Error::InvalidAnswer)?,
                },
            };
            let evidence = match record.evidence() {
                wire::CompletionEvidence::Unsent => host::CompletionEvidence::Unsent,
                wire::CompletionEvidence::Unknown => host::CompletionEvidence::Unknown,
                wire::CompletionEvidence::Response => host::CompletionEvidence::Response,
            };
            host::ModelFault::Completion { failure, evidence }
        }
        wire::ModelFault::Exhausted => host::ModelFault::Exhausted,
        wire::ModelFault::Provider => host::ModelFault::Provider,
        wire::ModelFault::ContextFull => host::ModelFault::ContextFull,
        wire::ModelFault::Refused => host::ModelFault::Refused,
        wire::ModelFault::Truncated => host::ModelFault::Truncated,
        wire::ModelFault::Malformed => host::ModelFault::Malformed,
    };
    Ok(result)
}

fn budget(source: &wire::BudgetFailure) -> host::Exhausted {
    match source {
        wire::BudgetFailure::Turns => host::Exhausted::Turns,
        wire::BudgetFailure::Spend => host::Exhausted::Spend,
        wire::BudgetFailure::Time => host::Exhausted::Time,
        wire::BudgetFailure::Tokens(record) => {
            let limit = match record.value() {
                wire::ReceivingLimit::Input => host::ReceivingLimit::Input,
                wire::ReceivingLimit::Output => host::ReceivingLimit::Output,
                wire::ReceivingLimit::CacheRead => host::ReceivingLimit::CacheRead,
                wire::ReceivingLimit::CacheWrite => host::ReceivingLimit::CacheWrite,
            };
            host::Exhausted::Tokens(limit)
        }
        wire::BudgetFailure::Overflow(record) => {
            let overflow = match record.value() {
                wire::BudgetOverflow::Spend => host::Overflow::Spend,
                wire::BudgetOverflow::Usage => host::Overflow::Usage,
            };
            host::Exhausted::Overflow(overflow)
        }
    }
}
