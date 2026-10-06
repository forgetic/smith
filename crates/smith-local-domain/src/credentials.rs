//! Names and validity of grants lent to the child agent (domain/host.md,
//! section 7). Secret credential bytes stay below the domain.

use skein_lib::List;
use smith_domain::Grant;

/// Grants collected for a start; each requested account settles once.
#[derive(Debug)]
pub(crate) struct Grants {
    pub(crate) values: List<Grant>,
    pub(crate) awaiting: u32,
}

impl Grants {
    pub(crate) fn new(capacity: u32) -> Grants {
        Grants { values: List::with_capacity(capacity), awaiting: 0 }
    }
}
