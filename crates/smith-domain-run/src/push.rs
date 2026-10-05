//! Bounded feedback for a change that did not land.
//!
//! Contract: domain/run.md, section 14; programming-model.md, sections 4.4 and 6.3.

/// The last bytes of a failed git invocation's diagnostic output. The fixed
/// protocol cap bounds every terminal, landing and reply without allocation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PushDiagnostic {
    output: [u8; 512],
    length: u16,
    cut: u64,
}

impl PushDiagnostic {
    /// The maximum diagnostic tail carried across the protocol boundary.
    pub const CAPACITY: usize = 512;

    /// Keep the latest diagnostic bytes, counting bytes already dropped by io.
    #[must_use]
    pub fn new(output: &[u8], cut: u64) -> Self {
        let length = output.len().min(Self::CAPACITY);
        let dropped = output.len().checked_sub(length).expect("the tail is within the output");
        let mut tail = [0; Self::CAPACITY];
        for (target, source) in tail.iter_mut().zip(output.get(dropped..).expect("the tail is within the output")) {
            *target = *source;
        }
        Self {
            output: tail,
            length: u16::try_from(length).expect("the fixed tail fits in a u16"),
            cut: cut.saturating_add(u64::try_from(dropped).expect("a byte length fits in u64")),
        }
    }

    /// An invocation for which io has no diagnostic output.
    #[must_use]
    pub const fn empty() -> Self {
        Self { output: [0; 512], length: 0, cut: 0 }
    }

    /// The retained diagnostic tail.
    #[must_use]
    pub fn output(&self) -> &[u8] {
        self.output.get(..usize::from(self.length)).expect("the constructor seals the tail length")
    }

    /// Bytes preceding the tail, dropped by io or by this value's constructor.
    #[must_use]
    pub const fn cut(&self) -> u64 {
        self.cut
    }
}

/// Why a declared change did not land.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PushReason {
    /// The host could not find the requested repository.
    MissingRepository,
    /// The host could not find the requested branch.
    MissingBranch,
    /// The host could not find the requested commit.
    MissingCommit,
    /// The entrance or operation was refused with the enclosing typed reason.
    Refused,
    /// The host could not reach its remote.
    Unreachable,
    /// The lower layer classified a broken operation.
    Broken,
    /// The injected operation deadline won the race.
    TimedOut,
    /// The caller cancelled and the terminal settled.
    Cancelled,
    /// The provider could not be reached or returned an unusable terminal.
    Unavailable,
    /// No capacity is currently available; a later call may fit.
    Busy,
    /// A configured ownership, count or encoded-byte cap would be exceeded.
    TooLarge,
    /// No writable repository contained a change.
    Nothing,
    /// A boundary supplied no more specific reason.
    Unknown,
}

/// Feedback from a push: the first failed repository in workspace order, its
/// typed reason and bounded git output. Branch movement takes precedence.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PushFailure {
    /// First failed repository in workspace order, or none for a workspace-wide failure.
    pub repository: Option<u32>,
    /// Typed terminal reason supplied by the lower layer.
    pub reason: PushReason,
    /// Fixed diagnostic tail, at most 512 bytes with the dropped-byte count retained.
    pub diagnostic: PushDiagnostic,
}

impl PushFailure {
    /// A failure not associated with a repository or diagnostic output.
    #[must_use]
    pub const fn new(reason: PushReason) -> Self {
        Self { repository: None, reason, diagnostic: PushDiagnostic::empty() }
    }
}
