//! The records that cross the boundary with the run's parent, the top-level
//! domain (programming-model.md, section 4.5). The run defines them; its parent depends on it.
//!
//! All three of the run's faces cross here:
//!
//! - The worker's, which the parent routes to and from the protocol layer. A
//!   [`Event::Start`] is a call, answered by exactly one [`Request::Answer`].
//!   An admitted run is named by [`Request::Admitted`] first, so that a
//!   [`Event::Cancel`] can name it. A run's host call, [`Request::Deliver`], is
//!   ended by exactly one actual [`Event::Delivered`]. After submission the
//!   host operation is never abandoned; its deadline bounds the real terminal.
//!   Duplicate or stale callback owners are inert (domain/run.md, sections 8 and 10).
//! - io's, for what the run itself does in its checkout, which the parent
//!   routes to and from the protocol layer. A [`Request::Read`] is ended by
//!   exactly one [`Event::Read`], a [`Request::Probe`] by one
//!   [`Event::Probed`], and a [`Request::Check`] by one [`Event::Checked`],
//!   or after a [`Request::Abort`] by [`Event::Aborted`] if the abort won.
//!   Each carries its deadline, and io runs the race (programming-model.md, section 5.3).
//! - The conversations', which the parent translates to and from the session
//!   child domain's vocabulary. A [`Request::Open`] is ended by exactly one
//!   [`Event::Ended`], after a [`Event::Started`] unless the conversation was
//!   refused at its entrance. Every event about a conversation carries the
//!   run's token for it, `conversation`; the run addresses a conversation by
//!   `peer`, the token it gave back when it started (programming-model.md, section 4.2). A conversation's
//!   [`Event::Delegated`] call is ended by exactly one [`Request::Return`],
//!   after an [`Event::Withdraw`] too; the call is named by the
//!   conversation's own token for it, `call`. It carries its deadline, and
//!   the run runs the race: past it, the run stops work before submission and
//!   returns it as timed out once that settles. A submitted delivery always
//!   owes its actual host terminal, including after withdrawal or expiry;
//!   caller-only expiry does not decide the run's shutdown (domain/run.md, 8.2 and 10).
//!
//! A request's `owner` is the run's token for what asked: the run itself for
//! a read or a probe, or a delivery call for a check and its abort or host
//! submission. Submitted host delivery has no cancellation request.
//! It is echoed on the terminal.

use alloc::boxed::Box;

use crate::delivery::{CallName, Delivered, Delivery, DeliveryFailure, DeliveryRefusal};

use skein_lib::{ReplyTo, Time, Token};

use crate::budget::{Budget, Exhausted, Spend};
use crate::charter::{Charter, Checkout, Families, Llm, Tools};
use crate::outcome::{Change, Declared, Problems};

/// parent -> run
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "bounded diagnostics stay inline and are included in worst_case")]
pub enum Event {
    /// Actual terminal of one host relay; old attempts and callback generations are inert.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    HostReturned {
        /// Live attempt identity, distinct from durable `CallName`. Contract: domain/run.md, section 5.2.
        relay: crate::RelayName,
        /// Bounded actual answer or settled retry classification. Contract: domain/run.md, section 5.2.
        reply: crate::HostReply,
    },
    /// From the worker, a call: start a run on `charter`, and answer once it
    /// has ended. `worker` is the worker's name for the run, echoed on
    /// `Admitted`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Start {
        /// Single-use right to answer this call; exactly one terminal consumes it.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        reply_to: ReplyTo,
        /// Parent-supplied stable logical host-run scope, preserved across relay
        /// recovery and restarted activations. Distinct from the returned live run
        /// token and every callback slab generation; echoed without interpretation.
        /// Contract: domain/run.md, sections 3.2 and 5.2; domain/host.md, section 2.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        worker: Token,
        /// Host-supplied admission policy, validated before the run starts.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        charter: Charter,
        /// Root-owned restore binding, consumed by main once; never decoded here.
        /// Contract: domain/run.md, sections 3 and 13.
        transcript: Option<Token>,
    },

    /// Parent-labelled live message; names, including zero, are opaque and
    /// unique for the active run. Admission yields a bounce only on refusal.
    /// Contract: domain/run.md, section 6.
    Message {
        /// Admitted live run handle. Contract: domain/run.md, section 6.
        run: Token,

        /// Parent-issued active-run unique name. Contract: domain/run.md, section 6.
        name: Token,

        /// Attested UTF-8, including the sender label, bounded before retention.
        /// Contract: domain/run.md, section 6.
        text: Box<[u8]>,
    },

    /// Actual concrete session turn; the root owns the body behind record.
    /// Contract: domain/run.md, sections 6 and 13.
    Turn {
        /// Run-issued conversation binding. Contract: domain/run.md, section 13.
        conversation: Token,

        /// Single-use root handoff name. Contract: domain/run.md, section 13.
        record: Token,

        /// Historical transcript sequence, independent of activation numbering.
        /// Contract: domain/run.md, section 13.
        sequence: u32,
    },

    /// From the worker: end the run `run` as cancelled. A run that has already
    /// answered, or decided how it ends, ignores it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Cancel {
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
    },
    /// The conversation was admitted, and `peer` names it from now on.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Started {
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Conversation-issued opaque handle used after admission.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        peer: Token,
    },
    /// The LLM stopped calling tools, `text` being its last message. The
    /// conversation waits for `Say` or `Close`, and its time keeps running.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Yielded {
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        stop: Stop,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        text: Box<[u8]>,
    },
    /// The conversation's LLM completed a turn, spending `spend`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Used {
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Cumulative or incremental accepted usage, as specified by the enclosing terminal.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        spend: Spend,
    },
    /// Terminal for `Open`: the conversation ended, having spent `spend` in
    /// all, once nothing it started was in flight.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Ended {
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Terminal classification after everything started beneath this entity has settled.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        end: End,
        /// Cumulative or incremental accepted usage, as specified by the enclosing terminal.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        spend: Spend,
    },
    /// Terminal for `Read`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// IO's one terminal for the run's pending text read.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        read: Read,
    },
    /// Terminal for `Probe`: whether an executable file is at the place. A
    /// failure or a deadline passed reads as not.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Probed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// Whether IO found an executable within the named root before its deadline.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        executable: bool,
    },
    /// A call the conversation's LLM made of the run, which the run answers
    /// with one `Return`. `deadline` is the conversation's own expiry: past
    /// it, pre-submission checks abort and return `TimedOut` once settled.
    /// After submission the actual host terminal remains owed through caller
    /// expiry or withdrawal, including while the conversation closes. A caller-only
    /// stop does not decide the run's independent shutdown outcome.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Delegated {
        /// Parent-translated concrete transcript origin, fixed-size and distinct
        /// from `call`'s live callback. Zero completion is refused before effects.
        /// Contract: domain/run.md, section 8.2; domain/session.md, sections 3 and 5.
        name: CallName,
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Root-issued live call token, echoed on the one finish, delivery or sub-agent terminal.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        call: Token,
        /// Typed run tool ask, validated against the asker's authority and charter.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        ask: Ask,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        deadline: Time,
    },
    /// The conversation abandons its call `call`: it is closing. Its `Return`
    /// still comes, once what the call was doing has settled.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Withdraw {
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Root-issued live call token, echoed on the one finish, delivery or sub-agent terminal.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        call: Token,
    },
    /// Terminal for `Check`: what the checks' process did.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Checked {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// Checks' process terminal, including its bounded diagnostic tail.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        ran: Ran,
    },
    /// Terminal for `Check`, after `Abort`: the checks were stopped.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Aborted {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
    },
    /// Actual host terminal for `Deliver`, including while shutdown settles.
    /// Constructor-sealed owned evidence is revalidated against admitted writable
    /// mounts; stale callback generations are inert.
    /// Contract: domain/run.md, sections 8.2 and 10.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Delivered {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// Typed terminal for the host's change-delivery request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        push: Delivery,
    },
}

/// run -> parent
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "bounded diagnostics stay inline and are included in worst_case")]
pub enum Request {
    /// A refused live message; refusal never advances the read fence.
    /// Contract: domain/run.md, section 6.
    MessageBounced {
        /// Supplied live run name. Contract: domain/run.md, section 6.
        run: Token,

        /// Unchanged parent message name. Contract: domain/run.md, section 6.
        name: Token,

        /// Entrance refusal, before bytes are retained. Contract: domain/run.md, section 6.
        reason: MessageRefusal,
    },

    /// Main yielded after a settled wait with an empty inbox. No terminal is owed.
    /// Contract: domain/run.md, sections 6 and 10.
    Waiting {
        /// Stable parent logical run scope. Contract: domain/run.md, section 6.
        worker: Token,

        /// Latest message consumed by an actual told turn. Contract: domain/run.md, section 6.
        read: Option<Token>,
    },

    /// One settled main turn, emitted before the final answer; root moves its body.
    /// Contract: domain/run.md, sections 6 and 13.
    Turn {
        /// Stable parent logical run scope. Contract: domain/run.md, section 13.
        worker: Token,

        /// Single-use root-owned concrete body binding. Contract: domain/run.md, section 13.
        record: Token,

        /// One-based activation-local output number. Contract: domain/run.md, section 13.
        number: u32,

        /// Latest message actually consumed by this turn. Contract: domain/run.md, section 6.
        read: Option<Token>,

        /// Actual cumulative run token usage. Contract: domain/run.md, sections 9 and 13.
        spent: Spend,
    },

    /// Relay an admitted main host-tool call without interpreting its bytes. Each
    /// actual attempt gets exactly one terminal; a durable name is decided once.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    HostCall {
        /// Host logical run identity, preserved across restart. Contract: domain/host.md, section 2.
        worker: Token,
        /// Live relay attempt, never the durable operation name. Contract: domain/run.md, section 5.2.
        relay: crate::RelayName,
        /// Immutable transcript-derived name, identical on every recovery attempt.
        /// Contract: domain/run.md, section 5.2.
        name: CallName,
        /// Exact declared tool name. Contract: domain/run.md, section 5.2.
        tool: Box<[u8]>,
        /// Declared scheduling effect, identical across attempts. Contract: domain/run.md, section 5.2.
        effect: crate::HostEffect,
        /// Complete immutable protocol-attested JSON object, including whitespace.
        /// Contract: domain/run.md, sections 5.2 and 12.
        input: crate::HostInput,
        /// This relay's bounded deadline; a timeout requests withdrawal and waits for the actual terminal.
        /// Contract: domain/run.md, section 5.2.
        deadline: Time,
    },
    /// Request settlement of a host relay, retaining its actual terminal right.
    /// This is transport withdrawal, never cancellation of submitted Delivery.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    WithdrawHost {
        /// The one live attempt to withdraw, answered by `HostReturned` exactly once.
        /// Contract: domain/run.md, section 5.2.
        relay: crate::RelayName,
    },
    /// To the worker: the run it names `worker` was admitted, and is `run` to
    /// the run child domain from now on.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Admitted {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        worker: Token,
        /// Admitted run token, retained and echoed within this child's boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        run: Token,
    },
    /// To the worker, the answer to a `Start`: exactly one per start.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Answer {
        /// Single-use reply right returned to the layer that issued it.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        to: ReplyTo,
        /// Single terminal value returned to the caller.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        answer: Answer,
    },
    /// Open a conversation, which every event about it names `conversation`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Open {
        /// Run-issued opaque conversation name, echoed on every conversation event.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        conversation: Token,
        /// Opener-supplied conversation policy, checked at admission.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        opening: Opening,
    },
    /// A new user message for `peer`, a conversation that has yielded. One that
    /// has ended meanwhile drops it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Say {
        /// Conversation-issued opaque handle used after admission.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        peer: Token,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        text: Box<[u8]>,
    },
    /// Close `peer`, in any state: it stops what is in flight, then ends.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Close {
        /// Conversation-issued opaque handle used after admission.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        peer: Token,
    },
    /// Read the first `max` bytes of the regular file at `at`, following
    /// symbolic links within its root, giving up at `deadline`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        at: Place,
        /// Inclusive maximum count allowed by this contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        max: u32,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        deadline: Time,
    },
    /// Find out whether an executable file is at `at`, giving up at
    /// `deadline`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Probe {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        at: Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        deadline: Time,
    },
    /// Run the executable at `program`, in its repository's root, as a
    /// contained process, stopping it at `deadline`; keep the last `tail`
    /// bytes of what it writes.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Check {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// IO-confined check executable, run as a contained process tree.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        program: Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        deadline: Time,
        /// Last output bytes retained under the caller's cap.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        tail: u32,
    },
    /// Stop the `Check` in flight for `owner`. Its terminal still comes:
    /// `Aborted`, or `Checked` if the checks ended first.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Abort {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
    },
    /// To the worker: checks of the run it names `worker` are running until
    /// `deadline` at the latest, so its watchdog waits that long. A notice,
    /// with no terminal. It is a request, not only a fact (`CheckStarted`),
    /// because the watchdog decides on it, and nothing may depend on whether
    /// a fact is kept.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Checking {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        worker: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        deadline: Time,
    },
    /// To the host, deliver the exact checked writable-directory state under
    /// the logical run named `worker`. The host interprets the unchanged
    /// generic fields and returns its real bounded terminal. Final Change
    /// and separately granted main delivery share the same exclusive checks;
    /// the terminal remains owed through shutdown.
    ///
    /// Contract: domain/run.md, sections 7.1, 8 and 14.
    Deliver {
        /// Durable transcript-derived host call name, scoped by the logical host run.
        /// Callback `owner` is separate; retries of this operation reuse this name.
        /// Contract: domain/run.md, section 8.2; domain/host.md, section 2.
        name: CallName,
        /// Bounded actual host-operation deadline; the host supplies exactly one
        /// terminal even during shutdown. The run never abandons submission.
        /// Contract: domain/run.md, sections 8.2 and 10.
        deadline: Time,
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        worker: Token,
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        owner: Token,
        /// Declared checkout change with validated host-named fields and bounded aggregate ownership, forwarded unchanged.
        ///
        /// Contract: domain/run.md, sections 7.1, 8 and 14.
        change: Change,
    },

    /// The one terminal for the conversation's call `call`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Return {
        /// Root-issued live call token, echoed on the one finish, delivery or sub-agent terminal.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        call: Token,
        /// The one terminal value for the enclosing call; ownership passes to its receiver.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        result: Returned,
    },
}

/// What a conversation's LLM asks of the run.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Ask {
    /// Main-only exclusive request to wait after this turn settles and yields.
    /// Contract: domain/run.md, section 6.
    Wait,

    /// Invoke a host-declared tool. Main-only declaration and effect are checked
    /// before effects; provider-written JSON bytes are relayed unchanged.
    /// Contract: domain/run.md, sections 5.1, 5.2 and 12.
    Host {
        /// Exact declaration name, not interpreted as host policy. Contract: domain/run.md, section 5.2.
        tool: Box<[u8]>,
        /// Protocol-selected effect; must equal the admitted declaration.
        /// Contract: domain/run.md, section 5.2; domain/session.md, section 5.
        effect: crate::HostEffect,
        /// Complete protocol-attested bounded object input. Contract: domain/run.md, sections 5.2 and 12.
        input: crate::HostInput,
    },
    /// Main-only, separately granted mid-run delivery. Fields use that grant's
    /// required-name caps; every extra value counts toward `Limits::outcome_bytes`.
    /// It takes the same exclusive checked snapshot as finishing Change, then
    /// returns the actual terminal and continues unless shutdown was pending.
    /// Contract: domain/run.md, sections 7.1, 8.1 and 8.4.
    Deliver {
        /// Opaque host-named metadata, passed unchanged after checked aggregate
        /// ownership and required field validation. No title/body interpretation.
        /// Contract: domain/run.md, sections 7.1 and 8.4.
        change: Change,
    },
    /// Finish the run with `outcome`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Finish {
        /// Declared or accepted result, validated against the host's contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        outcome: Declared,
    },
    /// Open a sub-agent: a conversation of its own on `brief`, with families
    /// of tools no wider than the asker's, on the LLM named `llm` among the
    /// charter's (the main conversation's if none is named), and a share of
    /// the budget no larger than `share` (what the run has left, if none is
    /// asked for). Its last message is the call's result.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    SubAgent {
        /// Text supplied for the sub-agent, bounded by the receiving session's byte cap.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        brief: Box<[u8]>,
        /// Explicit sub-agent tool authority, no wider than the asker's.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        families: Families,
        /// Configured sub-agent model or main model when no override is requested.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        llm: Option<Box<[u8]>>,
        /// Optional sub-agent usage ceiling, no larger than what the run has left.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        share: Option<Spend>,
    },
}

/// The run's answer to a delegated call.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
#[expect(clippy::large_enum_variant, reason = "bounded diagnostics stay inline and are included in worst_case")]
pub enum Returned {
    /// Main's wait intent was accepted; ordinary result and continuation settle first.
    /// Contract: domain/run.md, section 6.
    Waiting,

    /// Actual host result/error text; the host's error is forwarded, never retried.
    /// Contract: domain/run.md, section 5.2.
    HostAnswered(
        /// Exact constructor-bounded host text and error bit, at most receiving reply cap.
        /// Contract: domain/run.md, section 5.2.
        crate::HostAnswer,
    ),
    /// No permissible recovery remains after the earlier relay settled. This
    /// means outcome unknown, never evidence of failure or permission to decide twice.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    HostUnknown,
    /// Host-tool declaration or owned input was refused before any relay.
    /// Contract: domain/run.md, sections 5.1, 5.2 and 12.
    HostRejected(
        /// Typed pre-relay semantic admission failure. Contract: domain/run.md, sections 5.2 and 12.
        crate::HostProblem,
    ),
    /// Actual host landing evidence. A mid-run call continues normally; a finish
    /// call ends with its admitted Change. Receipt constructors cap each copy.
    /// Contract: domain/run.md, sections 8.2 and 8.4.
    Delivered(
        /// Constructor-bounded actual host receipts, at most 64 unique writable mounts.
        /// Contract: domain/run.md, sections 8.2 and 8.4.
        Delivered,
    ),
    /// Host found no changed writable directory; correctable LLM feedback.
    /// Contract: domain/run.md, section 8.2.
    Nothing,
    /// Host named correctable feedback, bounded by `DeliveryRefusal`. Any marker
    /// ordinal is revalidated against admitted writable mounts before forwarding.
    /// Contract: domain/run.md, sections 8.1 and 8.2.
    DeliveryRefused(
        /// Named bounded explanation and optional relative marker location.
        /// Contract: domain/run.md, sections 8.1 and 8.2.
        DeliveryRefusal,
    ),
    /// A non-Change outcome is accepted: the run finishes with it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Accepted,
    /// The outcome does not fit what the run may finish with.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Rejected {
        /// Bounded validation problems explaining why the declared outcome was rejected.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        problems: Problems,
    },
    /// The checks of the repository `repository` failed, as `ran` says.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    ChecksFailed {
        /// First failed repository in workspace order, or none for a workspace-wide failure.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        repository: Box<[u8]>,
        /// Checks' process terminal, including its bounded diagnostic tail.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        ran: Ran,
    },
    /// Delivery cannot land later because the host context moved, and
    /// the run ends.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Stale,
    /// The actual host operation failed. Its fixed generic reason and sealed
    /// 512-byte diagnostic/drop count return as feedback; no landing is claimed.
    ///
    /// Contract: domain/run.md, sections 8.2 and 10.
    DeliveryFailed {
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        failure: DeliveryFailure,
    },
    /// Nothing was decided: the call was withdrawn, or the run is ending
    /// otherwise.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Cancelled,
    /// Before submission, the call's deadline passed and its sub-agent or
    /// checks settled. A submitted delivery instead owes its real host terminal.
    ///
    /// Contract: domain/run.md, sections 8.2 and 10.
    TimedOut,
    /// The run has no room for another call now; it may have later.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Busy,
    /// The sub-agent's last message: at most the run's limit of its first
    /// bytes, with the `cut` bytes past them dropped, and why it stopped.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Answered {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        text: Box<[u8]>,
        /// Bytes omitted outside the retained output window.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        cut: u64,
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        stop: Stop,
    },
    /// The sub-agent ended without an answer: refused at its entrance, its
    /// LLM failed, or its share of the budget ran out.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Unanswered {
        /// Terminal classification after everything started beneath this entity has settled.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        end: End,
    },
    /// The run refuses the requested authority or origin before an effect.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Refused {
        /// Typed admission refusal; no work runs for a refused entrance.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        refusal: AskRefusal,
    },
}

/// Why a delegated run ask is refused before an effect.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AskRefusal {
    /// Parent supplied a zero transcript completion. No check or host effect
    /// begins; a valid session origin is one-based and bounded before dispatch.
    /// Contract: domain/run.md, section 8.2; domain/session.md, sections 3 and 5.
    Name,
    /// The asker lacks this mid-run delivery or sub-agent grant, or asked for families of tools
    /// it does not have itself.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    NotGranted,
    /// The sub-agent would be nested deeper than a run's limit.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TooDeep,
    /// The run has as many conversations as it may have at once.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TooMany,
    /// The charter has no LLM of that name for sub-agents.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    UnknownLlm,
    /// The share asked for, or what the run has left, leaves no room for a
    /// turn.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Unworkable,
    /// The run has spent past its budget.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Over,
}

/// What a check's process did: how it ended, and the tail of what it wrote,
/// with the `cut` bytes before the tail dropped.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Ran {
    /// Terminal process classification; only a zero exit code passes checks.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub exit: Exit,
    /// Owned output bytes retained within the enclosing output cap.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub output: Box<[u8]>,
    /// Bytes omitted outside the retained output window.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub cut: u64,
}

/// How a check's process ended.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Exit {
    /// It exited with `code`; zero passes.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Code {
        /// Typed or byte-valued terminal classification in the enclosing boundary.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        code: u8,
    },
    /// A signal killed it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Signalled,
    /// Its deadline passed, and io stopped it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TimedOut,
    /// It could not be started.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Unstarted,
}

/// Where a file is, for the run's own io: a repository's root, as io names
/// it, and the path beneath it, names joined by `/`. io resolves it beneath
/// the root.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Place {
    /// IO-issued repository root token, stored and echoed without interpreting it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub root: Token,
    /// Path relative to the named root, resolved and confined by the receiving IO layer.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub path: Box<[u8]>,
}

/// What a `Read` found. A file is read as text: io checks that it is UTF-8,
/// so the domain never parses, and cuts it at a character boundary, so what
/// the run passes on to an LLM is text too.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Read {
    /// The file's first characters, in at most as many bytes as asked for,
    /// cut where a character ends; `whole` when they are all of it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        text: Box<[u8]>,
        /// Whether the retained text includes the entire file.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        whole: bool,
    },
    /// Nothing is there, or not a regular file.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Missing,
    /// The file is not UTF-8 text.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    NotText,
    /// io failed, or the deadline passed first.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Failed,
}

/// What a conversation is opened with.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Opening {
    /// Root-owned restore binding for main, None for every child.
    /// Contract: domain/run.md, sections 3 and 13.
    pub transcript: Option<Token>,

    /// Main-only wait descriptor; children cannot acquire this authority.
    /// Contract: domain/run.md, section 6.
    pub wait: bool,

    /// Main's bounded host declarations, copied whole; every child receives none.
    /// Contract: domain/run.md, sections 5.1–5.3 and 12.
    pub host_tools: Box<[crate::HostTool]>,
    /// Whether the parent offers main the separately granted delivery tool.
    /// It is false for every child; this descriptor grants no final outcome form.
    /// Contract: domain/run.md, section 8.4.
    pub deliver: bool,
    /// The LLM it talks to.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub llm: Llm,
    /// System instructions supplied by the opener, retained within the enclosing byte cap.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub system: Box<[u8]>,
    /// The first user message.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub prompt: Box<[u8]>,
    /// The tools the LLM may run on the checkout.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub tools: Tools,
    /// The checkout they act on, and which of it may be written.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub checkout: Checkout,
    /// Its share of the run's budget: what the run has left when it opens, and
    /// the time to the run's deadline. The conversation keeps to it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub budget: Budget,
    /// Whether its LLM may call `finish`, which the conversation runs as a
    /// write: alone, never beside another call. Only main may.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub finish: bool,
    /// The families it has: its tools, and whether its LLM may ask for
    /// sub-agents, a call whose effect follows the families asked for (a
    /// sub-agent that may modify or run commands is a write).
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    pub families: Families,
}

/// Why a conversation's LLM stopped calling tools.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stop {
    /// It finished its turn.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    EndTurn,
    /// It ran out of tokens mid-answer.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    MaxTokens,
    /// It declined to answer.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Refusal,
    /// It asked for tools and named none.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    NoCalls,
}

/// How a conversation ended.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum End {
    /// Exact V2 history refusal before provider or tool effects.
    /// Contract: domain/run.md, section 13.
    TranscriptRefused {
        /// Small lossless sibling-independent admission reason.
        /// Contract: domain/run.md, section 13.
        reason: TranscriptRefusal,
    },

    /// The run closed it.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Closed,
    /// Refused at its entrance: no room for another conversation.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Busy,
    /// Refused at its entrance: the opening does not fit the conversations'
    /// limits.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Invalid,
    /// Its LLM could not go on.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Fault(Fault),
    /// Its share of the budget ran out.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Budget(Exhausted),
}

/// Exact neutral completion failure after retries or nonretryable refusal.
/// Diagnostic bytes were consumed by session policy; this record remains content-free.
/// Contract: domain/run.md, sections 2, 5 and 10.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionFailure {
    /// Shared-client receiving allowance exceeded.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Limit,

    /// Shared protocol response contract violated.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Protocol,

    /// Unsolicited actual lower cancellation.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Cancelled,

    /// Provider capacity refused the request.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Overloaded,

    /// Provider could not be reached or failed.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Unavailable,

    /// The actual completion deadline elapsed.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    TimedOut,

    /// Provider context allowance was exceeded.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    ContextTooLong,

    /// Provider rejected the request shape.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Invalid,

    /// Provider rejected the credential.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Unauthorized,

    /// Provider rate allowance requires the retained cooldown.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    RateLimited {
        /// Exact lower cooldown, without scheduling or recovery policy.
        /// Contract: domain/run.md, sections 2, 5 and 10.
        retry_after: skein_lib::Duration,
    },

    /// Provider account allowance requires the retained cooldown.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Exhausted {
        /// Exact lower cooldown, without scheduling or recovery policy.
        /// Contract: domain/run.md, sections 2, 5 and 10.
        retry_after: skein_lib::Duration,
    },
}

/// Actual transport evidence retained alongside a neutral completion failure.
/// No domain infers this from an error label or from requested cancellation.
/// Contract: domain/run.md, sections 2, 5 and 10.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CompletionEvidence {
    /// The lower proves no request bytes were sent.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Unsent,

    /// The request may have reached the peer; outcome is unknown.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Unknown,

    /// An actual peer response, including a refusal, was received.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Response,
}

/// What kept an LLM from going on.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fault {
    /// Full neutral actual failure and evidence after session retry policy.
    /// This is distinct from local model stopping rules and requested run Cancel.
    /// Contract: domain/run.md, sections 2, 5 and 10.
    Completion {
        /// Exact content-free shared-client classification and cooldown.
        /// Contract: domain/run.md, sections 2, 5 and 10.
        failure: CompletionFailure,

        /// Exact actual transport evidence, including across interrupted Delivery.
        /// Contract: domain/run.md, sections 2, 5 and 10.
        evidence: CompletionEvidence,
    },

    /// The account spent its provider allowance. The engine retries after cooldown.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Exhausted,
    /// Its provider failed for good: unreachable, overloaded past the
    /// retries, or refusing the call or its credentials.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Provider,
    /// The conversation outgrew the model's context, or the bytes a
    /// conversation may hold.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    ContextFull,
    /// It kept declining to answer.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Refused,
    /// It kept running out of tokens mid-answer.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Truncated,
    /// It kept asking for tools and naming none.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Malformed,
}

/// The answer to a `Start`.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub enum Answer {
    /// Settled main wait reached its idle threshold; all real rights have closed.
    /// Contract: domain/run.md, sections 6, 10 and 13.
    Parked {
        /// Actual cumulative run token usage. Contract: domain/run.md, section 9.
        spent: Spend,
        /// Number of actual main turns emitted in this activation.
        /// Contract: domain/run.md, section 13.
        turns: u32,
    },
    /// A mid-run delivery actually landed after shutdown was already decided.
    /// This is host evidence, not an LLM-declared Accepted result: even a
    /// Report-only charter preserves the real operation without inventing a Report.
    /// Contract: domain/run.md, sections 8.2, 8.4 and 10; domain/host.md, section 2.
    Delivered {
        /// Stable transcript origin of the landed host operation, scoped by the logical run.
        /// Contract: domain/run.md, section 8.2.
        name: CallName,
        /// Real bounded per-directory host receipts; this retained copy is priced.
        /// Contract: domain/run.md, sections 8.2 and 11.
        receipts: Delivered,
        /// Typed stop already decided when the mid-run operation landed.
        /// It does not erase the actual delivery evidence.
        /// Contract: domain/run.md, sections 8.4 and 10.
        stopped: Failure,
        /// Accepted cumulative usage, including late completions while settling.
        /// Contract: domain/run.md, sections 9 and 10.
        spent: Spend,
        /// Actual main turns emitted in this activation before this terminal.
        /// Contract: domain/run.md, section 13.
        turns: u32,
    },
    /// Refused at the entrance: nothing was done.
    ///
    /// A run is refused at its own entrance, before `Admitted`; or after
    /// `Admitted`, when its main conversation is refused at the
    /// conversations' entrance. The run then did nothing but look in its
    /// checkout: `Busy` says the agent had no room for the conversation, and
    /// a later retry may find some; `Invalid(Conversation)` that its opening
    /// does not fit the conversations' limits, which a retry will not change.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Refused(Refusal),
    /// The run finished with `outcome`, having spent `spent`. A change has
    /// been checked and delivered by the host.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Accepted {
        /// Declared or accepted result, validated against the host's contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        outcome: Declared,
        /// Accepted cumulative usage across the enclosing run or session.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        spent: Spend,
        /// Actual main turns emitted in this activation before this terminal.
        /// Contract: domain/run.md, section 13.
        turns: u32,
    },
    /// The run ended without an outcome, having spent `spent`.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Failed {
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        failure: Failure,
        /// Accepted cumulative usage across the enclosing run or session.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        spent: Spend,
        /// Actual main turns emitted in this activation before this terminal.
        /// Contract: domain/run.md, section 13.
        turns: u32,
    },
}

/// Why a run was refused.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// No room for another run, or for its conversation.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Busy,
    /// The charter does not fit the limits.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Invalid(Invalid),
}

/// What about a charter does not fit the limits.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Invalid {
    /// It holds more bytes than a run may.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    TooLarge,
    /// The checkout lists more repositories than a run may hold, or one name
    /// twice.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Checkout,
    /// The host declarations exceed count/storage bounds, repeat/reserve a name,
    /// have empty opaque fields or a zero deadline, or the receiving retry limits are invalid.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Grants,
    /// The result contract permits no form, exceeds the verdict count cap,
    /// declares invalid or duplicate names/caps/ranges, or its smallest
    /// accepted value for an allowed form cannot fit aggregate ownership.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Outcome,
    /// The budget asks for more than the limits allow, or for no turns, input,
    /// output or time.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Budget,
    /// The LLM's `max_tokens`, or a sub-agent LLM's, is zero or beyond the
    /// limits, or there are more sub-agent LLMs than a run may hold, or two of
    /// one model.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Llm,
    /// The main conversation was refused: its opening does not fit the
    /// conversations' limits.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Conversation,
}

/// Why a run ended without an outcome: what the worker acts on.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Failure {
    /// Exact transient history refusal; never silently starts fresh.
    /// Contract: domain/run.md, section 13.
    Transcript(
        /// Session admission classification, translated exhaustively by root.
        /// Contract: domain/run.md, section 13.
        TranscriptRefusal,
    ),

    /// The LLM could not do the work.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Model(Fault),
    /// The run's budget ran out.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Budget(Exhausted),
    /// The LLM did not keep to the run's rules.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Policy(Policy),
    /// The worker cancelled the run.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Cancelled,
    /// The host's delivery context moved: no later delivery from this run can
    /// land. The host decides what context a new run receives.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Stale,
}

/// The run's rules, as the LLM broke them.
///
/// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Policy {
    /// It kept stopping without finishing, through `nudges` nudges, having
    /// called `finish` with `rejected` outcomes that were refused.
    ///
    /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
    Unfinished {
        /// Number of run nudges already given after unfinished turns.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        nudges: u32,
        /// Number of declared outcomes rejected by the charter contract.
        ///
        /// Copy baseline: domain/run.md, sections 3, 7, 9, 10 and 14.
        rejected: u32,
    },
}

/// Live-message entrance refusals; no accepted message or read state changes.
/// Contract: domain/run.md, section 6.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MessageRefusal {
    /// FIFO capacity is exhausted. Contract: domain/run.md, section 6.
    Busy,

    /// Attested bytes exceed the configured cap. Contract: domain/run.md, section 6.
    TooLarge,

    /// Run has stopped or the live name is stale. Contract: domain/run.md, section 6.
    Inactive,

    /// Name equals a queued, offered or current-read name. Older names rely on
    /// the parent's active-run uniqueness promise. Contract: domain/run.md, section 6.
    ReusedName,
}

/// Exact concrete-history entrance classification, independent of the sibling type.
/// Contract: domain/run.md, section 13.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TranscriptRefusal {
    /// Unsupported record version. Contract: domain/run.md, section 13.
    Version,

    /// Configured endpoint differs. Contract: domain/run.md, section 13.
    Endpoint,

    /// Configured replay dialect differs. Contract: domain/run.md, section 13.
    Dialect,

    /// Invalid concrete record structure. Contract: domain/run.md, section 13.
    Malformed,

    /// History retains a live ticket. Contract: domain/run.md, section 13.
    Unresolved,

    /// Receiving ownership/count cap is incompatible. Contract: domain/run.md, section 13.
    TooLarge,
}
