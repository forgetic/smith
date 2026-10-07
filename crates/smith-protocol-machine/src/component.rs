//! File request admission and terminal ownership (protocol/agent.md,
//! section 2; domain/tools.md, sections 4 and 6).

use skein_lib::{Env, List, Map, Queue, Time, Token};
use smith_domain::{run, tools};

use crate::boundary::{Below, BelowEvent, FromDomain, ToDomain};
use crate::limits::Limits;

/// One host-admitted root and its immutable write authority.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Root {
    pub(crate) token: Token,
    pub(crate) writable: bool,
}

/// Which terminal the in-flight file request owes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A bounded whole-file load.
    Load,
    /// A bounded name-ordered directory scan.
    Scan,
    /// A conditional synced file replacement.
    Store,
}

/// One operation kept until io's terminal, even after cancellation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pending {
    pub(crate) kind: Kind,
}

/// Workspace authority and in-flight file requests; io keeps the filesystem.
#[derive(Debug)]
pub struct Component {
    roots: List<Root>,
    pending: Map<Token, Pending>,
}

impl Component {
    /// Construct with fixed capacity before any request is admitted.
    #[must_use]
    pub fn new(limits: &Limits) -> Component {
        Component { roots: List::with_capacity(limits.roots), pending: Map::with_capacity(limits.operations) }
    }

    /// Install the run's already-opened root tokens and write authority.
    /// A service adopts their descriptors into `FileIo` before calling this.
    pub fn workspace(&mut self, workspace: &run::Workspace) {
        assert!(self.pending.is_empty(), "workspace changes only between runs");
        self.roots = List::with_capacity(self.roots.capacity());
        for directory in &workspace.directories {
            self.roots.push(Root { token: directory.root, writable: directory.writable }).expect("admitted root count");
        }
    }

    /// Translate one domain request; the caller reserves one cell per output.
    pub fn from_domain(
        &mut self,
        env: &Env<Limits>,
        request: FromDomain,
        to_domain: &mut Queue<ToDomain>,
        below: &mut Queue<Below>,
    ) {
        match request {
            FromDomain::Op { owner, op, deadline } => {
                assert!(!self.pending.contains_key(&owner), "one in-flight request per owner");
                if env.now >= deadline {
                    to_domain.push(ToDomain::Done { owner, done: tools::Done::TimedOut });
                    return;
                }
                match crate::files::request(&self.roots, &env.limits, owner, op) {
                    Ok((kind, request)) => {
                        let pending = Pending { kind };
                        match self.pending.insert(owner, pending) {
                            Ok(None) => below.push(Below::File { request, deadline }),
                            Ok(Some(_)) => unreachable!("owner checked before insertion"),
                            Err(_) => to_domain.push(ToDomain::Done {
                                owner,
                                done: tools::Done::Failed { fault: tools::Fault::Other },
                            }),
                        }
                    }
                    Err(done) => to_domain.push(ToDomain::Done { owner, done }),
                }
            }
            FromDomain::Cancel { owner } => {
                if self.pending.contains_key(&owner) {
                    below.push(Below::CancelFile { owner });
                }
            }
        }
    }

    /// End a file operation once io has settled it.
    pub fn from_below(
        &mut self,
        _env: &Env<Limits>,
        event: BelowEvent,
        to_domain: &mut Queue<ToDomain>,
        _below: &mut Queue<Below>,
    ) {
        match event {
            BelowEvent::File(event) => {
                let owner = event.owner();
                let pending = self.pending.remove(&owner).expect("a file terminal has an admitted owner");
                let done = crate::files::terminal(pending.kind, event);
                to_domain.push(ToDomain::Done { owner, done });
            }
        }
    }

    /// File deadlines are fired by `FileIo`; there is no component timer.
    pub fn fire(&mut self, _env: &Env<Limits>, _to_domain: &mut Queue<ToDomain>, _below: &mut Queue<Below>) {}

    /// File deadlines belong to `FileIo`, so none is armed here.
    #[must_use]
    pub const fn next_deadline(&self) -> Option<Time> {
        None
    }
}
