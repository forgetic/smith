//! Sealed host delivery terminals and durable names (domain/run.md, section 8).
//! This module keeps no runtime state. Constructors bound owned receipts and
//! marker feedback before they cross a domain boundary; the receiver additionally
//! validates directory ordinals against its admitted writable mounts. It never
//! knows host policy, receipt encoding, provider ids, callback slabs or git.

use alloc::boxed::Box;
use core::mem::size_of;

/// Transcript-derived host operation name, supplied by the composing root.
/// The host scopes it by the same logical run and a distinct activation; it is not a live
/// callback token. Zero completion is refused before checks or host effects.
///
/// Contract: domain/run.md, section 8.2; domain/session.md, sections 3 and 5.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CallName {
    /// Host-supplied activation number, unique for each start of a logical run.
    /// The host never reuses it when a run restarts.
    /// Contract: domain/run.md, sections 3.2 and 8.2; domain/host.md, section 2.
    pub activation: u64,

    /// One-based accepted completion sequence, including restored V2 history.
    /// Exhaustion refuses effects before submission; V1 names only this activation.
    ///
    /// Contract: domain/run.md, section 8.2; domain/session.md, sections 3 and 5.
    pub completion: u32,

    /// Zero-based assistant block ordinal, bounded by assistant content byte storage and
    /// checked block-count representability before any completion effects.
    /// Repeated provider call ids in distinct positions or turns are permitted.
    ///
    /// Contract: domain/run.md, section 8.2; domain/session.md, sections 3 and 5.
    pub position: u32,
}

/// Maximum directories in a delivery-capable charter. Non-delivering Report-only
/// charters retain their configured mount cap. Every changed writable mount can
/// therefore have a receipt in one bounded host terminal.
///
/// Contract: domain/run.md, sections 8.1 and 8.2.
pub const MAX_DIRECTORIES: u32 = 64;

/// Host-produced opaque receipt for one changed mounted directory. Private
/// storage prevents unbounded terminals; the run never interprets its bytes.
///
/// Contract: domain/run.md, section 8.2.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Receipt {
    directory: u32,
    text: Box<[u8]>,
}

impl Receipt {
    /// Maximum opaque bytes owned by one receipt.
    ///
    /// Contract: domain/run.md, section 8.2.
    pub const CAPACITY: usize = 512;

    /// Seal one host receipt, refusing a directory outside the protocol cap,
    /// empty evidence or text beyond 512 bytes. Mounted write authority is checked
    /// again by the run on receipt of the terminal.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub fn new(directory: u32, text: Box<[u8]>) -> Option<Self> {
        if directory >= MAX_DIRECTORIES || text.is_empty() || text.len() > Self::CAPACITY {
            return None;
        }
        Some(Self { directory, text })
    }

    /// Host-order mount ordinal, checked against the admitted writable mounts.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub const fn directory(&self) -> u32 {
        self.directory
    }

    /// Uninterpreted host evidence, between one and 512 bytes.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub fn text(&self) -> &[u8] {
        &self.text
    }
}

/// Complete host evidence for changed directories in one landed operation.
/// Constructors ensure a nonempty, unique, at most 64-entry terminal. The host
/// supplies the changed-directory set; the run validates mounted write authority.
///
/// Contract: domain/run.md, section 8.2.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Delivered {
    receipts: Box<[Receipt]>,
}

impl Delivered {
    /// Seal a host terminal; duplicate ordinals, empty evidence and more than
    /// 64 changed directories are refused without becoming successful delivery.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub fn new(receipts: Box<[Receipt]>) -> Option<Self> {
        if receipts.is_empty() || receipts.len() > usize::try_from(MAX_DIRECTORIES).expect("fixed cap fits") {
            return None;
        }
        for (position, receipt) in receipts.iter().enumerate() {
            for previous in receipts.get(..position).expect("position in the slice") {
                if previous.directory == receipt.directory {
                    return None;
                }
            }
        }
        Some(Self { receipts })
    }

    /// Each changed directory's bounded opaque host receipt, in supplied order.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub fn receipts(&self) -> &[Receipt] {
        &self.receipts
    }

    /// Count owned receipt container storage and all opaque payload bytes.
    /// The sealed cap makes this count infallible; callers price every retained copy.
    ///
    /// Contract: domain/run.md, sections 8.2 and 11; programming-model.md, section 6.3.
    #[must_use]
    pub fn owned_bytes(&self) -> u64 {
        let mut bytes = u64::try_from(self.receipts.len())
            .expect("sealed count")
            .checked_mul(u64::try_from(size_of::<Receipt>()).expect("inline receipt fits"))
            .expect("sealed storage");
        for receipt in &self.receipts {
            bytes = bytes.checked_add(u64::try_from(receipt.text.len()).expect("sealed bytes")).expect("sealed sum");
        }
        bytes
    }

    /// Maximum owned receipt storage of any constructor-accepted terminal.
    ///
    /// Contract: domain/run.md, sections 8.2 and 11; programming-model.md, section 6.3.
    #[must_use]
    pub fn worst_case() -> u64 {
        u64::from(MAX_DIRECTORIES)
            .checked_mul(
                u64::try_from(size_of::<Receipt>().checked_add(Receipt::CAPACITY).expect("fixed receipt sum"))
                    .expect("fixed receipt size"),
            )
            .expect("fixed terminal size")
    }
}

/// Named file retaining conflict markers, supplied by the host as correctable
/// feedback. The run preserves the relative bytes and never parses file contents.
///
/// Contract: domain/run.md, sections 8.1 and 8.2.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Marker {
    directory: u32,
    path: Box<[u8]>,
}

impl Marker {
    /// Maximum bytes in a host's relative marker path.
    ///
    /// Contract: domain/run.md, sections 8.1 and 8.2.
    pub const CAPACITY: usize = 4096;

    /// Seal a nonempty relative path and bounded directory ordinal. Absolute,
    /// empty-component, parent-component and zero-byte paths are refused.
    /// The receiving run also checks that the mount exists and is writable.
    ///
    /// Contract: domain/run.md, sections 8.1 and 8.2.
    #[must_use]
    pub fn new(directory: u32, path: Box<[u8]>) -> Option<Self> {
        if directory >= MAX_DIRECTORIES || path.is_empty() || path.len() > Self::CAPACITY {
            return None;
        }
        if !relative_path(&path) {
            return None;
        }
        Some(Self { directory, path })
    }

    /// Host-order mount ordinal, revalidated against admitted writable mounts.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub const fn directory(&self) -> u32 {
        self.directory
    }

    /// Relative marker path, between one and 4096 bytes, preserved unchanged.
    ///
    /// Contract: domain/run.md, sections 8.1 and 8.2.
    #[must_use]
    pub fn path(&self) -> &[u8] {
        &self.path
    }
}

/// Correctable host refusal with a named opaque explanation and optional marker
/// location. Other host policy refusals do not invent a filesystem marker.
///
/// Contract: domain/run.md, sections 8.1 and 8.2.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeliveryRefusal {
    marker: Option<Marker>,
    explanation: Box<[u8]>,
}

impl DeliveryRefusal {
    /// Maximum bytes in the host's named refusal explanation.
    /// Contract: domain/run.md, section 8.2.
    pub const CAPACITY: usize = 512;

    /// Seal nonempty opaque named feedback up to 512 bytes, optionally locating
    /// a marker. The run revalidates any mount ordinal before forwarding it.
    /// Contract: domain/run.md, sections 8.1 and 8.2.
    #[must_use]
    pub fn new(marker: Option<Marker>, explanation: Box<[u8]>) -> Option<Self> {
        if explanation.is_empty() || explanation.len() > Self::CAPACITY {
            return None;
        }
        Some(Self { marker, explanation })
    }

    /// Optional bounded relative marker location, never inferred from the reason.
    /// Contract: domain/run.md, sections 8.1 and 8.2.
    #[must_use]
    pub fn marker(&self) -> Option<&Marker> {
        self.marker.as_ref()
    }

    /// Host-supplied named explanation, opaque to the run, at most 512 bytes.
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub fn explanation(&self) -> &[u8] {
        &self.explanation
    }

    /// Owned marker and explanation bytes; all retained copies use this count.
    /// Contract: domain/run.md, sections 8.2 and 11; programming-model.md, section 6.3.
    #[must_use]
    pub fn owned_bytes(&self) -> u64 {
        let path = match &self.marker {
            Some(marker) => marker.path.len(),
            None => 0,
        };
        u64::try_from(path.checked_add(self.explanation.len()).expect("sealed feedback")).expect("bounded feedback")
    }
}

/// Content-free classification of an actual host terminal, for bounded facts.
/// It carries no receipt, path, diagnostic or host policy text.
/// Contract: domain/run.md, sections 8.2 and 12.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DeliveryStatus {
    /// Actual landing. Contract: domain/run.md, section 8.2.
    Delivered,

    /// No changed directory. Contract: domain/run.md, section 8.2.
    Nothing,

    /// Correctable host refusal. Contract: domain/run.md, section 8.2.
    Refused,

    /// Actual host failure classification. Contract: domain/run.md, section 8.2.
    Failed(
        /// Fixed host reason, with no diagnostic or policy bytes. Contract: domain/run.md, sections 8.2 and 12.
        DeliveryReason,
    ),

    /// Host context moved. Contract: domain/run.md, section 8.2.
    Stale,
}

/// The last bytes of a failed host operation's diagnostic output. The fixed
/// protocol cap bounds every terminal, landing and reply without allocation.
///
/// Contract: domain/run.md, section 8.2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Diagnostic {
    output: [u8; 512],
    length: u16,
    cut: u64,
}

impl Diagnostic {
    /// The maximum diagnostic tail carried across the protocol boundary.
    ///
    /// Contract: domain/run.md, section 8.2.
    pub const CAPACITY: usize = 512;

    /// Keep the latest diagnostic bytes, counting bytes already dropped by io.
    ///
    /// Contract: domain/run.md, section 8.2.
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
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub const fn empty() -> Self {
        Self { output: [0; 512], length: 0, cut: 0 }
    }

    /// The retained diagnostic tail.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub fn output(&self) -> &[u8] {
        self.output.get(..usize::from(self.length)).expect("the constructor seals the tail length")
    }

    /// Bytes preceding the tail, dropped by io or this constructor; the count saturates at `u64::MAX`.
    ///
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub const fn cut(&self) -> u64 {
        self.cut
    }
}

/// Host-classified generic failure; these names carry no delivery-policy meaning.
/// Fixed discriminants and a sealed diagnostic bound every failed terminal.
///
/// Contract: domain/run.md, section 8.2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DeliveryReason {
    /// Host cannot reach the target. Contract: domain/run.md, section 8.2.
    Unreachable,

    /// Target refused the operation. Contract: domain/run.md, section 8.2.
    RefusedByTarget,

    /// Host deadline ended the actual operation. Contract: domain/run.md, section 8.2.
    TimedOut,

    /// Operation or malformed host terminal is broken. Contract: domain/run.md, section 8.2.
    Broken,

    /// Host's count or byte allowance is exceeded. Contract: domain/run.md, section 8.2.
    TooLarge,

    /// Required target data is missing. Contract: domain/run.md, section 8.2.
    Missing,

    /// Target temporarily has no capacity. Contract: domain/run.md, section 8.2.
    Busy,

    /// Target is currently unavailable. Contract: domain/run.md, section 8.2.
    Unavailable,

    /// Host operation itself ended cancelled; the run never abandons it. Contract: domain/run.md, section 8.2.
    Cancelled,

    /// Host has no more specific classification. Contract: domain/run.md, section 8.2.
    Unknown,
}

/// Host-produced failed delivery with a generic reason and fixed diagnostic.
/// The run forwards it as feedback and continues unless shutdown already started.
///
/// Contract: domain/run.md, section 8.2.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DeliveryFailure {
    /// Generic host classification, never a forge or policy label.
    /// Contract: domain/run.md, section 8.2.
    pub reason: DeliveryReason,

    /// Last at most 512 bytes plus a saturating count of preceding dropped bytes.
    /// Contract: domain/run.md, section 8.2.
    pub diagnostic: Diagnostic,
}

impl DeliveryFailure {
    /// Construct a reason-only host failure without diagnostic ownership.
    /// Contract: domain/run.md, section 8.2.
    #[must_use]
    pub const fn new(reason: DeliveryReason) -> Self {
        Self { reason, diagnostic: Diagnostic::empty() }
    }
}

/// Exactly one actual host terminal for an already submitted delivery. Sealed
/// owned values cap it before domain admission; mounted-directory validity is
/// still checked by the run. A stale callback is inert, never another delivery.
///
/// Contract: domain/run.md, sections 8.2 and 10.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[expect(clippy::large_enum_variant, reason = "fixed diagnostics are included in boundary and slab bounds")]
pub enum Delivery {
    /// Landed evidence for changed directories. Contract: domain/run.md, section 8.2.
    Delivered(
        /// Sealed per-directory opaque receipts, revalidated against writable mounts. Contract: domain/run.md, section 8.2.
        Delivered,
    ),

    /// No mounted writable directory changed. Contract: domain/run.md, section 8.2.
    Nothing,

    /// Named correctable feedback, optionally locating a marker file. Contract: domain/run.md, section 8.2.
    Refused(
        /// Sealed named refusal with optional marker detail. Contract: domain/run.md, section 8.2.
        DeliveryRefusal,
    ),

    /// Actual failed operation with bounded feedback. Contract: domain/run.md, section 8.2.
    Failed(
        /// Generic reason and fixed 512-byte diagnostic tail/drop count. Contract: domain/run.md, section 8.2.
        DeliveryFailure,
    ),

    /// Host context moved; this run cannot land later. Contract: domain/run.md, section 8.2.
    Stale,
}

impl Delivery {
    /// Project the actual terminal to content-free observation; this neither
    /// validates mounted authority nor decides lifecycle behavior.
    /// Contract: domain/run.md, sections 8.2 and 12.
    #[must_use]
    pub const fn status(&self) -> DeliveryStatus {
        match self {
            Self::Delivered(_) => DeliveryStatus::Delivered,
            Self::Nothing => DeliveryStatus::Nothing,
            Self::Refused(_) => DeliveryStatus::Refused,
            Self::Failed(failure) => DeliveryStatus::Failed(failure.reason),
            Self::Stale => DeliveryStatus::Stale,
        }
    }
}

/// Shared relative-path spelling check; confinement belongs to lower IO.
/// Contract: domain/run.md, sections 8.1, 8.2 and 12.
pub(crate) fn relative_path(path: &[u8]) -> bool {
    if path.is_empty() {
        return false;
    }
    let mut start = 0;
    for (position, byte) in path.iter().enumerate() {
        if *byte == 0 {
            return false;
        }
        if *byte == b'/' {
            let part = path.get(start..position).expect("ordered path positions");
            if part.is_empty() || part == b".." || part == b"." {
                return false;
            }
            start = position.checked_add(1).expect("position within the bounded path");
        }
    }
    let part = path.get(start..).expect("position within path");
    if part.is_empty() || part == b".." || part == b"." {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{Delivered, DeliveryRefusal, Diagnostic, MAX_DIRECTORIES, Marker, Receipt};
    use alloc::boxed::Box;
    use skein_lib::List;

    fn bytes(count: u32) -> Box<[u8]> {
        let mut bytes = List::with_capacity(count);
        for _ in 0..count {
            bytes.push(b'a').expect("fixed byte cap");
        }
        bytes.into_boxed()
    }

    #[test]
    fn every_sealed_ownership_cap_can_be_filled_exactly() {
        let mut receipts = List::with_capacity(MAX_DIRECTORIES);
        for directory in 0..MAX_DIRECTORIES {
            receipts
                .push(Receipt::new(directory, bytes(512)).expect("exact receipt cap"))
                .expect("fixed directory cap");
        }
        let delivered = Delivered::new(receipts.into_boxed()).expect("unique full terminal");
        assert_eq!(delivered.owned_bytes(), Delivered::worst_case());
        let marker = Marker::new(63, bytes(4096)).expect("exact relative path cap");
        let refusal = DeliveryRefusal::new(Some(marker), bytes(512)).expect("exact explanation cap");
        assert_eq!(refusal.owned_bytes(), 4608);
        let tail = Diagnostic::new(&bytes(513), 7);
        assert_eq!(tail.output().len(), 512);
        assert_eq!(tail.cut(), 8);
        assert_eq!(Diagnostic::new(&bytes(513), u64::MAX).cut(), u64::MAX);
    }

    #[test]
    fn constructors_refuse_malformed_and_oversized_success_evidence() {
        assert!(Receipt::new(64, bytes(1)).is_none());
        assert!(Receipt::new(0, bytes(0)).is_none());
        assert!(Receipt::new(0, bytes(513)).is_none());
        assert!(Delivered::new(Box::new([])).is_none());
        let receipt = Receipt::new(0, bytes(1)).expect("bounded receipt");
        assert!(Delivered::new(Box::new([receipt.clone(), receipt])).is_none());
        for path in [b"/abs".as_slice(), b"a/../b", b"a//b", b"a/", b".", b"zero\0byte"] {
            assert!(Marker::new(0, path.into()).is_none());
        }
        assert!(Marker::new(0, bytes(4097)).is_none());
        assert!(DeliveryRefusal::new(None, bytes(0)).is_none());
        assert!(DeliveryRefusal::new(None, bytes(513)).is_none());
    }
}
