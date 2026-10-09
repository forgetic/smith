//! The independent channel referee accepts a complete run and rejects
//! duplicate answers, missing terminals, gaps and credential leakage.

use skein_channel::StreamMode;
use skein_lib::{Time, Token};
use skein_world::domain::{Referee, Verdict};
use smith_channel::CEILINGS;
use smith_channel_world::{
    World,
    referee::{Meeting, Seen, review},
};

#[test]
fn a_completed_world_meets_the_answer_and_end_obligations() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v2/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits_and_parks();
    world.settle();
    review(world.observations(), Box::from(*b"secret")).assert_passed(0);
}

#[test]
fn a_host_call_terminal_pairs_across_both_halves() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v2/record_charter_smallest.bin")[..],
    ));
    world.settle();
    world.agent_admits();
    world.settle();
    let name = smith_domain::run::CallName { activation: 1, completion: 1, position: 0 };
    let relay = smith_domain::run::RelayName { owner: Token::new(1), attempt: 1 };
    world.agent_calls_host(name, relay);
    world.settle();
    world.host_answers(Token::new(1), smith_host_domain::channel::Reply::Busy);
    world.settle();
    world.agent_parks();
    world.settle();
    review(world.observations(), Box::default()).assert_passed(1);
}

fn rejected(seen: &[Seen]) {
    let mut referee = Referee::new(Meeting::new(Box::from(*b"secret")));
    for event in seen {
        let event = match event {
            Seen::Started => Seen::Started,
            Seen::Issued { name } => Seen::Issued { name: *name },
            Seen::Called { call, name } => Seen::Called { call: *call, name: *name },
            Seen::Settled { call } => Seen::Settled { call: *call },
            Seen::Returned => Seen::Returned,
            Seen::Turn { number, spent, body } => Seen::Turn { number: *number, spent: *spent, body: body.clone() },
            Seen::Answer { turns, spent } => Seen::Answer { turns: *turns, spent: *spent },
            Seen::Ended => Seen::Ended,
        };
        referee.observe(Time::ZERO, event, &mut Vec::new());
    }
    assert!(matches!(referee.verdict(), Verdict::Failed(_)));
}

#[test]
fn the_referee_rejects_a_duplicate_answer_and_a_missing_call_terminal() {
    rejected(&[Seen::Started, Seen::Answer { turns: 0, spent: 0 }, Seen::Answer { turns: 0, spent: 0 }]);
    rejected(&[
        Seen::Started,
        Seen::Issued { name: (1, 1, 0) },
        Seen::Called { call: Token::new(1), name: (1, 1, 0) },
        Seen::Answer { turns: 0, spent: 0 },
    ]);
}

#[test]
fn the_referee_rejects_a_turn_gap_and_credential_in_turn_bytes() {
    rejected(&[Seen::Started, Seen::Turn { number: 2, spent: 0, body: Box::default() }]);
    rejected(&[Seen::Started, Seen::Turn { number: 1, spent: 0, body: Box::from(*b"a-secret-z") }]);
}
