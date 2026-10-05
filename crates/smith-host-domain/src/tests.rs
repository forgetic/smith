//! Admission/source-law controls (Temper 19735a06; domain/host.md, 4 and 10).
//! The independent V2 scripted world carries the remaining boundary histories.
use crate::{Domain, End, Event, Request, Start};
use alloc::boxed::Box;
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};

fn limits() -> crate::Limits {
    crate::Limits {
        agents: 1,
        directories: 0,
        conflicts: 0,
        path_bytes: 0,
        name_bytes: 64,
        accounts: 0,
        charter_bytes: 64,
        transcript_bytes: 64,
        answered_bytes: 64,
        message_bytes: 64,
        messages: 1,
        calls: 1,
        call_bytes: 64,
        answer_bytes: crate::Delivered::worst_case(),
        turns: 1,
        turn_bytes: 64,
        unacknowledged_bytes: 64,
        fact_bytes: 64,
        outcome_bytes: 64,
        detail_bytes: 64,
        spawn_timeout: Duration::from_secs(1),
        no_progress: Duration::from_secs(10),
        long_span: Duration::from_secs(60),
        wall_time: Duration::from_secs(100),
        grace: Duration::from_secs(5),
        kill_after: Duration::from_secs(2),
        facts: 4,
    }
}

fn start() -> Start {
    Start {
        logical_run: Token::new(1),
        workspace: None,
        charter: Box::new([]),
        transcript: None,
        answered: Box::new([]),
        directories: Box::new([]),
        grants: Box::new([]),
    }
}
#[test]
fn a_spawn_beyond_the_slots_is_refused_as_busy_without_displacing_the_active_owner() {
    let limits = limits();
    let mut domain = Domain::new(&limits);
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(crate::max_out(&limits));
    crate::step(&mut domain, &env, Event::Spawn { client: Token::new(1), start: start() }, &mut out);
    let first = match out.pop().expect("first spawn admitted") {
        Request::Spawn { owner, .. } => owner,
        Request::Started { .. }
        | Request::Admitted { .. }
        | Request::Called { .. }
        | Request::Withdrawn { .. }
        | Request::Turn { .. }
        | Request::Waiting { .. }
        | Request::Rejected { .. }
        | Request::Exhausted { .. }
        | Request::Told { .. }
        | Request::Answered { .. }
        | Request::Faulted { .. }
        | Request::Bounced { .. }
        | Request::Gone { .. }
        | Request::Send { .. }
        | Request::Read { .. }
        | Request::Signal { .. }
        | Request::Wait { .. }
        | Request::Reap { .. } => panic!("expected actual Spawn"),
    };
    crate::step(&mut domain, &env, Event::Spawn { client: Token::new(2), start: start() }, &mut out);
    match out.pop().expect("second spawn refused") {
        Request::Gone { client, end, detail } => {
            assert_eq!(client, Token::new(2));
            assert_eq!(end, End::Busy);
            assert!(detail.is_empty());
        }
        Request::Spawn { .. }
        | Request::Started { .. }
        | Request::Admitted { .. }
        | Request::Called { .. }
        | Request::Withdrawn { .. }
        | Request::Turn { .. }
        | Request::Waiting { .. }
        | Request::Rejected { .. }
        | Request::Exhausted { .. }
        | Request::Told { .. }
        | Request::Answered { .. }
        | Request::Faulted { .. }
        | Request::Bounced { .. }
        | Request::Send { .. }
        | Request::Read { .. }
        | Request::Signal { .. }
        | Request::Wait { .. }
        | Request::Reap { .. } => panic!("expected exact Busy terminal"),
    }
    assert_eq!(domain.agents(), 1);
    crate::step(&mut domain, &env, Event::Unspawned { owner: first, detail: Box::new([]) }, &mut out);
    domain.reclaim();
    assert_eq!(domain.agents(), 0);
}
