//! The spawned capability's process face (domain/host.md, section 4).
//! This stateless vocabulary carries process and channel effects and their
//! exact terminals. It knows no parent policy or credential values; Reaped
//! proves the contained tree is empty after Exited.

use crate::{Down, Signal, Up};
use alloc::boxed::Box;
use skein_lib::{Time, Token};

/// Process and channel terminals sent by the lower adapter.
#[derive(PartialEq, Eq, Debug)]
pub enum Event {
    /// Terminal of lower Spawn; process owns its tree.
    Spawned { owner: Token, process: Token },
    /// Terminal of lower Spawn; no process resources remain.
    Unspawned {
        owner: Token,
        /// Operator tail kept up to `Limits::detail_bytes`.
        detail: Box<[u8]>,
    },
    /// Terminal of Send; release its exact reservation.
    Sent { owner: Token },
    /// Terminal of Send; drain what agent already wrote.
    Unsent { owner: Token },
    /// Terminal of Read; decoded V2 record.
    Received {
        owner: Token,
        /// Bounded agent metadata and payload.
        message: Up,
    },
    /// Terminal of Read; undecodable/version-mismatched or oversized channel frame.
    Malformed { owner: Token },
    /// Terminal of Read; channel EOF.
    Hangup { owner: Token },
    /// Terminal of Signal, whether or not signal reached process.
    Signalled { owner: Token },
    /// Terminal of Wait; descendants may still run.
    Exited { owner: Token },
    /// Terminal of Reap after Exited; proves `TreeEmpty`.
    Reaped {
        owner: Token,
        /// Operator tail only, never LLM feedback.
        detail: Box<[u8]>,
    },
}

/// Effects sent to the lower process adapter, each with one terminal.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "sealed payloads are priced by bounded queues and state")]
pub enum Request {
    /// Lower contained process spawn with credential-free environment.
    Spawn {
        owner: Token,
        /// Optional prepared workspace handle.
        workspace: Option<Token>,
        /// Lower Spawn must settle by this deadline.
        deadline: Time,
    },
    /// Lower channel write; exactly one Sent/Unsent.
    Send {
        owner: Token,
        process: Token,
        /// Owned downlink payload counted by receiving lower layer.
        message: Down,
    },
    /// Demand one record; exactly one Received/Malformed/Hangup.
    Read { owner: Token, process: Token },
    /// Signal every process-tree member; exactly one Signalled.
    Signal {
        owner: Token,
        process: Token,
        /// Terminate or Kill.
        signal: Signal,
    },
    /// Wait for main process exit; exactly one Exited.
    Wait { owner: Token, process: Token },
    /// Await empty contained tree after exit; exactly one Reaped.
    Reap { owner: Token, process: Token },
}
