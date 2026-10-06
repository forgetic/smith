//! The agent's run child domain at random: many noisy worlds, each settled with
//! every start answered once, with broad ending coverage among them.

use std::collections::BTreeSet;

use smith_domain_run::{Answer, Exhausted, Failure, Fault, Invalid, Policy, Refusal};
use smith_run_world::{Settings, World, noisy};

const ITERATIONS: u32 = 1_000_000;

fn settled(settings: &Settings) -> World {
    let mut world = World::new(*settings);
    world.run(ITERATIONS);
    world
}

fn answers(world: &World) -> Vec<&Answer> {
    world.answers().map(|answer| answer.expect("every start is answered")).collect()
}

/// Hundreds of worlds with random limits, scripts, faults and schedules: each
/// settles with its invariants holding (checked by `World::run`) and reaches
/// the listed endings. A dedicated focused story covers stale as a final answer.
#[test]
fn random_worlds_settle_with_every_start_answered_once() {
    let mut seen = BTreeSet::new();
    let (mut cancels, mut races, mut stale) = (0, 0, 0);
    let (mut waited, mut delivered, mut host_answered) = (0, 0, 0);
    let (mut busy, mut lost, mut answered) = (0, 0, 0);
    for seed in 0..300 {
        let world = settled(&noisy(seed));
        let stats = world.stats();
        cancels += stats.cancels;
        races += stats.partner.races;
        stale += stats.partner.stale;
        waited += stats.partner.waiting;
        delivered += stats.partner.delivered;
        host_answered += stats.partner.host_answered;
        busy += stats.relay_busy;
        lost += stats.relay_lost;
        answered += stats.relay_answered;
        for answer in answers(&world) {
            let kind = match answer {
                Answer::Parked { .. } => "parked",
                Answer::Accepted { .. } => "accepted",
                Answer::Refused(Refusal::Busy) => "busy",
                Answer::Refused(Refusal::Invalid(Invalid::Conversation)) => "conversation invalid",
                Answer::Refused(Refusal::Invalid(_)) => "invalid",
                Answer::Failed { failure, .. } => match failure {
                    Failure::Model(
                        Fault::Completion { .. } | Fault::Provider | Fault::ContextFull | Fault::Exhausted,
                    ) => "fault",
                    Failure::Model(Fault::Truncated | Fault::Refused | Fault::Malformed) => "stopped",
                    Failure::Budget(Exhausted::Turns) => "turns",
                    Failure::Budget(Exhausted::Time) => "time",
                    Failure::Budget(Exhausted::Spend) => "spend",
                    Failure::Budget(Exhausted::Overflow(_)) => {
                        panic!("bounded random partner usage and prices remain representable")
                    }
                    Failure::Budget(Exhausted::Tokens(_)) => panic!("source random partner has no receiving refusal"),
                    Failure::Policy(Policy::Unfinished { .. }) => "unfinished",
                    Failure::Cancelled => "cancelled",
                    Failure::Stale => "stale",
                    Failure::Transcript(_) => panic!("source random partner does not restore history"),
                },
            };
            seen.insert(kind);
        }
    }
    let expected = [
        "accepted",
        "busy",
        "cancelled",
        "conversation invalid",
        "fault",
        "invalid",
        "parked",
        "stopped",
        "time",
        "spend",
        "turns",
        "unfinished",
    ];
    assert!(expected.iter().all(|kind| seen.contains(kind)), "missing sampled ending: {seen:?}");
    // And the races: turns spent after a close, a close or nudge crossing a
    // conversation's own end, conversations out of time, cancels.
    assert!(cancels > 0 && races > 0 && stale > 0, "{cancels} {races} {stale}");
    assert!(
        waited > 0 && delivered > 0 && host_answered > 0 && busy > 0 && lost > 0 && answered > 0,
        "wait {waited}, delivered {delivered}, host answered {host_answered}, relays {busy}/{lost}/{answered}"
    );
}
