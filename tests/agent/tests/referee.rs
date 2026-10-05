//! Deliberately bad boundary transcripts must fail the independent referee
//! (testing-strategy.md, section 7; domain/run.md, section 14).

use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Referee, Verdict};
use smith_agent_world::referee::{Meeting, Seen};
use smith_domain::run::{
    Answer, Exit, Push, Refusal, Spend,
    outcome::{Change, ChangeSpec, Declared, DeclaredFailure, Field, FieldRule, OutcomeSpec, Report, TextSpec},
};

fn started() -> Referee<Meeting> {
    let mut referee = Referee::new(Meeting::default());
    referee.observe(
        Time::ZERO,
        Seen::Started {
            contract: OutcomeSpec {
                change: Some(ChangeSpec { checks: true, fields: Box::new([]) }),
                verdicts: Box::new([]),
                report: None,
                failure: None,
            },
            outcome_bytes: 1024,
            checks: true,
            within: Duration::from_secs(5),
        },
        &mut Vec::new(),
    );
    referee
}

fn broken(mut referee: Referee<Meeting>, seen: Seen, reason: &str) {
    referee.observe(Time::ZERO, seen, &mut Vec::new());
    let Verdict::Failed(failure) = referee.verdict() else { panic!("the bad transcript must fail") };
    assert_eq!(failure.why, reason);
}

#[test]
fn a_push_without_passing_checks_is_rejected() {
    broken(started(), Seen::Pushing { owner: Token::new(2), tree: b"code".to_vec() }, "checks pass before each push");
}

#[test]
fn a_host_that_lands_different_bytes_is_rejected() {
    let mut referee = started();
    referee.observe(Time::ZERO, Seen::Checked { owner: Token::new(2), exit: Exit::Code { code: 0 } }, &mut Vec::new());
    referee.observe(Time::ZERO, Seen::Pushing { owner: Token::new(2), tree: b"asked".to_vec() }, &mut Vec::new());
    broken(
        referee,
        Seen::Pushed { owner: Token::new(2), push: Push::Done, tree: b"other".to_vec() },
        "the host lands exactly the tree the agent left",
    );
}

#[test]
fn a_change_answer_without_a_landed_push_is_rejected() {
    let answer = Answer::Accepted {
        outcome: Declared::Change(Change {
            fields: Box::new([
                smith_domain::run::outcome::Field {
                    name: b"title".as_slice().into(),
                    value: b"change".as_slice().into(),
                },
                smith_domain::run::outcome::Field { name: b"body".as_slice().into(), value: Box::new([]) },
            ]),
        }),
        spent: Spend::ZERO,
    };
    broken(started(), Seen::Answered { answer, pending: 0 }, "an accepted change landed exactly once");
}

#[test]
fn an_answer_while_a_terminal_is_owed_is_rejected() {
    broken(
        started(),
        Seen::Answered { answer: Answer::Refused(Refusal::Busy), pending: 1 },
        "an answer waits for every request terminal",
    );
}

#[test]
fn an_answer_that_omits_a_provider_turn_is_rejected() {
    let mut referee = started();
    referee.observe(Time::ZERO, Seen::Completing { owner: Token::new(3) }, &mut Vec::new());
    referee.observe(
        Time::ZERO,
        Seen::Completed { owner: Token::new(3), spent: Spend { turns: 1, input: 7, ..Spend::ZERO } },
        &mut Vec::new(),
    );
    broken(
        referee,
        Seen::Answered { answer: Answer::Refused(Refusal::Busy), pending: 0 },
        "the answer accounts for every accepted provider turn exactly once",
    );
}

#[test]
fn a_missing_host_answer_expires_the_liveness_obligation() {
    let mut referee = started();
    referee.fire(Time::ZERO.saturating_add(Duration::from_secs(6)), &mut Vec::new());
    assert!(matches!(referee.verdict(), Verdict::Failed(_)), "the host answer deadline is real");
}

#[test]
fn a_duplicate_answer_is_rejected() {
    let mut referee = started();
    referee.observe(Time::ZERO, Seen::Answered { answer: Answer::Refused(Refusal::Busy), pending: 0 }, &mut Vec::new());
    broken(
        referee,
        Seen::Answered { answer: Answer::Refused(Refusal::Busy), pending: 0 },
        "exactly one answer per host start",
    );
}

fn text_started(failure: bool, cap: u64) -> Referee<Meeting> {
    let mut referee = Referee::new(Meeting::default());
    let rule = TextSpec {
        min: u32::from(failure),
        max: 8,
        fields: Box::new([FieldRule { name: b"source".as_slice().into(), max: 3 }]),
    };
    referee.observe(
        Time::ZERO,
        Seen::Started {
            contract: OutcomeSpec {
                change: None,
                verdicts: Box::new([]),
                report: if failure { None } else { Some(rule.clone()) },
                failure: if failure { Some(rule) } else { None },
            },
            outcome_bytes: cap,
            checks: false,
            within: Duration::from_secs(5),
        },
        &mut Vec::new(),
    );
    referee
}

#[test]
fn an_accepted_report_that_omits_the_hosts_field_is_rejected() {
    broken(
        text_started(false, 1024),
        Seen::Answered {
            answer: Answer::Accepted {
                outcome: Declared::Report(Report { text: Box::new([]), fields: Box::new([]) }),
                spent: Spend::ZERO,
            },
            pending: 0,
        },
        "an accepted result meets the host contract and byte cap",
    );
}

#[test]
fn an_accepted_failure_with_no_required_reason_is_rejected() {
    broken(
        text_started(true, 1024),
        Seen::Answered {
            answer: Answer::Accepted {
                outcome: Declared::Failure(DeclaredFailure {
                    reason: Box::new([]),
                    fields: Box::new([Field { name: b"source".as_slice().into(), value: b"ref".as_slice().into() }]),
                }),
                spent: Spend::ZERO,
            },
            pending: 0,
        },
        "an accepted result meets the host contract and byte cap",
    );
}

#[test]
fn extra_field_storage_cannot_be_omitted_from_the_referees_byte_charge() {
    broken(
        text_started(false, 64),
        Seen::Answered {
            answer: Answer::Accepted {
                outcome: Declared::Report(Report {
                    text: Box::new([]),
                    fields: Box::new([
                        Field { name: b"source".as_slice().into(), value: b"ref".as_slice().into() },
                        Field { name: b"extra".as_slice().into(), value: vec![b'x'; 64].into() },
                    ]),
                }),
                spent: Spend::ZERO,
            },
            pending: 0,
        },
        "an accepted result meets the host contract and byte cap",
    );
}
