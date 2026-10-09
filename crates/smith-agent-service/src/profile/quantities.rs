//! Declared deployment and model quantities retained at startup. This module
//! knows no transport destinations or derived byte limits. `standard` supplies
//! the transitional named bundle; the service composes derivation separately.
//! Contracts: protocol/limits.md, section 2; protocol/agent.md, section 4.

use alloc::boxed::Box;
use skein_lib::Duration;

/// A named deployment bundle, selected by the shell before startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Name {
    /// The standard bundle, refined as each limit layer is derived.
    Standard,
}

/// An exact share of a window, supplied by the profile to compaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fraction {
    pub numerator: u32,
    pub denominator: u32,
}

/// Deployment quantities from which the service derives ownership bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Declared {
    /// Open conversations across a run tree.
    pub conversations: u32,
    /// Bytes exchanged in one tool piece.
    pub tool_payload: u32,
    /// Tool calls one response may make.
    pub calls_per_response: u32,
    /// Default read window, in bytes.
    pub read_window: u32,
    /// Default kept command output, in bytes.
    pub shell_output: u32,
    /// Maximum first command output bytes.
    pub shell_head: u32,
    /// Maximum last command output bytes.
    pub shell_tail: u32,
    /// Maximum retained search matches.
    pub search_hits: u32,
    /// Maximum retained search bytes.
    pub search_bytes: u32,
    /// Maximum retained listing entries.
    pub list_entries: u32,
    /// Whole guide bytes before truncation.
    pub guide: u32,
    /// Shared LLM admission memory, in bytes.
    pub llm_pool: u64,
    /// Process memory bytes, required until the real pool lands.
    pub memory: Option<u64>,
}

/// Count and duration ceilings selected by the deployment profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// Maximum charter turns.
    pub max_turns: u32,
    /// Maximum charter spend, in the host unit.
    pub max_spend: u64,
    /// Maximum charter wall time.
    pub max_time: Duration,
    /// Maximum charter waiting time.
    pub max_waiting: Duration,
    /// Maximum brief sections.
    pub sections: u32,
    /// Maximum charter host tools.
    pub host_tools: u32,
    /// Maximum outcome verdicts.
    pub verdicts: u32,
    /// Maximum outcome items.
    pub items: u32,
    /// Maximum outcome fields.
    pub fields: u32,
    /// Unread messages per run.
    pub inbox: u32,
    /// Maximum unacknowledged channel turns.
    pub unacknowledged: u32,
    /// Window share at which compaction begins.
    pub compaction_threshold: Fraction,
    /// Idle connection retention.
    pub connection_keep: Duration,
    /// Maximum requested tool duration.
    pub tool_deadline: Duration,
    /// Default command duration.
    pub shell_timeout: Duration,
    /// Duration of each command group stop step.
    pub group_stop: Duration,
    /// IO close deadline.
    pub close: Duration,
    /// Event stream write deadline.
    pub write_deadline: Duration,
    /// Inline agent cancellation grace.
    pub cancel_grace: Duration,
    /// Spawned agent exit grace.
    pub exit_grace: Duration,
}

/// The named declarations and policies supplied by the shell to derivation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: Name,
    pub declared: Declared,
    pub policy: Policy,
}

/// One configured model's quantities, sent by the shell and kept until shutdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub name: Box<[u8]>,
    pub window: u32,
    pub output: u32,
    /// Largest opaque reasoning item, in bytes.
    pub reasoning_item: u32,
    pub head: Duration,
    pub idle: Duration,
}

/// One endpoint's progress deadlines and its declared models, supplied at startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub number: u32,
    pub connect: Duration,
    pub handshake: Duration,
    pub models: Box<[Model]>,
}

/// Numeric endpoint declarations used by service derivation, without credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    /// Configured environment encoding: names, values and one equals byte each.
    pub environment_bytes: u32,
    pub endpoints: Box<[Endpoint]>,
}

/// Today's transitional bundle; process memory remains a required override.
#[must_use]
pub const fn standard() -> Profile {
    Profile {
        name: Name::Standard,
        declared: Declared {
            conversations: 6,
            tool_payload: 65_536,
            calls_per_response: 16,
            read_window: 4096,
            shell_output: 512,
            shell_head: 256,
            shell_tail: 512,
            search_hits: 16,
            search_bytes: 1024,
            list_entries: 64,
            guide: 1024,
            llm_pool: 134_217_728,
            memory: None,
        },
        policy: Policy {
            max_turns: 64,
            max_spend: 1,
            max_time: Duration::from_secs(3600),
            max_waiting: Duration::from_secs(300),
            sections: 4,
            host_tools: 2,
            verdicts: 2,
            items: 256,
            fields: 32,
            inbox: 8,
            unacknowledged: 64,
            compaction_threshold: Fraction { numerator: 4, denominator: 5 },
            connection_keep: Duration::from_secs(15),
            tool_deadline: Duration::from_secs(20 * 60),
            shell_timeout: Duration::from_secs(2 * 60),
            group_stop: Duration::from_millis(250),
            close: Duration::from_secs(5),
            write_deadline: Duration::from_secs(30),
            cancel_grace: Duration::from_secs(5),
            exit_grace: Duration::from_secs(5),
        },
    }
}
