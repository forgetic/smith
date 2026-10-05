//! The records that cross the boundary with the protocol layer (programming-model.md, section 4.4). The
//! domain defines them; the protocol crate depends on it.
//!
//! Three peers are behind it, and each record names which by its variant:
//!
//! - The worker, the run child domain's face. A [`Event::Start`] is a call,
//!   answered by exactly one [`Request::Answer`], after a
//!   [`Request::Admitted`] that names the run if it was admitted, so that a
//!   [`Event::Cancel`] can name it. A run's host call, [`Request::Push`], is
//!   ended by exactly one [`Event::Pushed`], or after a
//!   [`Request::CancelHost`] by [`Event::HostCancelled`] if the cancel won.
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
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GrantName {
    /// Configured credential account, compared as an opaque numeric name.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub account: u32,
    /// Credential generation, echoed unchanged so stale refreshes cannot replace newer credentials.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub generation: u64,
}

/// Credential-validity notice supplied by the host; the domain keeps only its name and monotonic lifetime.
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Grant {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub name: GrantName,
    /// Remaining monotonic validity of the granted credential.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    pub valid: Duration,
}

/// Typed host, provider and IO inputs routed to the owned children. Start yields
/// one host answer; each lower terminal echoes the owner of its pending request.
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Event {
    /// From the worker, a call: start a run on `charter`, and answer once it
    /// has ended. `worker` is the worker's name for the run, echoed on
    /// `Admitted`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Start {
        /// Single-use right to answer this call; exactly one terminal consumes it.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        reply_to: ReplyTo,
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Host-supplied admission policy, validated before the run starts.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        charter: run::Charter,
        /// Host credential-name and remaining-validity notices, bounded by `Limits.accounts`.
        /// These select a usable credential generation; they grant no checkout or tool authority.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grants: Box<[Grant]>,
    },
    /// A refreshed credential, pushed by the engine through the worker.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Grant {
        /// Credential name or refresh notice; its secret value stays below the domain.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grant: Grant,
    },
    /// From the worker: end the run `run` as cancelled. A run that has already
    /// answered, or decided how it ends, ignores it.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Cancel {
        /// Root-issued admitted run token, supplied in `Request::Admitted` and echoed by cancellation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        run: Token,
    },
    /// Terminal for `Push`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Pushed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Typed terminal for the host's change-delivery request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        push: run::Push,
    },
    /// Terminal for `Push`, after `CancelHost`: it was abandoned.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    HostCancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Terminal for `Complete`: the LLM produced its next message.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Completed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Provider completion, validated and charged once before its tools run.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        completion: Completion,
    },
    /// Terminal for `Complete`: the call produced no message.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Failed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Typed reason why the pending operation produced no successful value.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        failure: Failure,
    },
    /// Terminal for `Complete`, after `Cancel`: the call was abandoned.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Cancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Terminal for `Io`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Done {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        done: tools::Done,
    },
    /// Terminal for `Read`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO's one terminal for the run's pending text read.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        read: run::Read,
    },
    /// Terminal for `Probe`: whether an executable file is at the place. A
    /// failure or a deadline passed reads as not.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Probed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Whether IO found an executable within the named root before its deadline.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        executable: bool,
    },
    /// Terminal for `Check`: what the checks' process did.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Checked {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Checks' process terminal, including its bounded diagnostic tail.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        ran: run::Ran,
    },
    /// Terminal for `Check`, after `Abort`: the checks were stopped.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Aborted {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
}

/// Owned host notices and calls, provider calls and IO operations emitted by
/// the root. Calls require the typed terminal documented on their variant.
/// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// To the worker: the run it names `worker` was admitted, and is `run`
    /// from now on.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Admitted {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Root-issued admitted run token, supplied in `Request::Admitted` and echoed by cancellation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        run: Token,
    },
    /// To the worker, the answer to a `Start`: exactly one per start.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Answer {
        /// Single-use reply right returned to the layer that issued it.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        to: ReplyTo,
        /// Single terminal value returned to the caller.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        answer: run::Answer,
    },
    /// To the worker: checks of the run it names `worker` are running until
    /// `deadline` at the latest, so its watchdog waits that long.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Checking {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// To the worker, a host call: commit what the checkout of the run it
    /// names `worker` holds, exactly as it is, and push it, with `change`'s
    /// title and body.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Push {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        worker: Token,
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Declared checkout change with bounded title and body, delivered as the host requested.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        change: run::outcome::Change,
    },
    /// Abandon the host call in flight for `owner`. Its terminal still comes:
    /// `HostCancelled`, or whichever outcome won the race.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    CancelHost {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Ask an LLM for the next assistant message, giving up after `timeout`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Complete {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Credential name or refresh notice; its secret value stays below the domain.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grant: GrantName,
        /// Complete provider-neutral conversation snapshot from the root, bounded by session ownership limits.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        prompt: Prompt,
        /// Maximum time allowed for this completion or contained command.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        timeout: Duration,
    },
    /// Provider authentication rejected this exact credential generation; a notice
    /// to the host, with no terminal owed. Repeated rejection of that generation is suppressed.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Rejected {
        /// Credential name or refresh notice; its secret value stays below the domain.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        grant: GrantName,
    },
    /// The provider reports its account allowance spent.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Exhausted {
        /// Configured credential account, compared as an opaque numeric name.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        account: u32,
        /// Optional provider cooldown, retained in the representation of this boundary.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        retry_after: Duration,
    },
    /// Abandon the `Complete` in flight for `owner`. Its terminal event still
    /// comes: `Cancelled`, or whichever outcome won the race.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Cancel {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Ask io for `op` for a session's tools, giving up at `deadline`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Io {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// Typed IO operation carrying only the authority and bounds required for it.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        op: tools::Op,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// Abandon the `Io` in flight for `owner`. Its terminal event still comes:
    /// `Done` with `Cancelled`, or whichever outcome won the race.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    CancelIo {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
    /// Read the first `max` bytes of the regular file at `at`, following
    /// symbolic links within its root, giving up at `deadline`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        at: run::Place,
        /// Requested retained UTF-8 file-byte cap; the IO terminal reports whether the whole file fits.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        max: u32,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// Find out whether an executable file is at `at`, giving up at
    /// `deadline`.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Probe {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        at: run::Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
    },
    /// Run the executable at `program`, in its repository's root, as a
    /// contained process, stopping it at `deadline`; keep the last `tail`
    /// bytes of what it writes.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Check {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
        /// IO-confined check executable, run as a contained process tree.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        program: run::Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        deadline: Time,
        /// Last output bytes retained under the caller's cap.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        tail: u32,
    },
    /// Stop the `Check` in flight for `owner`. Its terminal still comes:
    /// `Aborted`, or `Checked` if the checks ended first.
    /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
    Abort {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        /// Copy baseline: domain/run.md, sections 2, 3, 10 and 14; domain/host.md, sections 2 and 7.
        owner: Token,
    },
}
