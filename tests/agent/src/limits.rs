//! Copied process limits: temper `25ac2ad`, domain/run.md, section 14.
//! The world supplies immutable caps; none is inferred from domain state.

use skein_lib::Duration;
use smith_domain::{Limits, run, session, tools};

/// The most a charter may ask for in the calm world, which every session may
/// take.
pub const BUDGET: run::Budget = run::Budget {
    turns: 64,
    input: 1 << 24,
    output: 1 << 24,
    cache_read: 1 << 26,
    cache_write: 1 << 24,
    time: Duration::from_secs(3600),
};

const CEILING: session::Budget = session::Budget {
    turns: BUDGET.turns,
    input: BUDGET.input,
    output: BUDGET.output,
    cache_read: BUDGET.cache_read,
    cache_write: BUDGET.cache_write,
    time: BUDGET.time,
};

/// An agent process's limits in the calm world: room for the one run it
/// carries, with a few conversations, sub-agents nested two deep beneath
/// main.
pub const LIMITS: Limits = Limits {
    accounts: 4,
    skew: Duration::ZERO,
    run: run::Limits {
        runs: 1,
        conversations: 6,
        run_bytes: 1 << 16,
        repositories: 2,
        outlets: 2,
        verdicts: 2,
        calls: 16,
        budget: BUDGET,
        max_tokens: 4096,
        models: 2,
        depth: 2,
        run_conversations: 6,
        answer_bytes: 1024,
        nudges: 1,
        guide_bytes: 1024,
        io_timeout: Duration::from_secs(5),
        outcome_bytes: 4096,
        check_timeout: Duration::from_secs(60),
        check_tail: 512,
        facts: 1024,
    },
    session: session::Limits {
        sessions: 6,
        spend: 0,
        messages: 64,
        session_bytes: 1 << 20,
        budget: CEILING,
        max_tokens: 4096,
        retries: 3,
        backoff_base: Duration::from_millis(200),
        backoff_max: Duration::from_secs(5),
        call_timeout: Duration::from_secs(60),
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

/// An agent process's limits in random worlds: room for fewer conversations,
/// sessions and calls than its run may ask for, so that some are refused.
pub const TIGHT: Limits = Limits {
    accounts: LIMITS.accounts,
    skew: LIMITS.skew,
    run: run::Limits {
        conversations: 4,
        calls: 6,
        run_conversations: 4,
        answer_bytes: 256,
        guide_bytes: 256,
        check_tail: 128,
        ..LIMITS.run
    },
    session: session::Limits {
        sessions: 4,
        spend: 0,
        messages: 32,
        session_bytes: 1 << 16,
        retries: 2,
        call_timeout: Duration::from_secs(20),
        tool_timeout: Duration::from_secs(30),
        parallel_tools: 3,
        tools: tools::Limits { kits: 4, calls: 3, ..LIMITS.session.tools },
        ..LIMITS.session
    },
};
