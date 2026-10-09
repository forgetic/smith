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
