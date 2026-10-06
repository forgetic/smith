//! Golden bytes for each run call terminal and its nested refusal vocabulary.
//! Contract: domain/run.md, sections 5.2, 7, 8 and 10; testing-strategy.md, section 7.

use skein_lib::Duration;
use smith_domain::{feedback, run};

fn bytes(text: &[u8]) -> Box<[u8]> {
    text.into()
}

fn rendered(returned: run::Returned, expected: &str, error: bool) {
    let result = feedback(returned, 4096).expect("bounded complete feedback");
    assert_eq!(result.text.as_ref(), expected.as_bytes());
    assert_eq!(result.error, error);
}

#[test]
fn every_run_call_terminal_has_canonical_feedback() {
    let receipt = run::Receipt::new(1, bytes(b"r")).expect("receipt");
    let delivered = run::Delivered::new(Box::new([receipt])).expect("delivery");
    let refusal = run::DeliveryRefusal::new(None, bytes(b"review")).expect("refusal");
    let host = run::HostAnswer::new(bytes(b"host text"), false).expect("host text");
    let rows = [
        (run::Returned::Waiting, "waiting", false),
        (run::Returned::HostAnswered(host), "host text", false),
        (run::Returned::HostUnknown, "host-unknown", true),
        (run::Returned::HostRejected(run::HostProblem::Undeclared), "host-rejected reason=undeclared", true),
        (run::Returned::Delivered(delivered), "delivered\nreceipt directory=1 text=\"r\"", false),
        (run::Returned::Nothing, "nothing", true),
        (run::Returned::DeliveryRefused(refusal), "delivery-refused explanation=\"review\" marker=none", true),
        (run::Returned::Accepted, "accepted", false),
        (
            run::Returned::Rejected { problems: run::outcome::Problems { listed: Box::new([]), more: 2 } },
            "rejected more=2",
            true,
        ),
        (
            run::Returned::ChecksFailed {
                repository: bytes(b"repo"),
                ran: run::Ran { exit: run::Exit::TimedOut, output: bytes(b"tail"), cut: 3 },
            },
            "checks-failed repository=\"repo\" exit=timed-out cut=3 tail=\"tail\"",
            true,
        ),
        (run::Returned::Stale, "stale", true),
        (
            run::Returned::DeliveryFailed { failure: run::DeliveryFailure::new(0, run::DeliveryReason::Unknown) },
            "delivery-failed directory=0 reason=unknown cut=0 tail=\"\"",
            true,
        ),
        (run::Returned::Cancelled, "cancelled", true),
        (run::Returned::TimedOut, "timed-out", true),
        (run::Returned::Busy, "busy", true),
        (run::Returned::Answered { text: bytes(b"child"), cut: 0, stop: run::Stop::EndTurn }, "child", false),
        (run::Returned::Unanswered { end: run::End::Closed }, "unanswered end=closed", true),
        (run::Returned::Refused { refusal: run::AskRefusal::NotGranted }, "refused reason=not-granted", true),
    ];
    for (returned, expected, error) in rows {
        rendered(returned, expected, error);
    }
}

#[test]
fn host_and_ask_refusal_variants_keep_their_names() {
    for (problem, label) in [
        (run::HostProblem::Undeclared, "undeclared"),
        (run::HostProblem::Effect, "effect"),
        (run::HostProblem::TooLarge, "too-large"),
    ] {
        rendered(run::Returned::HostRejected(problem), &format!("host-rejected reason={label}"), true);
    }
    for (refusal, label) in [
        (run::AskRefusal::Name, "name"),
        (run::AskRefusal::NotGranted, "not-granted"),
        (run::AskRefusal::TooDeep, "too-deep"),
        (run::AskRefusal::TooMany, "too-many"),
        (run::AskRefusal::UnknownLlm, "unknown-llm"),
        (run::AskRefusal::Unworkable, "unworkable"),
        (run::AskRefusal::Over, "over"),
    ] {
        rendered(run::Returned::Refused { refusal }, &format!("refused reason={label}"), true);
    }
}

#[test]
fn every_contract_problem_renders_its_values_and_field_scope() {
    use run::outcome::{Form, Problem, Problems};

    let rows = [
        (Problem::TooLarge { max: 9 }, "too-large max=9"),
        (Problem::ChangeNotAllowed, "change-not-allowed"),
        (Problem::VerdictNotAllowed, "verdict-not-allowed"),
        (Problem::ReportNotAllowed, "report-not-allowed"),
        (Problem::FailureNotAllowed, "failure-not-allowed"),
        (Problem::UnknownVerdict, "unknown-verdict"),
        (Problem::TextTooLarge { form: Form::Report, max: 3 }, "text-too-large form=report max=3"),
        (Problem::TextTooLarge { form: Form::Verdict, max: 4 }, "text-too-large form=verdict max=4"),
        (Problem::TextTooLarge { form: Form::Failure, max: 5 }, "text-too-large form=failure max=5"),
        (Problem::TooFewItems { min: 2 }, "too-few-items min=2"),
        (Problem::TooManyItems { max: 6 }, "too-many-items max=6"),
        (Problem::KindNotAllowed { item: 7 }, "kind-not-allowed item=7"),
        (Problem::MissingField { item: None, field: bytes(b"name") }, "missing-field item=none field=\"name\""),
        (Problem::EmptyField { item: Some(2), field: bytes(b"name") }, "empty-field item=2 field=\"name\""),
        (Problem::RepeatedField { item: None, field: bytes(b"name") }, "repeated-field item=none field=\"name\""),
        (
            Problem::FieldTooLarge { item: Some(3), field: bytes(b"name"), max: 8 },
            "field-too-large item=3 field=\"name\" max=8",
        ),
    ];
    for (problem, expected) in rows {
        rendered(
            run::Returned::Rejected { problems: Problems { listed: Box::new([problem]), more: 0 } },
            &format!("rejected more=0\nproblem {expected}"),
            true,
        );
    }
}

#[test]
fn every_delivery_reason_and_check_exit_is_visible() {
    for (reason, label) in [
        (run::DeliveryReason::Unreachable, "unreachable"),
        (run::DeliveryReason::RefusedByTarget, "refused-by-target"),
        (run::DeliveryReason::TimedOut, "timed-out"),
        (run::DeliveryReason::Broken, "broken"),
        (run::DeliveryReason::TooLarge, "too-large"),
        (run::DeliveryReason::Missing, "missing"),
        (run::DeliveryReason::Busy, "busy"),
        (run::DeliveryReason::Unavailable, "unavailable"),
        (run::DeliveryReason::Cancelled, "cancelled"),
        (run::DeliveryReason::Unknown, "unknown"),
    ] {
        rendered(
            run::Returned::DeliveryFailed { failure: run::DeliveryFailure::new(0, reason) },
            &format!("delivery-failed directory=0 reason={label} cut=0 tail=\"\""),
            true,
        );
    }
    rendered(
        run::Returned::DeliveryFailed { failure: run::DeliveryFailure::new(2, run::DeliveryReason::Unreachable) },
        "delivery-failed directory=2 reason=unreachable cut=0 tail=\"\"",
        true,
    );
    for (exit, label) in [
        (run::Exit::Code { code: 7 }, "code:7"),
        (run::Exit::Signalled, "signalled"),
        (run::Exit::TimedOut, "timed-out"),
        (run::Exit::Unstarted, "unstarted"),
    ] {
        rendered(
            run::Returned::ChecksFailed { repository: bytes(b"r"), ran: run::Ran { exit, output: bytes(b""), cut: 0 } },
            &format!("checks-failed repository=\"r\" exit={label} cut=0 tail=\"\""),
            true,
        );
    }
}

#[test]
fn every_unanswered_end_and_transcript_refusal_is_visible() {
    let rows = [
        (run::End::PriceOverflow, "price-overflow"),
        (run::End::UsageOverflow, "usage-overflow"),
        (run::End::Receiving(run::ReceivingLimit::Input), "receiving:input"),
        (run::End::Receiving(run::ReceivingLimit::Output), "receiving:output"),
        (run::End::Receiving(run::ReceivingLimit::CacheRead), "receiving:cache-read"),
        (run::End::Receiving(run::ReceivingLimit::CacheWrite), "receiving:cache-write"),
        (run::End::Closed, "closed"),
        (run::End::Busy, "busy"),
        (run::End::Invalid, "invalid"),
        (run::End::Budget(run::Exhausted::Turns), "budget:turns"),
        (run::End::Budget(run::Exhausted::Spend), "budget:spend"),
        (run::End::Budget(run::Exhausted::Time), "budget:time"),
    ];
    for (end, label) in rows {
        rendered(run::Returned::Unanswered { end }, &format!("unanswered end={label}"), true);
    }
    for (reason, label) in [
        (run::TranscriptRefusal::Version, "version"),
        (run::TranscriptRefusal::Endpoint, "endpoint"),
        (run::TranscriptRefusal::Dialect, "dialect"),
        (run::TranscriptRefusal::Malformed, "malformed"),
        (run::TranscriptRefusal::Unresolved, "unresolved"),
        (run::TranscriptRefusal::TooLarge, "too-large"),
    ] {
        rendered(
            run::Returned::Unanswered { end: run::End::TranscriptRefused { reason } },
            &format!("unanswered end=transcript-refused:{label}"),
            true,
        );
    }
}

#[test]
fn every_model_fault_and_completion_failure_keeps_its_evidence() {
    for (fault, label) in [
        (run::Fault::Exhausted, "exhausted"),
        (run::Fault::Provider, "provider"),
        (run::Fault::ContextFull, "context-full"),
        (run::Fault::Refused, "refused"),
        (run::Fault::Truncated, "truncated"),
        (run::Fault::Malformed, "malformed"),
    ] {
        rendered(
            run::Returned::Unanswered { end: run::End::Fault(fault) },
            &format!("unanswered end=fault:{label}"),
            true,
        );
    }
    let cooldown = Duration::from_nanos(19);
    for (failure, label) in [
        (run::CompletionFailure::Limit, "limit"),
        (run::CompletionFailure::Protocol, "protocol"),
        (run::CompletionFailure::Cancelled, "unsolicited-cancelled"),
        (run::CompletionFailure::Overloaded, "overloaded"),
        (run::CompletionFailure::Unavailable, "unavailable"),
        (run::CompletionFailure::TimedOut, "timed-out"),
        (run::CompletionFailure::ContextTooLong, "context-too-long"),
        (run::CompletionFailure::Invalid, "invalid"),
        (run::CompletionFailure::Unauthorized, "unauthorized"),
        (run::CompletionFailure::RateLimited { retry_after: cooldown }, "rate-limited retry-after-ns=19"),
        (run::CompletionFailure::Exhausted { retry_after: cooldown }, "exhausted retry-after-ns=19"),
    ] {
        for (evidence, evidence_label) in [
            (run::CompletionEvidence::Unsent, "unsent"),
            (run::CompletionEvidence::Unknown, "unknown"),
            (run::CompletionEvidence::Response, "response"),
        ] {
            rendered(
                run::Returned::Unanswered { end: run::End::Fault(run::Fault::Completion { failure, evidence }) },
                &format!("unanswered end=fault:completion:{label} evidence={evidence_label}"),
                true,
            );
        }
    }
}
