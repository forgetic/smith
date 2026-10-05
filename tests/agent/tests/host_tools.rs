//! Real composed-domain opaque-host stories and positive-first outside-history
//! controls (domain/run.md, sections 5.2, 13 and 14; testing-strategy.md, section 7).

use skein_lib::{Duration, Time, Token};
use skein_world::domain::assert_replays;
use smith_agent_world::{
    HostSchedule, Job, Settings, World,
    host_referee::{History, Submission},
};
use smith_domain::run;

fn story(schedule: HostSchedule, seed: u64) -> World {
    let mut world = World::new(Settings { job: Job::HostTools, host: schedule, ..Settings::calm(seed) });
    world.run(20_000);
    assert!(matches!(world.answer(), run::Answer::Accepted { outcome: run::outcome::Declared::Report(_), .. }));
    assert!(!world.host_submissions().is_empty(), "real provider calls the offered opaque host declaration");
    assert!(world.checked().is_empty() && world.pushes().is_empty());
    world
}

#[test]
fn host_busy_lost_then_replayed_answer_preserves_opaque_input_name_scope_and_exact_text() {
    let world = story(HostSchedule::Replay, 812);
    let submissions = world.host_submissions();
    assert_eq!(submissions.len(), 3);
    assert_eq!(world.host_decisions(), 1, "lost committed response is replayed, never decided twice");
    let first = &submissions[0];
    for (index, submission) in submissions.iter().enumerate() {
        assert_eq!(
            (submission.worker, submission.name, &submission.tool, submission.effect, &submission.input),
            (first.worker, first.name, &first.tool, first.effect, &first.input)
        );
        assert_eq!(submission.relay.attempt, u32::try_from(index + 1).expect("three attempts"));
        assert!(submission.deadline <= submission.at.saturating_add(Duration::from_millis(300)));
    }
    assert_eq!(first.input.bytes(), br#" {"opaque":{"policy":"outside","numbers":[1,2]},"unchanged":"\u0041"} "#);
    assert!(world.prompts().iter().flat_map(|query| &query.messages).flat_map(|message| &message.parts).any(|part| matches!(part, smith_fake_llm_domain::api::Part::ToolOutput { output, is_error: false, .. } if output.as_ref() == b"opaque host answer: first decision")), "actual exact host text is continuation feedback");
    assert_replays(812, 816, |seed| {
        let world = story(HostSchedule::Replay, seed);
        (world.trace().to_vec(), format!("{:?} {:?}", world.host_submissions(), world.answer()))
    });
}

#[test]
fn exhausted_lost_then_busy_keeps_unknown_and_continues_to_report() {
    let world = story(HostSchedule::Unknown, 813);
    assert_eq!(world.host_submissions().len(), 3);
    assert!(world.prompts().iter().flat_map(|query| &query.messages).flat_map(|message| &message.parts).any(|part| matches!(part, smith_fake_llm_domain::api::Part::ToolOutput { output, is_error: true, .. } if output.windows(b"HostUnknown".len()).any(|part| part == b"HostUnknown"))));
}

#[test]
fn declared_timeout_requests_withdrawal_then_actual_terminal_allows_same_name_recovery() {
    let world = story(HostSchedule::Withdraw, 814);
    assert_eq!(world.host_submissions().len(), 2);
    assert!(world.trace().iter().any(|line| line.contains("WithdrawHost")));
    assert!(world.host_submissions()[1].at > world.host_submissions()[0].deadline);
}

#[test]
fn actual_answer_after_withdrawal_is_retained_without_another_relay() {
    let world = story(HostSchedule::LateAnswer, 815);
    assert_eq!(world.host_submissions().len(), 1);
    assert!(world.trace().iter().any(|line| line.contains("WithdrawHost")));
}

fn submission(at: u64, attempt: u32, owner: u64) -> Submission {
    Submission {
        worker: Token::new(91),
        relay: run::RelayName { owner: Token::new(owner), attempt },
        name: run::CallName { completion: 4, position: 2 },
        tool: b"opaque".as_slice().into(),
        effect: run::HostEffect::Write,
        input: run::HostInput::attested(b" {\"unchanged\":true} ".as_slice().into()).expect("attested fixture"),
        at: Time::ZERO.saturating_add(Duration::from_secs(at)),
        deadline: Time::ZERO.saturating_add(Duration::from_secs(at + 1)),
    }
}

fn positive_prefix() -> History {
    let mut history = History::default();
    history.called(b"host_call".as_slice().into()).expect("public provider ID");
    let first = submission(0, 1, 11);
    history.submit(first.clone()).expect("positive initial relay");
    history.withdraw(first.relay).expect("positive request retains relay");
    history
        .terminal(
            Time::ZERO.saturating_add(Duration::from_secs(1)),
            first.relay,
            &run::HostReply::Unanswered(run::Unanswered::Lost),
        )
        .expect("positive actual terminal");
    history
        .submit(submission(2, 2, 99))
        .expect("new callback after simulated translated activation restart retains logical worker and input");
    history
}

#[test]
fn outside_history_positive_control_and_chronology_identity_feedback_negative_controls() {
    let mut positive = positive_prefix();
    let second = submission(2, 2, 99);
    let answer = run::HostAnswer::new(b"first record".as_slice().into(), false).expect("small answer");
    positive
        .terminal(
            Time::ZERO.saturating_add(Duration::from_secs(3)),
            second.relay,
            &run::HostReply::Answered(answer.clone()),
        )
        .expect("positive answer replay");
    assert_eq!(positive.submit(submission(4, 3, 100)), Err("recovery after actual answer"));
    positive.feedback(&run::Returned::HostAnswered(answer.clone())).expect("closed positive exact feedback");
    let mut history = positive_prefix();
    assert_eq!(
        history.terminal(Time::ZERO.saturating_add(Duration::from_secs(1)), second.relay, &run::HostReply::Busy),
        Err("terminal precedes live admission")
    );
    assert_eq!(history.submit(submission(4, 3, 100)), Err("duplicate live relay"));
    assert_eq!(history.feedback(&run::Returned::HostUnknown), Err("feedback before terminal or twice"));
    history
        .terminal(Time::ZERO.saturating_add(Duration::from_secs(3)), second.relay, &run::HostReply::Busy)
        .expect("Busy only this attempt predecision");
    assert_eq!(history.feedback(&run::Returned::Busy), Err("feedback erased or fabricated host evidence"));
    assert_eq!(history.feedback(&run::Returned::Cancelled), Err("feedback erased or fabricated host evidence"));
    assert_eq!(
        history.feedback(&run::Returned::HostAnswered(answer)),
        Err("feedback erased or fabricated host evidence")
    );
    let mut changed = submission(4, 3, 100);
    changed.worker = Token::new(92);
    assert_eq!(history.submit(changed), Err("recovery changed immutable operation"));
    assert_eq!(history.submit(submission(3, 3, 100)), Err("recovery precedes terminal/backoff"));
    history.feedback(&run::Returned::HostUnknown).expect("closed uncertain positive control");
}

fn prompt_feedback(returned: Option<run::Returned>) -> smith_domain::llm::Prompt {
    use smith_domain::llm;
    let content: Box<[llm::Block]> = match returned {
        Some(returned) => Box::new([llm::Block::ToolResult {
            id: b"host_call".as_slice().into(),
            result: llm::Returned::Served { error: matches!(&returned, run::Returned::HostUnknown), returned },
        }]),
        None => Box::new([]),
    };
    llm::Prompt {
        endpoint: llm::Endpoint(0),
        model: b"fake".as_slice().into(),
        system: Box::new([]),
        tools: smith_domain::tools::Grants { inspect: false, modify: false, shell: false },
        served: Box::new([]),
        messages: Box::new([llm::Message { role: llm::Role::User, content }]),
        max_tokens: 100,
    }
}

#[test]
fn outside_known_and_unknown_feedback_cannot_be_omitted_or_rewritten_but_shutdown_can_skip_continuation() {
    for unknown in [false, true] {
        let mut history = positive_prefix();
        let answer = run::HostAnswer::new(b"actual first record".as_slice().into(), false).expect("bounded answer");
        let terminal = if unknown { run::HostReply::Busy } else { run::HostReply::Answered(answer.clone()) };
        history
            .terminal(Time::ZERO.saturating_add(Duration::from_secs(3)), submission(2, 2, 99).relay, &terminal)
            .expect("settled positive prefix");
        assert_eq!(history.finish(true), Err("continuation omitted host feedback"));
        assert_eq!(history.finish(false), Err("shutdown was not observed"));
        assert_eq!(history.prompt(&prompt_feedback(None)), Err("continuation omitted host feedback"));
        assert_eq!(history.prompt(&prompt_feedback(Some(run::Returned::Nothing))), Err("host feedback was rewritten"));
        let actual = if unknown { run::Returned::HostUnknown } else { run::Returned::HostAnswered(answer) };
        history.prompt(&prompt_feedback(Some(actual))).expect("closed positive exact paired feedback");
        history.finish(true).expect("full continuation positive control");
    }
}

#[test]
fn real_composed_cancellation_while_host_relay_is_live_retains_recorded_answer_and_stops_recovery() {
    let seed = 818;
    let baseline = story(HostSchedule::LateAnswer, seed);
    let first = &baseline.host_submissions()[0];
    let cut = first.at.saturating_since(Time::ZERO).saturating_add(Duration::from_millis(20));
    let mut cancelled = World::new(Settings {
        job: Job::HostTools,
        host: HostSchedule::LateAnswer,
        cancel_at: Some(cut),
        ..Settings::calm(seed)
    });
    cancelled.run(20_000);
    assert!(matches!(cancelled.answer(), run::Answer::Failed { failure: run::Failure::Cancelled, .. }));
    assert_eq!(cancelled.host_submissions().len(), 1, "no fresh recovery after actual cancellation");
    assert_eq!(cancelled.host_decisions(), 1);
    let [(relay, at, run::HostReply::Answered(answer))] = cancelled.host_terminals() else {
        panic!("exactly one actual recorded answer settles original relay");
    };
    assert_eq!(*relay, cancelled.host_submissions()[0].relay);
    assert_eq!(answer.text(), b"opaque host answer: first decision");
    let stopped = cancelled.host_shutdown_at().expect("actual parent cancellation observed");
    assert!(cancelled.host_submissions()[0].at < stopped && stopped < *at);
    assert_eq!(cancelled.prompts().len(), 1, "shutdown needs no next provider completion");
    assert!(cancelled.trace().iter().any(|line| line.contains("WithdrawHost")));
    assert!(cancelled.answered_at() >= *at, "run settles after actual terminal");
}

#[test]
fn outside_shutdown_control_requires_actual_stop_and_original_terminal() {
    let mut history = positive_prefix();
    history.shutdown(Time::ZERO.saturating_add(Duration::from_millis(2500)));
    assert_eq!(history.finish(false), Err("shutdown abandoned actual relay terminal"));
    history
        .terminal(Time::ZERO.saturating_add(Duration::from_secs(3)), submission(2, 2, 99).relay, &run::HostReply::Busy)
        .expect("actual original terminal");
    assert_eq!(history.submit(submission(4, 3, 100)), Err("recovery after observed shutdown"));
    history.finish(false).expect("closed actual shutdown control needs no continuation");
}
