//! Owned requests and terminals at the machine boundary. Only the caller
//! retains domain state; io owns descriptors (protocol/agent.md, section 2).

use skein_io::file;
use skein_lib::{Time, Token};
use smith_domain::tools;

/// A domain request sent by the agent service to the machine.
#[derive(Debug)]
pub enum FromDomain {
    /// One tools operation, ending in `ToDomain::Done`.
    Op { owner: Token, op: tools::Op, deadline: Time },
    /// Withdraw one operation; its terminal remains owed.
    Cancel { owner: Token },
}

/// One terminal returned to the agent service for the domain.
#[derive(Debug, PartialEq, Eq)]
pub enum ToDomain {
    /// The tools operation's single terminal.
    Done { owner: Token, done: tools::Done },
}

/// A request to skein's file driver, in domain emission order.
#[derive(Debug, PartialEq, Eq)]
pub enum Below {
    /// One bounded operation with its absolute domain deadline; the service
    /// calls `file_layer::down_until` after earlier requests are submitted.
    File { request: file::Request, deadline: Time },
    /// Withdraw this active file owner through `file_layer::cancel`.
    CancelFile { owner: Token },
}

/// A terminal from skein's file driver.
#[derive(Debug)]
pub enum BelowEvent {
    /// One file operation ended.
    File(file::Event),
}
