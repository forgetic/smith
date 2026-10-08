//! The local host's terminal boundary (protocol/hosts.md, section 5.1;
//! domain/host.md, section 8). It retains one bounded, possibly continued
//! line and whether an interrupt has already cancelled the current run. It
//! never knows the chat, the agent, or how the terminal is opened. `feed`,
//! `interrupt`, `answer_finished`, and `show` are its entry points. Their
//! caller reserves `max_out` and settles each output before feeding more.
//!
//! | State | Input | Output |
//! |---|---|---|
//! | Collecting | newline without trailing backslash | one person line |
//! | Collecting | newline with trailing backslash | retain continued line |
//! | Any | first interrupt | cancel the run |
//! | Cancelled | second interrupt | stop the process |
//! | Any | answer finished | clear interrupt count |

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod delivery_store;
mod store;
mod terminal;

pub use delivery_store::{decode_delivery, save_delivery};
pub use store::{File, StoreError, decode_state, decode_turns, save_state, save_turn, turn_name};
pub use terminal::{Event, Limits, Terminal, max_out, worst_case};
