//! Static receiving compatibility and simultaneous translation ownership.
//! All prices use checked arithmetic before preparation or final allocation.

use core::mem::size_of;

use skein_lib::{Duration, List};
use skein_llm::{Block, Message, Tool, client};
use smith_domain::{llm, session};

use crate::types::{Context, ResolvedCall, ToolSchema};

/// Immutable protocol bounds for application translation and the shared Client.
/// The caller supplies these at startup, independently from application scheduling limits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// Actual shared Client document, HTTP/SSE and receiving bounds.
    pub client: client::Limits,

    /// Aggregate retained served/application declaration wrappers and all owned fields.
    pub tool_bytes: u64,

    /// Maximum rendered tool result, including its cut marker, in bytes.
    pub rendered_result: u32,

    /// Default command deadline stated by the tool description.
    pub shell_default: Duration,

    /// Maximum command deadline stated by the tool description.
    pub shell_maximum: Duration,
}

/// Actual root Complete receiving contract, checked before preparing the Client.
/// No receiving field is inferred from an application's token budget.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Receiving {
    /// Maximum complete translated content ownership, including all block cells and replay envelopes.
    pub max_completion_bytes: u64,

    /// Maximum actual assistant blocks, including text, refusal and opaque blocks.
    pub max_completion_blocks: u32,

    /// Aggregate decoded-call ownership across this whole completion, including wrappers and fields.
    pub decoded_call_bytes: u64,

    /// Exact diagnostic bytes accepted on the actual Failed boundary.
    pub max_failure_bytes: u32,
}

/// Complete translated ownership for every completion admitted by the shared Client.
/// Original text/IDs/arguments are under `answer_bytes`; each replay separately
/// receives its full raw opaque allowance plus the exported envelope header.
/// Decoded ownership is one aggregate allowance, never multiplied per block.
/// It must first cover one full fallback `Decoded` cell per possible output part.
/// Returns None on overflow or an invalid shared-client configuration.
#[must_use]
pub fn completion_worst_case(client_limits: &client::Limits, decoded_call_bytes: u64) -> Option<u64> {
    client::worst_case(client_limits)?;
    let parts = u64::from(client_limits.dialect.parts);
    let decoded_cells = parts.checked_mul(u64::try_from(size_of::<llm::Decoded>()).ok()?)?;
    if decoded_call_bytes < decoded_cells {
        return None;
    }
    let replay = u64::from(skein_llm::replay_bytes(&client_limits.dialect)?).checked_mul(parts)?;
    let cell = size_of::<llm::Said>().max(size_of::<session::llm::Block>());
    let wrappers = u64::try_from(cell).ok()?.checked_mul(parts)?;
    u64::from(client_limits.dialect.answer_bytes)
        .checked_add(replay)?
        .checked_add(wrappers)?
        .checked_add(decoded_call_bytes)
}

/// Protocol footprint, including one Client, its retained declaration context,
/// native prompt/schema/replay transit and simultaneous translated output.
/// Caller-owned root input, endpoint/credential grants, application decoder input
/// and observation queues are priced separately by their owners. None means
/// receiving incompatibility or arithmetic overflow; no Client is prepared.
#[must_use]
pub fn worst_case(limits: &Limits, receiving: &Receiving) -> Option<u64> {
    if limits.rendered_result < crate::CUT_MARKER_BYTES || limits.rendered_result > limits.client.dialect.string_bytes {
        return None;
    }
    let completion = completion_worst_case(&limits.client, receiving.decoded_call_bytes)?;
    if completion > receiving.max_completion_bytes
        || limits.client.dialect.parts > receiving.max_completion_blocks
        || limits.client.dialect.detail_bytes > receiving.max_failure_bytes
    {
        return None;
    }
    let parts = limits.client.dialect.parts;
    // Native schemas and restored history replays can both retain a full
    // document before Client request admission, plus the Client's pure
    // preparation copies. Canonical feedback can independently reach the
    // string cap even when that exceeds the configured document cap.
    let schema_tokens = u64::from(parts)
        .checked_mul(u64::from(limits.client.dialect.tokens))?
        .checked_mul(u64::try_from(size_of::<skein_json::Token>()).ok()?)?;
    let slots = List::<Tool>::worst_case(parts)?
        .checked_add(List::<Message>::worst_case(parts)?)?
        .checked_add(List::<Block>::worst_case(parts)?)?
        .checked_add(List::<ToolSchema>::worst_case(parts)?)?
        .checked_add(List::<Option<ResolvedCall>>::worst_case(parts)?)?;
    client::worst_case(&limits.client)?
        .checked_add(u64::try_from(size_of::<Context>()).ok()?)?
        .checked_add(limits.tool_bytes.checked_mul(2)?)?
        .checked_add(slots.checked_mul(2)?)?
        .checked_add(schema_tokens.checked_mul(4)?)?
        .checked_add(u64::from(parts).checked_mul(u64::from(limits.client.dialect.document_bytes))?.checked_mul(4)?)?
        .checked_add(u64::from(parts).checked_mul(u64::from(limits.client.dialect.string_bytes))?.checked_mul(2)?)?
        .checked_add(u64::from(limits.client.dialect.request_bytes).checked_mul(2)?)?
        .checked_add(skein_llm::replay_worst_case(&limits.client.dialect)?)?
        .checked_add(completion.checked_mul(2)?)?
        .checked_add(u64::from(receiving.max_failure_bytes))
}

/// Derive one result's rendering from the workspace caps and host/delegated text.
/// Every invalid byte expands to four visible escape bytes; a read's every
/// byte may be a line, with a ten-digit number, colon and space. Fixed room
/// holds counts, separators, status words and the explicit omitted-byte marker.
/// Contract: protocol/limits.md, section 3.4; protocol/llm.md, section 5.
#[must_use]
pub fn render_worst_case(tools: &smith_domain::tools::Limits, host_answer: u32, delegated_answer: u64) -> Option<u32> {
    let read = u64::from(tools.read_bytes).checked_mul(16)?.checked_add(96)?;
    let listed = tools
        .list_bytes
        .checked_mul(4)?
        .checked_add(u64::from(tools.list_entries).checked_mul(11)?)?
        .checked_add(64)?;
    let search = u64::from(tools.search_bytes)
        .checked_mul(4)?
        .checked_add(u64::from(tools.search_hits).checked_mul(14)?)?
        .checked_add(64)?;
    let shell =
        u64::from(tools.shell_head).checked_add(u64::from(tools.shell_tail))?.checked_mul(4)?.checked_add(96)?;
    let ambiguity = u64::from(tools.match_lines).checked_mul(12)?.checked_add(64)?;
    let host = u64::from(host_answer).checked_add(7)?;
    let largest = read.max(listed).max(search).max(shell).max(ambiguity).max(host).max(delegated_answer).max(128);
    u32::try_from(largest.checked_add(u64::from(crate::render::CUT_MARKER_BYTES))?).ok()
}
