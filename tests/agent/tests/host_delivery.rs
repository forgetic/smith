//! Actual root submissions cross the real host kit; parent operation, concrete
//! Turn commitment, Send and physical containment have independent rights.
//! Full Debug payloads are bounded test encodings, not production codecs.
//! The EOF control stops driving the simulated root after actual process exit:
//! it proves retained parent ownership, never a post-EOF receipt or root Turn.
//! Contract: domain/run.md, sections 8.2, 10 and 14; domain/host.md, sections 2,
//! 4.1, 6 and 9; testing-strategy.md, sections 2.3, 6 and 7.

use skein_lib::{Duration, Token};
use skein_world::domain::Span;
use smith_agent_world::{DeliverySubmission, Job, Settings, World as Agent, messages_referee::Seen};
use smith_domain::{run, session::llm};
use smith_host_domain::{self as host, Down, Event, Reply, RunResult, Up};
use smith_host_world::{Lower, World as Host};

const RECEIPT: &[u8] = b"landed\xff\0\"\\\n";
const FEEDBACK: &[u8] = b"delivered\nreceipt directory=0 text=\"landed\\xff\\x00\\x22\\x5c\\x0a\"";
const ID: &[u8] = b"call_0000000000000003";

fn receipt(text: &[u8]) -> host::Delivered {
    host::Delivered::new(Box::new([host::Receipt::new(0, text.into()).expect("bounded actual parent receipt")]))
        .expect("one changed writable directory")
}

fn root_receipt(receipts: &host::Delivered) -> run::Delivered {
    run::Delivered::new(
        receipts
            .receipts()
            .iter()
            .map(|receipt| {
                run::Receipt::new(receipt.directory(), receipt.text().into()).expect("same sealed receiving cap")
            })
            .collect(),
    )
    .expect("same complete unique changed-directory set")
}

fn setup() -> (Agent, Host) {
    let calm = Settings::calm(941);
    let agent = Agent::with_parent_deliveries(Settings {
        job: Job::MidReport,
        network: Span::millis(0, 0),
        provider: skein_fake_llm_domain::Config {
            latency_min: Duration::ZERO,
            latency_max: Duration::ZERO,
            ..calm.provider
        },
        ..calm
    });
    let mut host = Host::new(
        941,
        host::Limits {
            calls: 2,
            call_bytes: 4096,
            turns: 16,
            turn_bytes: 65_536,
            unacknowledged_bytes: 16 * 65_536,
            ..smith_host_world::limits()
        },
    );
    let mut start = smith_host_world::start();
    start.logical_run = Token::new(1);
    start.transcript = None;
    start.answered = Box::new([]);
    start.workspace = Some(Token::new(1));
    start.directories = Box::new([host::Directory {
        name: b"work".as_slice().into(),
        writable: true,
        git: true,
        conflicts: Box::new([]),
    }]);
    host.spawn(start);
    host.spawned();
    host.sent();
    (agent, host)
}

fn forward(agent: &Agent, host: &mut Host, next: &mut usize) {
    for (at, seen) in &agent.messages_seen()[*next..] {
        host.stage.tick(*at);
        match seen {
            Seen::Admitted => host.up(Up::Admitted),
            Seen::Turn { number, read, spent, turn } => {
                let body = format!("{turn:?}").into_bytes();
                assert!(body.len() <= 65_536);
                host.up(Up::Turn {
                    turn: host::Turn {
                        number: *number,
                        read: *read,
                        spent: spent.units,
                        spend_overflow: spent.units_overflow,
                        usage_overflow: spent.usage_overflow,
                        body: body.into(),
                    },
                });
                if *number == 1 {
                    host.event(Event::Acknowledge { agent: host.agent(), turn: 1 });
                    assert!(host.lower.contains(Lower::Send), "actual early ACK send is retained");
                }
            }
            Seen::Answer { .. }
            | Seen::Input { .. }
            | Seen::Bounced { .. }
            | Seen::Waiting { .. }
            | Seen::Prompt { .. }
            | Seen::Completed { .. }
            | Seen::CompletionEnded => {}
        }
    }
    *next = agent.messages_seen().len();
}

fn submitted(agent: &mut Agent, host: &mut Host, next: &mut usize) -> DeliverySubmission {
    for _ in 0..2000 {
        assert!(!agent.drive(1));
        forward(agent, host, next);
        if !agent.delivery_submissions().is_empty() {
            break;
        }
    }
    let [submission] = agent.delivery_submissions() else { panic!("one actual root delivery") };
    assert_eq!(submission.worker, Token::new(1));
    assert_eq!(submission.name, run::CallName { activation: 1, completion: 2, position: 0 });
    assert!(submission.at < submission.deadline);
    let [field] = submission.change.fields.as_ref() else { panic!("one whole caller field") };
    assert_eq!(field.name.as_ref(), b"ticket");
    assert_eq!(field.value.as_ref(), b"opaque-host-value");
    assert!(field.value.len() <= 128, "actual charter field cap");
    assert_eq!(agent.checked(), [true]);
    assert_eq!(agent.code(), b"pub fn answer() -> u32 { 43 }\n");
    let fields = format!("{:?}", submission.change).into_bytes();
    assert_eq!(
        fields,
        format!("Change {{ fields: [Field {{ name: {:?}, value: {:?} }}] }}", b"ticket", b"opaque-host-value")
            .into_bytes()
    );
    assert!(fields.len() <= 4096, "actual host receiving cap");
    let expected_called = format!(
        "down Called {{ client: {:?}, logical_run: {:?}, call: {:?}, name: {:?}, deadline: {:?}, ask: Deliver {{ fields: {:?} }} }}",
        Token::new(1),
        Token::new(1),
        submission.owner,
        host::CallName { activation: 1, completion: 2, position: 0 },
        submission.deadline,
        fields
    );
    host.up(Up::Call {
        call: submission.owner,
        name: host::CallName {
            activation: submission.name.activation,
            completion: submission.name.completion,
            position: submission.name.position,
        },
        deadline: submission.deadline,
        ask: host::Ask::Deliver { fields: fields.into() },
    });
    assert!(
        host.trace.lines().iter().any(|line| line.ends_with(&expected_called)),
        "real parent Called preserves complete observed scope/name/deadline/fields: {:?}",
        host.trace.lines()
    );
    assert!(host.seen.calls.contains(&submission.owner), "actual Called owns the parent right");
    assert!(host.lower.contains(Lower::Send), "earlier actual Turn ACK Send still coexists");
    assert!(agent.pushes().is_empty(), "submission never auto-answers the operation");
    submission.clone()
}

fn cancel(agent: &mut Agent, host: &mut Host, submission: &DeliverySubmission, next: &mut usize) {
    host.event(Event::Stop { agent: host.agent() });
    assert!(host.seen.calls.contains(&submission.owner));
    host.sent(); // Completes the earlier ACK; now the real Cancel Send is issued.
    assert!(matches!(host.seen.down.last(), Some(Down::Cancel)));
    agent.cancel_run();
    host.sent();
    for _ in 0..16 {
        assert!(!agent.drive(1), "cancel cannot invent the submitted delivery terminal");
        forward(agent, host, next);
    }
    assert!(agent.pushes().is_empty());
    assert!(host.seen.calls.contains(&submission.owner), "Cancel/ACK/Send cannot consume parent operation");
}

fn final_answer(agent: &Agent) -> host::Answer {
    let run::Answer::Failed { failure: run::Failure::Cancelled, turns, spent } = agent.answer() else {
        panic!("the run keeps its decided cancellation after delivery settles")
    };
    assert!(agent.turns().iter().flat_map(|turn| &turn.messages).flat_map(|message| &message.content).any(|block| {
        matches!(block, llm::Block::ToolResult { id, result: llm::Returned::Text { text, error: false, replay: None } }
            if id.as_ref() == ID && text.as_ref() == FEEDBACK)
    }), "the concrete Turn keeps paired opaque receipt feedback");
    host::Answer {
        turns: *turns,
        completions: spent.turns,
        input: spent.input,
        output: spent.output,
        cache_read: spent.cache_read,
        cache_write: spent.cache_write,
        spent: spent.units,
        spend_overflow: spent.units_overflow,
        usage_overflow: spent.usage_overflow,
        result: RunResult::Failed { failure: host::RunFailure::Cancelled },
    }
}

fn finish_host(host: &mut Host) {
    assert!(host.seen.answer.is_some(), "the settled run's last word reaches the host");
    assert!(host.seen.turns.is_empty());
    assert!(host.lower.contains(Lower::Send));
    assert!(matches!(host.seen.down.last(), Some(Down::Acknowledge { turn: 2 })));
    host.event(Event::Exited { owner: host.owner() });
    host.event(Event::Reaped { owner: host.owner(), detail: b"actual empty tree".as_slice().into() });
    host.event(Event::Hangup { owner: host.owner() });
    assert!(host.seen.exited && host.seen.empty && host.seen.eof);
    assert!(host.seen.gone.is_none(), "the final ACK Send terminal is still owed");
    host.sent();
    assert_eq!(host.seen.gone, Some(host::End::Stopped));
    host.settled();
}

fn live_story() {
    let (mut agent, mut host) = setup();
    let mut next = 0;
    let submission = submitted(&mut agent, &mut host, &mut next);
    cancel(&mut agent, &mut host, &submission, &mut next);
    assert_eq!(
        agent.return_delivery(Token::new(u64::MAX), run::Delivery::Nothing),
        Err("no actual parent delivery right")
    );
    host.event(Event::Answer {
        agent: host.agent(),
        call: submission.owner,
        reply: Reply::Delivery(host::Delivery::Delivered(receipt(RECEIPT))),
    });
    assert!(host.seen.calls.is_empty(), "only actual parent Reply consumes its operation right");
    let Some(Down::Answer { call, reply: Reply::Delivery(host::Delivery::Delivered(receipts)) }) =
        host.seen.down.last()
    else {
        panic!("one actual issued delivery response")
    };
    assert_eq!(*call, submission.owner);
    assert_eq!(receipts.receipts()[0].text(), RECEIPT);
    agent
        .return_delivery(*call, run::Delivery::Delivered(root_receipt(receipts)))
        .expect("actual parent response reaches root");
    assert_eq!(agent.return_delivery(*call, run::Delivery::Nothing), Err("actual parent terminal is already queued"));
    let mut done = false;
    for _ in 0..2000 {
        done = agent.drive(1);
        forward(&agent, &mut host, &mut next);
        if done {
            break;
        }
    }
    assert!(done, "real root and all its lower operations settle");
    assert_eq!(agent.delivery_submissions().len(), 1);
    assert_eq!(agent.pushes().len(), 1);
    assert_eq!(agent.return_delivery(submission.owner, run::Delivery::Nothing), Err("no actual parent delivery right"));
    let answer = final_answer(&agent);
    assert!(host.lower.contains(Lower::Send), "actual reply Send remains outstanding through the final Turn");
    let sends = host.seen.down.len();
    host.event(Event::Acknowledge { agent: host.agent(), turn: 2 });
    assert!(host.seen.turns.is_empty(), "parent committed the exact final Turn while the channel listens");
    assert_eq!(host.seen.down.len(), sends, "final ACK queues behind the actual held reply Send");
    host.sent(); // Actual reply terminal releases the queued final ACK Send.
    assert_eq!(host.seen.down.len(), sends + 1);
    assert!(matches!(host.seen.down.last(), Some(Down::Acknowledge { turn: 2 })));
    assert!(host.lower.contains(Lower::Send), "issued final ACK Send precedes the last word");
    host.up(Up::Answer { answer });
    finish_host(&mut host);
}

#[test]
fn actual_root_delivery_receipt_turn_and_host_rights_settle_after_cancel() {
    live_story();
}

#[test]
fn actual_root_submission_parent_right_outlives_exit_tree_eof_without_fabricated_return() {
    let (mut agent, mut host) = setup();
    let mut next = 0;
    let submission = submitted(&mut agent, &mut host, &mut next);
    cancel(&mut agent, &mut host, &submission, &mut next);
    let turns = agent.turns().len();
    host.event(Event::Exited { owner: host.owner() });
    // The simulated root is deliberately never driven again after this actual
    // process exit. This branch claims no root terminal or post-EOF Turn.
    assert!(host.seen.withdrawals.contains(&submission.owner));
    assert!(host.seen.calls.contains(&submission.owner));
    host.event(Event::Reaped { owner: host.owner(), detail: b"actual empty tree".as_slice().into() });
    host.event(Event::Hangup { owner: host.owner() });
    assert!(host.seen.exited && host.seen.empty && host.seen.eof);
    assert!(host.seen.turns.is_empty() && !host.lower.contains(Lower::Send));
    assert!(host.seen.gone.is_none(), "withdrawal/tree/EOF/ACK/Send cannot consume parent operation");
    host.event(Event::Answer {
        agent: host.agent(),
        call: submission.owner,
        reply: Reply::Delivery(host::Delivery::Delivered(receipt(RECEIPT))),
    });
    assert!(host.seen.calls.is_empty());
    assert!(
        !host.seen.down.iter().any(|message| matches!(message, Down::Answer { .. })),
        "lost channel gets no fabricated response"
    );
    assert_eq!(agent.turns().len(), turns);
    assert!(agent.pushes().is_empty(), "no post-exit root receipt was invented");
    assert_eq!(host.seen.gone, Some(host::End::Stopped));
    host.settled();
}
