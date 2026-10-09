//! The standard host profile keeps the spawned-agent and delivery bounds.
//! It never knows a chat or an agent's charter. `host_limits` grants a window
//! large enough for the agent's largest turn. Contract: shell.md, section 2.1.

use skein_lib::Duration;
use smith_host_domain as host;

/// Build the standard host bounds around the largest turn its agent can emit.
#[must_use]
pub fn host_limits(largest_turn: u64) -> host::Limits {
    host::Limits {
        agents: 1,
        directories: 2,
        conflicts: 64,
        path_bytes: 4096,
        name_bytes: 256,
        accounts: 4,
        charter_bytes: 262_144,
        transcript_bytes: 33_554_432,
        answered_bytes: 65_536,
        message_bytes: 4096,
        messages: 8,
        calls: 16,
        call_bytes: 65_536,
        answer_bytes: host::Delivered::worst_case().max(65_536),
        turns: 64,
        turn_bytes: largest_turn.max(262_144),
        unacknowledged_bytes: largest_turn.max(33_554_432),
        fact_bytes: 4096,
        outcome_bytes: 4096,
        detail_bytes: 4096,
        spawn_timeout: Duration::from_secs(20),
        no_progress: Duration::from_secs(300),
        long_span: Duration::from_secs(3600),
        wall_time: Duration::from_secs(3600),
        grace: Duration::from_secs(5),
        kill_after: Duration::from_secs(2),
        facts: 1024,
    }
}
