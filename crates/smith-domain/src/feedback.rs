//! Canonical provider-neutral application feedback, with no protocol callback.
//! This module keeps no entity or terminal right. Its two checked passes measure
//! then write one owning text, preserving all semantic fields and opaque bytes.
//! Provider-attested host text and ordinary child text move verbatim; opaque
//! evidence uses lossless quoted ASCII byte literals. Contract: domain/run.md,
//! sections 7, 8, 10 and 14; programming-model.md, sections 4.5, 6.3 and 8.

use alloc::boxed::Box;

use run::outcome::{Form, Problem, Problems};
use skein_lib::Writer;
use smith_domain_run::{self as run, Returned, Stop};

/// One complete concrete application result, supplied to session AnsweredV2.
/// It carries no authority or pending operation; ownership passes to the caller.
/// Contract: domain/run.md, sections 7, 8 and 14.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Feedback {
    /// Complete valid protocol text, at most the checked receiving byte cap.
    /// Attested host/child bytes remain exact; opaque evidence is escaped losslessly.
    /// Contract: domain/run.md, sections 7, 8 and 14.
    pub text: Box<[u8]>,

    /// Exact application error classification, independent of transport evidence.
    /// Contract: domain/run.md, sections 7, 8 and 14.
    pub error: bool,
}

/// Pure feedback construction failed before allocation. A root must reserve a
/// compatible cap before admitting an effect whose actual result uses this helper.
/// Contract: domain/run.md, sections 7, 8 and 14.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FeedbackRefusal {
    /// The complete text exceeds the caller's cap or checked length arithmetic.
    /// No partial evidence or replacement operation result is constructed.
    /// Contract: domain/run.md, sections 7, 8 and 14.
    TooLarge,
}

/// Consume a semantic terminal into complete canonical feedback. This is a pure
/// mechanical application translation, never a new request, retry or effect.
/// The caller prices simultaneous semantic and rendered ownership. TooLarge
/// precedes allocation; actual admitted results require a compatible receiving cap.
/// Contract: domain/run.md, sections 7, 8, 10 and 14.
///
/// # Errors
/// Returns TooLarge when complete feedback cannot fit max_bytes.
pub fn feedback(returned: Returned, max_bytes: u64) -> Result<Feedback, FeedbackRefusal> {
    let error = crate::translate::failed(&returned);
    let returned = match returned {
        Returned::HostAnswered(answer) => {
            let (text, error) = answer.into_parts();
            return unchanged(text, error, max_bytes);
        }
        Returned::Answered { text, cut: 0, stop: Stop::EndTurn } => return unchanged(text, false, max_bytes),
        returned @ (Returned::Waiting
        | Returned::HostUnknown
        | Returned::HostRejected(_)
        | Returned::Delivered(_)
        | Returned::Nothing
        | Returned::DeliveryRefused(_)
        | Returned::Accepted
        | Returned::Rejected { .. }
        | Returned::ChecksFailed { .. }
        | Returned::Stale
        | Returned::DeliveryFailed { .. }
        | Returned::Cancelled
        | Returned::TimedOut
        | Returned::Busy
        | Returned::Answered { .. }
        | Returned::Unanswered { .. }
        | Returned::Refused { .. }) => returned,
    };
    let mut measured = Text { len: Some(0), writer: None };
    render(&mut measured, &returned);
    let length = measured.len.ok_or(FeedbackRefusal::TooLarge)?;
    if u64::try_from(length).ok().ok_or(FeedbackRefusal::TooLarge)? > max_bytes {
        return Err(FeedbackRefusal::TooLarge);
    }
    let mut written = Text { len: Some(0), writer: Some(Writer::new(length)) };
    render(&mut written, &returned);
    Ok(Feedback { text: written.writer.expect("measured before allocation").finish(), error })
}

fn unchanged(text: Box<[u8]>, error: bool, max_bytes: u64) -> Result<Feedback, FeedbackRefusal> {
    let Some(length) = u64::try_from(text.len()).ok() else {
        return Err(FeedbackRefusal::TooLarge);
    };
    if length > max_bytes {
        return Err(FeedbackRefusal::TooLarge);
    }
    Ok(Feedback { text, error })
}

/// Conservative complete-feedback text bound under the run's admitted payload
/// caps, including four-byte ASCII expansion, every label/count and child cut/stop.
/// Returned None means arithmetic overflow; callers refuse before effects.
/// Semantic ownership and this text are separately priced during construction.
/// Contract: domain/run.md, sections 7, 8 and 14.
#[must_use]
pub fn feedback_worst_case(limits: &run::Limits) -> Option<u64> {
    let fields = limits.run_bytes.max(limits.outcome_bytes).checked_mul(4)?;
    let problems = fields.checked_add(256)?.checked_mul(u64::from(Problems::LISTED))?.checked_add(128)?;
    let checks = limits.run_bytes.checked_add(u64::from(limits.check_tail))?.checked_mul(4)?.checked_add(256)?;
    let count = u64::from(limits.repositories.min(run::MAX_DIRECTORIES));
    let receipts = u64::try_from(run::Receipt::CAPACITY)
        .ok()?
        .checked_mul(4)?
        .checked_add(128)?
        .checked_mul(count)?
        .checked_add(128)?;
    let refusal = u64::try_from(run::Marker::CAPACITY)
        .ok()?
        .checked_add(u64::try_from(run::DeliveryRefusal::CAPACITY).ok()?)?
        .checked_mul(4)?
        .checked_add(256)?;
    let child = u64::from(limits.answer_bytes).checked_add(128)?;
    Some(problems.max(checks).max(receipts).max(refusal).max(child).max(u64::from(limits.host_reply_bytes)).max(4096))
}

fn render(text: &mut Text, returned: &Returned) {
    match returned {
        Returned::Waiting => text.put(b"waiting"),
        Returned::HostAnswered(answer) => text.put(answer.text()),
        Returned::HostUnknown => text.put(b"host-unknown"),
        Returned::HostRejected(problem) => {
            text.put(b"host-rejected reason=");
            text.put(match problem {
                run::HostProblem::Undeclared => b"undeclared",
                run::HostProblem::Effect => b"effect",
                run::HostProblem::TooLarge => b"too-large",
            });
        }
        Returned::Delivered(delivered) => render_receipts(text, delivered),
        Returned::Nothing => text.put(b"nothing"),
        Returned::DeliveryRefused(refusal) => render_delivery_refusal(text, refusal),
        Returned::Accepted => text.put(b"accepted"),
        Returned::Rejected { problems } => {
            text.put(b"rejected more=");
            text.number(u64::from(problems.more));
            for problem in &problems.listed {
                text.put(b"\nproblem ");
                render_problem(text, problem);
            }
        }
        Returned::ChecksFailed { repository, ran } => render_checks(text, repository, ran),
        Returned::Stale => text.put(b"stale"),
        Returned::DeliveryFailed { failure } => {
            text.put(b"delivery-failed reason=");
            text.put(delivery_reason(failure.reason));
            text.put(b" cut=");
            text.number(failure.diagnostic.cut());
            text.put(b" tail=");
            text.bytes(failure.diagnostic.output());
        }
        Returned::Cancelled => text.put(b"cancelled"),
        Returned::TimedOut => text.put(b"timed-out"),
        Returned::Busy => text.put(b"busy"),
        Returned::Answered { text: answer, cut, stop } => render_child(text, answer, *cut, *stop),
        Returned::Unanswered { end } => {
            text.put(b"unanswered end=");
            render_end(text, *end);
        }
        Returned::Refused { refusal } => {
            text.put(b"refused reason=");
            text.put(match refusal {
                run::AskRefusal::Name => b"name",
                run::AskRefusal::NotGranted => b"not-granted",
                run::AskRefusal::TooDeep => b"too-deep",
                run::AskRefusal::TooMany => b"too-many",
                run::AskRefusal::UnknownLlm => b"unknown-llm",
                run::AskRefusal::Unworkable => b"unworkable",
                run::AskRefusal::Over => b"over",
            });
        }
    }
}

fn render_receipts(text: &mut Text, delivered: &run::Delivered) {
    text.put(b"delivered");
    for receipt in delivered.receipts() {
        text.put(b"\nreceipt directory=");
        text.number(u64::from(receipt.directory()));
        text.put(b" text=");
        text.bytes(receipt.text());
    }
}

fn render_delivery_refusal(text: &mut Text, refusal: &run::DeliveryRefusal) {
    text.put(b"delivery-refused explanation=");
    text.bytes(refusal.explanation());
    match refusal.marker() {
        Some(marker) => {
            text.put(b" marker-directory=");
            text.number(u64::from(marker.directory()));
            text.put(b" marker-path=");
            text.bytes(marker.path());
        }
        None => text.put(b" marker=none"),
    }
}

fn render_checks(text: &mut Text, repository: &[u8], ran: &run::Ran) {
    text.put(b"checks-failed repository=");
    text.bytes(repository);
    text.put(b" exit=");
    render_exit(text, ran.exit);
    text.put(b" cut=");
    text.number(ran.cut);
    text.put(b" tail=");
    text.bytes(&ran.output);
}

fn render_child(text: &mut Text, answer: &[u8], cut: u64, stop: Stop) {
    text.put(answer);
    text.put(b"\n[child-result cut=");
    text.number(cut);
    text.put(b" stop=");
    text.put(match stop {
        Stop::EndTurn => b"end-turn",
        Stop::MaxTokens => b"max-tokens",
        Stop::Refusal => b"refusal",
        Stop::NoCalls => b"no-calls",
    });
    text.put(b"]");
}

fn render_problem(text: &mut Text, problem: &Problem) {
    match problem {
        Problem::TooLarge { max } => {
            text.put(b"too-large max=");
            text.number(*max);
        }
        Problem::ChangeNotAllowed => text.put(b"change-not-allowed"),
        Problem::VerdictNotAllowed => text.put(b"verdict-not-allowed"),
        Problem::ReportNotAllowed => text.put(b"report-not-allowed"),
        Problem::FailureNotAllowed => text.put(b"failure-not-allowed"),
        Problem::UnknownVerdict => text.put(b"unknown-verdict"),
        Problem::TextTooShort { form, min } => {
            text.put(b"text-too-short form=");
            render_form(text, *form);
            text.put(b" min=");
            text.number(u64::from(*min));
        }
        Problem::TextTooLarge { form, max } => {
            text.put(b"text-too-large form=");
            render_form(text, *form);
            text.put(b" max=");
            text.number(u64::from(*max));
        }
        Problem::TooFewItems { min } => {
            text.put(b"too-few-items min=");
            text.number(u64::from(*min));
        }
        Problem::TooManyItems { max } => {
            text.put(b"too-many-items max=");
            text.number(u64::from(*max));
        }
        Problem::KindNotAllowed { item } => {
            text.put(b"kind-not-allowed item=");
            text.number(u64::from(*item));
        }
        Problem::MissingField { item, field } => {
            text.put(b"missing-field");
            render_field(text, *item, field);
        }
        Problem::EmptyField { item, field } => {
            text.put(b"empty-field");
            render_field(text, *item, field);
        }
        Problem::RepeatedField { item, field } => {
            text.put(b"repeated-field");
            render_field(text, *item, field);
        }
        Problem::FieldTooLarge { item, field, max } => {
            text.put(b"field-too-large");
            render_field(text, *item, field);
            text.put(b" max=");
            text.number(u64::from(*max));
        }
    }
}

fn render_form(text: &mut Text, form: Form) {
    text.put(match form {
        Form::Report => b"report",
        Form::Verdict => b"verdict",
        Form::Failure => b"failure",
    });
}

fn render_field(text: &mut Text, item: Option<u32>, field: &[u8]) {
    text.put(b" item=");
    match item {
        Some(item) => text.number(u64::from(item)),
        None => text.put(b"none"),
    }
    text.put(b" field=");
    text.bytes(field);
}

fn render_exit(text: &mut Text, exit: run::Exit) {
    match exit {
        run::Exit::Code { code } => {
            text.put(b"code:");
            text.number(u64::from(code));
        }
        run::Exit::Signalled => text.put(b"signalled"),
        run::Exit::TimedOut => text.put(b"timed-out"),
        run::Exit::Unstarted => text.put(b"unstarted"),
    }
}

fn render_end(text: &mut Text, end: run::End) {
    match end {
        run::End::TranscriptRefused { reason } => {
            text.put(b"transcript-refused:");
            text.put(match reason {
                run::TranscriptRefusal::Version => b"version",
                run::TranscriptRefusal::Endpoint => b"endpoint",
                run::TranscriptRefusal::Dialect => b"dialect",
                run::TranscriptRefusal::Malformed => b"malformed",
                run::TranscriptRefusal::Unresolved => b"unresolved",
                run::TranscriptRefusal::TooLarge => b"too-large",
            });
        }
        run::End::Closed => text.put(b"closed"),
        run::End::Busy => text.put(b"busy"),
        run::End::Invalid => text.put(b"invalid"),
        run::End::Fault(fault) => {
            text.put(b"fault:");
            render_fault(text, fault);
        }
        run::End::Budget(exhausted) => {
            text.put(b"budget:");
            text.put(match exhausted {
                run::Exhausted::Turns => b"turns",
                run::Exhausted::Input => b"input",
                run::Exhausted::Output => b"output",
                run::Exhausted::CacheRead => b"cache-read",
                run::Exhausted::CacheWrite => b"cache-write",
                run::Exhausted::Time => b"time",
            });
        }
    }
}

fn render_fault(text: &mut Text, fault: run::Fault) {
    match fault {
        run::Fault::Completion { failure, evidence } => {
            text.put(b"completion:");
            render_completion_failure(text, failure);
            text.put(b" evidence=");
            text.put(match evidence {
                run::CompletionEvidence::Unsent => b"unsent",
                run::CompletionEvidence::Unknown => b"unknown",
                run::CompletionEvidence::Response => b"response",
            });
        }
        run::Fault::Exhausted => text.put(b"exhausted"),
        run::Fault::Provider => text.put(b"provider"),
        run::Fault::ContextFull => text.put(b"context-full"),
        run::Fault::Refused => text.put(b"refused"),
        run::Fault::Truncated => text.put(b"truncated"),
        run::Fault::Malformed => text.put(b"malformed"),
    }
}

fn render_completion_failure(text: &mut Text, failure: run::CompletionFailure) {
    match failure {
        run::CompletionFailure::Limit => text.put(b"limit"),
        run::CompletionFailure::Protocol => text.put(b"protocol"),
        run::CompletionFailure::Cancelled => text.put(b"unsolicited-cancelled"),
        run::CompletionFailure::Overloaded => text.put(b"overloaded"),
        run::CompletionFailure::Unavailable => text.put(b"unavailable"),
        run::CompletionFailure::TimedOut => text.put(b"timed-out"),
        run::CompletionFailure::ContextTooLong => text.put(b"context-too-long"),
        run::CompletionFailure::Invalid => text.put(b"invalid"),
        run::CompletionFailure::Unauthorized => text.put(b"unauthorized"),
        run::CompletionFailure::RateLimited { retry_after } => {
            text.put(b"rate-limited retry-after-ns=");
            text.number(retry_after.as_nanos());
        }
        run::CompletionFailure::Exhausted { retry_after } => {
            text.put(b"exhausted retry-after-ns=");
            text.number(retry_after.as_nanos());
        }
    }
}

fn delivery_reason(reason: run::DeliveryReason) -> &'static [u8] {
    match reason {
        run::DeliveryReason::Unreachable => b"unreachable",
        run::DeliveryReason::RefusedByTarget => b"refused-by-target",
        run::DeliveryReason::TimedOut => b"timed-out",
        run::DeliveryReason::Broken => b"broken",
        run::DeliveryReason::TooLarge => b"too-large",
        run::DeliveryReason::Missing => b"missing",
        run::DeliveryReason::Busy => b"busy",
        run::DeliveryReason::Unavailable => b"unavailable",
        run::DeliveryReason::Cancelled => b"cancelled",
        run::DeliveryReason::Unknown => b"unknown",
    }
}

struct Text {
    len: Option<usize>,
    writer: Option<Writer>,
}

impl Text {
    fn put(&mut self, bytes: &[u8]) {
        self.len = match self.len {
            Some(len) => len.checked_add(bytes.len()),
            None => None,
        };
        match &mut self.writer {
            Some(writer) => {
                writer.put(bytes).expect("second pass is identical to checked measure");
            }
            None => {}
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.put(b"\"");
        for byte in bytes {
            if (0x20..=0x7e).contains(byte) && *byte != b'"' && *byte != b'\\' {
                self.put(core::slice::from_ref(byte));
            } else {
                let high = byte.checked_div(16).expect("nonzero divisor");
                let low = byte.checked_rem(16).expect("nonzero divisor");
                self.put(&[b'\\', b'x', digit(high), digit(low)]);
            }
        }
        self.put(b"\"");
    }

    fn number(&mut self, number: u64) {
        let mut digits = [b'0'; 20];
        let mut rest = number;
        let mut first = 19_usize;
        for (index, digit) in digits.iter_mut().enumerate().rev() {
            let value = u8::try_from(rest.checked_rem(10).expect("nonzero divisor")).expect("decimal digit");
            *digit = b'0'.checked_add(value).expect("decimal digit");
            if value != 0 {
                first = index;
            }
            rest = rest.checked_div(10).expect("nonzero divisor");
        }
        self.put(digits.get(first..).expect("decimal digits start within fixed array"));
    }
}

fn digit(value: u8) -> u8 {
    match value {
        0..=9 => b'0'.checked_add(value).expect("hex digit"),
        10..=15 => b'a'.checked_add(value.checked_sub(10).expect("high hex digit")).expect("hex digit"),
        _ => unreachable!("nibble extracted by division or remainder"),
    }
}

#[cfg(test)]
mod tests {
    use super::{FeedbackRefusal, feedback};
    use alloc::boxed::Box;
    use smith_domain_run as run;

    fn bytes(text: &[u8]) -> Box<[u8]> {
        text.into()
    }

    #[test]
    fn actual_host_and_ordinary_child_text_move_exactly_with_error_and_no_copy() {
        for error in [false, true] {
            let text = bytes("actual ✓\n".as_bytes());
            let pointer = text.as_ptr();
            let answer = run::HostAnswer::new(text, error).expect("sealed host text");
            let got = feedback(run::Returned::HostAnswered(answer), 64).expect("compatible cap");
            assert_eq!(got.text.as_ref(), "actual ✓\n".as_bytes());
            assert_eq!(got.text.as_ptr(), pointer);
            assert_eq!(got.error, error);
        }
        let text = bytes(b"child\n");
        let pointer = text.as_ptr();
        let got =
            feedback(run::Returned::Answered { text, cut: 0, stop: run::Stop::EndTurn }, 64).expect("ordinary child");
        assert_eq!(got.text.as_ref(), b"child\n");
        assert_eq!(got.text.as_ptr(), pointer);
        assert!(!got.error);
    }

    #[test]
    fn child_cut_and_every_stop_survive_canonical_feedback() {
        for (stop, name) in [
            (run::Stop::EndTurn, "end-turn"),
            (run::Stop::MaxTokens, "max-tokens"),
            (run::Stop::Refusal, "refusal"),
            (run::Stop::NoCalls, "no-calls"),
        ] {
            for cut in [0, u64::MAX] {
                let got = feedback(run::Returned::Answered { text: bytes(b"child"), cut, stop }, 128)
                    .expect("compatible bound");
                let expected = if cut == 0 && stop == run::Stop::EndTurn {
                    "child".to_owned()
                } else {
                    format!("child\n[child-result cut={cut} stop={name}]")
                };
                assert_eq!(got.text.as_ref(), expected.as_bytes());
                assert!(!got.error);
            }
        }
    }

    #[test]
    fn actual_opaque_receipts_diagnostics_markers_and_check_tail_are_lossless_ascii() {
        let opaque = [0xff, b'"', b'\\', 0, b'\n', b' ', b'a'];
        let encoded = br#""\xff\x22\x5c\x00\x0a a""#;
        let receipts = run::Delivered::new(Box::new([run::Receipt::new(2, bytes(&opaque)).expect("bounded receipt")]))
            .expect("actual sealed receipts");
        let got = feedback(run::Returned::Delivered(receipts), 128).expect("full evidence fits");
        assert_eq!(got.text.as_ref(), [b"delivered\nreceipt directory=2 text=".as_slice(), encoded].concat());
        assert!(!got.error);
        let got = feedback(
            run::Returned::DeliveryFailed {
                failure: run::DeliveryFailure {
                    reason: run::DeliveryReason::Unknown,
                    diagnostic: run::Diagnostic::new(&opaque, u64::MAX),
                },
            },
            128,
        )
        .expect("diagnostic");
        assert_eq!(
            got.text.as_ref(),
            [b"delivery-failed reason=unknown cut=18446744073709551615 tail=".as_slice(), encoded].concat()
        );
        assert!(got.error);
        let marker = run::Marker::new(1, bytes(&[b'd', b'/', 0xff, b'"'])).expect("bounded marker");
        let refused = run::DeliveryRefusal::new(Some(marker), bytes(&opaque)).expect("bounded refusal");
        let got = feedback(run::Returned::DeliveryRefused(refused), 128).expect("named feedback");
        assert_eq!(
            got.text.as_ref(),
            [b"delivery-refused explanation=".as_slice(), encoded, br#" marker-directory=1 marker-path="d/\xff\x22""#]
                .concat()
        );
        let got = feedback(
            run::Returned::ChecksFailed {
                repository: bytes(&opaque),
                ran: run::Ran { exit: run::Exit::Code { code: 255 }, output: bytes(&opaque), cut: u64::MAX },
            },
            256,
        )
        .expect("checks");
        assert_eq!(
            got.text.as_ref(),
            [
                b"checks-failed repository=".as_slice(),
                encoded,
                b" exit=code:255 cut=18446744073709551615 tail=",
                encoded
            ]
            .concat()
        );
        assert!(got.error);
        assert!(got.text.is_ascii());
    }

    #[test]
    fn exact_text_cap_accepts_whole_result_and_one_byte_less_refuses() {
        assert_eq!(feedback(run::Returned::Accepted, 7), Err(FeedbackRefusal::TooLarge));
        assert_eq!(
            feedback(run::Returned::Accepted, 8),
            Ok(super::Feedback { text: bytes(b"accepted"), error: false })
        );
    }
}
