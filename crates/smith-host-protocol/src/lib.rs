//! Host's agent channel opening (protocol/channel.md, sections 2 and 5;
//! domain/host.md, section 3).
//!
//! The component keeps Skein's framed channel and its opening state. It never
//! knows credentials' values, host policy or stream handles. `new`, `open`,
//! `from_below` and `fire` accept stream events and return lower demands and
//! host opening events. A service later attaches one component per agent.
//!
//! | State | Input | Output |
//! |---|---|---|
//! | Idle | Open | Send Open with empty pipe credential |
//! | Opening | Ready | Check agent receive terms, then Opened or Refuse limits |
//! | Opened | End/Closed | Hangup |

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod component;
mod limits;
mod translate;

#[cfg(test)]
mod tests;

pub use component::{Component, OpenEvent};
pub use limits::{Error, Limits, MaxOut, max_out, worst_case};
pub use translate::{Values, encode_start};
