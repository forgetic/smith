//! Historical `ChatGPT` subscription identity from tongs' redacted subject
//! recordings, 2026-06-13 (recorder f4e0a2b). These were successful exchanges
//! made by tongs; fresh captures of Codex itself remain a deployment task.
//!
//! Contract: domain/session.md, sections 4 and 12; programming-model.md, sections 4.4 and 6.3.

/// Named subject archive from which these fixed provider identity bytes were copied.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const PROVENANCE: &[u8] = b"tongs subject capture 2026-06-13 f4e0a2b";

/// Subject-captured originator identity value.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const ORIGINATOR: &[u8] = b"pi";

/// Subject-captured user-agent identity bytes; this codec does not derive a runtime identity.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const USER_AGENT: &[u8] = b"pi (linux; x86_64)";

/// Subject-captured provider beta feature header, emitted unchanged.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const BETA: &[u8] = b"responses=experimental";

/// Provider header name for the configured account identity.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const ACCOUNT_HEADER: &[u8] = b"chatgpt-account-id";

/// Provider header name for the caller-supplied session identity.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const SESSION_HEADER: &[u8] = b"session_id";

/// Provider header name for the caller-supplied request identity.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const REQUEST_HEADER: &[u8] = b"x-request-id";

/// Subject-captured output-verbosity setting.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const VERBOSITY: &[u8] = b"low";

/// Subject-captured provider tool name for reading; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const READ_TOOL: &[u8] = b"read";

/// Subject-captured provider tool name for listing; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const LIST_TOOL: &[u8] = b"list";

/// Subject-captured provider tool name for search; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const SEARCH_TOOL: &[u8] = b"search";

/// Subject-captured provider tool name for writing; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const WRITE_TOOL: &[u8] = b"write";

/// Subject-captured provider tool name for editing; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const EDIT_TOOL: &[u8] = b"edit";

/// Subject-captured provider tool name for shell execution; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const SHELL_TOOL: &[u8] = b"shell";

/// Subject-captured provider tool name for sub-agent; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const SUBAGENT_TOOL: &[u8] = b"subagent";

/// Subject-captured provider tool name for finish; the protocol supplies its schema.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const FINISH_TOOL: &[u8] = b"finish";

use alloc::boxed::Box;
use skein_lib::bytes;

/// Fixed identity headers; the owner supplies bearer, account and ids.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Header {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub name: Box<[u8]>,
    /// Fixed subject-capture HTTP header bytes, copied without credentials.
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub value: Box<[u8]>,
}

/// Fixed subject-capture identity headers for this dialect, without credentials or generated session IDs.
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[must_use]
pub fn headers() -> Box<[Header]> {
    Box::new([header(b"originator", ORIGINATOR), header(b"user-agent", USER_AGENT), header(b"OpenAI-Beta", BETA)])
}

fn header(name: &[u8], value: &[u8]) -> Header {
    Header { name: bytes::copy_of(name), value: bytes::copy_of(value) }
}
