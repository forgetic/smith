//! The standard agent profile keeps fixed deployment and policy bounds and
//! computes the service limits from them. It never knows endpoint destinations,
//! credentials or a run's charter. `standard_limits` returns typed refusals.
//! Contract: shell.md, section 2.1; protocol/agent.md, section 4.

use skein_http as http;
use skein_io as io;
use skein_lib::Duration;
use skein_llm::{self as shared, openai};
use skein_llm_connection as connection;
use skein_tls as tls;
use smith_domain::{self as domain, run, session, tools};
use smith_protocol_channel as channel;
use smith_protocol_llm as llm;
use smith_protocol_machine as machine;

use crate as service;

/// The number of configured destinations the standard profile admits.
pub const ENDPOINTS: u32 = 3;

/// The number of credential accounts the standard profile admits.
pub const ACCOUNTS: u32 = 4;

/// A standard profile construction refusal, returned to the shell for wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileError {
    /// The LLM receiving footprint cannot be represented.
    Receiving,
    /// The channel body or frame footprint cannot be represented.
    ChannelSchema(smith_channel::Overflow),
    /// The standard profile's channel version is absent.
    ChannelVersion,
    /// The channel frame footprint cannot be represented.
    ChannelFrame,
    /// The IO route count cannot be represented.
    IoRoutes,
}

/// The largest run budget under the standard profile.
const BUDGET: run::Budget = run::Budget { turns: 64, spend: 1, time: Duration::from_secs(3600) };

const CEILING: session::Budget = session::Budget {
    turns: BUDGET.turns,
    input: 1 << 24,
    output: 1 << 24,
    cache_read: 1 << 26,
    cache_write: 1 << 24,
    time: BUDGET.time,
};

/// One run with room for main and several child conversations.
pub const LIMITS: domain::Limits = domain::Limits {
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
        offer_messages: 8,
        offer_bytes: 32_782,
        waiting: Duration::from_secs(300),
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

/// Build the standard agent deployment limits for the shell-selected memory ceiling.
pub fn standard_limits(memory: u64) -> Result<service::Limits, ProfileError> {
    let client = client_limits();
    let decoded_call_bytes = 4096;
    let completion = llm::completion_worst_case(&client, decoded_call_bytes).ok_or(ProfileError::Receiving)?;
    let bodies = smith_channel::CEILINGS;
    let output_bytes = channel_output(&bodies)?;
    let io = io::Limits {
        sockets: 32,
        refusals: 4,
        intake: 19_000,
        receive: 4096,
        output: output_bytes.max(19_000),
        sends: 8,
        accepts: 1,
        backlog: 2,
        close_timeout: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    };
    let mut domain = LIMITS;
    domain.session.completion_bytes = completion;
    // Before skein's derivation removes its sent-text cap, rendering takes the
    // smaller cap and marks oversized outcomes rather than refusing the call.
    let rendered_result = llm::render_worst_case(
        &domain.session.tools,
        domain.run.host_reply_bytes.max(domain.run.message_bytes),
        domain.session.delegated_result_bytes,
    )
    .ok_or(ProfileError::Receiving)?
    .min(client.dialect.string_bytes);
    let queue = domain::max_out(&domain).max(256);
    let operations = io::operations(&io).ok_or(ProfileError::IoRoutes)?;
    let routes = operations.checked_add(64).ok_or(ProfileError::IoRoutes)?;
    Ok(service::Limits {
        domain,
        channel: channel::Limits {
            bodies,
            charter: smith_charter::CEILINGS,
            transcript: smith_transcript::CEILINGS,
            channel: skein_channel::Limits {
                chunk: 4096,
                credential: 0,
                skip: 4096,
                output_bytes,
                output_frames: 4,
                kinds: 18,
            },
            endpoints: ENDPOINTS,
            calls: 16,
            turns: 64,
            fact_reserve_frames: 1,
            fact_reserve_bytes: 128,
            grants: 8,
        },
        llm: llm::ComponentLimits {
            adapter: llm::Limits { client, tool_bytes: 32_768, rendered_result },
            connection: connection::Limits {
                endpoints: ENDPOINTS,
                connections: 6,
                calls: 6,
                per_endpoint: 2,
                idle_keep: Duration::from_secs(15),
                io,
                tls: tls_limits(),
            },
            receiving: llm::Receiving {
                max_completion_bytes: completion,
                max_completion_blocks: client.dialect.parts,
                decoded_call_bytes,
                max_failure_bytes: domain.session.failure_bytes,
            },
            contract_bytes: 4096,
            accounts: ACCOUNTS,
            grant_value_bytes: 2048,
            connect: Some(Duration::from_secs(10)),
            handshake: Some(Duration::from_secs(10)),
            head: Some(Duration::from_secs(60)),
            idle: Some(Duration::from_secs(30)),
        },
        machine: machine::Limits {
            operations: 16,
            roots: 2,
            path_bytes: 4096,
            file_bytes: 65_536,
            entries: 64,
            entry_bytes: 4096,
            processes: 8,
            output_bytes: 1024,
            search_hits: 16,
            search_bytes: 1024,
            search_line_bytes: 4096,
            env_bytes: 8192,
            stop_grace: Duration::from_millis(250),
        },
        io,
        file_slots: 32,
        file_read: 65_536,
        file_entries: 64,
        file_bytes: 65_536,
        file_timeout: Duration::from_secs(30),
        queue,
        routes,
        memory,
    })
}

// Endpoint-derived request heads include both credential ceilings and fit one
// native TLS plaintext piece (skein's llm-connection.md, section 7).
fn tls_limits() -> tls::client::Limits {
    tls::client::Limits { read: 4096, send: tls::client::MAX_PLAINTEXT, records: tls::client::MAX_RECORD }
}

fn client_limits() -> shared::client::Limits {
    shared::client::Limits {
        http: http::client::Limits { request: 4096, head: 4096, headers: 32, read: 256, send: 31 },
        sse: http::sse::Limits { line: 4096, event: 8192, field: 128, chunk: 128 },
        dialect: openai::Limits {
            request_bytes: 8192,
            document_bytes: 8192,
            string_bytes: 4096,
            depth: 32,
            tokens: 1024,
            parts: 16,
            input_bytes: 2048,
            opaque_bytes: 2048,
            answer_bytes: 8192,
            detail_bytes: 256,
        },
        error_bytes: 4096,
    }
}

fn channel_output(bodies: &smith_channel::Limits) -> Result<u32, ProfileError> {
    let schema = match smith_channel::schema(bodies) {
        Ok(schema) => schema,
        Err(error) => return Err(ProfileError::ChannelSchema(error)),
    };
    let version = schema.version(2).ok_or(ProfileError::ChannelVersion)?;
    let mut largest = 0;
    for kind in &version.kinds {
        largest = largest.max(kind.largest);
    }
    largest.checked_add(8).ok_or(ProfileError::ChannelFrame)
}
