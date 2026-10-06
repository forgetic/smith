//! Issued-name settlement, actual lower races and genuine waiting claims.
//! Contract: domain/host.md, section 4.2; testing-strategy.md, sections 6 and 7.

use skein_lib::{Duration, Token};
use smith_host_domain::{
    Answer, Ask, Bounce, CallName, Down, Effect, Event, Fault, MessageRefusal, RunResult, Turn, Up,
};
use smith_host_world::{Lower, World, limits};

fn message(world: &mut World, name: u64) {
    world.event(Event::Message { agent: world.agent(), name: Token::new(name), body: Box::from(&b"opaque"[..]) });
}

fn refusal(world: &mut World, name: u64, reason: MessageRefusal) {
    world.up(Up::MessageBounced { name: Token::new(name), reason });
}

fn turn(world: &mut World, read: Option<u64>) {
    world.up(Up::Turn {
        turn: Turn {
            number: 1,
            spent: 0,
            spend_overflow: false,
            usage_overflow: false,
            read: read.map(Token::new),
            body: Box::from(&b"real turn"[..]),
        },
    });
    world.event(Event::Acknowledge { agent: world.agent(), turn: 1 });
    world.sent();
}

fn final_word() -> Up {
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
            result: RunResult::Parked,
        },
    }
}

#[test]
fn each_actual_reason_and_both_send_terminal_orders_settle_only_one_credit() {
    for reason in [MessageRefusal::Busy, MessageRefusal::TooLarge, MessageRefusal::Inactive, MessageRefusal::ReusedName]
    {
        for received_first in [false, true] {
            let mut world = World::new(401, smith_host_domain::Limits { messages: 1, ..limits() });
            world.live();
            message(&mut world, 0);
            if !received_first {
                world.sent();
            }
            refusal(&mut world, 0, reason);
            assert_eq!(world.seen.message_bounces, [(Token::new(1), Token::new(0), reason)]);
            assert!(world.seen.bounces.is_empty());
            if received_first {
                message(&mut world, 0);
                assert_eq!(world.seen.bounces, [Bounce::ReusedName], "pending Send still protects its name");
            }
            message(&mut world, u64::MAX);
            assert_eq!(world.seen.bounces.len(), usize::from(received_first));
            if received_first {
                assert!(matches!(world.seen.down.last(), Some(Down::Message { name, .. }) if name.raw() == 0));
                world.sent();
            }
            assert!(matches!(world.seen.down.last(), Some(Down::Message { name, .. }) if name.raw() == u64::MAX));
            world.sent();
            world.up(Up::Waiting { read: Some(Token::new(u64::MAX)) });
            assert_eq!(world.seen.message_bounces.len(), 1);
            world.cleanup();
            world.settled();
        }
    }
}

#[test]
fn interior_first_last_and_single_removal_preserve_opaque_fifo_and_current_read() {
    for rejected in [0, u64::MAX, 7] {
        let mut world = World::new(402, limits());
        world.live();
        for name in [0, u64::MAX, 7] {
            message(&mut world, name);
            world.sent();
        }
        refusal(&mut world, rejected, MessageRefusal::Busy);
        for accepted in [0, u64::MAX, 7].into_iter().filter(|name| *name != rejected) {
            world.up(Up::Waiting { read: Some(Token::new(accepted)) });
            assert_eq!(world.seen.fault, None, "remaining issued FIFO survives removal of {rejected}");
        }
        assert_eq!(world.seen.message_bounces.len(), 1);
        world.cleanup();
        world.settled();
    }
    let mut world = World::new(403, limits());
    world.live();
    message(&mut world, 8);
    world.sent();
    world.up(Up::Waiting { read: Some(Token::new(8)) });
    message(&mut world, 3);
    world.sent();
    refusal(&mut world, 3, MessageRefusal::TooLarge);
    world.up(Up::Waiting { read: Some(Token::new(8)) });
    assert_eq!(world.seen.fault, None, "settlement never regresses the actual read fence");
    world.cleanup();
    world.settled();
}

#[test]
fn unknown_queued_read_and_duplicate_names_never_settle_issued_credit() {
    for invalid in 0..5 {
        let mut world = World::new(404, limits());
        world.live();
        message(&mut world, 1);
        let target = match invalid {
            0 => 99,
            1 => {
                message(&mut world, 2);
                2
            }
            2 => {
                world.up(Up::Waiting { read: Some(Token::new(1)) });
                1
            }
            3 => {
                refusal(&mut world, 1, MessageRefusal::Busy);
                1
            }
            4 => {
                world.sent();
                refusal(&mut world, 1, MessageRefusal::Busy);
                1
            }
            _ => unreachable!("bounded mutation"),
        };
        let before = world.seen.message_bounces.len();
        refusal(&mut world, target, MessageRefusal::Inactive);
        assert_eq!(world.seen.message_bounces.len(), before);
        assert_eq!(world.seen.fault, Some(Fault::Rules), "invalid issued name mutation {invalid}");
        world.cleanup();
        world.settled();
    }
}

#[test]
fn refused_name_cannot_be_a_read_watermark_but_pending_send_does_not_retain_credit() {
    let mut world = World::new(405, limits());
    world.live();
    message(&mut world, 9);
    refusal(&mut world, 9, MessageRefusal::Busy);
    world.up(Up::Waiting { read: Some(Token::new(9)) });
    assert_eq!(world.seen.fault, Some(Fault::Rules));
    assert_eq!(world.seen.message_bounces.len(), 1);
    world.cleanup();
    world.settled();
}

#[test]
fn earlier_refusal_crosses_another_messages_actual_unsent_without_invented_terminals() {
    let mut world = World::new(406, limits());
    world.live();
    message(&mut world, 1);
    world.sent();
    message(&mut world, 2);
    world.event(Event::Unsent { owner: world.owner() });
    assert!(!world.lower.contains(Lower::Send));
    refusal(&mut world, 1, MessageRefusal::Busy);
    assert_eq!(world.seen.message_bounces, [(Token::new(1), Token::new(1), MessageRefusal::Busy)]);
    assert_eq!(world.seen.fault, None);
    world.cleanup();
    world.settled();
}

#[test]
fn exact_issued_inactive_after_final_answer_keeps_original_grace_and_accounting() {
    let mut world = World::new(407, limits());
    world.live();
    message(&mut world, 0);
    world.up(final_word());
    let deadline = world.domain.next_deadline();
    world.at(4);
    refusal(&mut world, 0, MessageRefusal::Inactive);
    assert_eq!(world.domain.next_deadline(), deadline);
    assert_eq!(world.seen.answer.as_ref().expect("one real final word").turns, 0);
    assert_eq!(world.seen.answer.as_ref().expect("same final accounting").spent, 0);
    assert_eq!(world.seen.message_bounces.len(), 1);
    assert!(world.lower.contains(Lower::Send), "late notice does not consume the actual Send");
    assert_eq!(world.seen.fault, None);
    world.at(5);
    assert_eq!(world.seen.signals, [smith_host_domain::Signal::Terminate]);
    world.cleanup();
    world.settled();
}

#[test]
fn issued_refusals_after_reported_fault_and_both_tree_signals_cannot_restart_shutdown() {
    let mut world = World::new(408, limits());
    world.live();
    for name in [1, 2] {
        message(&mut world, name);
        world.sent();
    }
    world.at(10);
    assert_eq!(world.seen.fault, Some(Fault::NoProgress));
    let deadline = world.domain.next_deadline();
    world.at(11);
    refusal(&mut world, 1, MessageRefusal::Inactive);
    assert_eq!(world.domain.next_deadline(), deadline);
    world.at(12);
    assert_eq!(world.seen.signals, [smith_host_domain::Signal::Terminate, smith_host_domain::Signal::Kill]);
    let deadline = world.domain.next_deadline();
    world.at(13);
    refusal(&mut world, 2, MessageRefusal::Inactive);
    assert_eq!(world.domain.next_deadline(), deadline);
    assert_eq!(world.seen.message_bounces.len(), 2);
    assert_eq!(world.seen.fault, Some(Fault::NoProgress));
    assert!(world.seen.answer.is_none());
    world.cleanup();
    world.settled();
}

#[test]
fn genuine_waiting_claim_restores_only_when_every_attempted_wake_is_refused() {
    for accepted_wake in [false, true] {
        let mut world = World::new(409, limits());
        world.live();
        world.up(Up::Waiting { read: None });
        if accepted_wake {
            message(&mut world, 1);
            world.sent();
        }
        message(&mut world, 2);
        world.sent();
        refusal(&mut world, 2, MessageRefusal::TooLarge);
        world.at(10);
        assert_eq!(world.seen.fault, if accepted_wake { Some(Fault::NoProgress) } else { None });
        if !accepted_wake {
            world.at(900);
            world.sent();
            assert_eq!(world.seen.fault, None, "waiting pauses only progress, never the wall clock");
        }
        world.cleanup();
        world.settled();
    }
}

#[test]
fn actual_call_long_and_main_turn_invalidate_the_old_waiting_claim() {
    for activity in 0..3 {
        let mut world = World::new(410, limits());
        world.live();
        world.up(Up::Waiting { read: None });
        message(&mut world, 1);
        world.sent();
        match activity {
            0 => {
                world.up(Up::Call {
                    call: Token::new(40),
                    name: CallName { completion: 1, position: 0 },
                    deadline: world.stage.env.now,
                    ask: Ask::Host { tool: Box::from(&b"tool"[..]), effect: Effect::Read, body: Box::new([]) },
                });
                world.event(Event::Answer {
                    agent: world.agent(),
                    call: Token::new(40),
                    reply: smith_host_domain::Reply::Unavailable,
                });
                world.sent();
                refusal(&mut world, 1, MessageRefusal::Busy);
            }
            1 => {
                world.up(Up::Long { span: Duration::ZERO });
                refusal(&mut world, 1, MessageRefusal::Busy);
            }
            2 => turn(&mut world, Some(1)),
            _ => unreachable!("bounded actual activity"),
        }
        world.at(10);
        assert_eq!(world.seen.fault, Some(Fault::NoProgress), "actual activity {activity} cleared old Waiting");
        world.cleanup();
        world.settled();
    }
}

#[test]
fn zero_message_capacity_never_creates_an_agent_refusal_right() {
    let mut world = World::new(411, smith_host_domain::Limits { messages: 0, ..limits() });
    world.live();
    message(&mut world, 0);
    assert_eq!(world.seen.bounces, [Bounce::Full]);
    refusal(&mut world, 0, MessageRefusal::Busy);
    assert_eq!(world.seen.fault, Some(Fault::Rules));
    assert!(world.seen.message_bounces.is_empty());
    world.cleanup();
    world.settled();
}

#[test]
fn a_refusal_without_actual_waiting_never_invents_pause_and_new_waiting_restores_it() {
    for genuine_waiting in [false, true] {
        let mut world = World::new(412, limits());
        world.live();
        message(&mut world, 1);
        world.sent();
        if genuine_waiting {
            turn(&mut world, Some(1));
            world.up(Up::Waiting { read: Some(Token::new(1)) });
        } else {
            refusal(&mut world, 1, MessageRefusal::TooLarge);
        }
        world.at(20);
        assert_eq!(world.seen.fault, if genuine_waiting { None } else { Some(Fault::NoProgress) });
        world.cleanup();
        world.settled();
    }
}

#[test]
fn one_peers_unknown_name_cannot_consume_another_peers_issued_reservation() {
    let mut first = World::new(413, limits());
    let mut second = World::new(414, limits());
    first.live();
    second.live();
    message(&mut first, 11);
    first.sent();
    message(&mut second, 22);
    second.sent();
    refusal(&mut first, 22, MessageRefusal::Busy);
    assert_eq!(first.seen.fault, Some(Fault::Rules));
    assert!(first.seen.message_bounces.is_empty());
    refusal(&mut second, 22, MessageRefusal::Busy);
    assert_eq!(second.seen.fault, None);
    assert_eq!(second.seen.message_bounces.len(), 1);
    refusal(&mut first, 11, MessageRefusal::Inactive);
    assert_eq!(first.seen.message_bounces.len(), 1, "invalid notice did not mutate the first issued queue");
    first.cleanup();
    first.settled();
    second.cleanup();
    second.settled();
}

#[test]
fn full_calls_and_messages_keep_exact_output_room_and_every_real_parent_terminal() {
    let bounds = limits();
    let mut world = World::new(415, bounds);
    world.live();
    for position in 0..bounds.calls {
        world.up(Up::Call {
            call: Token::new(100 + u64::from(position)),
            name: CallName { completion: 1, position },
            deadline: world.stage.env.now.saturating_add(Duration::from_secs(100)),
            ask: Ask::Host { tool: b"tool".as_slice().into(), effect: Effect::Read, body: Box::new([]) },
        });
    }
    for name in 0..bounds.messages {
        message(&mut world, u64::from(name));
        world.sent();
    }
    refusal(&mut world, 99, MessageRefusal::Inactive);
    assert_eq!(world.seen.fault, Some(Fault::Rules));
    assert_eq!(world.seen.withdrawals.len(), usize::try_from(bounds.calls).expect("finite maximum calls"));
    assert!(world.seen.message_bounces.is_empty());
    for name in 0..bounds.messages {
        refusal(&mut world, u64::from(name), MessageRefusal::Inactive);
    }
    assert_eq!(world.seen.message_bounces.len(), usize::try_from(bounds.messages).expect("finite maximum messages"));
    for position in 0..bounds.calls {
        world.event(Event::Answer {
            agent: world.agent(),
            call: Token::new(100 + u64::from(position)),
            reply: smith_host_domain::Reply::Unavailable,
        });
    }
    assert!(world.seen.calls.is_empty());
    world.cleanup();
    world.settled();
}
