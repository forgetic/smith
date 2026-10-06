//! One exclusive checked snapshot for final and mid-run delivery (domain/run.md,
//! sections 8 and 10). State keeps opaque fields, durable name, caller deadline
//! and whether this is a finishing Change. It never knows receipt encoding,
//! provider ids, callback identity or host delivery policy. Main's write batch
//! waits throughout checks and submission; children have already returned.
//! Before submission withdrawal aborts checks. Afterwards no cancellation is
//! emitted: the actual bounded host terminal settles even during shutdown.

use crate::boundary::{Exit, Place, Ran, Request, Returned};
use crate::call::{self, Call, Withdrawal};
use crate::conventions;
use crate::delivery::{CallName, Delivery, DeliveryFailure, DeliveryReason};
use crate::limits::Limits;
use crate::outcome::Change;
use crate::run::Run;
use crate::workspace::{self, Directory};
use core::mem;
use skein_lib::bytes::copy_of;
use skein_lib::{Env, Id, Queue, Time, Token};

#[derive(Debug)]
pub(crate) struct Landing {
    change: Change,
    name: CallName,
    deadline: Time,
    finish: bool,
    stage: Stage,
}

#[derive(Debug)]
enum Stage {
    Checking { check: u32 },
    Aborting { why: Withdrawal },
    Delivering,
    Closed,
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum Settled {
    Going,
    Finished(Change),
    MidDelivered,
    Refused,
    Stale,
    Cancelled,
}

pub(crate) fn landing(change: Change, name: CallName, deadline: Time, finish: bool) -> Landing {
    Landing { change, name, deadline, finish, stage: Stage::Closed }
}

pub(crate) fn begin(landing: &mut Landing, id: Id<Call>, run: &Run, env: &Env<Limits>, out: &mut Queue<Request>) {
    landing.stage = next(landing, id, run, 0, env, out);
}

#[expect(clippy::too_many_arguments, reason = "a cell handler takes the fields it touches")]
pub(crate) fn checked(
    landing: &mut Landing,
    id: Id<Call>,
    owner: Token,
    run: &Run,
    may_finish: bool,
    ran: Ran,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
) -> Settled {
    let stage = mem::replace(&mut landing.stage, Stage::Closed);
    match stage {
        Stage::Checking { check } => {
            let passed = match ran.exit {
                Exit::Code { code: 0 } => true,
                Exit::Code { .. } | Exit::Signalled | Exit::TimedOut | Exit::Unstarted => false,
            };
            let required = match &run.charter.outcome.change {
                Some(spec) => spec.checks_must_pass,
                None => true,
            };
            if !passed && required {
                let repository = copy_of(&repository(run, check).name);
                back(owner, Returned::ChecksFailed { repository, ran }, Settled::Refused, out)
            } else if env.now >= landing.deadline {
                back(owner, Returned::TimedOut, Settled::Cancelled, out)
            } else if !may_finish {
                back(owner, Returned::Cancelled, Settled::Cancelled, out)
            } else {
                landing.stage = next(landing, id, run, check.checked_add(1).expect("bounded mounted checks"), env, out);
                Settled::Going
            }
        }
        Stage::Aborting { why } => back(owner, call::stopped(why), Settled::Cancelled, out),
        Stage::Delivering | Stage::Closed => unreachable!("checks end only while they run"),
    }
}

pub(crate) fn aborted(landing: &mut Landing, owner: Token, out: &mut Queue<Request>) -> Settled {
    let stage = mem::replace(&mut landing.stage, Stage::Closed);
    match stage {
        Stage::Aborting { why } => back(owner, call::stopped(why), Settled::Cancelled, out),
        Stage::Checking { .. } | Stage::Delivering | Stage::Closed => unreachable!("terminal follows an abort"),
    }
}

pub(crate) fn delivered(
    landing: &mut Landing,
    owner: Token,
    delivery: Delivery,
    run: &Run,
    out: &mut Queue<Request>,
) -> Settled {
    let stage = mem::replace(&mut landing.stage, Stage::Closed);
    match stage {
        Stage::Delivering => {}
        Stage::Checking { .. } | Stage::Aborting { .. } | Stage::Closed => unreachable!("terminal follows submission"),
    }
    match delivery {
        Delivery::Delivered(receipts) => {
            for receipt in receipts.receipts() {
                if !writable(run, receipt.directory()) {
                    return malformed(owner, out);
                }
            }
            let settled =
                if landing.finish { Settled::Finished(landing.change.clone()) } else { Settled::MidDelivered };
            back(owner, Returned::Delivered(receipts), settled, out)
        }
        Delivery::Nothing => back(owner, Returned::Nothing, Settled::Refused, out),
        Delivery::Refused(refusal) => {
            if let Some(marker) = refusal.marker()
                && !writable(run, marker.directory())
            {
                return malformed(owner, out);
            }
            back(owner, Returned::DeliveryRefused(refusal), Settled::Refused, out)
        }
        Delivery::Failed(failure) => back(owner, Returned::DeliveryFailed { failure }, Settled::Refused, out),
        Delivery::Stale => back(owner, Returned::Stale, Settled::Stale, out),
    }
}

fn malformed(owner: Token, out: &mut Queue<Request>) -> Settled {
    back(
        owner,
        Returned::DeliveryFailed { failure: DeliveryFailure::new(DeliveryReason::Broken) },
        Settled::Refused,
        out,
    )
}

fn writable(run: &Run, directory: u32) -> bool {
    let position = usize::try_from(directory).expect("bounded directory ordinal fits");
    match workspace::directories(run.workspace.as_ref()).get(position) {
        Some(repository) => repository.writable,
        None => false,
    }
}

pub(crate) fn withdraw(landing: &mut Landing, id: Id<Call>, why: Withdrawal, out: &mut Queue<Request>) {
    let stage = mem::replace(&mut landing.stage, Stage::Closed);
    landing.stage = match stage {
        Stage::Checking { .. } => {
            out.push(Request::Abort { owner: id.token() });
            Stage::Aborting { why }
        }
        // Submission owes its actual terminal. Caller-only withdrawal or expiry
        // grants no authority to decide the run's independent shutdown outcome.
        Stage::Delivering => Stage::Delivering,
        Stage::Aborting { why: first } => {
            assert!(why == Withdrawal::Withdrawn, "withdraw cancels the call alarm");
            Stage::Aborting { why: first }
        }
        Stage::Closed => unreachable!("only a live call is withdrawn"),
    };
}

fn next(landing: &Landing, id: Id<Call>, run: &Run, check: u32, env: &Env<Limits>, out: &mut Queue<Request>) -> Stage {
    let Some(&index) = run.found.checks.get(check) else {
        let deadline = landing.deadline.min(env.now.saturating_add(env.limits.delivery_timeout));
        assert!(env.now < deadline, "admission and completed checks refuse expired submission");
        out.push(Request::Deliver {
            host_run: run.host_name,
            owner: id.token(),
            name: landing.name,
            deadline,
            change: landing.change.clone(),
        });
        return Stage::Delivering;
    };
    let root = repository_at(run, index).root;
    let deadline = landing.deadline.min(env.now.saturating_add(env.limits.check_timeout));
    let program = Place { root, path: copy_of(conventions::checks(&run.charter)) };
    out.push(Request::Check { owner: id.token(), program, deadline, tail: env.limits.check_tail });
    out.push(Request::Checking { host_run: run.host_name, deadline });
    Stage::Checking { check }
}

fn back(owner: Token, result: Returned, settled: Settled, out: &mut Queue<Request>) -> Settled {
    out.push(Request::Return { spent: 0, call: owner, result });
    settled
}

fn repository(run: &Run, check: u32) -> &Directory {
    let index = run.found.checks.get(check).expect("only discovered checks run");
    repository_at(run, *index)
}

fn repository_at(run: &Run, index: u32) -> &Directory {
    workspace::directories(run.workspace.as_ref())
        .get(usize::try_from(index).expect("bounded mount"))
        .expect("checks belong to admitted mounts")
}
