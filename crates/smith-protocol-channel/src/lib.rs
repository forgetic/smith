//! Agent's host channel opening and Start translation (protocol/channel.md,
//! sections 2, 3 and 5; protocol/charter.md, sections 2 and 5).
//!
//! The component keeps Skein's framed channel, the run's channel phase and configured
//! endpoint names. It never knows host policy, credential values, addresses or
//! stream handles. `new`, `from_below`, `fire`, `send_admitted` and `send_answer`
//! return lower demands and a decoded Start with owned domain charter policy.
//!
//! | State | Input | Output |
//! |---|---|---|
//! | Opening | Open | Accept the highest shared version |
//! | Opening | Ready | Check host receive terms, then Opened or Refuse limits |
//! | Opened | Start | Decode its charter and resolve endpoint names, or answer invalid |
//! | Started | Admitted | Send the domain's admission notice once |
//! | Started/Admitted | Answer | Send the final answer and finish writes |
//! | Opened | End/Closed | Ended |

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
pub use translate::{Endpoint, Endpoints, answer_record, decode_charter, encode_result};
