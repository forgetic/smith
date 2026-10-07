//! Turns told by the agent and waiting to be saved and acknowledged
//! (domain/host.md, sections 2 and 6). The answer stays here until the last
//! save terminal; a slow store therefore cannot let the person see it early.

use skein_lib::Queue;
use smith_domain::run;

/// Bounded unsaved activation numbers and conversation positions, and a held child answer.
#[derive(Debug)]
pub(crate) struct Turns {
    pub(crate) unsaved: Queue<(u32, u32)>,
    pub(crate) answer: Option<run::Answer>,
}

impl Turns {
    pub(crate) fn new(capacity: u32) -> Turns {
        Turns { unsaved: Queue::with_capacity(capacity), answer: None }
    }
}
