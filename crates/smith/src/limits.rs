//! Baseline limits selected by the `standard` configuration profile.
//! They cap one run without learning its charter or credential contents.
//! Contract: protocol/agent.md, section 4; domain/run.md, section 14.

use skein_lib::Duration;
use smith_domain::{Limits, run, session, tools};

/// The largest run budget under the standard profile.
const BUDGET: run::Budget = run::Budget { turns: 128, spend: 1, time: Duration::from_secs(3600) };

const CEILING: session::Budget = session::Budget {
    turns: BUDGET.turns,
    input: 1 << 24,
    output: 1 << 24,
    cache_read: 1 << 26,
    cache_write: 1 << 24,
    time: BUDGET.time,
};

/// One run with room for main and several child conversations.
pub const LIMITS: Limits = Limits {
    accounts: 4,
    endpoints: 3,
    decoded_call_bytes: 4096,
    skew: Duration::ZERO,
    run: run::Limits {
        runs: 1,
        conversations: 6,
        run_bytes: 1 << 16,
        brief_sections: 4,
        directories: 2,
        directory_name_bytes: 256,
        conflicts: 64,
        conflict_path_bytes: 4096,
        host_tools: 2,
        host_input_bytes: 65_536,
        host_reply_bytes: 65_536,
        answered_calls: 16,
        answered_bytes: 4096,
        host_timeout: Duration::from_secs(60),
        host_backoff: Duration::from_millis(50),
        verdicts: 2,
        calls: 16,
        budget: BUDGET,
        max_tokens: 4096,
        models: 2,
        run_conversations: 6,
        answer_bytes: 1024,
        nudges: 1,
        guide_bytes: 1024,
        io_timeout: Duration::from_secs(5),
        outcome_bytes: 4096,
        delivery_timeout: Duration::from_secs(60),
        check_timeout: Duration::from_secs(60),
        check_tail: 512,
        facts: 1024,
        messages: 8,
        message_bytes: 4096,
        waiting: skein_lib::Duration::from_secs(300),
    },
    session: session::Limits {
        sessions: 6,
        spend: 1,
        protocol_allowance: 0,
        messages: 64,
        session_bytes: 33_554_432,
        completion_bytes: 4096,
        completion_blocks: 16,
        failure_bytes: 512,
        delegated_result_bytes: 4_194_304,
        budget: CEILING,
        max_tokens: 4096,
        retries: 3,
        backoff_base: Duration::from_millis(200),
        backoff_max: Duration::from_secs(5),
        // Advanced reasoning can exceed a minute before its final tool call.
        // The protocol's head and idle waits share this finite ceiling; the
        // session's earlier expiry still cancels the call independently.
        call_timeout: Duration::from_secs(300),
        tool_timeout: Duration::from_secs(60),
        facts: 1024,
        parallel_tools: 4,
        tools: tools::Limits {
            kits: 6,
            calls: 4,
            repos: 2,
            path_bytes: 256,
            known_files: 16,
            file_bytes: 1 << 16,
            read_bytes: 4096,
            list_entries: 64,
            list_bytes: 4096,
            match_lines: 8,
            file_timeout: Duration::from_secs(30),
            env_bytes: 256,
            shell_timeout: Duration::from_secs(30),
            shell_timeout_max: Duration::from_secs(120),
            shell_head: 256,
            shell_tail: 256,
            search_hits: 16,
            search_bytes: 1024,
            search_timeout: Duration::from_secs(30),
            facts: 1024,
        },
    },
};
