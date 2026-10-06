//! Independent actual-agent landing chronology. The kit knows issued channel
//! metadata; this oracle additionally observes the scripted agent's real stop
//! decision/Cancel receipt (domain/host.md, sections 2, 4 and 10; domain/run.md, sections 8.2 and 10).
use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Expectations, Judge, Referee, Verdict};
use smith_host_domain::{Answer, Ask, CallName, Delivered, Delivery, Event, Receipt, Reply, RunFailure, RunResult, Up};
use smith_host_world::{World, limits};

#[derive(Debug)]
enum Observation {
    Pending(CallName),
    ActualAgentStop(RunFailure),
    ActualLanding(CallName, Delivered),
    Final(Answer),
}
#[derive(Debug, Default)]
struct Evidence {
    pending: Option<CallName>,
    stopped: Option<(CallName, RunFailure)>,
    landed: Option<(CallName, Delivered)>,
    required: Option<RunFailure>,
}
impl Expectations for Evidence {
    type Seen = Observation;
    type Name = &'static str;
    type Stimulus = ();
    fn observe(&mut self, seen: Observation, judge: &mut Judge<&'static str, ()>) {
        match seen {
            Observation::Pending(name) => {
                assert!(self.pending.replace(name).is_none());
            }
            Observation::ActualAgentStop(failure) => {
                if let Some(name) = self.pending {
                    self.stopped = Some((name, failure));
                }
            }
            Observation::ActualLanding(name, receipts) => {
                if self.pending.take() != Some(name) {
                    judge.fail(format_args!("actual landing must end its pending name"));
                }
                if let Some((stopped_name, failure)) = self.stopped
                    && stopped_name == name
                {
                    self.required = Some(failure);
                }
                self.landed = Some((name, receipts));
            }
            Observation::Final(answer) => match answer.result {
                RunResult::Delivered { name, receipts, stopped } => {
                    match &self.landed {
                        Some((actual_name, actual_receipts))
                            if *actual_name == name && *actual_receipts == receipts => {}
                        Some(_) | None => {
                            judge.fail(format_args!("final landing must match actual stable name and receipts"));
                        }
                    }
                    if self.required != Some(stopped) {
                        judge.fail(format_args!(
                            "interrupted classification needs actual preceding stop and its unchanged cause"
                        ));
                    }
                }
                RunResult::Refused { .. }
                | RunResult::Accepted { .. }
                | RunResult::Parked
                | RunResult::Failed { .. } => {
                    if self.required.is_some() {
                        judge.fail(format_args!("actual interrupted landing evidence must not be omitted"));
                    }
                }
            },
        }
    }
}
fn receipts() -> Delivered {
    Delivered::new(Box::new([Receipt::new(0, Box::from(&b"real landing"[..])).expect("receipt")])).expect("delivery")
}
fn name() -> CallName {
    CallName { activation: 1, completion: 2, position: 3 }
}
fn observe(referee: &mut Referee<Evidence>, observation: Observation) {
    referee.observe(Time::ZERO, observation, &mut Vec::new());
}
fn failed(referee: &Referee<Evidence>) -> bool {
    match referee.verdict() {
        Verdict::Failed(_) => true,
        Verdict::Passed | Verdict::Open { .. } | Verdict::Stopped { .. } => false,
    }
}

#[test]
fn actual_agent_interruption_plus_successful_landing_requires_full_final_evidence() {
    let mut world = World::new(60, limits());
    world.live();
    let mut oracle = Referee::new(Evidence::default());
    world.up(Up::Call {
        call: Token::new(20),
        name: name(),
        deadline: Time::ZERO.saturating_add(Duration::from_secs(20)),
        ask: Ask::Deliver { fields: Box::new([]) },
    });
    observe(&mut oracle, Observation::Pending(name()));
    world.event(Event::Stop { agent: world.agent() });
    world.sent();
    // Script explicitly consumes the delivered Cancel before the operation
    // response. Parent Stop alone is intentionally absent from the oracle.
    observe(&mut oracle, Observation::ActualAgentStop(RunFailure::Cancelled));
    world.event(Event::Answer {
        agent: world.agent(),
        call: Token::new(20),
        reply: Reply::Delivery(Delivery::Delivered(receipts())),
    });
    world.sent();
    observe(&mut oracle, Observation::ActualLanding(name(), receipts()));
    let answer = Answer {
        turns: 0,
        completions: 2,
        input: 11,
        output: 13,
        cache_read: 17,
        cache_write: 19,
        spent: 10,
        spend_overflow: false,
        usage_overflow: false,
        result: RunResult::Delivered { name: name(), receipts: receipts(), stopped: RunFailure::Cancelled },
    };
    world.up(Up::Answer { answer });
    assert_eq!(world.seen.fault, None);
    let actual = world.seen.answer.take().expect("actual forwarded Request::Answered");
    assert_eq!(
        actual,
        Answer {
            turns: 0,
            completions: 2,
            input: 11,
            output: 13,
            cache_read: 17,
            cache_write: 19,
            spent: 10,
            spend_overflow: false,
            usage_overflow: false,
            result: RunResult::Delivered { name: name(), receipts: receipts(), stopped: RunFailure::Cancelled }
        },
        "real last-word payload survives unchanged"
    );
    observe(&mut oracle, Observation::Final(actual));
    oracle.assert_holding(60);
    world.cleanup();
    world.settled();
}

#[test]
fn actual_landing_before_later_agent_stop_has_no_interrupted_requirement() {
    let mut oracle = Referee::new(Evidence::default());
    observe(&mut oracle, Observation::Pending(name()));
    observe(&mut oracle, Observation::ActualLanding(name(), receipts()));
    observe(&mut oracle, Observation::ActualAgentStop(RunFailure::Cancelled));
    observe(
        &mut oracle,
        Observation::Final(Answer {
            turns: 0,
            completions: 2,
            input: 11,
            output: 13,
            cache_read: 17,
            cache_write: 19,
            spent: 10,
            spend_overflow: false,
            usage_overflow: false,
            result: RunResult::Failed { failure: RunFailure::Cancelled },
        }),
    );
    assert!(!failed(&oracle));
}

#[test]
fn oracle_rejects_omitted_invented_mismatched_and_later_stop_landing_evidence() {
    for mutation in 0..6 {
        let mut oracle = Referee::new(Evidence::default());
        if mutation != 1 {
            observe(&mut oracle, Observation::Pending(name()));
        }
        if mutation != 4 && mutation != 1 {
            observe(&mut oracle, Observation::ActualAgentStop(RunFailure::Cancelled));
        }
        if mutation != 1 {
            observe(&mut oracle, Observation::ActualLanding(name(), receipts()));
        }
        if mutation == 4 {
            observe(&mut oracle, Observation::ActualAgentStop(RunFailure::Cancelled));
        }
        let result = if mutation == 0 {
            RunResult::Failed { failure: RunFailure::Cancelled }
        } else {
            RunResult::Delivered {
                name: if mutation == 2 { CallName { activation: 1, completion: 3, position: 3 } } else { name() },
                receipts: if mutation == 3 {
                    Delivered::new(Box::new([Receipt::new(0, Box::from(&b"invented"[..])).expect("receipt")]))
                        .expect("delivery")
                } else {
                    receipts()
                },
                stopped: if mutation == 5 {
                    RunFailure::Budget(smith_host_domain::Exhausted::Time)
                } else {
                    RunFailure::Cancelled
                },
            }
        };
        observe(
            &mut oracle,
            Observation::Final(Answer {
                turns: 0,
                completions: 2,
                input: 11,
                output: 13,
                cache_read: 17,
                cache_write: 19,
                spent: 10,
                spend_overflow: false,
                usage_overflow: false,
                result,
            }),
        );
        assert!(failed(&oracle), "negative mutation {mutation}");
    }
}
