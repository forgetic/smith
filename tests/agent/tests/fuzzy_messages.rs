//! Bounded message interleavings and replay on the existing root world.
//! Fixture choices inject inputs; actual boundaries alone classify the endings,
//! read fences and cancellation chronology. The existing message
//! referee checks complete positive histories unchanged; no generic scheduler,
//! referee or provider script is copied here. Two accepted names per world fit
//! the existing six-turn script: observed 0→99 and 99→7 pairs span the sweep,
//! never a claimed single 0→99→7 conversation.
//! Contract: domain/run.md, sections 6, 10, 13 and 14; testing-strategy.md,
//! sections 3, 6, 7 and 8; programming-model.md, section 10.2.

use skein_lib::{Duration, Rng, Time, Token};
use skein_world::domain::{Referee, Span, assert_replays};
use smith_agent_world::{
    Job, Settings, World,
    messages_referee::{Meeting, Seen},
};
use smith_domain::run;

const SEEDS: [u64; 16] = [0, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377, 610, 987];
const CLASSES: [&str; 16] = [
    "Parked",
    "Budget(Time)",
    "Cancelled",
    "read zero",
    "descending99→7",
    "read advances",
    "input during call",
    "input after actual Waiting",
    "Completed after actual Cancel",
    "actual Cancelled completion",
    "actual inputs",
    "actual Turns",
    "actual Waiting",
    "input before first Completed",
    "input after Completed",
    "zero→99",
];

#[derive(Clone, Copy, Debug)]
enum Fixture {
    Idle,
    Wall,
    WakeCancel,
    EarlyCancel,
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    counts: [u64; 16],
    answer: String,
    fences: Vec<Option<Token>>,
    answered_at: Time,
}

fn settings(seed: u64, fixture: Fixture) -> Settings {
    let calm = Settings::calm(seed);
    let wall = matches!(fixture, Fixture::Wall);
    Settings {
        job: Job::Waiting,
        waiting: Duration::from_secs(if wall { 10 } else { 3 }),
        budget: run::Budget { time: if wall { Duration::from_secs(2) } else { calm.budget.time }, ..calm.budget },
        limits: smith_domain::Limits {
            run: run::Limits { messages: 2, message_bytes: 16, ..calm.limits.run },
            ..calm.limits
        },
        provider: skein_fake_llm_domain::Config {
            latency_min: Duration::from_millis(100),
            latency_max: Duration::from_millis(100),
            ..calm.provider
        },
        network: Span::millis(10, 20),
        races: 500,
        ..calm
    }
}

fn inputs(seed: u64, late: bool) -> Vec<(Time, Token, Box<[u8]>)> {
    let mut rng = Rng::new(seed ^ 0x619d);
    let first = if seed.is_multiple_of(2) { 0 } else { 99 };
    let next = if first == 0 { 99 } else { 7 };
    let burst = 50 + rng.below(10);
    let mut inputs = Vec::new();
    inputs.push((
        Time::ZERO.saturating_add(Duration::from_millis(burst)),
        Token::new(first),
        b"person: first".as_slice().into(),
    ));
    if !late {
        inputs.push((
            Time::ZERO.saturating_add(Duration::from_millis(burst + 1)),
            Token::new(next),
            b"person: second".as_slice().into(),
        ));
    }
    if late {
        inputs.push((
            Time::ZERO.saturating_add(Duration::from_millis(1000 + rng.below(50))),
            Token::new(next),
            b"person: second".as_slice().into(),
        ));
    }
    inputs
}

fn world(seed: u64, fixture: Fixture, cancel_at: Option<Duration>, late: bool) -> World {
    let mut world = World::new(Settings { cancel_at, ..settings(seed, fixture) });
    for (at, name, text) in inputs(seed, late) {
        world.message_at(at, name, b"person".as_slice().into(), text[8..].into());
    }
    world
}

fn cancellation_anchor(seed: u64, fixture: Fixture, late: bool) -> Option<Duration> {
    if !matches!(fixture, Fixture::WakeCancel | Fixture::EarlyCancel) {
        return None;
    }
    let mut baseline = world(seed, Fixture::Idle, None, late);
    baseline.run(2000);
    let earliest = if late { inputs(seed, true).last().expect("actual wake fixture input").0 } else { Time::ZERO };
    // Anchor the text request after Wait has settled. The unchanged oracle
    // requires actual Wait feedback; it must not be fed an invented waiting
    // result for a late ToolUse that legitimately remained unstarted on Cancel.
    let (started, returned) = baseline
        .messages_seen()
        .iter()
        .enumerate()
        .find_map(|(index, (at, seen))| {
            let Seen::Completed { parts } = seen else { return None };
            if !matches!(parts.as_slice(), [skein_fake_llm_domain::api::Part::Text { .. }]) {
                return None;
            }
            let prompt = baseline.messages_seen()[..index]
                .iter()
                .rposition(|(_, seen)| matches!(seen, Seen::Prompt { .. }))
                .expect("actual text request precedes its terminal");
            let started = baseline.messages_seen()[prompt].0;
            (started >= earliest).then_some((started, *at))
        })
        .expect("actual initial/wake text request and matching completion establish the race edges");
    let mut rng = Rng::new(seed ^ 0x21ab);
    let cancel = started.saturating_add(Duration::from_millis(100 + rng.between(1, 9)));
    assert!(
        cancel > started && cancel < returned,
        "seed {seed}: anchored Cancel lies before the actual bounded terminal {started:?}/{cancel:?}/{returned:?}"
    );
    Some(cancel.saturating_since(Time::ZERO))
}

fn chronology(world: &World, counts: &mut [u64; 16]) {
    // This literal is the original root's parent Cancel entrance. It cannot
    // match an outgoing provider callback Cancel or a tools cancellation.
    let cancels = world
        .trace()
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("agent <- Cancel { run: "))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert!(cancels.len() <= 1, "one actual parent Cancel for the original admitted run");
    if let Some(cancel) = cancels.first() {
        counts[8] += u64::try_from(
            world.trace()[cancel + 1..].iter().filter(|line| line.contains("agent <- Completed { owner: ")).count(),
        )
        .expect("bounded late completions");
        counts[9] += u64::try_from(
            world.trace()[cancel + 1..].iter().filter(|line| line.contains("agent <- Cancelled { owner: ")).count(),
        )
        .expect("bounded cancellation terminals");
    }
}

fn observations(world: &World, seed: u64, idle: Duration) -> [u64; 16] {
    let mut referee = Referee::new(Meeting::new(0, idle));
    let mut stimuli = Vec::new();
    let mut counts = [0; 16];
    let (mut calling, mut waiting, mut completed) = (false, false, false);
    let mut read = None;
    let answer = world
        .messages_seen()
        .iter()
        .position(|(_, seen)| matches!(seen, Seen::Answer { .. }))
        .expect("one actual final Answer observation");
    for (index, (at, seen)) in world.messages_seen().iter().enumerate() {
        referee.observe(*at, seen.clone(), &mut stimuli);
        match seen {
            Seen::Input { .. } => {
                assert!(index < answer, "seed {seed}: every actual input precedes the actual final Answer");
                counts[10] += 1;
                counts[6] += u64::from(calling);
                counts[7] += u64::from(waiting);
                counts[13 + usize::from(completed)] += 1;
                waiting = false;
            }
            Seen::Prompt { .. } => {
                calling = true;
                waiting = false;
            }
            Seen::Completed { .. } => {
                calling = false;
                completed = true;
            }
            Seen::CompletionEnded => calling = false,
            Seen::Waiting { .. } => {
                waiting = true;
                counts[12] += 1;
            }
            Seen::Turn { read: actual, .. } => {
                counts[11] += 1;
                if *actual != read {
                    counts[5] += 1;
                    counts[3] += u64::from(*actual == Some(Token::new(0)));
                    counts[4] += u64::from(read == Some(Token::new(99)) && *actual == Some(Token::new(7)));
                    counts[15] += u64::from(read == Some(Token::new(0)) && *actual == Some(Token::new(99)));
                    read = *actual;
                }
            }
            Seen::Started { .. } | Seen::Admitted | Seen::Answer { .. } => {}
        }
        referee.assert_holding(seed);
    }
    referee.assert_passed(seed);
    assert!(stimuli.is_empty(), "existing message oracle chooses no alternate schedule");
    match world.answer() {
        run::Answer::Parked { .. } => counts[0] += 1,
        run::Answer::Failed { failure: run::Failure::Budget(run::Exhausted::Time), .. } => counts[1] += 1,
        run::Answer::Failed { failure: run::Failure::Cancelled, .. } => counts[2] += 1,
        answer @ (run::Answer::Refused(_) | run::Answer::Accepted { .. } | run::Answer::Failed { .. }) => {
            panic!("seed {seed}: unexpected actual message ending {answer:?}")
        }
    }
    chronology(world, &mut counts);
    counts
}

fn run(seed: u64, fixture: Fixture) -> (Vec<String>, Outcome) {
    let late = !matches!(fixture, Fixture::EarlyCancel);
    let cancel = cancellation_anchor(seed, fixture, late);
    let mut world = world(seed, fixture, cancel, late);
    world.run(2000);
    assert_eq!(world.judged().1, 1, "seed {seed}: original Start answered exactly once");
    assert_eq!(world.lost(), 0, "seed {seed}: facts capacity preserved");
    let actual = world
        .messages_seen()
        .iter()
        .filter_map(|(at, seen)| match seen {
            Seen::Input { name, text } => Some((*at, *name, text.clone())),
            Seen::Started { .. }
            | Seen::Admitted
            | Seen::Prompt { .. }
            | Seen::Completed { .. }
            | Seen::CompletionEnded
            | Seen::Turn { .. }
            | Seen::Waiting { .. }
            | Seen::Answer { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, inputs(seed, late), "seed {seed}: every complete scheduled input actually arrived unchanged");
    let outcome = Outcome {
        counts: observations(&world, seed, settings(seed, fixture).waiting),
        answer: format!("{:?}", world.answer()),
        fences: world.turn_metadata().iter().map(|(_, read, _)| *read).collect(),
        answered_at: world.answered_at(),
    };
    (world.trace().into(), outcome)
}

#[test]
fn bounded_message_schedules_replay_with_every_required_class() {
    let mut observed = [0_u64; 16];
    for seed in SEEDS {
        for fixture in [Fixture::Idle, Fixture::Wall, Fixture::WakeCancel, Fixture::EarlyCancel] {
            eprintln!("message sweep seed {seed}, fixture {fixture:?}");
            let trace = assert_replays(seed, seed ^ 0x819a, |seed| run(seed, fixture));
            assert!(!trace.is_empty());
            let (_, outcome) = run(seed, fixture);
            for (total, count) in observed.iter_mut().zip(outcome.counts) {
                *total += count;
            }
        }
    }
    eprintln!("actual message classes {CLASSES:?}: {observed:?}");
    for (name, count) in CLASSES.iter().zip(observed) {
        assert!(count > 0, "required actual {name} class did not occur: {observed:?}");
    }
}
