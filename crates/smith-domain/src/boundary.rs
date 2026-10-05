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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GrantName {
    /// Configured credential account, compared as an opaque numeric name.
    pub account: u32,
    /// Credential generation, echoed unchanged so stale refreshes cannot replace newer credentials.
    pub generation: u64,
}

/// Credential-validity notice supplied by the host; the domain keeps only its name and monotonic lifetime.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Grant {
    /// Boundary name, compared byte for byte; it carries no authority by itself.
    pub name: GrantName,
    /// Remaining monotonic validity of the granted credential.
    pub valid: Duration,
}

/// protocol -> domain
#[derive(PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed diagnostic tails keep boundary records bounded without allocation"
)]
pub enum Event {
    /// From the worker, a call: start a run on `charter`, and answer once it
    /// has ended. `worker` is the worker's name for the run, echoed on
    /// `Admitted`.
    Start {
        /// Single-use right to answer this call; exactly one terminal consumes it.
        reply_to: ReplyTo,
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        worker: Token,
        /// Host-supplied admission policy, validated before the run starts.
        charter: run::Charter,
        /// Explicit authority supplied by the opener, never inferred from role or text.
        grants: Box<[Grant]>,
    },
    /// A refreshed credential, pushed by the engine through the worker.
    Grant {
        /// Credential name or refresh notice; its secret value stays below the domain.
        grant: Grant,
    },
    /// From the worker: end the run `run` as cancelled. A run that has already
    /// answered, or decided how it ends, ignores it.
    Cancel {
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
    },
    /// Terminal for `Push`.
    Pushed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Typed terminal for the host's change-delivery request.
        push: run::Push,
    },
    /// Terminal for `Push`, after `CancelHost`: it was abandoned.
    HostCancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
    /// Terminal for `Complete`: the LLM produced its next message.
    Completed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Provider completion, validated and charged once before its tools run.
        completion: Completion,
    },
    /// Terminal for `Complete`: the call produced no message.
    Failed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Typed reason why the pending operation produced no successful value.
        failure: Failure,
    },
    /// Terminal for `Complete`, after `Cancel`: the call was abandoned.
    Cancelled {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
    /// Terminal for `Io`.
    Done {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// IO's one terminal for the named pending operation.
        done: tools::Done,
    },
    /// Terminal for `Read`.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// IO's one terminal for the run's pending text read.
        read: run::Read,
    },
    /// Terminal for `Probe`: whether an executable file is at the place. A
    /// failure or a deadline passed reads as not.
    Probed {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Whether IO found an executable within the named root before its deadline.
        executable: bool,
    },
    /// Terminal for `Check`: what the checks' process did.
    Checked {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Checks' process terminal, including its bounded diagnostic tail.
        ran: run::Ran,
    },
    /// Terminal for `Check`, after `Abort`: the checks were stopped.
    Aborted {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
}

/// domain -> protocol
#[derive(PartialEq, Eq, Debug)]
pub enum Request {
    /// To the worker: the run it names `worker` was admitted, and is `run`
    /// from now on.
    Admitted {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        worker: Token,
        /// The run child's limits or opaque run identity, according to the enclosing record.
        run: Token,
    },
    /// To the worker, the answer to a `Start`: exactly one per start.
    Answer {
        /// Single-use reply right returned to the layer that issued it.
        to: ReplyTo,
        /// Single terminal value returned to the caller.
        answer: run::Answer,
    },
    /// To the worker: checks of the run it names `worker` are running until
    /// `deadline` at the latest, so its watchdog waits that long.
    Checking {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        worker: Token,
        /// Injected monotonic deadline, never obtained from a live clock.
        deadline: Time,
    },
    /// To the worker, a host call: commit what the checkout of the run it
    /// names `worker` holds, exactly as it is, and push it, with `change`'s
    /// title and body.
    Push {
        /// Scripted host or worker's opaque run name, echoed without interpretation.
        worker: Token,
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Declared checkout change with bounded title and body, delivered as the host requested.
        change: run::outcome::Change,
    },
    /// Abandon the host call in flight for `owner`. Its terminal still comes:
    /// `HostCancelled`, or whichever outcome won the race.
    CancelHost {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
    /// Ask an LLM for the next assistant message, giving up after `timeout`.
    Complete {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Credential name or refresh notice; its secret value stays below the domain.
        grant: GrantName,
        /// First user message supplied by the opener.
        prompt: Prompt,
        /// Maximum time allowed for this completion or contained command.
        timeout: Duration,
    },
    /// The declared result failed the charter's contract.
    Rejected {
        /// Credential name or refresh notice; its secret value stays below the domain.
        grant: GrantName,
    },
    /// The provider reports its account allowance spent.
    Exhausted {
        /// Configured credential account, compared as an opaque numeric name.
        account: u32,
        /// Optional provider cooldown, retained in the representation of this boundary.
        retry_after: Duration,
    },
    /// Abandon the `Complete` in flight for `owner`. Its terminal event still
    /// comes: `Cancelled`, or whichever outcome won the race.
    Cancel {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
    /// Ask io for `op` for a session's tools, giving up at `deadline`.
    Io {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// Typed IO operation carrying only the authority and bounds required for it.
        op: tools::Op,
        /// Injected monotonic deadline, never obtained from a live clock.
        deadline: Time,
    },
    /// Abandon the `Io` in flight for `owner`. Its terminal event still comes:
    /// `Done` with `Cancelled`, or whichever outcome won the race.
    CancelIo {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
    /// Read the first `max` bytes of the regular file at `at`, following
    /// symbolic links within its root, giving up at `deadline`.
    Read {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        at: run::Place,
        /// Inclusive maximum count allowed by this contract.
        max: u32,
        /// Injected monotonic deadline, never obtained from a live clock.
        deadline: Time,
    },
    /// Find out whether an executable file is at `at`, giving up at
    /// `deadline`.
    Probe {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// IO-confined place relative to the named repository root.
        at: run::Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        deadline: Time,
    },
    /// Run the executable at `program`, in its repository's root, as a
    /// contained process, stopping it at `deadline`; keep the last `tail`
    /// bytes of what it writes.
    Check {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
        /// IO-confined check executable, run as a contained process tree.
        program: run::Place,
        /// Injected monotonic deadline, never obtained from a live clock.
        deadline: Time,
        /// Last output bytes retained under the caller's cap.
        tail: u32,
    },
    /// Stop the `Check` in flight for `owner`. Its terminal still comes:
    /// `Aborted`, or `Checked` if the checks ended first.
    Abort {
        /// Requester-issued opaque name, echoed on the one terminal for this request.
        owner: Token,
    },
}
