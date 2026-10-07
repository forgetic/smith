//! Agent's host channel opening (protocol/channel.md, sections 2 and 5).
//!
//! The component keeps Skein's framed channel and its opening state. It never
//! knows host policy, credential meaning or stream handles. `new`, `from_below`
//! and `fire` accept stream events and return lower demands plus opening events.
//!
//! | State | Input | Output |
//! |---|---|---|
//! | Opening | Open | Accept the highest shared version |
//! | Opening | Ready | Check host receive terms, then Opened or Refuse limits |
//! | Opened | End/Closed | Ended |

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod component;
mod limits;

#[cfg(test)]
mod tests;

pub use component::{Component, OpenEvent};
pub use limits::{Error, Limits, MaxOut, max_out, worst_case};
