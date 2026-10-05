//! Content-free bounded host observations (domain/host.md, section 4;
//! programming-model.md, section 3). Dropping facts never changes a terminal.
use crate::{End, Fault};
use skein_lib::Token;
/// Parent-drained diagnostic observation; no payload or credential detail (domain/host.md, section 4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fact {
    /// A contained process started; independent from run admission (domain/host.md, section 4).
    Started {
        /// Opaque parent owner (domain/host.md, section 4).
        client: Token,
    },
    /// Run accepted its start (domain/host.md, section 2).
    Admitted {
        /// Opaque parent owner (domain/host.md, section 2).
        client: Token,
    },
    /// One agent last word was accepted (domain/host.md, section 2).
    Answered {
        /// Opaque parent owner (domain/host.md, section 2).
        client: Token,
    },
    /// One typed agent failure was reported (domain/host.md, section 4).
    Faulted {
        /// Opaque parent owner (domain/host.md, section 4).
        client: Token,
        /// Content-free failure class (domain/host.md, section 4).
        fault: Fault,
    },
    /// Spawn containment terminal, after every retained right (domain/host.md, section 4).
    Gone {
        /// Opaque parent owner (domain/host.md, section 4).
        client: Token,
        /// Entrance refusal or completed cleanup (domain/host.md, section 4).
        end: End,
    },
}
