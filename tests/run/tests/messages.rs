//! Actual run-world message terminals and positive-first mutations.
//! Contract: domain/run.md, sections 6 and 13.
use skein_lib::{Time, Token};
use skein_world::domain::Verdict;
use smith_run_world::{
    Settings, World,
    messages_referee::{Seen, judge},
};

fn observed() -> Vec<(Time, Token, Seen)> {
    let mut settings = Settings::calm(917);
    settings.run.facts = 512;
    settings.host.jobs = 1;
    settings.host.turns_min = 8;
    settings.host.turns_max = 8;
    settings.partner.yields = 1000;
    let mut world = World::new(settings);
    world.inject_messages(1000);
    world.run(100_000);
    assert_eq!(world.facts().1, 0, "the observation queue was fully drained");
    let seen = world.message_seen().to_vec();
    assert!(seen.iter().any(|(_, _, seen)| matches!(seen, Seen::Read { .. })));
    assert!(seen.iter().any(|(_, _, seen)| matches!(seen, Seen::Unread { .. })));
    assert!(judge(&seen).iter().all(|verdict| *verdict == Verdict::Passed));
    seen
}

#[test]
fn every_accepted_message_has_one_terminal_and_the_answer_retains_its_told_fence() {
    let good = observed();
    for change in 0..4 {
        let mut bad = good.clone();
        let terminal = bad.iter().position(|(_, _, seen)| matches!(seen, Seen::Read { .. })).expect("actual read");
        match change {
            0 => {
                bad.remove(terminal);
            }
            1 => {
                bad.insert(terminal, bad[terminal]);
            }
            2 => {
                let (_, _, fence) = bad
                    .iter_mut()
                    .find(|(_, _, seen)| matches!(seen, Seen::Fence { read: Some(_) }))
                    .expect("told fence");
                *fence = Seen::Fence { read: Some(Token::new(0)) };
            }
            3 => {
                let (_, _, answer) =
                    bad.iter_mut().find(|(_, _, seen)| matches!(seen, Seen::Answer { .. })).expect("answer");
                *answer = Seen::Answer { read: Some(Token::new(0)) };
            }
            _ => unreachable!(),
        }
        assert!(judge(&bad).iter().any(|verdict| matches!(verdict, Verdict::Failed(_))), "mutation {change}");
    }
}

#[test]
fn a_start_carrying_messages_opens_on_the_briefs_instruction_then_the_messages_in_order() {
    let mut settings = Settings::calm(918);
    settings.host.jobs = 1;
    settings.host.turns_min = 8;
    settings.host.turns_max = 8;
    settings.partner.yields = 1000;
    let mut world = World::new(settings);
    world.carry_messages(Box::new([
        smith_domain_run::Message {
            name: Token::new(99),
            label: b"person".as_slice().into(),
            text: b"first".as_slice().into(),
        },
        smith_domain_run::Message {
            name: Token::new(0),
            label: b"person".as_slice().into(),
            text: b"second".as_slice().into(),
        },
    ]));
    world.run(100_000);
    assert_eq!(
        world.opening_prompts()[0].as_ref(),
        b"Begin the work your brief describes.\n\nperson: first\n\nperson: second"
    );
    let read: Vec<_> = world
        .message_seen()
        .iter()
        .filter_map(|(_, _, seen)| if let Seen::Read { name } = seen { Some(*name) } else { None })
        .collect();
    assert_eq!(read, [Token::new(99), Token::new(0)]);
    assert!(judge(world.message_seen()).iter().all(|verdict| *verdict == Verdict::Passed));
}

#[test]
fn an_empty_brief_without_wait_opens_on_its_instructions() {
    let mut settings = Settings::calm(919);
    settings.host.jobs = 1;
    settings.host.turns_min = 8;
    settings.host.turns_max = 8;
    settings.partner.yields = 1000;
    let mut world = World::new(settings);
    world.empty_brief(false);
    world.run(100_000);
    assert_eq!(world.opening_prompts()[0].as_ref(), b"Begin the work your instructions describe.");
}

#[test]
fn an_empty_brief_parks_at_idle_wall_or_zero_waiting_without_opening() {
    use skein_lib::Duration;
    use skein_world::domain::Span;
    for (idle, wall) in [(7, 30), (30, 7), (0, 30)] {
        let mut settings = Settings::calm(920 + idle);
        settings.host.jobs = 1;
        settings.host.time = Span { min: Duration::from_secs(wall), max: Duration::from_secs(wall) };
        settings.network = Span::millis(0, 0);
        settings.hop = Span::millis(0, 0);
        settings.checkout.io = Span::millis(0, 0);
        let mut world = World::new(settings);
        world.empty_brief(true);
        world.waiting_time(Duration::from_secs(idle));
        world.run(100_000);
        assert!(world.opening_prompts().is_empty());
        let answers: Vec<_> = world.answers().map(|answer| answer.expect("one terminal")).collect();
        assert!(
            matches!(answers.as_slice(), [smith_domain_run::Answer::Parked { spent, turns: 0 }] if *spent == smith_domain_run::Spend::ZERO)
        );
    }
}

#[test]
fn an_empty_brief_opens_on_the_first_message_after_its_actual_awaiting_notice() {
    let mut settings = Settings::calm(921);
    settings.host.jobs = 1;
    settings.host.turns_min = 8;
    settings.host.turns_max = 8;
    settings.partner.yields = 1000;
    let mut world = World::new(settings);
    world.empty_brief(true);
    world.message_when_awaiting(smith_domain_run::Message {
        name: Token::new(0),
        label: b"person".as_slice().into(),
        text: b"first work".as_slice().into(),
    });
    world.run(100_000);
    assert_eq!(world.opening_prompts()[0].as_ref(), b"person: first work");
    assert_eq!(world.stats().opens, 1);
    assert!(
        world.message_seen().iter().any(|(_, _, seen)| matches!(seen, Seen::Read { name } if *name == Token::new(0)))
    );
    assert!(judge(world.message_seen()).iter().all(|verdict| *verdict == Verdict::Passed));
}
