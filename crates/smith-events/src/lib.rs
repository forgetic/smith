//! Versioned JSON-line event codec, with owned bounded records and no domain dependencies.
//! No process state or credential is known here. `write` measures and encodes a line;
//! `read` checks its version and decodes a line without retaining stream state.
//! Contract: protocol/events.md, sections 3, 5.3, 6 and 8.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod limits;
mod read;
mod records;
mod write;

pub use limits::{Limits, largest_record, largest_size, worst_case};
pub use read::{Line, Reader, read};
pub use records::{
    Agent, AnswerClass, AnswerFailure, Block, Budget, Capture, CheckCompleted, CheckStarted, CommandExit, Completion,
    CompletionFailure, ConversationClosed, ConversationEnd, ConversationKind, ConversationOpened, Count, Cpu,
    Delivered, Delivery, Effect, Event, Evidence, FailureClass, Family, Field, Form, Level, Loss, Message, Mode, Model,
    Notice, NoticeKind, Outcome, PeakRss, Prompt, Record, ResponseCompleted, ResponseStarted, Role, RunCompleted,
    RunResult, RunStarted, SessionEnded, SessionStarted, Source, Status, Stop, TextDelta, ToolCompleted, ToolStarted,
    Tools, Usage, Verdict, Versions,
};
pub use write::write;

/// The event vocabulary version emitted and accepted by this codec.
pub const VERSION: u64 = 1;

/// A codec refusal naming unsupported versions and malformed or oversized values.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Error {
    /// A line names a different vocabulary version.
    Version(u64),
    /// A line is not a complete JSON object.
    Malformed,
    /// A known field has a missing or invalid value.
    Shape,
    /// Input or configured bounds cannot be honored.
    Limits,
    /// A caller supplied content that is not UTF-8.
    Text,
}

#[cfg(test)]
mod tests;
