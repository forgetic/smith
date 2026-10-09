//! Content-free slot observations (domain/host.md, sections 9.1 and 10).
//! The agent bounds this queue independently of its roots' observation drains.
//! The owner reserves room before each step and drains facts before reclaim.

use skein_lib::{Time, Token};
use smith_host_domain::End;

/// A slot observation emitted by the inline agent for its owner to drain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fact {
    pub at: Time,
    pub kind: FactKind,
}

/// One inline lifecycle observation, sent without payload or credential detail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FactKind {
    /// A slot opened under this parent client.
    Opened { client: Token },
    /// The parent or wall bound requested cancellation once.
    Stopped { client: Token },
    /// A spawn refused or all owned rights settled under this parent client.
    Gone { client: Token, end: End },
}
