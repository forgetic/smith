//! The shared parent referee rejects lost rights and duplicate terminals.
use skein_lib::Token;
use smith_agent_world::Job;
use smith_host_domain::{self as host, End, RunResult, parent};
use smith_host_world::{
    Seen,
    inline::{self, World},
};

fn spawn(world: &mut World, client: u64, job: Job) {
    world.event(parent::Event::Spawn { client: Token::new(client), start: inline::start(client, job) });
}

#[test]
#[should_panic(expected = "Gone retains every lower terminal")]
fn the_common_parent_referee_rejects_a_gone_with_an_open_inline_completion() {
    Seen::default().observe_parent(
        parent::Request::Gone { client: Token::new(1), end: End::Stopped, detail: Box::default() },
        true,
        false,
    );
}

#[test]
#[should_panic]
fn the_common_parent_referee_rejects_two_run_answers() {
    let mut seen = Seen::default();
    for _ in 0..2 {
        seen.observe_parent(
            parent::Request::Answered {
                client: Token::new(1),
                answer: host::Answer { read: None, turns: 0, spent: 0, result: RunResult::Parked },
            },
            true,
            true,
        );
    }
}

#[test]
#[should_panic]
fn the_common_parent_referee_rejects_a_gone_with_an_unanswered_parent_call() {
    let mut seen = Seen::default();
    seen.calls.insert(Token::new(2));
    seen.observe_parent(
        parent::Request::Gone { client: Token::new(1), end: End::Stopped, detail: Box::default() },
        true,
        true,
    );
}

#[test]
#[should_panic(expected = "one actual parent terminal")]
fn the_common_parent_referee_rejects_two_answers_to_one_inline_call() {
    let mut world = World::new(319, inline::limits());
    world.auto_answer = false;
    spawn(&mut world, 1, Job::HostTools);
    for _ in 0..100 {
        if !world.seen[&Token::new(1)].calls.is_empty() {
            break;
        }
        world.at(world.next_deadline().expect("actual provider alarm"));
    }
    let agent = world.handle(Token::new(1));
    let call = *world.seen[&Token::new(1)].calls.first().expect("actual relayed call");
    for _ in 0..2 {
        world.event(parent::Event::Answer {
            agent,
            call,
            reply: host::Reply::Host { error: false, body: b"host completed".as_slice().into() },
        });
    }
}
