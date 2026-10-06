//! Content-free observations of the local host (domain/host.md, section 9).

/// A bounded observation that never decides host behavior.
#[derive(Clone, Copy, Debug)]
pub enum Fact {}
