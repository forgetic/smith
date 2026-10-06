//! Deliberately bad boundary transcripts must fail the independent referee
//! (testing-strategy.md, section 7; domain/run.md, section 14).

use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Referee, Verdict};
use smith_agent_world::referee::{Meeting, Seen};
use smith_domain::run::{
    Answer, Exit, Refusal, Spend,
    outcome::{Change, ChangeSpec, Declared, DeclaredFailure, Field, FieldRule, OutcomeSpec, Report, TextSpec},
};

fn started() -> Referee<Meeting> {
    let mut referee = Referee::new(Meeting::default());
    referee.observe(
        Time::ZERO,
        Seen::Started {
            delivery: false,
            contract: OutcomeSpec {
                change: Some(ChangeSpec { checks_must_pass: true, fields: Box::new([]) }),
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
    broken(
        started(),
        Seen::Pushing {
            finishing: true,
            name: smith_domain::run::CallName { activation: 1, completion: 1, position: 0 },
            owner: Token::new(2),
            tree: b"code".to_vec(),
        },
        "delivery is the exclusive checked snapshot",
    );
}

#[test]
fn a_host_that_lands_different_bytes_is_rejected() {
    let mut referee = started();
    referee.observe(Time::ZERO, Seen::Checking { owner: Token::new(2), tree: b"asked".to_vec() }, &mut Vec::new());
    referee.observe(Time::ZERO, Seen::Checked { owner: Token::new(2), exit: Exit::Code { code: 0 } }, &mut Vec::new());
    referee.observe(
        Time::ZERO,
        Seen::Pushing {
            finishing: true,
            name: smith_domain::run::CallName { activation: 1, completion: 1, position: 0 },
            owner: Token::new(2),
            tree: b"asked".to_vec(),
        },
        &mut Vec::new(),
    );
    broken(
        referee,
        Seen::Delivered { owner: Token::new(2), push: smith_agent_world::delivered(), tree: b"other".to_vec() },
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
        turns: 0,
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
    assert!(matches!(referee.verdict(), Verdict::Failed(_)), "the host answer deadline is enforced");
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
    let rule = TextSpec { max: 8, fields: Box::new([FieldRule { name: b"source".as_slice().into(), max: 3 }]) };
    referee.observe(
        Time::ZERO,
        Seen::Started {
            delivery: false,
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
                turns: 0,
            },
            pending: 0,
        },
        "an accepted result meets the host contract and byte cap",
    );
}

#[test]
fn empty_failure_reason_with_required_field_is_accepted() {
    let mut referee = text_started(true, 1024);
    referee.observe(
        Time::ZERO,
        Seen::Answered {
            answer: Answer::Accepted {
                outcome: Declared::Failure(DeclaredFailure {
                    reason: Box::new([]),
                    fields: Box::new([Field { name: b"source".as_slice().into(), value: b"ref".as_slice().into() }]),
                }),
                spent: Spend::ZERO,
                turns: 0,
            },
            pending: 0,
        },
        &mut Vec::new(),
    );
    assert_eq!(referee.judged().1, 1);
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
                turns: 0,
            },
            pending: 0,
        },
        "an accepted result meets the host contract and byte cap",
    );
}

fn mid_history() -> Referee<Meeting> {
    let mut referee = Referee::new(Meeting::default());
    let owner = Token::new(70);
    let name = smith_domain::run::CallName { activation: 1, completion: 2, position: 1 };
    let observations = [
        Seen::Started {
            delivery: true,
            checks: true,
            within: Duration::from_secs(5),
            outcome_bytes: 1024,
            contract: OutcomeSpec {
                change: None,
                verdicts: Box::new([]),
                failure: None,
                report: Some(TextSpec { max: 32, fields: Box::new([]) }),
            },
        },
        Seen::Checking { owner, tree: b"checked".to_vec() },
        Seen::Checked { owner, exit: Exit::Code { code: 0 } },
        Seen::Pushing { finishing: false, owner, name, tree: b"checked".to_vec() },
    ];
    for observation in observations {
        referee.observe(Time::ZERO, observation, &mut Vec::new());
    }
    referee.observe(
        Time::ZERO,
        Seen::Delivered { owner, push: smith_agent_world::delivered(), tree: b"checked".to_vec() },
        &mut Vec::new(),
    );
    pending_host_answer(&referee);
    referee
}

fn pending_host_answer(referee: &Referee<Meeting>) {
    let Verdict::Open { pending } = referee.verdict() else {
        panic!("the positive prefix has no failure and still owes its host answer")
    };
    assert_eq!(pending.len(), 1, "exactly the start's answer obligation remains");
    assert!(pending[0].contains("host answer"));
}

#[test]
fn both_ordinary_and_interrupted_mid_delivery_histories_are_valid() {
    for interrupted in [false, true] {
        let mut referee = mid_history();
        let answer = if interrupted {
            Answer::Failed { failure: smith_domain::run::Failure::Cancelled, spent: Spend::ZERO, turns: 0 }
        } else {
            Answer::Accepted {
                outcome: Declared::Report(Report { text: b"continued".as_slice().into(), fields: Box::new([]) }),
                spent: Spend::ZERO,
                turns: 0,
            }
        };
        referee.observe(Time::ZERO, Seen::Answered { answer, pending: 0 }, &mut Vec::new());
        assert_eq!(referee.verdict(), Verdict::Passed);
        assert_eq!(referee.judged().1, 1, "one final answer obligation is actually met");
    }
}

#[test]
fn an_ordinary_mid_report_cannot_invent_a_forbidden_final_change() {
    broken(
        mid_history(),
        Seen::Answered {
            pending: 0,
            answer: Answer::Accepted {
                outcome: Declared::Change(Change { fields: Box::new([]) }),
                spent: Spend::ZERO,
                turns: 0,
            },
        },
        "an accepted result meets the host contract and byte cap",
    );
}

#[test]
fn duplicate_terminal_and_reused_durable_name_are_rejected_after_positive_prefix() {
    broken(
        mid_history(),
        Seen::Delivered { owner: Token::new(70), push: smith_agent_world::delivered(), tree: b"checked".to_vec() },
        "a host push terminal names a pending push",
    );
    let mut referee = mid_history();
    let owner = Token::new(71);
    referee.observe(Time::ZERO, Seen::Checking { owner, tree: b"checked".to_vec() }, &mut Vec::new());
    referee.observe(Time::ZERO, Seen::Checked { owner, exit: Exit::Code { code: 0 } }, &mut Vec::new());
    broken(
        referee,
        Seen::Pushing {
            finishing: false,
            owner,
            name: smith_domain::run::CallName { activation: 1, completion: 2, position: 1 },
            tree: b"checked".to_vec(),
        },
        "durable names are nonzero and distinct across actual submissions",
    );
}

#[test]
fn changed_bytes_between_check_and_submission_are_rejected_after_positive_check() {
    let mut referee = started();
    let owner = Token::new(72);
    referee.observe(Time::ZERO, Seen::Checking { owner, tree: b"checked".to_vec() }, &mut Vec::new());
    referee.observe(Time::ZERO, Seen::Checked { owner, exit: Exit::Code { code: 0 } }, &mut Vec::new());
    pending_host_answer(&referee);
    broken(
        referee,
        Seen::Pushing {
            finishing: true,
            owner,
            name: smith_domain::run::CallName { activation: 1, completion: 4, position: 0 },
            tree: b"later write".to_vec(),
        },
        "delivery is the exclusive checked snapshot",
    );
}
