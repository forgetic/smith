//! Public canonical feedback controls with independently specified exact bytes.
//! Contract: domain/run.md, sections 5.4, 7, 8 and 10;
//! testing-strategy.md, section 7.

use smith_domain::{Feedback, FeedbackRefusal, feedback, run};

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
    let got = feedback(run::Returned::Answered { text, cut: 0, stop: run::Stop::EndTurn }, 64).expect("ordinary child");
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
            let got =
                feedback(run::Returned::Answered { text: bytes(b"child"), cut, stop }, 128).expect("compatible bound");
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
        [b"checks-failed repository=".as_slice(), encoded, b" exit=code:255 cut=18446744073709551615 tail=", encoded]
            .concat()
    );
    assert!(got.error);
    assert!(got.text.is_ascii());
}

#[test]
fn exact_text_cap_accepts_whole_result_and_one_byte_less_refuses() {
    assert_eq!(feedback(run::Returned::Accepted, 7), Err(FeedbackRefusal::TooLarge));
    assert_eq!(feedback(run::Returned::Accepted, 8), Ok(Feedback { text: bytes(b"accepted"), error: false }));
}
