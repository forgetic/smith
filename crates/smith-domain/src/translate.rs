//! The translations between the run's vocabulary and the session's (programming-model.md, section 4.5):
//! siblings share no types, so the run's conversations and the sessions meet
//! here, through small total functions, each an exhaustive match, so that a
//! variant added on either side breaks the build in one place.

use alloc::boxed::Box;

use skein_lib::{List, Token};
use smith_domain_run::charter::{Families, Tools};
use smith_domain_run::{self as run, Ask, Directory, Opening, Spend, Workspace};
use smith_domain_session::{self as session, Budget, Dimension, Spec, Yield, llm};
use smith_domain_tools::{Authority, Effect, Grants, Name, Repo};

/// The ticket of `finish` among the tools a session is offered: tickets are
/// the top level's, and name values within one session.
pub(crate) const FINISH: Token = Token::new(0);

/// The ticket of the sub-agent tool.
pub(crate) const SUB_AGENT: Token = Token::new(1);

/// The first ticket of a session's calls and answers.
pub(crate) const DELIVER: Token = Token::new(2);

/// First live ticket, separate from the fixed served-tool descriptors.
/// Contract: domain/run.md, sections 8.2 and 8.4.
pub(crate) const WAIT: Token = Token::new(3);

/// First live descriptor/ask ticket, separate from all fixed tool descriptors.
/// Contract: domain/run.md, sections 6 and 13.
pub(crate) const FIRST: u64 = 4;

/// The tools the run serves a conversation.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent immutable tool grants, not lifecycle states or exclusive alternatives"
)]
pub(crate) struct Offered {
    pub(crate) host_tools: Box<[run::HostTool]>,
    pub(crate) finish: bool,
    pub(crate) deliver: bool,
    pub(crate) agents: bool,
    pub(crate) wait: bool,
}

/// The session's spec for a conversation the run opens with `opening`, and
/// the tools the run serves it; or `None` if the checkout cannot be laid out
/// for the tools.
///
/// The run's LLM is the session's endpoint, model and answer size; its system
/// text and first message are the session's; its share of the budget is the
/// session's budget. The tools it runs on the checkout, the checkout and which
/// of it may be written make the tools' authority. What the run serves is
/// offered as descriptors: `finish` if the opening says so, a write, run
/// alone; sub-agents if its families have them, a write when the families
/// asked for may write, which the widest the asker may give them do.
pub(crate) fn spec(opening: Opening, receiving: Budget) -> Option<(Spec, Offered)> {
    let Opening {
        host_tools,
        llm,
        system,
        prompt,
        tools,
        workspace,
        budget,
        finish,
        deliver,
        families,
        wait,
        activation: _,
        transcript: _,
    } = opening;
    let authority = authority(workspace, tools)?;
    let offered = Offered { host_tools: host_tools.clone(), finish, deliver, agents: families.agents, wait };
    let capacity = u32::try_from(host_tools.len()).ok()?.checked_add(4)?;
    let mut delegated = List::with_capacity(capacity);
    for (index, tool) in host_tools.iter().enumerate() {
        let ticket = FIRST.checked_add(u64::try_from(index).ok()?)?;
        let effect = host_effect(tool.effect);
        delegated.push(llm::Descriptor { ticket: Token::new(ticket), effect }).expect("bounded declaration inventory");
    }
    if wait {
        delegated
            .push(llm::Descriptor { ticket: WAIT, effect: Effect::Write })
            .expect("room for all fixed descriptors");
    }
    if finish {
        delegated.push(llm::Descriptor { ticket: FINISH, effect: Effect::Write }).expect("room for both");
    }
    if deliver {
        delegated.push(llm::Descriptor { ticket: DELIVER, effect: Effect::Write }).expect("room for all three");
    }
    if families.agents {
        delegated.push(llm::Descriptor { ticket: SUB_AGENT, effect: writes(families) }).expect("room for both");
    }
    let run::Budget { turns, spend: _, time } = budget;
    let spec = Spec {
        endpoint: llm::Endpoint(llm.endpoint.0),
        model: llm.model,
        system,
        authority,
        delegated: delegated.into_boxed(),
        prompt,
        max_tokens: llm.max_tokens,
        budget: Budget { turns, time, ..receiving },
    };
    Some((spec, offered))
}

/// The tools' authority over `checkout` with `tools`, or `None` if a
/// repository's name cannot be a directory's.
///
/// Each repository is mounted at the root, under its name, which is how the
/// run names it to the LLM; relative paths start in the first repository, the
/// one the work is about, or at the root if there is none. Commands run with
/// an empty environment: a charter carries none yet.
fn authority(workspace: Option<Workspace>, tools: Tools) -> Option<Authority> {
    let mounted = match workspace {
        Some(workspace) => workspace.directories,
        None => Box::default(),
    };
    let count = u32::try_from(mounted.len()).ok()?;
    let mut repos = List::with_capacity(count);
    for Directory { name, root, writable, git: _, conflicts: _ } in mounted {
        let name = Name::new(name)?;
        let repo = Repo { mount: Box::new([name]), root, writable };
        repos.push(repo).expect("room for every repository");
    }
    let repos = repos.into_boxed();
    let cwd = match repos.first() {
        Some(first) => first.mount.clone(),
        None => Box::default(),
    };
    let Tools { inspect, modify, shell } = tools;
    Some(Authority { cwd, repos, grants: Grants { inspect, modify, shell }, env: Box::default() })
}

/// The effect of a call that opens a sub-agent with `families`: a write if it
/// may write.
fn writes(families: Families) -> Effect {
    if families.tools.modify || families.tools.shell { Effect::Write } else { Effect::Read }
}

/// The effect of a call to a tool the run serves.
pub(crate) fn effect(ask: &Ask) -> Effect {
    match ask {
        Ask::Host { effect, .. } => host_effect(*effect),
        Ask::Wait | Ask::Finish { .. } | Ask::Deliver { .. } => Effect::Write,
        Ask::SubAgent { families, .. } => writes(*families),
    }
}

/// Why the session yielded, as the run hears it.
pub(crate) const fn stop(stop: Yield) -> run::Stop {
    match stop {
        Yield::Done => run::Stop::EndTurn,
        Yield::Truncated => run::Stop::MaxTokens,
        Yield::Refused => run::Stop::Refusal,
        Yield::Malformed => run::Stop::NoCalls,
    }
}

/// What `turns` completions that used `usage` spent.
pub(crate) const fn spend(turns: u32, usage: llm::Usage) -> Spend {
    let llm::Usage { input_tokens, output_tokens, cache_read_tokens, cache_write_tokens } = usage;
    Spend {
        turns,
        input: input_tokens,
        output: output_tokens,
        cache_read: cache_read_tokens,
        cache_write: cache_write_tokens,
        units: 0,
    }
}

/// How the session ended, as the run hears it: a failed call is its
/// provider's fault, but for a conversation too long for the model, which is
/// a full context, as is a transcript past the session's limits.
pub(crate) fn end(end: session::End) -> run::End {
    match end {
        session::End::TranscriptRefused { reason } => {
            run::End::TranscriptRefused { reason: transcript_refusal(reason) }
        }
        session::End::PriceOverflow => run::End::PriceOverflow,
        session::End::UsageOverflow => run::End::UsageOverflow,
        session::End::Busy => run::End::Busy,
        session::End::Invalid => run::End::Invalid,
        session::End::Closed => run::End::Closed,
        session::End::Failed { failure, evidence } => run::End::Fault(run::Fault::Completion {
            failure: completion_failure(failure),
            evidence: completion_evidence(evidence),
        }),
        session::End::Budget { spent } => exhausted(spent),
        session::End::TranscriptFull => run::End::Fault(run::Fault::ContextFull),
    }
}

fn exhausted(spent: Dimension) -> run::End {
    match spent {
        Dimension::Unit => run::End::Budget(run::Exhausted::Spend),
        Dimension::Turns => run::End::Budget(run::Exhausted::Turns),
        Dimension::Input => run::End::Receiving(run::ReceivingLimit::Input),
        Dimension::Output => run::End::Receiving(run::ReceivingLimit::Output),
        Dimension::CacheRead => run::End::Receiving(run::ReceivingLimit::CacheRead),
        Dimension::CacheWrite => run::End::Receiving(run::ReceivingLimit::CacheWrite),
        Dimension::Time => run::End::Budget(run::Exhausted::Time),
    }
}

/// Whether the run's answer to a call is a failure, for the LLM to read as
/// one.
pub(crate) const fn failed(returned: &run::Returned) -> bool {
    match returned {
        run::Returned::HostAnswered(answer) => answer.error(),
        run::Returned::Waiting
        | run::Returned::Accepted
        | run::Returned::Delivered(_)
        | run::Returned::Answered { .. } => false,
        run::Returned::HostUnknown
        | run::Returned::HostRejected(_)
        | run::Returned::Nothing
        | run::Returned::DeliveryRefused(_)
        | run::Returned::Rejected { .. }
        | run::Returned::ChecksFailed { .. }
        | run::Returned::Stale
        | run::Returned::DeliveryFailed { .. }
        | run::Returned::Cancelled
        | run::Returned::TimedOut
        | run::Returned::Busy
        | run::Returned::Unanswered { .. }
        | run::Returned::Refused { .. } => true,
    }
}

pub(crate) const fn host_effect(effect: run::HostEffect) -> Effect {
    match effect {
        run::HostEffect::Read => Effect::Read,
        run::HostEffect::Write => Effect::Write,
    }
}

/// Lossless small sibling translation; no provider parsing or retry policy.
/// Contract: domain/run.md, sections 5, 10 and 14; domain/session.md, section 5.
fn completion_failure(failure: llm::Failure) -> run::CompletionFailure {
    match failure {
        llm::Failure::Limit => run::CompletionFailure::Limit,
        llm::Failure::Protocol => run::CompletionFailure::Protocol,
        llm::Failure::Cancelled => run::CompletionFailure::Cancelled,
        llm::Failure::Overloaded => run::CompletionFailure::Overloaded,
        llm::Failure::Unavailable => run::CompletionFailure::Unavailable,
        llm::Failure::TimedOut => run::CompletionFailure::TimedOut,
        llm::Failure::ContextTooLong => run::CompletionFailure::ContextTooLong,
        llm::Failure::Invalid => run::CompletionFailure::Invalid,
        llm::Failure::Unauthorized => run::CompletionFailure::Unauthorized,
        llm::Failure::RateLimited { retry_after } => run::CompletionFailure::RateLimited { retry_after },
        llm::Failure::Exhausted { retry_after } => run::CompletionFailure::Exhausted { retry_after },
    }
}

/// Transport evidence is copied exactly, without inferring absence of effects.
/// Contract: domain/run.md, sections 5, 10 and 14; domain/session.md, section 5.
fn completion_evidence(evidence: llm::Evidence) -> run::CompletionEvidence {
    match evidence {
        llm::Evidence::Unsent => run::CompletionEvidence::Unsent,
        llm::Evidence::Unknown => run::CompletionEvidence::Unknown,
        llm::Evidence::Response => run::CompletionEvidence::Response,
    }
}

/// Small total sibling translation; no history refusal becomes a fresh retry.
/// Contract: domain/run.md, section 13; domain/session.md, section 3.
const fn transcript_refusal(reason: session::record::Refusal) -> run::TranscriptRefusal {
    match reason {
        session::record::Refusal::Version => run::TranscriptRefusal::Version,
        session::record::Refusal::Endpoint => run::TranscriptRefusal::Endpoint,
        session::record::Refusal::Dialect => run::TranscriptRefusal::Dialect,
        session::record::Refusal::Malformed => run::TranscriptRefusal::Malformed,
        session::record::Refusal::Unresolved => run::TranscriptRefusal::Unresolved,
        session::record::Refusal::TooLarge => run::TranscriptRefusal::TooLarge,
    }
}
