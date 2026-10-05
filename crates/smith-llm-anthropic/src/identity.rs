//! Subscription identity data. Header values were observed in tongs' redacted
//! subject recordings (2026-06-13, recorder f4e0a2b), whose provider responses
//! were successful. They are historical compatibility evidence, not a fresh
//! capture of Claude Code. A billing line and metadata must be supplied from
//! the deployment's verified identity rather than invented here.
//! Contract: domain/session.md, sections 4 and 12; programming-model.md, sections 4.4 and 6.3.
/// Named subject archive from which these fixed provider identity bytes were copied.
pub const PROVENANCE: &[u8] = b"tongs subject capture 2026-06-13 f4e0a2b";

/// Provider API version header preserved from the named subject capture.
pub const VERSION: &[u8] = b"2023-06-01";

/// Subject-captured user-agent identity bytes; this codec does not derive a runtime identity.
pub const USER_AGENT: &[u8] = b"claude-cli/2.1.139 (external, sdk-cli)";

/// Subject-captured provider beta feature headers, emitted unchanged.
pub const BETAS: &[u8] = b"claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27,prompt-caching-scope-2026-01-05,advisor-tool-2026-03-01,advanced-tool-use-2025-11-20,context-1m-2025-08-07,effort-2025-11-24,extended-cache-ttl-2025-04-11";

/// Subject-captured system identity text, preserved as provider vocabulary.
pub const SYSTEM: &[u8] = b"You are Claude Code, Anthropic's official CLI for Claude.";

/// Subject-captured prompt-cache lifetime label.
pub const CACHE_TTL: &[u8] = b"1h";

/// Provider header name for the caller-supplied session identity.
pub const SESSION_HEADER: &[u8] = b"X-Claude-Code-Session-Id";

/// Provider header name for the caller-supplied request identity.
pub const REQUEST_HEADER: &[u8] = b"x-client-request-id";

/// Subject-captured provider tool name for reading; the protocol supplies its schema.
pub const READ_TOOL: &[u8] = b"Read";

/// Subject-captured provider tool name for listing; the protocol supplies its schema.
pub const LIST_TOOL: &[u8] = b"Glob";

/// Subject-captured provider tool name for search; the protocol supplies its schema.
pub const SEARCH_TOOL: &[u8] = b"Grep";

/// Subject-captured provider tool name for writing; the protocol supplies its schema.
pub const WRITE_TOOL: &[u8] = b"Write";

/// Subject-captured provider tool name for editing; the protocol supplies its schema.
pub const EDIT_TOOL: &[u8] = b"Edit";

/// Subject-captured provider tool name for shell execution; the protocol supplies its schema.
pub const SHELL_TOOL: &[u8] = b"Bash";

/// Subject-captured provider tool name for sub-agent; the protocol supplies its schema.
pub const SUBAGENT_TOOL: &[u8] = b"Task";

/// Subject-captured provider tool name for finish; the protocol supplies its schema.
pub const FINISH_TOOL: &[u8] = b"Finish";

use alloc::boxed::Box;
use skein_lib::bytes;

/// Owned fixed identity headers. Credentials and per-call/session ids are
/// added by the connection's owner, never kept in this module.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Header {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    pub name: Box<[u8]>,
    /// Owned field value or environment value, bounded by its enclosing record.
    pub value: Box<[u8]>,
}

/// Fixed subject-capture identity headers for this dialect, without credentials or generated session IDs.
#[must_use]
pub fn headers() -> Box<[Header]> {
    Box::new([
        header(b"anthropic-version", VERSION),
        header(b"anthropic-beta", BETAS),
        header(b"user-agent", USER_AGENT),
        header(b"x-app", b"cli"),
        header(b"X-Stainless-Arch", b"x64"),
        header(b"X-Stainless-Lang", b"js"),
        header(b"X-Stainless-OS", b"Linux"),
        header(b"X-Stainless-Package-Version", b"0.93.0"),
        header(b"X-Stainless-Retry-Count", b"0"),
        header(b"X-Stainless-Runtime", b"node"),
        header(b"X-Stainless-Runtime-Version", b"v24.3.0"),
        header(b"X-Stainless-Timeout", b"600"),
    ])
}

fn header(name: &[u8], value: &[u8]) -> Header {
    Header { name: bytes::copy_of(name), value: bytes::copy_of(value) }
}
