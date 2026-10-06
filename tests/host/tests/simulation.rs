//! Preserved Temper 19735a06 supervision stories, entirely through V2 boundary
//! observations (domain/host.md, section 10; testing-strategy.md, sections 2.2, 6).
use skein_lib::{Duration, Time, Token};
use smith_host_domain::{
    Answer, Ask, Bounce, CallName, Delivered, Delivery, Down, Effect, End, Event, Fault, Grant, Invalid, ModelFault,
    Receipt, Reply, RunFailure, RunResult, Signal, Turn, Up,
};
use smith_host_world::{Lower, World, limits, start};

fn call(world: &mut World, callback: u64, completion: u32, delivery: bool, deadline: u64) {
    let ask = if delivery {
        Ask::Deliver { fields: Box::from(&b"generic fields"[..]) }
    } else {
        Ask::Host { tool: Box::from(&b"tool"[..]), effect: Effect::Write, body: Box::from(&b"request"[..]) }
    };
    world.up(Up::Call {
        call: Token::new(callback),
        name: CallName { completion, position: 2 },
        deadline: Time::ZERO.saturating_add(Duration::from_secs(deadline)),
        ask,
    });
}
fn reply(world: &mut World, callback: u64, response: Reply) {
    world.event(Event::Answer { agent: world.agent(), call: Token::new(callback), reply: response });
}
fn last(world: &mut World, result: RunResult, turns: u32, spent: u64) {
    let completions = turns.max(u32::from(spent > 0));
    world.up(Up::Answer {
        answer: Answer {
            turns,
            completions,
            input: u64::from(completions) * 11,
            output: u64::from(completions) * 13,
            cache_read: u64::from(completions) * 17,
            cache_write: u64::from(completions) * 19,
            spent,
            spend_overflow: false,
            usage_overflow: false,
            result,
        },
    });
}
fn finish(world: &mut World) {
    last(world, RunResult::Parked, 0, 0);
    world.cleanup();
    world.settled();
}
fn faulted(world: &mut World, fault: Fault) {
    assert_eq!(world.seen.fault, Some(fault));
    world.cleanup();
    world.settled();
}
fn silently_faulted(world: &mut World, fault: Fault) {
    assert_eq!(world.seen.fault, None, "explicit Stop already owns shutdown");
    assert!(world.seen.answer.is_none());
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    let mut found = false;
    while let Some(fact) = world.domain.pop_fact() {
        match fact {
            smith_host_domain::Fact::Faulted { fault: actual, .. } => {
                assert_eq!(actual, fault);
                found = true;
            }
            smith_host_domain::Fact::Started { .. }
            | smith_host_domain::Fact::Admitted { .. }
            | smith_host_domain::Fact::Answered { .. }
            | smith_host_domain::Fact::Gone { .. } => {}
        }
    }
    assert!(found, "diagnostic retains the actual breach");
    world.cleanup();
    world.settled();
}

fn receipts(text: &[u8]) -> Delivered {
    Delivered::new(Box::new([Receipt::new(0, Box::from(text)).expect("valid receipt")])).expect("valid sealed delivery")
}
fn turn(world: &mut World, number: u32, spent: u64, read: Option<Token>, bytes: usize) {
    world.up(Up::Turn {
        turn: Turn {
            number,
            spent,
            spend_overflow: false,
            usage_overflow: false,
            read,
            body: vec![b't'; bytes].into_boxed_slice(),
        },
    });
}
fn message(world: &mut World, name: u64, bytes: usize) {
    world.event(Event::Message {
        agent: world.agent(),
        name: Token::new(name),
        body: vec![b'm'; bytes].into_boxed_slice(),
    });
}
fn grant(world: &mut World, generation: u64) {
    world.event(Event::Grant {
        agent: world.agent(),
        grant: Grant { account: 1, generation, valid: Duration::from_secs(60) },
    });
}
fn count_answers(world: &World, wanted: Token) -> usize {
    world
        .seen
        .down
        .iter()
        .filter(|message| match message {
            Down::Answer { call, .. } => *call == wanted,
            Down::Start { .. }
            | Down::Message { .. }
            | Down::Acknowledge { .. }
            | Down::Grant { .. }
            | Down::Cancel => false,
        })
        .count()
}

#[test]
fn process_started_precedes_agent_admitted_and_start_is_first() {
    let mut world = World::new(1, limits());
    world.spawn(start());
    assert!(world.seen.agent.is_none());
    assert!(!world.seen.admitted);
    // A guessed lower token before Started is not a legitimate parent handle.
    world.event(Event::Message { agent: world.owner(), name: Token::new(2), body: Box::from(&b"early"[..]) });
    assert_eq!(world.seen.bounces, [Bounce::Ending]);
    world.spawned();
    assert!(!world.seen.admitted);
    match world.seen.down.first().expect("Start issued") {
        Down::Start { start: actual } => {
            assert_eq!(actual.charter, start().charter);
            assert_eq!(actual.transcript, start().transcript);
            assert_eq!(actual.answered, start().answered);
            assert_eq!(actual.workspace, None);
        }
        Down::Message { .. } | Down::Answer { .. } | Down::Acknowledge { .. } | Down::Grant { .. } | Down::Cancel => {
            panic!("Start must be first")
        }
    }
    // Messages may queue while Start is still owned by the lower Send.
    for name in 11..14 {
        message(&mut world, name, 64);
    }
    assert_eq!(world.seen.down.len(), 1);
    world.sent();
    world.up(Up::Admitted);
    assert!(world.seen.admitted);
    for _ in 0..3 {
        world.sent();
    }
    finish(&mut world);
}

#[test]
fn admission_refusals_preserve_separate_process_rights() {
    for field in 0..5 {
        let mut world = World::new(field, limits());
        let mut request = start();
        let invalid = match field {
            0 => {
                request.charter = vec![0; 257].into_boxed_slice();
                Invalid::Charter
            }
            1 => {
                request.transcript = Some(vec![0; 257].into_boxed_slice());
                Invalid::Transcript
            }
            2 => {
                request.answered = vec![0; 257].into_boxed_slice();
                Invalid::Answered
            }
            3 => {
                request.grants = Box::new([Grant { account: 1, generation: 0, valid: Duration::ZERO }]);
                Invalid::Grants
            }
            4 => {
                request.directories = Box::new([smith_host_domain::Directory {
                    name: Box::from(&b"a"[..]),
                    writable: false,
                    git: false,
                    conflicts: Box::new([Box::from(&b"file"[..])]),
                }]);
                Invalid::Directories
            }
            _ => unreachable!(),
        };
        world.spawn(request);
        assert_eq!(world.seen.gone, Some(End::Invalid(invalid)));
        world.settled();
    }
    let mut bounds = limits();
    bounds.agents = 0;
    let mut world = World::new(9, bounds);
    world.spawn(start());
    assert_eq!(world.seen.gone, Some(End::Busy));
    world.settled();
    let mut world = World::new(10, limits());
    world.spawn(start());
    world.event(Event::Unspawned { owner: world.owner(), detail: (0..100u8).collect::<Vec<u8>>().into_boxed_slice() });
    assert_eq!(world.seen.gone, Some(End::Unspawned));
    assert_eq!(world.seen.gone_detail.as_deref(), Some((68..100).collect::<Vec<u8>>().as_slice()));
    world.settled();
}

#[test]
fn admission_refusal_and_working_traffic_are_distinct() {
    let mut world = World::new(11, limits());
    world.spawn(start());
    world.spawned();
    world.sent();
    last(&mut world, RunResult::Refused { detail: Box::from(&b"refused"[..]) }, 0, 0);
    world.cleanup();
    world.settled();
    for record in [Up::Fact { body: Box::new([]) }, Up::Admitted] {
        let mut world = World::new(12, limits());
        world.live();
        world.up(record);
        if world.seen.fault.is_some() {
            faulted(&mut world, Fault::Rules);
        } else {
            finish(&mut world);
        }
    }
    let mut world = World::new(13, limits());
    world.spawn(start());
    world.spawned();
    world.sent();
    world.up(Up::Fact { body: Box::new([]) });
    faulted(&mut world, Fault::Rules);
}

#[test]
fn a_live_run_calls_tells_waits_and_finishes() {
    let mut world = World::new(14, limits());
    world.live();
    call(&mut world, 20, 1, false, 100);
    world.up(Up::Fact { body: Box::from(&b"opaque fact"[..]) });
    assert_eq!(world.seen.told, 1);
    world.up(Up::Waiting { read: None });
    world.at(40);
    assert_eq!(world.seen.fault, None);
    reply(&mut world, 20, Reply::Host { error: false, body: Box::from(&b"result"[..]) });
    world.sent();
    last(&mut world, RunResult::Accepted { outcome: Box::from(&b"accepted"[..]) }, 0, 9);
    world.cleanup();
    world.settled();
}

#[test]
fn a_run_may_fail_as_it_reports_it() {
    for failure in [
        RunFailure::Model(ModelFault::Provider),
        RunFailure::Budget(smith_host_domain::Exhausted::Time),
        RunFailure::Budget(smith_host_domain::Exhausted::Turns),
        RunFailure::Budget(smith_host_domain::Exhausted::Spend),
        RunFailure::Receiving(smith_host_domain::ReceivingLimit::Input),
        RunFailure::Receiving(smith_host_domain::ReceivingLimit::Output),
        RunFailure::Receiving(smith_host_domain::ReceivingLimit::CacheRead),
        RunFailure::Receiving(smith_host_domain::ReceivingLimit::CacheWrite),
        RunFailure::Policy(smith_host_domain::Policy::Unfinished { nudges: 2, rejected: 3 }),
        RunFailure::Stale,
    ] {
        let mut world = World::new(15, limits());
        world.live();
        last(&mut world, RunResult::Failed { failure }, 0, 10);
        assert_eq!(world.seen.answer.as_ref().expect("last word").result, RunResult::Failed { failure });
        world.cleanup();
        world.settled();
    }
}

#[test]
fn busy_overflow_reserves_names_and_pauses_reads_until_actual_send_terminal() {
    let mut bounds = limits();
    bounds.calls = 1;
    let mut world = World::new(16, bounds);
    world.live();
    call(&mut world, 20, 1, false, 100);
    call(&mut world, 21, 2, false, 100);
    call(&mut world, 22, 3, false, 100);
    call(&mut world, 23, 4, false, 100);
    assert!(!world.lower.contains(Lower::Read));
    assert_eq!(world.seen.calls.len(), 1);
    world.sent();
    assert!(world.lower.contains(Lower::Read));
    world.sent();
    world.sent();
    reply(&mut world, 20, Reply::Unavailable);
    world.sent();
    finish(&mut world);
}

#[test]
fn durable_call_names_and_callback_owners_are_independently_fenced() {
    for (callback, name) in [(20, 2), (21, 1), (21, 0)] {
        let mut world = World::new(17, limits());
        world.live();
        call(&mut world, 20, 1, false, 100);
        call(&mut world, callback, name, false, 100);
        assert_eq!(world.seen.fault, Some(Fault::Rules));
        reply(&mut world, 20, Reply::Unavailable);
        world.cleanup();
        world.settled();
    }
    let mut world = World::new(18, limits());
    world.live();
    call(&mut world, 20, 1, false, 100);
    reply(&mut world, 20, Reply::Host { error: false, body: Box::from(&b"durable replay"[..]) });
    world.sent();
    call(&mut world, 21, 1, false, 100);
    assert!(world.seen.calls.contains(&Token::new(21)));
    reply(&mut world, 21, Reply::Host { error: false, body: Box::from(&b"durable replay"[..]) });
    world.sent();
    finish(&mut world);
}

#[test]
fn an_overflow_name_is_fenced_until_its_busy_answer_terminal_then_reusable() {
    for settle in [false, true] {
        let mut bounds = limits();
        bounds.calls = 1;
        let mut world = World::new(19, bounds);
        world.live();
        call(&mut world, 20, 1, false, 100);
        call(&mut world, 21, 2, false, 100);
        if settle {
            world.sent();
        }
        call(&mut world, 22, 2, false, 100);
        if settle {
            assert_eq!(world.seen.fault, None);
            world.sent();
        } else {
            assert_eq!(world.seen.fault, Some(Fault::Rules));
        }
        reply(&mut world, 20, Reply::Unavailable);
        if settle {
            world.sent();
            finish(&mut world);
        } else {
            world.cleanup();
            world.settled();
        }
    }
}

#[test]
fn payloads_beyond_the_limits_break_the_rules() {
    for record in [
        Up::Fact { body: vec![0; 65].into_boxed_slice() },
        Up::Turn {
            turn: Turn {
                number: 1,
                spent: 0,
                spend_overflow: false,
                usage_overflow: false,
                read: None,
                body: vec![0; 65].into_boxed_slice(),
            },
        },
        Up::Call {
            call: Token::new(20),
            name: CallName { completion: 1, position: 1 },
            deadline: Time::ZERO,
            ask: Ask::Deliver { fields: vec![0; 129].into_boxed_slice() },
        },
        Up::Answer {
            answer: Answer {
                turns: 0,
                completions: 0,
                input: 0,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                spent: 0,
                spend_overflow: false,
                usage_overflow: false,
                result: RunResult::Accepted { outcome: vec![0; 129].into_boxed_slice() },
            },
        },
    ] {
        let mut world = World::new(20, limits());
        world.live();
        world.up(record);
        faulted(&mut world, Fault::TooLarge);
    }
}

#[test]
fn malformed_hangup_and_unsent_have_actual_lower_terminals() {
    let mut world = World::new(21, limits());
    world.live();
    world.event(Event::Malformed { owner: world.owner() });
    faulted(&mut world, Fault::Rules);
    let mut world = World::new(22, limits());
    world.live();
    world.event(Event::Hangup { owner: world.owner() });
    faulted(&mut world, Fault::Exited);
    let mut world = World::new(23, limits());
    world.live();
    message(&mut world, 30, 1);
    world.event(Event::Unsent { owner: world.owner() });
    assert_eq!(world.seen.fault, None);
    world.cleanup();
    world.settled();
    assert_eq!(world.seen.fault, Some(Fault::Exited));
}

#[test]
fn inbound_messages_are_ordered_bounded_and_named_opaquely() {
    let mut world = World::new(24, limits());
    world.live();
    for name in [u64::MAX, 0, 31] {
        message(&mut world, name, 64);
    }
    message(&mut world, 32, 64);
    message(&mut world, 33, 65);
    message(&mut world, 0, 1);
    assert_eq!(world.seen.bounces, [Bounce::Full, Bounce::TooLarge, Bounce::ReusedName]);
    for _ in 0..3 {
        world.sent();
    }
    let names: Vec<Token> = world
        .seen
        .down
        .iter()
        .filter_map(|word| match word {
            Down::Message { name, .. } => Some(*name),
            Down::Start { .. } | Down::Answer { .. } | Down::Acknowledge { .. } | Down::Grant { .. } | Down::Cancel => {
                None
            }
        })
        .collect();
    assert_eq!(names, [Token::new(u64::MAX), Token::new(0), Token::new(31)]);
    finish(&mut world);
}

#[test]
fn read_watermarks_cover_only_sent_names_and_release_exact_prefix() {
    let mut world = World::new(25, limits());
    world.live();
    message(&mut world, 30, 1);
    message(&mut world, 31, 1);
    message(&mut world, 32, 1);
    world.sent();
    world.sent();
    world.sent();
    turn(&mut world, 1, 2, Some(Token::new(31)), 1);
    message(&mut world, 33, 1);
    message(&mut world, 34, 1);
    message(&mut world, 35, 1);
    assert_eq!(world.seen.bounces, [Bounce::Full]);
    world.sent();
    world.sent();
    world.event(Event::Acknowledge { agent: world.agent(), turn: 1 });
    world.sent();
    last(&mut world, RunResult::Parked, 1, 2);
    world.cleanup();
    world.settled();
    for fence in [None, Some(Token::new(999)), Some(Token::new(30))] {
        let mut world = World::new(26, limits());
        world.live();
        message(&mut world, 30, 1);
        world.sent();
        world.up(Up::Waiting { read: Some(Token::new(30)) });
        if fence == Some(Token::new(30)) {
            message(&mut world, 30, 1);
            assert_eq!(world.seen.bounces, [Bounce::ReusedName]);
            finish(&mut world);
        } else {
            world.up(Up::Waiting { read: fence });
            faulted(&mut world, Fault::Rules);
        }
    }
    let mut world = World::new(27, limits());
    world.spawn(start());
    world.spawned();
    message(&mut world, 30, 1);
    world.up(Up::Admitted);
    world.up(Up::Waiting { read: Some(Token::new(30)) });
    faulted(&mut world, Fault::Rules);
}

#[test]
fn old_acknowledged_name_history_is_a_parent_namespace_promise() {
    let mut world = World::new(28, limits());
    world.live();
    message(&mut world, 30, 1);
    message(&mut world, 31, 1);
    world.sent();
    world.sent();
    world.up(Up::Waiting { read: Some(Token::new(31)) });
    // Bounded host metadata rejects current watermark/outstanding reuse, but
    // deliberately does not claim complete history for older opaque names.
    message(&mut world, 30, 1);
    assert!(world.seen.bounces.is_empty());
    world.sent();
    finish(&mut world);
}

#[test]
fn exact_ack_metadata_preserves_parent_payloads_and_shutdown_rights() {
    let mut world = World::new(29, limits());
    world.live();
    turn(&mut world, 1, 1, None, 64);
    turn(&mut world, 2, 2, None, 64);
    assert!(!world.lower.contains(Lower::Read));
    world.at(40);
    assert_eq!(world.seen.fault, None);
    world.event(Event::Acknowledge { agent: world.agent(), turn: 2 });
    assert!(world.seen.turns.contains_key(&1));
    assert!(!world.lower.contains(Lower::Read), "ACK metadata still reserved during its real Send");
    world.sent();
    assert!(world.lower.contains(Lower::Read));
    world.event(Event::Acknowledge { agent: world.agent(), turn: 2 });
    last(&mut world, RunResult::Parked, 2, 2);
    world.cleanup();
    assert_eq!(world.seen.gone, None);
    world.event(Event::Acknowledge { agent: world.agent(), turn: 1 });
    world.settled();
}

#[test]
fn turn_numbers_spend_and_answer_counts_are_fenced() {
    for mutation in 0..4 {
        let mut world = World::new(30, limits());
        world.live();
        turn(&mut world, 1, 10, None, 1);
        match mutation {
            0 => turn(&mut world, 1, 11, None, 1),
            1 => turn(&mut world, 3, 11, None, 1),
            2 => turn(&mut world, 2, 9, None, 1),
            3 => last(&mut world, RunResult::Parked, 0, 10),
            _ => unreachable!(),
        }
        assert_eq!(world.seen.fault, Some(Fault::Rules));
        world.event(Event::Acknowledge { agent: world.agent(), turn: 1 });
        world.cleanup();
        world.settled();
    }
    let mut world = World::new(31, limits());
    world.live();
    turn(&mut world, 1, 10, None, 1);
    last(&mut world, RunResult::Parked, 1, 9);
    assert_eq!(world.seen.fault, Some(Fault::Rules));
    world.event(Event::Acknowledge { agent: world.agent(), turn: 1 });
    world.cleanup();
    world.settled();
}

#[test]
fn a_withdrawn_call_keeps_its_actual_once_only_answer_right() {
    let mut world = World::new(32, limits());
    world.live();
    call(&mut world, 20, 1, false, 100);
    world.up(Up::Withdraw { call: Token::new(20) });
    assert!(world.seen.calls.contains(&Token::new(20)));
    reply(&mut world, 20, Reply::Withdrawn);
    world.up(Up::Withdraw { call: Token::new(20) });
    world.sent();
    assert_eq!(count_answers(&world, Token::new(20)), 1);
    finish(&mut world);
    let mut world = World::new(33, limits());
    world.live();
    call(&mut world, 20, 1, false, 100);
    world.up(Up::Withdraw { call: Token::new(20) });
    world.up(Up::Withdraw { call: Token::new(20) });
    assert_eq!(world.seen.fault, Some(Fault::Rules));
    reply(&mut world, 20, Reply::Unavailable);
    world.cleanup();
    world.settled();
}

#[test]
fn an_oversized_generic_answer_is_too_large_but_actual_delivery_always_fits() {
    let mut world = World::new(34, limits());
    world.live();
    call(&mut world, 20, 1, false, 100);
    reply(
        &mut world,
        20,
        Reply::Host {
            error: false,
            body: vec![0; usize::try_from(limits().answer_bytes + 1).expect("small")].into_boxed_slice(),
        },
    );
    match world.seen.down.last().expect("actual send") {
        Down::Answer { reply: Reply::TooLarge, .. } => {}
        Down::Answer { .. }
        | Down::Start { .. }
        | Down::Message { .. }
        | Down::Acknowledge { .. }
        | Down::Grant { .. }
        | Down::Cancel => panic!("too large reply"),
    }
    world.sent();
    finish(&mut world);
}

#[test]
fn the_watchdog_pauses_calls_waiting_turn_credit_and_bounded_longs() {
    let mut world = World::new(35, limits());
    world.live();
    call(&mut world, 20, 1, false, 20);
    world.at(20);
    assert_eq!(world.seen.fault, None);
    world.at(29);
    assert_eq!(world.seen.fault, None);
    world.at(30);
    assert_eq!(world.seen.fault, Some(Fault::NoProgress));
    reply(&mut world, 20, Reply::Unavailable);
    world.cleanup();
    world.settled();
    let mut world = World::new(36, limits());
    world.live();
    world.up(Up::Waiting { read: None });
    world.at(50);
    assert_eq!(world.seen.fault, None);
    message(&mut world, 30, 1);
    world.sent();
    world.at(59);
    assert_eq!(world.seen.fault, None);
    world.at(60);
    faulted(&mut world, Fault::NoProgress);
    let mut world = World::new(37, limits());
    world.live();
    world.up(Up::Long { span: Duration::from_secs(30) });
    world.at(39);
    assert_eq!(world.seen.fault, None);
    world.at(40);
    faulted(&mut world, Fault::NoProgress);
    let mut world = World::new(38, limits());
    world.live();
    world.up(Up::Long { span: Duration::from_secs(30) });
    world.at(2);
    world.up(Up::LongDone);
    world.at(12);
    faulted(&mut world, Fault::NoProgress);
    let mut world = World::new(39, limits());
    world.live();
    world.up(Up::Long { span: Duration::from_secs(121) });
    faulted(&mut world, Fault::Rules);
}

#[test]
fn a_wait_crossing_an_unread_or_queued_message_does_not_pause() {
    let mut world = World::new(40, limits());
    world.live();
    message(&mut world, 30, 1);
    world.up(Up::Waiting { read: None });
    world.sent();
    world.at(10);
    faulted(&mut world, Fault::NoProgress);
}

#[test]
fn wall_clock_never_pauses_and_cancelled_spend_is_preserved() {
    for mode in 0..3 {
        let mut bounds = limits();
        bounds.wall_time = Duration::from_secs(20);
        let mut world = World::new(41, bounds);
        world.live();
        world.up(Up::Waiting { read: None });
        world.at(20);
        assert_eq!(world.seen.fault, None);
        world.sent();
        if mode == 0 {
            last(&mut world, RunResult::Accepted { outcome: Box::new([]) }, 0, 12);
            world.cleanup();
            world.settled();
            assert_eq!(world.seen.answer.as_ref().expect("answer").spent, 12);
        } else if mode == 1 {
            last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 12);
            faulted(&mut world, Fault::WallTime);
        } else {
            world.at(25);
            faulted(&mut world, Fault::WallTime);
        }
    }
}

#[test]
fn explicit_stop_cancels_once_and_does_not_restart_grace() {
    let mut world = World::new(42, limits());
    world.live();
    world.event(Event::Stop { agent: world.agent() });
    world.sent();
    world.at(4);
    world.event(Event::Stop { agent: world.agent() });
    world.at(5);
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    world.at(7);
    assert_eq!(world.seen.signals, [Signal::Terminate, Signal::Kill]);
    last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 15);
    assert_eq!(world.seen.answer.as_ref().expect("late honest answer").spent, 15);
    world.cleanup();
    world.settled();
}

#[test]
fn stop_and_answer_during_wall_or_draining_preserve_original_shutdown_deadline() {
    let mut bounds = limits();
    bounds.wall_time = Duration::from_secs(20);
    let mut world = World::new(43, bounds);
    world.live();
    world.up(Up::Waiting { read: None });
    world.at(20);
    world.sent();
    world.event(Event::Stop { agent: world.agent() });
    world.at(24);
    last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 0);
    world.at(25);
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    world.cleanup();
    world.settled();
    let mut world = World::new(44, limits());
    world.live();
    world.event(Event::Exited { owner: world.owner() });
    world.event(Event::Stop { agent: world.agent() });
    world.cleanup();
    world.settled();
    assert_eq!(world.seen.fault, None);
}

#[test]
fn buffered_answer_after_exit_is_heard_before_tree_empty_and_gone() {
    let mut world = World::new(45, limits());
    world.live();
    world.event(Event::Exited { owner: world.owner() });
    turn(&mut world, 1, 5, None, 1);
    last(&mut world, RunResult::Parked, 1, 5);
    world.event(Event::Reaped { owner: world.owner(), detail: Box::new([]) });
    world.event(Event::Hangup { owner: world.owner() });
    assert_eq!(world.seen.gone, None);
    world.event(Event::Acknowledge { agent: world.agent(), turn: 1 });
    world.settled();
}

#[test]
fn actual_delivery_right_outlives_process_tree_and_eof_without_abandonment() {
    for delivered in [false, true] {
        let mut world = World::new(46, limits());
        world.live();
        call(&mut world, 20, 1, true, 100);
        world.event(Event::Stop { agent: world.agent() });
        world.sent();
        world.cleanup();
        assert_eq!(world.seen.gone, None);
        assert!(world.seen.withdrawals.contains(&Token::new(20)));
        reply(
            &mut world,
            20,
            Reply::Delivery(if delivered {
                Delivery::Delivered(receipts(b"actual durable effect"))
            } else {
                Delivery::Nothing
            }),
        );
        world.settled();
        assert_eq!(count_answers(&world, Token::new(20)), 0, "lost channel gets no fabricated response");
    }
}

#[test]
fn interrupted_landing_proof_survives_reply_terminal_and_matches_last_word() {
    let receipt = receipts(b"landed");
    for mutation in 0..3 {
        let mut world = World::new(47, limits());
        world.live();
        call(&mut world, 20, 1, true, 100);
        world.event(Event::Stop { agent: world.agent() });
        world.sent();
        reply(&mut world, 20, Reply::Delivery(Delivery::Delivered(receipt.clone())));
        world.sent();
        let name = CallName { completion: if mutation == 1 { 2 } else { 1 }, position: 2 };
        let final_receipts = if mutation == 2 { receipts(b"invented") } else { receipt.clone() };
        last(
            &mut world,
            RunResult::Delivered { name, receipts: final_receipts, stopped: RunFailure::Cancelled },
            0,
            25,
        );
        if mutation == 0 {
            assert_eq!(world.seen.answer.as_ref().expect("verified real landing").spent, 25);
            world.cleanup();
            world.settled();
        } else {
            silently_faulted(&mut world, Fault::Rules);
        }
    }
    let mut world = World::new(48, limits());
    world.live();
    last(
        &mut world,
        RunResult::Delivered {
            name: CallName { completion: 1, position: 2 },
            receipts: receipt,
            stopped: RunFailure::Budget(smith_host_domain::Exhausted::Time),
        },
        0,
        1,
    );
    faulted(&mut world, Fault::Rules);
}

#[test]
fn ordinary_earlier_landing_then_later_stop_remains_an_ordinary_answer() {
    let mut world = World::new(49, limits());
    world.live();
    call(&mut world, 20, 1, true, 100);
    reply(&mut world, 20, Reply::Delivery(Delivery::Delivered(receipts(b"ordinary"))));
    world.sent();
    world.event(Event::Stop { agent: world.agent() });
    world.sent();
    last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 9);
    world.cleanup();
    world.settled();
}

#[test]
fn every_actual_delivery_terminal_survives_withdrawal_and_expired_deadline() {
    let terminals = [
        Delivery::Nothing,
        Delivery::Stale,
        Delivery::Delivered(receipts(b"landed")),
        Delivery::Refused(smith_host_domain::DeliveryRefusal::new(None, Box::from(&b"reason"[..])).expect("refusal")),
        Delivery::Refused(
            smith_host_domain::DeliveryRefusal::new(
                Some(smith_host_domain::Marker::new(0, Box::from(&b"relative/file"[..])).expect("marker")),
                Box::from(&b"resolve"[..]),
            )
            .expect("refusal"),
        ),
        Delivery::Failed(smith_host_domain::DeliveryFailure {
            reason: smith_host_domain::DeliveryReason::Broken,
            diagnostic: smith_host_domain::Diagnostic::new(b"tail", 0),
        }),
    ];
    for terminal in terminals {
        let mut world = World::new(50, limits());
        world.live();
        call(&mut world, 20, 1, true, 5);
        world.up(Up::Withdraw { call: Token::new(20) });
        world.at(5);
        world.at(6);
        reply(&mut world, 20, Reply::Delivery(terminal.clone()));
        match world.seen.down.last().expect("delivery response") {
            Down::Answer { reply: Reply::Delivery(actual), .. } => assert_eq!(*actual, terminal),
            Down::Answer { .. }
            | Down::Start { .. }
            | Down::Message { .. }
            | Down::Acknowledge { .. }
            | Down::Grant { .. }
            | Down::Cancel => panic!("full actual terminal required"),
        }
        world.sent();
        finish(&mut world);
    }
}

#[test]
fn grant_generation_races_are_fenced_by_actual_emission_not_queued_refresh() {
    let mut world = World::new(51, limits());
    world.spawn(start());
    world.spawned();
    world.up(Up::Admitted);
    grant(&mut world, 2);
    grant(&mut world, 3);
    world.up(Up::Rejected { account: 1, generation: 1 });
    assert_eq!(world.seen.rejected, [(1, 1)]);
    world.sent();
    match world.seen.down.last().expect("coalesced refresh") {
        Down::Grant { grant } => assert_eq!(grant.generation, 3),
        Down::Start { .. } | Down::Message { .. } | Down::Answer { .. } | Down::Acknowledge { .. } | Down::Cancel => {
            panic!("latest refresh")
        }
    }
    world.up(Up::Rejected { account: 1, generation: 1 });
    world.up(Up::Rejected { account: 1, generation: 3 });
    world.sent();
    finish(&mut world);
    for (account, generation) in [(1, 2), (1, 3), (2, 1), (1, 0)] {
        let mut world = World::new(52, limits());
        world.spawn(start());
        world.spawned();
        world.up(Up::Admitted);
        grant(&mut world, 2);
        world.up(Up::Rejected { account, generation });
        faulted(&mut world, Fault::Rules);
    }
}

#[test]
fn credential_refresh_coalescing_preserves_other_traffic_and_actual_answer_order() {
    let mut world = World::new(53, limits());
    world.live();
    message(&mut world, 30, 1);
    grant(&mut world, 2);
    call(&mut world, 20, 1, false, 100);
    reply(&mut world, 20, Reply::Unavailable);
    grant(&mut world, 3);
    world.sent();
    world.sent();
    world.sent();
    assert_eq!(count_answers(&world, Token::new(20)), 1);
    finish(&mut world);
}

#[test]
fn an_agent_has_gone_only_after_every_actual_io_right_even_after_answer() {
    let mut world = World::new(54, limits());
    world.live();
    last(&mut world, RunResult::Parked, 0, 0);
    world.event(Event::Exited { owner: world.owner() });
    world.event(Event::Reaped { owner: world.owner(), detail: Box::new([]) });
    assert_eq!(world.seen.gone, None);
    world.at(5);
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    world.event(Event::Hangup { owner: world.owner() });
    assert_eq!(world.seen.gone, None);
    world.event(Event::Signalled { owner: world.owner() });
    world.settled();
}

#[test]
fn trailing_records_terminate_without_fabricating_a_second_run_answer() {
    let mut world = World::new(55, limits());
    world.live();
    last(&mut world, RunResult::Parked, 0, 0);
    world.up(Up::Fact { body: Box::new([]) });
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    assert_eq!(world.seen.fault, None);
    world.cleanup();
    world.settled();
}

#[test]
fn facts_change_nothing_when_none_are_kept() {
    let mut none = limits();
    none.facts = 0;
    let mut world = World::new(56, none);
    world.live();
    finish(&mut world);
    assert_eq!(world.domain.facts_lost(), 4);
    let mut world = World::new(56, limits());
    world.live();
    finish(&mut world);
    let mut facts = 0;
    while world.domain.pop_fact().is_some() {
        facts += 1;
    }
    assert_eq!(facts, 4);
    assert_eq!(world.domain.facts_lost(), 0);
}

#[test]
fn a_seed_replays_the_same_v2_boundary_history() {
    skein_world::domain::assert_replays(57, 58, |seed| {
        let mut world = World::new(seed, limits());
        world.live();
        world.at(seed % 3);
        world.up(Up::Fact { body: Box::from(&b"progress"[..]) });
        finish(&mut world);
        (world.trace.lines().to_vec(), world.seen.gone)
    });
}

#[test]
fn final_answers_cannot_abandon_parent_or_queued_delivery_terminals() {
    for queued in [false, true] {
        for result in [
            RunResult::Accepted { outcome: Box::new([]) },
            RunResult::Failed { failure: RunFailure::Cancelled },
            RunResult::Delivered {
                name: CallName { completion: 1, position: 2 },
                receipts: receipts(b"landed"),
                stopped: RunFailure::Budget(smith_host_domain::Exhausted::Time),
            },
        ] {
            let mut world = World::new(61, limits());
            world.live();
            if queued {
                message(&mut world, 30, 1);
            }
            call(&mut world, 20, 1, true, 100);
            if queued {
                reply(&mut world, 20, Reply::Delivery(Delivery::Delivered(receipts(b"landed"))));
            }
            last(&mut world, result, 0, 1);
            assert_eq!(world.seen.fault, Some(Fault::Rules));
            world.cleanup();
            if !queued {
                assert_eq!(world.seen.gone, None);
                reply(&mut world, 20, Reply::Delivery(Delivery::Nothing));
            }
            world.settled();
        }
    }
    // The actual response can reach the agent before its Send terminal returns.
    let mut world = World::new(62, limits());
    world.live();
    call(&mut world, 20, 1, true, 100);
    reply(&mut world, 20, Reply::Delivery(Delivery::Delivered(receipts(b"landed"))));
    last(
        &mut world,
        RunResult::Delivered {
            name: CallName { completion: 1, position: 2 },
            receipts: receipts(b"landed"),
            stopped: RunFailure::Budget(smith_host_domain::Exhausted::Time),
        },
        0,
        1,
    );
    assert!(world.seen.answer.is_some());
    world.cleanup();
    world.settled();
}

#[test]
fn cancelled_and_draining_paths_keep_work_and_drop_answers_only_after_a_reported_fault() {
    let mut world = World::new(63, limits());
    world.live();
    world.event(Event::Stop { agent: world.agent() });
    world.sent();
    call(&mut world, 20, 1, false, 100);
    world.up(Up::Fact { body: Box::new([]) });
    reply(&mut world, 20, Reply::Unavailable);
    world.sent();
    last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 7);
    world.cleanup();
    world.settled();
    for malformed in [false, true] {
        let mut world = World::new(64, limits());
        world.live();
        world.event(Event::Stop { agent: world.agent() });
        world.sent();
        if malformed {
            world.event(Event::Malformed { owner: world.owner() });
        } else {
            world.up(Up::Long { span: Duration::from_secs(121) });
        }
        assert_eq!(world.seen.fault, None, "explicit parent stop owns shutdown");
        assert_eq!(world.seen.signals, [Signal::Terminate]);
        let mut saw_diagnostic = false;
        while let Some(fact) = world.domain.pop_fact() {
            match fact {
                smith_host_domain::Fact::Faulted { fault, .. } => {
                    assert_eq!(fault, Fault::Rules);
                    saw_diagnostic = true;
                }
                smith_host_domain::Fact::Started { .. }
                | smith_host_domain::Fact::Admitted { .. }
                | smith_host_domain::Fact::Answered { .. }
                | smith_host_domain::Fact::Gone { .. } => {}
            }
        }
        assert!(saw_diagnostic);
        if !malformed {
            world.up(Up::Fact { body: Box::new([]) });
            assert_eq!(world.seen.told, 0);
            last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 8);
            assert!(world.seen.answer.is_some(), "first last word still heard after untold diagnostic");
        }
        world.cleanup();
        world.settled();
    }
    let mut world = World::new(65, limits());
    world.live();
    world.at(10);
    assert_eq!(world.seen.fault, Some(Fault::NoProgress));
    world.up(Up::Answer {
        answer: Answer {
            turns: 0,
            completions: 1,
            input: 11,
            output: 13,
            cache_read: 17,
            cache_write: 19,
            spent: 10,
            spend_overflow: false,
            usage_overflow: false,
            result: RunResult::Parked,
        },
    });
    assert!(world.seen.answer.is_none());
    world.at(12);
    world.up(Up::Fact { body: Box::new([]) });
    assert_eq!(world.seen.told, 0);
    world.cleanup();
    world.settled();
    let mut world = World::new(66, limits());
    world.live();
    world.event(Event::Exited { owner: world.owner() });
    world.event(Event::Malformed { owner: world.owner() });
    faulted(&mut world, Fault::Rules);
    let mut world = World::new(67, limits());
    world.live();
    world.event(Event::Exited { owner: world.owner() });
    world.at(5);
    assert_eq!(world.seen.fault, Some(Fault::Exited));
    world.cleanup();
    world.settled();
}

#[test]
fn wall_shutdown_hangup_exit_and_breach_report_the_actual_original_cause() {
    for mode in 0..4 {
        let mut bounds = limits();
        bounds.wall_time = Duration::from_secs(20);
        let mut world = World::new(68, bounds);
        world.live();
        world.up(Up::Waiting { read: None });
        world.at(20);
        world.sent();
        match mode {
            0 => {
                world.event(Event::Hangup { owner: world.owner() });
                assert_eq!(world.seen.fault, Some(Fault::WallTime));
            }
            1 => {
                world.event(Event::Exited { owner: world.owner() });
                last(&mut world, RunResult::Failed { failure: RunFailure::Cancelled }, 0, 3);
                assert_eq!(world.seen.fault, Some(Fault::WallTime));
            }
            2 => {
                world.up(Up::Long { span: Duration::from_secs(121) });
                assert_eq!(world.seen.fault, Some(Fault::Rules));
            }
            3 => {
                world.event(Event::Malformed { owner: world.owner() });
                assert_eq!(world.seen.fault, Some(Fault::Rules));
            }
            _ => unreachable!(),
        }
        world.cleanup();
        world.settled();
    }
}

#[test]
fn continual_actual_progress_reaches_only_the_independent_wall_deadline() {
    let mut bounds = limits();
    bounds.wall_time = Duration::from_secs(20);
    let mut world = World::new(69, bounds);
    world.live();
    for second in [6, 12, 18] {
        world.schedule.send(
            Time::ZERO.saturating_add(Duration::from_secs(second)),
            Event::Received { owner: world.owner(), message: Up::Fact { body: Box::from(&b"actual progress"[..]) } },
        );
        world.at(second);
        assert_eq!(world.seen.fault, None);
        assert!(world.seen.signals.is_empty());
    }
    assert_eq!(world.seen.told, 3);
    world.at(20);
    assert_eq!(world.seen.fault, None);
    assert!(world.seen.signals.is_empty());
    match world.seen.down.last().expect("wall Cancel") {
        Down::Cancel => {}
        Down::Start { .. }
        | Down::Message { .. }
        | Down::Answer { .. }
        | Down::Acknowledge { .. }
        | Down::Grant { .. } => panic!("wall must politely cancel first"),
    }
    world.sent();
    world.at(25);
    assert_eq!(world.seen.fault, Some(Fault::WallTime));
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    world.cleanup();
    world.settled();
}

#[test]
fn surviving_descendants_keep_reap_right_through_terminate_and_kill() {
    let mut world = World::new(70, limits());
    world.live();
    last(&mut world, RunResult::Parked, 0, 4);
    world.event(Event::Exited { owner: world.owner() });
    assert!(world.lower.contains(Lower::Reap));
    assert_eq!(world.seen.gone, None);
    world.at(5);
    assert_eq!(world.seen.signals, [Signal::Terminate]);
    assert_eq!(world.seen.gone, None);
    world.at(7);
    assert_eq!(world.seen.signals, [Signal::Terminate, Signal::Kill]);
    assert_eq!(world.seen.gone, None);
    assert!(world.lower.contains(Lower::Reap), "real descendant containment is still outstanding");
    world.event(Event::Hangup { owner: world.owner() });
    world.event(Event::Reaped { owner: world.owner(), detail: Box::from(&b"descendants gone"[..]) });
    assert_eq!(world.seen.gone, None, "both actual signal rights also remain");
    world.event(Event::Signalled { owner: world.owner() });
    assert_eq!(world.seen.gone, None);
    world.event(Event::Signalled { owner: world.owner() });
    world.settled();
    assert_eq!(world.seen.fault, None);
    assert_eq!(world.seen.gone_detail.as_deref(), Some(b"descendants gone".as_slice()));
}
