//! The records that cross the boundary with the protocol layer (programming-model.md, section 4.4). The
//! domain defines them; the protocol crate depends on it.
//!
//! Three peers are behind it, and each record names which by its variant:
//!
//! - The worker, the run child domain's face. A [`Event::Start`] is a call,
//!   answered by exactly one [`Request::Answer`], after a
//!   [`Request::Admitted`] that names the run if it was admitted, so that a
//!   [`Event::Cancel`] can name it. A run's host call, [`Request::Deliver`], is
//!   ended by exactly one actual [`Event::Delivered`]. After submission the
//!   host operation is never abandoned; its deadline bounds the real terminal.
//!   Duplicate or stale callback owners are inert (domain/run.md, sections 8 and 10).
//!   [`Request::Checking`] is a notice, with no terminal.
//! - LLM providers, for the sessions: a [`Request::Complete`] is ended by one
//!   of [`Event::Completed`], [`Event::Failed`] or, after a
//!   [`Request::Cancel`] that won its race, [`Event::Cancelled`]. Its prompt
//!   and its completion are in the conversation vocabulary ([`crate::llm`]):
//!   a prompt's messages hold text, tool calls sent back as the LLM wrote
//!   them, and their results (the tools' outcome, the run's answer to a tool
//!   it serves, the problem of a call that is none, or not run); a
//!   completion holds text and tool calls, each decoded into a call to the
//!   session's own tools, an ask of a tool the run serves, or a problem.
//! - io, in two families of records for now: the file and process operations
//!   of the sessions' tools, as the tools define them (a [`Request::Io`] is
//!   ended by [`Event::Done`], with `Cancelled` if a [`Request::CancelIo`]
//!   won its race), and the run's own looks in its checkout and
//!   its checks (a [`Request::Read`] by [`Event::Read`], a [`Request::Probe`]
//!   by [`Event::Probed`], a [`Request::Check`] by [`Event::Checked`], or
//!   after a [`Request::Abort`] by [`Event::Aborted`] if the abort won). They
//!   share a shape (a command run as a contained process, in a root, with a
//!   deadline, is a check for the run and a shell call for the tools), and
//!   become one vocabulary once the io layer is designed. Every io request
//!   carries its deadline, and io runs the race (programming-model.md, section 5.3).
//!
//! A request's `owner` is the token of whoever asked, echoed on its terminal
//! event. Tokens of different families may be equal: the variant routes a
//! terminal to the child domain that asked.

use skein_lib::{Duration, ReplyTo, Time, Token};
use smith_domain_run as run;
use smith_domain_tools as tools;

use crate::llm::{Completion, Failure, Prompt};
use alloc::boxed::Box;

/// A credential name, never its value.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GrantName {
    /// Configured credential account, compared as an opaque numeric name.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub account: u32,
    /// Credential generation, echoed unchanged so stale refreshes cannot replace newer credentials.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub generation: u64,
}

/// Credential-validity notice supplied by the host; the domain keeps only its name and monotonic lifetime.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Grant {
    /// Host-supplied numeric credential account and generation; no secret credential bytes.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub name: GrantName,
    /// Remaining monotonic validity of the granted credential.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub valid: Duration,
}

/// Typed host, provider and IO inputs routed to the owned children. Start yields
/// one host answer; each lower terminal echoes the owner of its pending request.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Debug)]
pub enum Event {
    /// Actual terminal for one host relay attempt; old callbacks are inert.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    HostReturned {
        /// Live attempt identity, distinct from durable operation scope. Contract: domain/run.md, section 5.2.
        relay: run::RelayName,
        /// Bounded host text or actual settled retry classification. Contract: domain/run.md, section 5.2.
        reply: run::HostReply,
    },
    /// From the worker, a call: start a run on `charter`, and answer once it
    /// has ended. `worker` is the worker's name for the run, echoed on
    /// `Admitted`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Start {
        /// Single-use right to answer this call; exactly one terminal consumes it.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        reply_to: ReplyTo,
        /// Parent-supplied stable logical host-run scope, preserved across relay
        /// recovery and restarted activations; distinct from live run/callback tokens.
        /// Contract: domain/run.md, sections 3.2 and 5.2; domain/host.md, section 2.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Host-supplied admission policy, validated before the run starts.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        charter: run::Charter,
        /// Optional immutable host mounts and initial conflicts, moved to run admission.
        /// Contract: domain/run.md, sections 3.2, 8.3 and 14.
        workspace: Option<run::Workspace>,
        /// Typed V2 history including committed post-transcript actual results.
        /// False charter.resume ignores it; semantic refusal never starts fresh.
        /// Receiving record/count caps are checked before root retention.
        /// Contract: domain/run.md, sections 3 and 13; domain/session.md, section 3.
        transcript: Option<smith_domain_session::record::Transcript>,
        /// Host credential-name and remaining-validity notices, bounded by `Limits.accounts`.
        /// These select a usable credential generation; they grant no checkout or tool authority.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grants: Box<[Grant]>,
    },

    /// Parent-labelled live message, FIFO and bounded before retention by
    /// `Limits.run.messages` and `Limits.run.message_bytes`. Accepted messages
    /// emit no separate admission terminal; refusal emits `MessageBounced`.
    /// Contract: domain/run.md, section 6.
    Message {
        /// Admitted live run handle. Contract: domain/run.md, section 6.
        run: Token,

        /// Active-run unique opaque name, including zero. Contract: domain/run.md, section 6.
        name: Token,

        /// Attested UTF-8 including sender label, at most `Limits.run.message_bytes`.
        /// Contract: domain/run.md, section 6.
        text: Box<[u8]>,
    },

    /// A refreshed credential, pushed by the engine through the worker.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Grant {
        /// Credential name or refresh notice; its secret value stays below the domain.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grant: Grant,
    },
    /// From the worker: end the run `run` as cancelled. A run that has already
    /// answered, or decided how it ends, ignores it.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Cancel {
        /// Root-issued admitted run token, supplied in `Request::Admitted` and echoed by cancellation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        run: Token,
    },
    /// Actual host terminal for `Deliver`, including while shutdown settles.
    /// Constructor-sealed owned evidence is revalidated against admitted writable
    /// mounts; stale callback generations are inert.
    /// Contract: domain/run.md, sections 8.2 and 10.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Delivered {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Typed terminal for the host's change-delivery request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        push: run::Delivery,
    },

    /// Terminal for `Complete`: the LLM produced its next message.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Completed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Provider completion, validated and charged once before its tools run.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        completion: Completion,
    },
    /// Terminal for `Complete`: the call produced no message.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Failed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        failure: Failure,

        /// Exact content-free transport evidence carried by the actual terminal.
        /// Contract: domain/run.md, sections 4, 5 and 12.
        evidence: crate::llm::Evidence,

        /// Bounded exact shared-client diagnostic, consumed and dropped by policy.
        /// It never controls text-based retry decisions or enters saved history.
        /// Contract: domain/run.md, sections 4, 5 and 12.
        detail: Box<[u8]>,
    },
    /// Terminal for `Complete`, after `Cancel`: the call was abandoned.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Cancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Terminal for `Io`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Done {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        done: tools::Done,
    },
    /// Terminal for `Read`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO's one terminal for the run's pending text read.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        read: run::Read,
    },
    /// Terminal for `Probe`: whether an executable file is at the place. A
    /// failure or a deadline passed reads as not.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Probed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Whether IO found an executable within the named root before its deadline.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        executable: bool,
    },
    /// Terminal for `Check`: what the checks' process did.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Checked {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Checks' process terminal, including its bounded diagnostic tail.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        ran: run::Ran,
    },
    /// Terminal for `Check`, after `Abort`: the checks were stopped.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Aborted {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
}

/// Owned host notices and calls, provider calls and IO operations emitted by
/// the root. Calls require the typed terminal documented on their variant.
///
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// Refused live message; existing FIFO and read state remain unchanged.
    /// Contract: domain/run.md, section 6.
    MessageBounced {
        /// Supplied live run name. Contract: domain/run.md, section 6.
        run: Token,

        /// Unchanged parent name. Contract: domain/run.md, section 6.
        name: Token,

        /// Refusal before retention. Contract: domain/run.md, section 6.
        reason: run::MessageRefusal,
    },

    /// Settled main wait and yield with empty inbox; wall time keeps running.
    /// This observation to the parent owes no terminal; Start's reply right
    /// remains pending until the run's actual final Answer.
    /// Contract: domain/run.md, sections 6 and 10.
    Waiting {
        /// Stable parent run scope. Contract: domain/run.md, section 6.
        worker: Token,

        /// Latest name consumed by an actual turn. Contract: domain/run.md, section 6.
        read: Option<Token>,
    },

    /// Actual main turn; caller owns durable payload and host ACK metadata has
    /// its separate existing owner. Emitted before the final Answer.
    /// Contract: domain/run.md, section 13; domain/host.md, section 6.
    Turn {
        /// Stable parent run scope. Contract: domain/run.md, section 13.
        worker: Token,

        /// One-based output number within this activation. Contract: domain/run.md, section 13.
        number: u32,

        /// Latest message consumed by this actual turn. Contract: domain/run.md, section 6.
        read: Option<Token>,

        /// Cumulative actual token usage, independently enforced from record prices.
        /// Contract: domain/run.md, sections 9 and 13.
        spent: run::Spend,

        /// Full concrete record including replay and actual terminal results.
        /// Contract: domain/session.md, section 3.
        turn: smith_domain_session::record::Turn,
    },

    /// Opaque declared main host tool, forwarded without interpreting its input.
    /// One actual terminal is owed per attempt; durable decisions replay by name.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    HostCall {
        /// Stable logical run scope supplied by Start. Contract: domain/host.md, section 2.
        worker: Token,
        /// One live relay attempt. Contract: domain/run.md, section 5.2.
        relay: run::RelayName,
        /// Immutable accepted-transcript operation name. Contract: domain/run.md, section 5.2.
        name: run::CallName,
        /// Exact declared tool name. Contract: domain/run.md, section 5.2.
        tool: Box<[u8]>,
        /// Checked declaration effect, used for session scheduling. Contract: domain/session.md, section 5.
        effect: run::HostEffect,
        /// Complete bounded attested object bytes, unchanged. Contract: domain/run.md, sections 5.2 and 12.
        input: run::HostInput,
        /// Per-relay bounded deadline; withdrawal still owes its actual terminal.
        /// Contract: domain/run.md, section 5.2.
        deadline: Time,
    },
    /// Ask the host to settle its live relay, retaining the original terminal.
    /// Submitted delivery has no such cancellation operation.
    /// Contract: domain/run.md, section 5.2; domain/host.md, section 2.
    WithdrawHost {
        /// The live relay attempt, echoed by its actual `HostReturned` terminal.
        /// Contract: domain/run.md, section 5.2.
        relay: run::RelayName,
    },
    /// To the worker: the run it names `worker` was admitted, and is `run`
    /// from now on.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Admitted {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Root-issued admitted run token, supplied in `Request::Admitted` and echoed by cancellation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        run: Token,
    },
    /// To the worker, the answer to a `Start`: exactly one per start.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Answer {
        /// Single-use reply right returned to the layer that issued it.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        to: ReplyTo,
        /// Single terminal value returned to the caller.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        answer: run::Answer,
    },
    /// To the worker: checks of the run it names `worker` are running until
    /// `deadline` at the latest, so its watchdog waits that long.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Checking {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// To the host, deliver the exact checked writable-directory state under
    /// the logical run named `worker`. The host interprets the unchanged
    /// generic fields and returns its real bounded terminal. Final Change
    /// and separately granted main delivery share the same exclusive checks;
    /// the terminal remains owed through shutdown.
    ///
    /// Contract: domain/run.md, sections 7.1, 8 and 14; domain/host.md, section 2.
    Deliver {
        /// Durable transcript-derived host call name, scoped by the logical host run.
        /// Callback `owner` is separate; retries of this operation reuse this name.
        /// Contract: domain/run.md, section 8.2; domain/host.md, section 2.
        name: run::CallName,
        /// Bounded actual host-operation deadline; the host supplies exactly one
        /// terminal even during shutdown. The run never abandons submission.
        /// Contract: domain/run.md, sections 8.2 and 10.
        deadline: Time,
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Declared checkout change with validated host-named fields and bounded aggregate ownership, forwarded unchanged.
        ///
        /// Contract: domain/run.md, sections 7.1, 8 and 14; domain/host.md, section 2.
        change: run::outcome::Change,
    },

    /// Ask an LLM for the next assistant message, giving up after `timeout`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Complete {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Credential name or refresh notice; its secret value stays below the domain.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grant: GrantName,
        /// Complete provider-neutral conversation snapshot from the root, bounded by session ownership limits.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        prompt: Prompt,
        /// Maximum time allowed for this completion or contained command.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        timeout: Duration,

        /// Maximum owned translated completion bytes, including block cells,
        /// replay envelopes and decoded calls. The adapter verifies its configured
        /// bound before preparing the provider request; actual terminals obey it.
        /// Contract: domain/run.md, sections 3 and 14.
        max_completion_bytes: u64,

        /// Maximum translated completion blocks, reserved with result skeletons
        /// before this request. One actual terminal remains owed after Cancel.
        /// Contract: domain/run.md, sections 3 and 14.
        max_completion_blocks: u32,

        /// Maximum exact shared-client failure diagnostic bytes. The adapter
        /// verifies compatibility before prepare; policy consumes the actual
        /// terminal and drops detail without retaining text in facts/history.
        /// Contract: domain/run.md, sections 4, 5 and 12.
        max_failure_bytes: u32,

        /// Aggregate decoded application-call ownership permitted in this
        /// completion, included by the adapter in its full translated byte bound.
        /// Contract: domain/run.md, sections 3 and 14.
        decoded_call_bytes: u64,
    },
    /// Provider authentication rejected this exact credential generation; a notice
    /// to the host, with no terminal owed. Repeated rejection of that generation is suppressed.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Rejected {
        /// Credential name or refresh notice; its secret value stays below the domain.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grant: GrantName,
    },
    /// The provider reports its account allowance spent.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Exhausted {
        /// Configured credential account, compared as an opaque numeric name.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        account: u32,
        /// Optional provider cooldown, retained in the representation of this boundary.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        retry_after: Duration,
    },
    /// Abandon the `Complete` in flight for `owner`. Its terminal event still
    /// comes: `Cancelled`, or whichever outcome won the race.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Cancel {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Ask io for `op` for a session's tools, giving up at `deadline`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Io {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Typed IO operation carrying only the authority and bounds required for it.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        op: tools::Op,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// Abandon the `Io` in flight for `owner`. Its terminal event still comes:
    /// `Done` with `Cancelled`, or whichever outcome won the race.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    CancelIo {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Read the first `max` bytes of the regular file at `at`, following
    /// symbolic links within its root, giving up at `deadline`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        at: run::Place,
        /// Requested retained UTF-8 file-byte cap; the IO terminal reports whether the whole file fits.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        max: u32,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// Find out whether an executable file is at `at`, giving up at
    /// `deadline`.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Probe {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        at: run::Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// Run the executable at `program`, in its repository's root, as a
    /// contained process, stopping it at `deadline`; keep the last `tail`
    /// bytes of what it writes.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Check {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO-confined check executable, run as a contained process tree.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        program: run::Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
        /// Last output bytes retained under the caller's cap.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        tail: u32,
    },
    /// Stop the `Check` in flight for `owner`. Its terminal still comes:
    /// `Aborted`, or `Checked` if the checks ended first.
    ///
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Abort {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        ///
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
}
