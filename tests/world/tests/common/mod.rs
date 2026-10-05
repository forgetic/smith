//! A consumer story for the shared domain kit, with no smith implementation
//! (testing-strategy.md, sections 2.2, 6 and 7; domain/README.md, section 4).

use std::collections::BTreeSet;

use skein_lib::{Duration, Rng, Time};
use skein_world::domain::{Expectations, Judge, Ledger, Referee, Schedule, Span, Stage, Trace};

#[derive(Debug)]
enum Seen {
    Issued(u64),
    Ended(u64),
}

#[derive(Debug)]
struct Terminals {
    pending: BTreeSet<u64>,
}

impl Expectations for Terminals {
    type Seen = Seen;
    type Name = u64;
    type Stimulus = ();

    fn observe(&mut self, seen: Seen, judge: &mut Judge<u64, ()>) {
        match seen {
            Seen::Issued(number) => {
                judge.check(self.pending.insert(number), "each request is issued once");
                judge.expect(number, Duration::from_secs(1));
            }
            Seen::Ended(number) => {
                judge.check(self.pending.remove(&number), "each terminal has one issued request");
                let met = judge.meet(&number);
                judge.check(met, "each terminal meets its pending deadline");
            }
        }
    }
}

/// Run the shared kit's numbered-delivery story from an injected seed.
/// Returns its timed boundary trace and final terminal count for exact replay;
/// six events fit the fixture and every delivery must settle within 32 passes
/// (testing-strategy.md, sections 6 and 7; domain/README.md, section 4).
pub(super) fn run(seed: u64) -> (Vec<String>, u32) {
    let mut random = Rng::new(seed);
    let latency = Span::millis(1, 10);
    let mut stage: Stage<(), u64, u64> = Stage::new((), 1, 1);
    let mut deliveries = Schedule::new();
    let mut requests = Ledger::new("consumer request");
    let mut referee = Referee::new(Terminals { pending: BTreeSet::new() });
    let mut stimuli = Vec::new();
    let mut trace = Trace::default();
    for number in 1_u64..=6 {
        stage.push(number);
        requests.open(number, number);
        referee.observe(Time::ZERO, Seen::Issued(number), &mut stimuli);
    }
    let mut now = Time::ZERO;
    let mut ended = 0_u32;
    let mut pressure_seen = false;
    for _ in 0..32 {
        stage.tick(now);
        while let Some(number) = stage.next_event() {
            stage.out.push(number);
            trace.log(now, format_args!("issued {number}"));
        }
        pressure_seen |= stage.has_events() && !stage.has_room();
        while let Some(number) = stage.out.pop() {
            let _delivery = deliveries.send(now.saturating_add(latency.draw(&mut random)), number);
        }
        while let Some(number) = deliveries.next(now) {
            assert_eq!(requests.end(number), number, "the terminal returns its original request");
            referee.observe(now, Seen::Ended(number), &mut stimuli);
            ended = ended.checked_add(1).expect("six fixture requests");
            trace.log(now, format_args!("ended {number}"));
        }
        referee.fire(now, &mut stimuli);
        referee.assert_holding(seed);
        assert!(stimuli.is_empty(), "the terminal referee injects nothing");
        if requests.is_empty() {
            break;
        }
        if !stage.has_events() {
            now = [deliveries.next_time(), referee.next_deadline()]
                .into_iter()
                .flatten()
                .min()
                .expect("pending requests have a delivery or liveness deadline");
        }
    }
    requests.assert_settled();
    referee.assert_passed(seed);
    assert!(deliveries.is_empty(), "every scheduled terminal was consumed");
    assert!(!stage.has_events(), "every input passed the output-room fence");
    assert!(stage.out.is_empty(), "all output ownership moved to deliveries");
    assert!(pressure_seen, "the one-slot stage exercised output pressure");
    assert_eq!(ended, 6, "every request ended exactly once");
    assert_eq!(referee.judged(), (18, 6), "independent safety and liveness checks ran");
    (trace.lines().to_vec(), ended)
}
