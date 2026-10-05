//! Memory stays within the worst case (programming-model.md, section 6.3), measured by
//! a counting allocator: the run child domain with every run holding a charter
//! of exactly its byte limit, and every conversation started and spending.

use std::mem::size_of;

use skein_lib::{Duration, Env, Queue, ReplyTo, Time, Token, Wall};
use skein_world::domain::heap::{self, Meter};
use smith_domain_run::charter::{Checkout, Endpoint, Families, Grants, Llm, Outlet, Repository, Tools};
use smith_domain_run::outcome::{Change, ChangeSpec, Declared, Field, FieldRule, ItemRule, OutcomeSpec, VerdictRule};
use smith_domain_run::{
    Answer, Ask, Budget, Charter, Domain, End, Event, Exit, Invalid, Limits, MAX_OUT, Ran, Read, Refusal, Request,
    Spend, Stop, worst_case,
};

#[global_allocator]
static HEAP: heap::Counting = heap::Counting;

fn bytes(len: u64) -> Box<[u8]> {
    vec![b'x'; usize::try_from(len).expect("a test length fits")].into_boxed_slice()
}

fn size(of: usize) -> u64 {
    u64::try_from(of).expect("a size fits")
}

const BUDGET: Budget = Budget {
    turns: 10,
    input: 1000,
    output: 1000,
    cache_read: 1000,
    cache_write: 1000,
    time: Duration::from_secs(3600),
};

const LIMITS: Limits = Limits {
    runs: 1,
    conversations: 2,
    run_bytes: 1024,
    repositories: 1,
    outlets: 1,
    verdicts: 1,
    calls: 2,
    budget: BUDGET,
    max_tokens: 1024,
    models: 1,
    depth: 1,
    run_conversations: 2,
    answer_bytes: 128,
    nudges: 1,
    guide_bytes: 512,
    io_timeout: Duration::from_secs(5),
    outcome_bytes: 256,
    delivery_timeout: Duration::from_secs(60),
    check_timeout: Duration::from_secs(60),
    check_tail: 1024,
    facts: 16,
};

/// A charter that holds exactly `held` bytes, as the run counts them: one of
/// every part held in a box, at its fixed size plus a byte of payload each,
/// and a brief of the rest.
fn charter(held: u64) -> Charter {
    let parts =
        (size(size_of::<Repository>()) + 1) + (size(size_of::<Outlet>()) + 1) + 1 + (size(size_of::<Llm>()) + 1);
    let rule = size(size_of::<VerdictRule>()) + 1 + size(size_of::<ItemRule>()) + 1 + size(size_of::<FieldRule>()) + 1;
    let change_rules = 2 * size(size_of::<FieldRule>()) + 5 + 4;
    Charter {
        brief: bytes(held - parts - rule - change_rules),
        checkout: Checkout {
            repositories: Box::new([Repository { name: bytes(1), root: Token::new(1), writable: true }]),
        },
        grants: Grants {
            deliver: None,
            tools: Tools { inspect: true, modify: true, shell: true },
            forge: true,
            agents: true,
            outlets: Box::new([Outlet { name: bytes(1) }]),
        },
        outcome: OutcomeSpec {
            change: Some(ChangeSpec {
                fields: Box::new([
                    smith_domain_run::outcome::FieldRule { name: b"title".as_slice().into(), max: 1024 },
                    smith_domain_run::outcome::FieldRule { name: b"body".as_slice().into(), max: 1024 },
                ]),
            }),
            verdicts: Box::new([VerdictRule {
                name: bytes(1),
                text_max: 1024,
                fields: Box::new([]),
                items: smith_domain_run::outcome::ItemSpec {
                    min: 0,
                    max: 1,
                    kinds: {
                        let required: Box<[Box<[u8]>]> = Box::new([bytes(1)]);
                        let kinds: Box<[Box<[u8]>]> = Box::new([bytes(1)]);
                        kinds
                            .into_vec()
                            .into_iter()
                            .map(|kind| smith_domain_run::outcome::ItemRule {
                                kind,
                                fields: required
                                    .iter()
                                    .map(|name| smith_domain_run::outcome::FieldRule { name: name.clone(), max: 1024 })
                                    .collect(),
                            })
                            .collect()
                    },
                },
            }]),
            report: None,
            failure: None,
        },
        budget: BUDGET,
        llm: Llm { account: 0, endpoint: Endpoint(0), model: bytes(1), max_tokens: 1 },
        models: Box::new([Llm { account: 0, endpoint: Endpoint(0), model: bytes(1), max_tokens: 1 }]),
    }
}

/// What a step asked for, without the payload.
#[derive(PartialEq, Eq, Debug)]
enum Asked {
    Read { owner: Token },
    Probe { owner: Token },
    Check { owner: Token },
    Open { conversation: Token },
    Answer { answer: Answer },
    Other,
}

/// Fills every run of a domain under `limits` with a charter of exactly its
/// byte limit and a guide of exactly its limit too, and has each one's main
/// conversation start, spend, yield, be nudged, ask for a sub-agent that
/// answers with more than the answer limit, and finish with a change of
/// exactly the outcome limit, which is checked and pushed. A sub-agent's call
/// holds the answer, cut at the limit, while the child is being closed, and
/// moves it into its return once the child has ended. Each run ends winding
/// down with the change, while its landing, returned, still holds its copy
/// until the reclaim point. The peak of the heap in every step is checked
/// against the worst case.
fn fill(limits: Limits) {
    let bound = worst_case(&limits).expect("the test limits fit");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(MAX_OUT);
    let meter = Meter::new();
    let mut domain = Domain::new(&limits);
    // The requests are the parent's to route and their receivers' to count:
    // each is dropped, keeping only what it asked for, and the step's peak
    // checked less them.
    let mut step = |event: Event| -> Vec<Asked> {
        meter.start();
        smith_domain_run::step(&mut domain, &env, event, &mut out);
        let measured = meter.end();
        let mut asked = Vec::new();
        while let Some(request) = out.pop() {
            asked.push(match request {
                Request::Open { conversation, .. } => Asked::Open { conversation },
                Request::Read { owner, .. } => Asked::Read { owner },
                Request::Probe { owner, .. } => Asked::Probe { owner },
                Request::Check { owner, .. } => Asked::Check { owner },
                Request::Answer { answer, to: _ } => Asked::Answer { answer },
                Request::Admitted { .. }
                | Request::Say { .. }
                | Request::Close { .. }
                | Request::Abort { .. }
                | Request::Checking { .. }
                | Request::Deliver { .. }
                | Request::Return { .. } => Asked::Other,
            });
        }
        meter.check(measured, bound, &limits);
        asked
    };
    let spend = Spend { turns: 1, input: 1, output: 1, cache_read: 1, cache_write: 1 };
    let expiry = Time::ZERO.saturating_add(limits.budget.time);
    for run in 0..limits.runs {
        let worker = Token::new(u64::from(run));
        let start = Event::Start { reply_to: ReplyTo::new(worker), worker, charter: charter(limits.run_bytes) };
        let [Asked::Other, Asked::Read { owner }] = step(start)[..] else {
            panic!("a charter of exactly the byte limit is admitted");
        };
        let read = Read::Text { text: bytes(u64::from(limits.guide_bytes)), whole: false };
        let [Asked::Probe { owner }] = step(Event::Read { owner, read })[..] else {
            panic!("the run looks for the checks of its writable repository");
        };
        let [Asked::Open { conversation }] = step(Event::Probed { owner, executable: true })[..] else {
            panic!("the run opens main once it has prepared");
        };
        assert!(step(Event::Started { conversation, peer: worker }).is_empty(), "starting is quiet");
        assert!(step(Event::Used { conversation, spend }).is_empty(), "within the budget");
        let yielded = Event::Yielded { conversation, stop: Stop::EndTurn, text: bytes(100) };
        assert_eq!(step(yielded), [Asked::Other], "nudged");
        let families =
            Families { tools: Tools { inspect: true, modify: false, shell: false }, forge: false, agents: false };
        let ask = Ask::SubAgent { brief: bytes(10), families, llm: Some(bytes(1)), share: None };
        let delegated = Event::Delegated {
            name: smith_domain_run::CallName { completion: 1, position: 0 },
            conversation,
            call: worker,
            ask,
            deadline: expiry,
        };
        let [Asked::Open { conversation: child }] = step(delegated)[..] else {
            panic!("the sub-agent opens");
        };
        assert!(step(Event::Started { conversation: child, peer: worker }).is_empty(), "starting is quiet");
        assert!(step(Event::Used { conversation: child, spend }).is_empty(), "within the budget");
        let answer = bytes(u64::from(limits.answer_bytes) + 10);
        let yielded = Event::Yielded { conversation: child, stop: Stop::EndTurn, text: answer };
        assert_eq!(step(yielded), [Asked::Other], "the sub-agent is closed");
        let ended = Event::Ended { conversation: child, end: End::Closed, spend };
        assert_eq!(step(ended), [Asked::Other], "its call returns its answer");
        let change = Change {
            fields: Box::new([
                smith_domain_run::outcome::Field { name: b"title".as_slice().into(), value: bytes(1) },
                smith_domain_run::outcome::Field {
                    name: b"body".as_slice().into(),
                    value: bytes(limits.outcome_bytes - 2 * size(size_of::<Field>()) - 5 - 4 - 1),
                },
            ]),
        };
        let ask = Ask::Finish { outcome: Declared::Change(change) };
        let call = Token::new(u64::from(run) + 1_000_000);
        let finish = Event::Delegated {
            conversation,
            call,
            ask,
            deadline: expiry,
            name: smith_domain_run::CallName { completion: 1, position: 0 },
        };
        let [Asked::Check { owner }, Asked::Other] = step(finish)[..] else {
            panic!("the change is being checked");
        };
        let ran = Ran { exit: Exit::Code { code: 0 }, output: bytes(0), cut: 0 };
        assert_eq!(step(Event::Checked { owner, ran }), [Asked::Other], "checked, it is pushed");
        assert_eq!(step(Event::Delivered { owner, push: delivered() }), [Asked::Other, Asked::Other], "accepted");
    }
    let held = meter.held();
    let charters = limits.run_bytes + u64::from(limits.guide_bytes);
    // Each run's ending and its landing hold a copy of the change.
    let full = u64::from(limits.runs) * (charters + 2 * limits.outcome_bytes);
    assert!(held >= full, "{limits:?}: every run holds its byte limit");

    // A byte more is refused.
    let mut domain = Domain::new(&Limits { runs: 1, conversations: 2, ..limits });
    let worker = Token::new(0);
    let start = Event::Start { reply_to: ReplyTo::new(worker), worker, charter: charter(limits.run_bytes + 1) };
    smith_domain_run::step(&mut domain, &env, start, &mut out);
    let Some(Request::Answer { to: _, answer }) = out.pop() else { panic!("expected an answer") };
    assert_eq!(answer, Answer::Refused(Refusal::Invalid(Invalid::TooLarge)));
}

#[test]
fn a_domain_with_every_run_full_stays_within_its_worst_case() {
    fill(LIMITS);
    fill(Limits { runs: 64, conversations: 128, calls: 128, run_bytes: 65_536, guide_bytes: 32_768, ..LIMITS });
    fill(Limits { runs: 1000, conversations: 2000, calls: 2000, run_bytes: 2048, guide_bytes: 16, ..LIMITS });
}

fn delivered() -> smith_domain_run::Delivery {
    smith_domain_run::Delivery::Delivered(
        smith_domain_run::Delivered::new(Box::new([smith_domain_run::Receipt::new(
            0,
            b"host receipt".as_slice().into(),
        )
        .expect("sealed receipt")]))
        .expect("one directory"),
    )
}

#[test]
fn full_receipt_and_marker_terminals_price_each_owned_copy() {
    let meter = Meter::new();
    meter.start();
    let receipts: Box<[smith_domain_run::Receipt]> = (0..smith_domain_run::MAX_DIRECTORIES)
        .map(|directory| smith_domain_run::Receipt::new(directory, bytes(512)).expect("exact cap"))
        .collect();
    let original = smith_domain_run::Delivered::new(receipts).expect("full unique terminal");
    let output_copy = original.clone();
    let final_answer_copy = original.clone();
    let measured = meter.end();
    let bound = smith_domain_run::Delivered::worst_case().checked_mul(3).expect("three bounded copies");
    assert_eq!(original.owned_bytes(), smith_domain_run::Delivered::worst_case());
    assert_eq!(meter.check(measured, bound, &"three full delivery evidence copies"), bound);
    assert_eq!(output_copy, final_answer_copy);
    meter.start();
    drop((original, output_copy, final_answer_copy));
    let measured = meter.end();
    meter.check(measured, bound, &"delivery copies released");
    assert_eq!(meter.held(), 0);

    meter.start();
    let marker = smith_domain_run::Marker::new(63, bytes(4096)).expect("exact relative path cap");
    let refusal = smith_domain_run::DeliveryRefusal::new(Some(marker), bytes(512)).expect("exact explanation cap");
    let another = refusal.clone();
    let measured = meter.end();
    meter.check(measured, 2 * 4608, &"two full marker refusals");
    assert_eq!(refusal.owned_bytes(), 4608);
    meter.start();
    drop((refusal, another));
    let measured = meter.end();
    meter.check(measured, 2 * 4608, &"marker copies released");
    assert_eq!(meter.held(), 0);
}

// Outputs are dropped before the measurement check: they are owned and priced
// by the receiver. Tokens are the only observations this driver retains.
fn delivery_memory_step(
    domain: &mut Domain,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
    meter: &Meter,
    event: Event,
) -> (Option<Token>, bool) {
    meter.start();
    smith_domain_run::step(domain, env, event, out);
    let measured = meter.end();
    let mut token = None;
    let mut answered = false;
    while let Some(request) = out.pop() {
        match request {
            Request::Read { owner, .. }
            | Request::Probe { owner, .. }
            | Request::Check { owner, .. }
            | Request::Deliver { owner, .. } => token = Some(owner),
            Request::Open { conversation, .. } => token = Some(conversation),
            Request::Return { result: smith_domain_run::Returned::Delivered(receipts), .. } => {
                assert_eq!(receipts.owned_bytes(), smith_domain_run::Delivered::worst_case());
            }
            Request::Answer {
                answer: Answer::Delivered { receipts, stopped: smith_domain_run::Failure::Cancelled, .. },
                ..
            } => {
                assert_eq!(receipts.owned_bytes(), smith_domain_run::Delivered::worst_case());
                answered = true;
            }
            Request::Admitted { .. } | Request::Checking { .. } | Request::Close { .. } => {}
            unexpected @ (Request::Answer { .. }
            | Request::Return { .. }
            | Request::Say { .. }
            | Request::Abort { .. }) => panic!("unexpected delivery fixture output {unexpected:?}"),
        }
    }
    meter.check(measured, worst_case(&env.limits).expect("bounded limits"), &"full real delivery path");
    (token, answered)
}

#[test]
fn actual_interrupted_delivery_and_final_answer_fill_all_receipt_caps() {
    let limits = Limits { repositories: smith_domain_run::MAX_DIRECTORIES, run_bytes: 65_536, ..LIMITS };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(MAX_OUT);
    let meter = Meter::new();
    let mut domain = Domain::new(&limits);
    let repositories: Box<[Repository]> = (0..smith_domain_run::MAX_DIRECTORIES)
        .map(|directory| Repository {
            name: Box::new([u8::try_from(directory + 1).expect("64 distinct names")]),
            root: Token::new(u64::from(directory)),
            writable: true,
        })
        .collect();
    let parts = u64::from(smith_domain_run::MAX_DIRECTORIES) * (size(size_of::<Repository>()) + 1) + 1;
    let charter = Charter {
        brief: bytes(limits.run_bytes - parts),
        checkout: Checkout { repositories },
        grants: Grants {
            deliver: Some(ChangeSpec { fields: Box::new([]) }),
            tools: Tools { inspect: true, modify: true, shell: true },
            forge: false,
            agents: false,
            outlets: Box::new([]),
        },
        outcome: OutcomeSpec {
            change: None,
            verdicts: Box::new([]),
            report: Some(smith_domain_run::outcome::TextSpec { min: 0, max: 1, fields: Box::new([]) }),
            failure: None,
        },
        budget: BUDGET,
        llm: Llm { account: 0, endpoint: Endpoint(0), model: bytes(1), max_tokens: 1 },
        models: Box::new([]),
    };
    let worker = Token::new(10);
    let (owner, _) = delivery_memory_step(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Event::Start { reply_to: ReplyTo::new(worker), worker, charter },
    );
    let run = owner.expect("preparation read");
    let mut owner = run;
    for _ in 0..smith_domain_run::MAX_DIRECTORIES {
        let (next, _) =
            delivery_memory_step(&mut domain, &env, &mut out, &meter, Event::Read { owner, read: Read::Missing });
        owner = next.expect("preparation probe");
        let (next, _) =
            delivery_memory_step(&mut domain, &env, &mut out, &meter, Event::Probed { owner, executable: true });
        owner = next.expect("next read or main opening");
    }
    let conversation = owner;
    delivery_memory_step(&mut domain, &env, &mut out, &meter, Event::Started { conversation, peer: worker });
    let (owner, _) = delivery_memory_step(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Event::Delegated {
            conversation,
            call: worker,
            name: smith_domain_run::CallName { completion: 1, position: 0 },
            ask: Ask::Deliver { change: Change { fields: Box::new([]) } },
            deadline: Time::ZERO.saturating_add(BUDGET.time),
        },
    );
    let owner = owner.expect("first actual writable check");
    for _ in 0..smith_domain_run::MAX_DIRECTORIES {
        delivery_memory_step(
            &mut domain,
            &env,
            &mut out,
            &meter,
            Event::Checked { owner, ran: Ran { exit: Exit::Code { code: 0 }, output: bytes(0), cut: 0 } },
        );
    }
    delivery_memory_step(&mut domain, &env, &mut out, &meter, Event::Cancel { run });
    let receipts = (0..smith_domain_run::MAX_DIRECTORIES)
        .map(|directory| smith_domain_run::Receipt::new(directory, bytes(512)).expect("exact receipt cap"))
        .collect();
    let terminal = smith_domain_run::Delivered::new(receipts).expect("64 unique receipts");
    delivery_memory_step(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Event::Delivered { owner, push: smith_domain_run::Delivery::Delivered(terminal) },
    );
    assert!(
        meter.held() >= limits.run_bytes + smith_domain_run::Delivered::worst_case(),
        "actual run retains a full charter and all actual receipts"
    );
    let (_, answered) = delivery_memory_step(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO },
    );
    assert!(answered, "one typed interrupted-landing answer carries every receipt");
}
