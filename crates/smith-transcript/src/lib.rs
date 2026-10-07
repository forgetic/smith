//! Bounded turn records; this crate retains no runtime state and knows no
//! session, provider configuration or host policy. Generated constructors,
//! encoders and decoders are the entry points. Contract:
//! `protocol/transcript.md`, sections 2-7.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[rustfmt::skip]
#[path = "generated/v2.rs"]
pub mod v2;

pub use v2::{
    Ambiguous, AmbiguousParts, Block, Call, CallParts, CallProblem, CommandEnd, CommandEndParts, Edited, EditedParts,
    Entry, EntryParts, Exit, ExitCode, ExitCodeParts, ExitSignal, ExitSignalParts, Failed, FailedParts, Fault,
    FieldProblem, FieldProblemParts, Found, FoundParts, Hit, HitParts, Invalid, InvalidParts, Kind, LineNumber,
    LineNumberParts, Listed, ListedParts, Message, MessageParts, Opaque, OpaqueParts, Outcome, OwnedOutcome,
    OwnedOutcomeParts, Read, ReadParts, Replay, ReplayParts, Returned, Role, Said, SaidParts, Text, TextParts,
    TooLarge, TooLargeParts, ToolResult, ToolResultParts, Turn, TurnParts, Usage, UsageParts, Written, WrittenParts,
};
