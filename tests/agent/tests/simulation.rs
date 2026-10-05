//! Agent obligations preserved from temper `25ac2ad` on an explicit scripted
//! host (domain/run.md, sections 13 and 14; domain/host.md, section 2).
//! The host snapshots landed bytes, not forge commits or engine decisions.

use skein_lib::Duration;
use skein_world::domain::assert_replays;
use smith_agent_world::{BUDGET, HostReply, Job, Settings, World};
use smith_domain::{
    Fact,
    run::{self, Answer, Delivery, Exhausted, Failure, outcome::Declared},
};

const ITERATIONS: u32 = 20_000;

const FIXED: &[u8] = b"pub fn answer() -> u32 { 43 }\n";

fn settled(settings: &Settings) -> World {
    let mut world = World::new(*settings);
    world.run(ITERATIONS);
    assert_eq!(world.judged().1, 1, "the referee met the host's one answer obligation");
    world
}

fn count(world: &World, predicate: impl Fn(&run::facts::Fact) -> bool) -> usize {
    world
        .facts()
        .iter()
        .filter(|fact| match fact {
            Fact::Run { fact } => predicate(fact),
            Fact::Session { .. } => false,
        })
        .count()
}

#[test]
fn a_coding_run_fails_checks_then_fixes_and_lands_the_exact_tree() {
    let world = settled(&Settings::calm(1));
    assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Change(_), .. }));
    assert_eq!(world.checked(), [false, true]);
    assert_eq!(world.pushes(), [smith_agent_world::delivered()]);
    assert_eq!(world.code(), FIXED);
    assert_eq!(world.landed(), FIXED);
    assert_eq!(
        count(&world, |fact| matches!(fact, run::facts::Fact::Called { ask: run::facts::Asked::Finish, .. })),
        2
    );
    assert_eq!(world.lost(), 0);
}

#[test]
fn a_review_retries_the_verdict_its_charter_rejected() {
    let world = settled(&Settings { job: Job::Review, writable: false, ..Settings::calm(2) });
    let Answer::Accepted { outcome: Declared::Verdict(verdict), .. } = world.answer() else {
        panic!("a review answers its verdict")
    };
    assert_eq!(&*verdict.name, b"request-changes");
    assert_eq!(verdict.items.len(), 1);
    assert!(world.checked().is_empty() && world.pushes().is_empty());
    assert_eq!(
        count(&world, |fact| matches!(fact, run::facts::Fact::Returned { result: run::facts::Return::Rejected, .. })),
        1
    );
}

#[test]
fn a_writable_review_still_lands_only_its_verdict() {
    let world = settled(&Settings { job: Job::Review, ..Settings::calm(2) });
    assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Verdict(_), .. }));
    assert!(world.landed().is_empty() && world.checked().is_empty() && world.pushes().is_empty());
}

#[test]
fn a_report_answers_the_host_without_checks_or_pushes() {
    let world = settled(&Settings { job: Job::Reporting, writable: false, ..Settings::calm(10) });
    let Answer::Accepted { outcome: Declared::Report(report), .. } = world.answer() else {
        panic!("a real report answers its typed host contract")
    };
    assert_eq!(&*report.text, b"The answer is 42, and the checks want 43.");
    assert_eq!(&*report.fields[0].name, b"source");
    assert_eq!(&*report.fields[0].value, b"README.md");
    assert!(world.pushes().is_empty() && world.checked().is_empty());
}

#[test]
fn sub_agents_nest_and_return_results_to_their_askers() {
    let world = settled(&Settings { job: Job::Delegating, ..Settings::calm(3) });
    assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Change(_), .. }));
    assert_eq!(world.checked(), [true]);
    assert_eq!(world.landed(), FIXED);
    assert_eq!(
        count(&world, |fact| matches!(fact, run::facts::Fact::Called { ask: run::facts::Asked::SubAgent, .. })),
        4
    );
    assert_eq!(
        count(&world, |fact| matches!(fact, run::facts::Fact::Returned { result: run::facts::Return::Answered, .. })),
        4
    );
    assert_eq!(count(&world, |fact| matches!(fact, run::facts::Fact::Opened { .. })), 5);
    assert_eq!(count(&world, |fact| matches!(fact, run::facts::Fact::Opened { depth: 2, .. })), 1);
    assert_eq!(
        count(&world, |fact| matches!(fact, run::facts::Fact::Called { ask: run::facts::Asked::Finish, .. })),
        1,
        "the fixer's finish was never offered and never reached the run"
    );
}

#[test]
fn the_shared_budget_ends_the_run_after_every_conversation_settles() {
    let world =
        settled(&Settings { job: Job::Spending, budget: run::Budget { turns: 8, ..BUDGET }, ..Settings::calm(4) });
    let Answer::Failed { failure: Failure::Budget(Exhausted::Turns), spent } = world.answer() else {
        panic!("the run spends its shared turns")
    };
    assert!(spent.turns > 8 && spent.turns <= 12, "only in-flight completions can finish past the shared ceiling");
    assert_eq!(count(&world, |fact| matches!(fact, run::facts::Fact::Opened { .. })), 3);
    assert!(world.landed().is_empty());
}

#[test]
fn a_push_on_a_moved_branch_answers_stale_and_lands_nothing() {
    let world = settled(&Settings { push: HostReply::Stale, ..Settings::calm(5) });
    assert!(matches!(world.answer(), Answer::Failed { failure: Failure::Stale, .. }));
    assert_eq!(world.checked(), [false, true]);
    assert_eq!(world.pushes(), [Delivery::Stale]);
    assert!(world.landed().is_empty());
}

#[test]
fn refused_push_feedback_reaches_the_llm_and_is_retried() {
    let failure = run::DeliveryFailure {
        reason: run::DeliveryReason::RefusedByTarget,
        diagnostic: run::Diagnostic::new(b"remote: push refused", 0),
    };
    let world = settled(&Settings { push: HostReply::Failed(failure), ..Settings::calm(9) });
    assert!(matches!(world.answer(), Answer::Failed { failure: Failure::Policy(_), .. }));
    assert_eq!(world.checked(), [false, true, true]);
    assert_eq!(world.pushes(), [Delivery::Failed(failure), Delivery::Failed(failure)]);
    let feedback =
        world.prompts().iter().flat_map(|prompt| &prompt.messages).flat_map(|message| &message.parts).any(|part| {
            matches!(part, smith_fake_llm_domain::api::Part::ToolOutput { output, is_error: true, .. }
            if &**output == b"remote: push refused")
        });
    assert!(feedback, "the host's exact bounded diagnostic returned to the provider");
    assert!(world.landed().is_empty());
}

#[test]
fn host_cancellation_at_many_moments_closes_the_whole_tree() {
    let mut cancelled = 0;
    let mut nested = 0;
    let mut done = 0;
    for seed in 0..40 {
        let settings = Settings {
            job: Job::Delegating,
            cancel_at: Some(Duration::from_millis(seed * 500)),
            races: 500,
            ..Settings::calm(600 + seed)
        };
        let world = settled(&settings);
        match world.answer() {
            Answer::Failed { failure: Failure::Cancelled, .. } => {
                cancelled += 1;
                nested +=
                    usize::from(count(&world, |fact| matches!(fact, run::facts::Fact::Opened { depth: 1, .. })) > 0);
            }
            Answer::Accepted { .. } => done += 1,
            answer @ (Answer::Refused(_) | Answer::Failed { .. } | Answer::Delivered { .. }) => {
                panic!("seed {seed}: expected cancelled or already finished, got {answer:?}")
            }
        }
    }
    assert!(
        cancelled > 0 && nested > 0 && done > 0,
        "cancels crossed early, nested and already-finished runs: {cancelled}/{nested}/{done}"
    );
}

#[test]
fn a_deadline_mid_tree_answers_only_after_the_tree_settles() {
    let mut nested = 0;
    for seed in 0..10 {
        let world = settled(&Settings {
            job: Job::Delegating,
            budget: run::Budget { time: Duration::from_secs(4), ..BUDGET },
            ..Settings::calm(700 + seed)
        });
        assert!(matches!(world.answer(), Answer::Failed { failure: Failure::Budget(Exhausted::Time), .. }));
        nested += usize::from(count(&world, |fact| matches!(fact, run::facts::Fact::Opened { depth: 1, .. })) > 0);
        assert!(world.answered_at() <= skein_lib::Time::ZERO.saturating_add(Duration::from_secs(5)));
    }
    assert!(nested > 0, "some deadlines crossed nested work");
}

#[test]
fn checks_past_their_deadline_fail_and_are_not_pushed() {
    let world = settled(&Settings { check: skein_world::domain::Span::millis(90_000, 120_000), ..Settings::calm(8) });
    assert!(matches!(world.answer(), Answer::Failed { failure: Failure::Policy(_), .. }));
    assert_eq!(world.checked(), [false, false, false]);
    assert!(world.pushes().is_empty());
}

#[test]
fn a_host_start_beyond_run_capacity_is_answered_busy_without_work() {
    let calm = Settings::calm(11);
    let limits = smith_domain::Limits { run: run::Limits { runs: 0, ..calm.limits.run }, ..calm.limits };
    let world = settled(&Settings { limits, ..calm });
    assert_eq!(world.answer(), &Answer::Refused(run::Refusal::Busy));
    assert!(world.prompts().is_empty() && world.checked().is_empty() && world.pushes().is_empty());
}

#[test]
fn a_scripted_host_retries_a_failed_request_with_a_fresh_agent() {
    // This models a new host start, not a forge merge or an engine plan.
    let failed = settled(&Settings { push: HostReply::Stale, ..Settings::calm(12) });
    assert!(matches!(failed.answer(), Answer::Failed { failure: Failure::Stale, .. }));
    let retried = settled(&Settings::calm(13));
    assert_eq!(retried.landed(), FIXED);
    assert!(matches!(retried.answer(), Answer::Accepted { .. }));
}

#[test]
fn a_world_replays_its_boundaries_and_answer_from_its_seed() {
    assert_replays(42, 43, |seed| {
        let world = settled(&Settings { job: Job::Delegating, ..Settings::calm(seed) });
        (world.trace().to_vec(), (format!("{:?}", world.answer()), world.answered_at(), world.landed().to_vec()))
    });
}

#[test]
fn dropping_facts_changes_no_agent_decision_or_host_boundary() {
    for job in [Job::Coding, Job::Review, Job::Reporting, Job::Failing] {
        let calm = Settings { job, ..Settings::calm(44) };
        let kept = settled(&calm);
        let limits = smith_domain::Limits {
            run: run::Limits { facts: 0, ..calm.limits.run },
            session: smith_domain::session::Limits {
                facts: 0,
                tools: smith_domain::tools::Limits { facts: 0, ..calm.limits.session.tools },
                ..calm.limits.session
            },
            ..calm.limits
        };
        let silent = settled(&Settings { limits, drain_facts: false, ..calm });
        assert_eq!(silent.answer(), kept.answer());
        assert_eq!(silent.trace(), kept.trace());
        assert_eq!(silent.landed(), kept.landed());
        assert!(silent.facts().is_empty() && silent.lost() > 0);
    }
}

#[test]
fn a_declared_failure_corrects_its_contract_and_is_an_accepted_result() {
    let world = settled(&Settings { job: Job::Failing, writable: false, ..Settings::calm(19) });
    let Answer::Accepted { outcome: Declared::Failure(failure), .. } = world.answer() else {
        panic!("a declared inability is a result, not a runtime failure")
    };
    assert_eq!(&*failure.reason, b"The host has not supplied the needed access.");
    assert_eq!(&*failure.fields[0].name, b"cause");
    assert_eq!(&*failure.fields[0].value, b"missing-authority");
    assert_eq!(
        count(&world, |fact| matches!(fact, run::facts::Fact::Returned { result: run::facts::Return::Rejected, .. })),
        1
    );
    assert!(world.pushes().is_empty() && world.checked().is_empty());
    assert_replays(19, 20, |seed| {
        let world = settled(&Settings { job: Job::Failing, ..Settings::calm(seed) });
        (world.trace().to_vec(), format!("{:?}", world.answer()))
    });
}

#[test]
fn separately_granted_mid_delivery_continues_to_a_real_report() {
    let world = settled(&Settings { job: Job::MidReport, ..Settings::calm(40) });
    assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Report(_), .. }));
    assert_eq!(world.checked(), [true]);
    assert_eq!(world.pushes(), [smith_agent_world::delivered()]);
    assert_eq!(world.landed(), FIXED);
    let Delivery::Delivered(receipts) = smith_agent_world::delivered() else {
        unreachable!("the host fixture returns sealed receipts")
    };
    // This world renders ordinary served results with Debug; opaque byte arrays
    // therefore appear as decimal bytes, not their UTF-8 spelling. Compare the
    // complete expected feedback, including mount ordinal and actual receipt bytes.
    let expected_feedback = format!(
        "{:?}",
        smith_domain::llm::Returned::Served { returned: run::Returned::Delivered(receipts), error: false }
    )
    .into_bytes();
    let receipt_reached_llm =
        world.prompts().iter().flat_map(|prompt| &prompt.messages).flat_map(|message| &message.parts).any(|part| {
            matches!(part,
            smith_fake_llm_domain::api::Part::ToolOutput { output, is_error: false, .. }
            if output.as_ref() == expected_feedback.as_slice())
        });
    assert!(receipt_reached_llm, "actual receipts are continuation feedback");
}

#[test]
fn named_marker_refusal_is_corrected_before_the_next_checked_delivery() {
    let world = settled(&Settings { job: Job::MarkerReport, ..Settings::calm(41) });
    assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Report(_), .. }));
    assert_eq!(world.checked(), [true, true]);
    assert!(matches!(&world.pushes()[0], Delivery::Refused(refusal)
        if refusal.marker().is_some_and(|marker| marker.path() == b"conflict.txt")));
    assert_eq!(world.pushes()[1], smith_agent_world::delivered());
    assert_eq!(world.landed(), FIXED);
}

#[test]
fn five_actual_host_terminals_preserve_report_only_contract_and_stale_ends_it() {
    for reply in [
        HostReply::Delivered,
        HostReply::Nothing,
        HostReply::Refused,
        HostReply::Failed(run::DeliveryFailure::new(run::DeliveryReason::Broken)),
        HostReply::Stale,
    ] {
        let world = settled(&Settings { job: Job::MidReport, push: reply, ..Settings::calm(42) });
        if reply == HostReply::Stale {
            assert!(matches!(world.answer(), Answer::Failed { failure: Failure::Stale, .. }));
        } else {
            assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Report(_), .. }));
        }
        assert_eq!(world.pushes().len(), 1);
        assert_eq!(world.landed().is_empty(), reply != HostReply::Delivered);
    }
}

#[test]
fn mid_landing_during_explicit_shutdown_preserves_actual_receipts_and_spend() {
    // Deterministic zero-latency provider/IO except the host terminal: choose a
    // cancellation within that actual pending operation by replaying its observed
    // submission timestamp, never by inspecting private state.
    let baseline = settled(&Settings { job: Job::MidReport, ..Settings::calm(43) });
    let submitted = baseline.delivery_names()[0].1;
    let base = Settings::calm(43);
    let world = settled(&Settings {
        job: Job::MidReport,
        cancel_at: Some(submitted.saturating_since(skein_lib::Time::ZERO).saturating_add(Duration::from_nanos(1))),
        ..base
    });
    let Answer::Delivered { name, receipts, stopped: Failure::Cancelled, .. } = world.answer() else {
        panic!("interrupted Report-only delivery retains its actual terminal")
    };
    assert_eq!(*name, world.delivery_names()[0].0);
    assert_eq!(receipts.receipts()[0].text(), b"scripted receipt");
    assert_eq!(world.landed(), FIXED);
}

#[test]
fn mid_delivery_replays_same_names_and_facts_are_observations() {
    let settings = Settings { job: Job::MarkerReport, ..Settings::calm(44) };
    assert_replays(44, 45, |seed| {
        let world = settled(&Settings { seed, ..settings });
        (world.trace().to_vec(), format!("{:?} {:?}", world.answer(), world.delivery_names()))
    });
    let observed = settled(&settings);
    assert_eq!(observed.delivery_names()[0].0.completion, 2);
    assert_eq!(observed.delivery_names()[1].0.completion, 4);
    assert_ne!(observed.delivery_names()[0].0, observed.delivery_names()[1].0);
    let quiet = settled(&Settings { drain_facts: false, ..settings });
    assert_eq!(observed.answer(), quiet.answer());
    assert_eq!(observed.landed(), quiet.landed());
}

#[test]
fn submitted_delivery_gets_a_real_timed_out_host_terminal_during_shutdown() {
    let baseline = settled(&Settings { job: Job::MidReport, ..Settings::calm(45) });
    let submitted = baseline.delivery_names()[0].1;
    let mut settings = Settings {
        job: Job::MidReport,
        cancel_at: Some(submitted.saturating_since(skein_lib::Time::ZERO).saturating_add(Duration::from_nanos(1))),
        ..Settings::calm(45)
    };
    settings.limits.run.delivery_timeout = Duration::from_nanos(10);
    let world = settled(&settings);
    assert_eq!(world.pushes(), [Delivery::Failed(run::DeliveryFailure::new(run::DeliveryReason::TimedOut))]);
    assert!(world.landed().is_empty());
    assert!(matches!(world.answer(), Answer::Failed { failure: Failure::Cancelled, .. }));
}

#[test]
fn real_mid_delivery_then_final_change_checks_and_lands_each_snapshot() {
    let world = settled(&Settings { job: Job::MidChange, ..Settings::calm(46) });
    assert!(matches!(world.answer(), Answer::Accepted { outcome: Declared::Change(_), .. }));
    assert_eq!(world.pushes(), [smith_agent_world::delivered(), smith_agent_world::delivered()]);
    assert_eq!(world.checked(), [true, true]);
    assert_eq!(world.landed(), b"pub fn answer() -> u32 { 43 /* checked answer */ }\n");
    assert_eq!(world.delivery_names()[0].0.completion, 2);
    assert_eq!(world.delivery_names()[1].0.completion, 4);
    assert_eq!(world.judged().1, 1, "both deliveries precede the one independently judged final answer");
}
