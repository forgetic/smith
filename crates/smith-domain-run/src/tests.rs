//! Feed the domain events, inspect the requests that come out.

use alloc::boxed::Box;

use skein_lib::{Duration, Env, List, Queue, ReplyTo, Time, Token, Wall};

use crate::charter::{Checkout, Endpoint, Grants, HostTool, Llm, Repository, Tools};
use crate::facts::{Answered, Asked, Fact, Return};
use crate::outcome::{
    Change, ChangeSpec, Declared, Field, FieldRule, Item, OutcomeSpec, Problem, Problems, Verdict, VerdictRule,
};
use crate::prepare::{Found, Guide};
use crate::{
    Answer, Ask, AskRefusal, Budget, Charter, Delivery, Domain, End, Event, Exhausted, Exit, Failure, Fault, Invalid,
    Limits, MAX_OUT, Opening, Place, Policy, Ran, Read, Refusal, Request, Returned, Spend, Stop, fire, step,
    worst_case,
};

const BUDGET: Budget = Budget {
    turns: 10,
    input: 10_000,
    output: 2_000,
    cache_read: 1_000,
    cache_write: 1_000,
    time: Duration::from_secs(600),
};

/// Fixed admission and ownership limits shared by focused run and result tests.
/// Contract: domain/run.md, sections 3.1, 7.1 and 13.
pub(crate) const LIMITS: Limits = Limits {
    runs: 2,
    conversations: 4,
    run_bytes: 4096,
    repositories: 2,
    host_tools: 2,
    host_input_bytes: 65_536,
    host_reply_bytes: 65_536,
    host_timeout: Duration::from_secs(60),
    host_backoff: Duration::from_millis(50),
    host_attempts: 3,
    verdicts: 2,
    calls: 2,
    budget: Budget {
        turns: 100,
        input: 1_000_000,
        output: 100_000,
        cache_read: 1_000_000,
        cache_write: 1_000_000,
        time: Duration::from_secs(3600),
    },
    max_tokens: 4096,
    models: 2,
    depth: 2,
    run_conversations: 3,
    answer_bytes: 16,
    nudges: 2,
    guide_bytes: 64,
    io_timeout: Duration::from_secs(10),
    outcome_bytes: 1024,
    delivery_timeout: Duration::from_secs(300),
    check_timeout: Duration::from_secs(300),
    check_tail: 4096,
    facts: 64,
};

/// The domain, its environment, and room for one step's output.
struct Harness {
    domain: Domain,
    env: Env<Limits>,
    out: Queue<Request>,
}

impl Harness {
    fn new(limits: Limits) -> Harness {
        Harness {
            domain: Domain::new(&limits),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            out: Queue::with_capacity(MAX_OUT),
        }
    }

    /// Steps `event`, returning what it emitted, oldest first.
    fn step(&mut self, event: Event) -> Box<[Request]> {
        step(&mut self.domain, &self.env, event, &mut self.out);
        self.drain()
    }

    fn fire(&mut self) -> Box<[Request]> {
        assert!(self.domain.is_due(self.env.now), "an alarm is due");
        fire(&mut self.domain, &self.env, &mut self.out);
        self.drain()
    }

    fn drain(&mut self) -> Box<[Request]> {
        let mut requests = List::with_capacity(MAX_OUT);
        for _ in 0..MAX_OUT {
            let Some(request) = self.out.pop() else { break };
            requests.push(request).expect("room for MAX_OUT");
        }
        assert!(self.out.is_empty(), "a step emits at most MAX_OUT");
        requests.into_boxed()
    }

    /// Starts a run of `charter` for call `call`: what it emitted.
    fn start(&mut self, call: u64, charter: Charter) -> Box<[Request]> {
        let reply_to = ReplyTo::new(Token::new(call));
        self.step(Event::Start { reply_to, worker: Token::new(call), charter })
    }

    /// Starts a run of the test charter for call `call`, which is admitted:
    /// the run's token, once it has looked for its one repository's guide.
    fn prepare(&mut self, call: u64) -> Token {
        let emitted = self.start(call, charter());
        let [Request::Admitted { worker, run }, Request::Read { owner, .. }] = &*emitted else {
            panic!("expected an admitted run, got {emitted:?}");
        };
        assert_eq!((*worker, owner), (Token::new(call), run));
        *run
    }

    /// Starts a run of the test charter for call `call`, which is admitted and
    /// finds no guide: the run's token and its main conversation's.
    fn admit(&mut self, call: u64) -> (Token, Token) {
        let run = self.prepare(call);
        let emitted = self.step(Event::Read { owner: run, read: Read::Missing });
        let [Request::Open { conversation, opening: _ }] = &*emitted else {
            panic!("expected main to open, got {emitted:?}");
        };
        (run, *conversation)
    }

    /// Admits a run for call `call` and starts its main conversation as
    /// `peer`: the run's token and main's.
    fn running(&mut self, call: u64, peer: u64) -> (Token, Token) {
        let (run, conversation) = self.admit(call);
        assert!(self.step(Event::Started { conversation, peer: Token::new(peer) }).is_empty(), "starting is quiet");
        (run, conversation)
    }

    fn after(&mut self, span: Duration) {
        self.env.now = self.env.now.checked_add(span).expect("the test stays in range");
    }
}

pub(crate) fn bytes(text: &[u8]) -> Box<[u8]> {
    Box::from(text)
}

pub(crate) fn rule(name: &[u8], min: u32, max: u32) -> VerdictRule {
    VerdictRule {
        name: bytes(name),
        text_max: 1024,
        fields: Box::new([]),
        items: crate::outcome::ItemSpec {
            min,
            max,
            kinds: Box::new([
                crate::outcome::ItemRule {
                    kind: bytes(b"blocking"),
                    fields: Box::new([
                        FieldRule { name: bytes(b"path"), max: 1024 },
                        FieldRule { name: bytes(b"body"), max: 1024 },
                    ]),
                },
                crate::outcome::ItemRule {
                    kind: bytes(b"nit"),
                    fields: Box::new([
                        FieldRule { name: bytes(b"path"), max: 1024 },
                        FieldRule { name: bytes(b"body"), max: 1024 },
                    ]),
                },
            ]),
        },
    }
}

pub(crate) fn charter() -> Charter {
    Charter {
        brief: bytes(b"Review the change."),
        checkout: Checkout {
            repositories: Box::new([Repository { name: bytes(b"temper"), root: Token::new(900), writable: false }]),
        },
        grants: Grants {
            deliver: None,
            tools: Tools { inspect: true, modify: false, shell: true },

            agents: false,
            host_tools: Box::new([HostTool {
                name: bytes(b"comment"),
                description: b"Host action".as_slice().into(),
                schema: b"{}".as_slice().into(),
                effect: crate::HostEffect::Read,
                timeout: Duration::from_secs(5),
            }]),
        },
        outcome: OutcomeSpec {
            change: None,
            verdicts: Box::new([rule(b"approve", 0, 0), rule(b"request", 1, 8)]),
            report: None,
            failure: None,
        },
        budget: BUDGET,
        llm: Llm { account: 0, endpoint: Endpoint(1), model: bytes(b"model-a"), max_tokens: 1024 },
        models: Box::new([]),
    }
}

fn spend(input: u64) -> Spend {
    Spend { turns: 1, input, output: 10, cache_read: 0, cache_write: 0 }
}

/// What a run answered, and to which call.
fn answered(emitted: Box<[Request]>) -> (u64, Answer) {
    let Ok(one) = Box::<[Request; 1]>::try_from(emitted) else {
        panic!("expected one request");
    };
    let [Request::Answer { to, answer }] = *one else {
        panic!("expected an answer");
    };
    (to.into_token().raw(), answer)
}

fn failed(failure: Failure, spent: Spend) -> Answer {
    Answer::Failed { failure, spent }
}

#[test]
fn an_admitted_run_reads_its_checkout_then_opens_main_with_the_whole_budget() {
    let mut h = Harness::new(LIMITS);
    let emitted = h.start(7, charter());
    let [Request::Admitted { worker, run }, Request::Read { owner, at, max, deadline }] = &*emitted else {
        panic!("expected an admitted run, got {emitted:?}");
    };
    assert_eq!((*worker, owner), (Token::new(7), run));
    assert_eq!(at, &Place { root: Token::new(900), path: bytes(b"AGENTS.md") });
    assert_eq!((*max, *deadline), (LIMITS.guide_bytes, Time::ZERO.saturating_add(LIMITS.io_timeout)));
    assert_eq!((h.domain.runs(), h.domain.conversations()), (1, 1), "main has its slot from the start");

    let read = Read::Text { text: bytes(b"Run the tests."), whole: true };
    let emitted = h.step(Event::Read { owner: *run, read });
    let [Request::Open { conversation: _, opening }] = &*emitted else {
        panic!("expected main to open, got {emitted:?}");
    };
    let mut found = Found::with_capacity(1);
    found.guides.push(Guide { repository: 0, text: bytes(b"Run the tests."), whole: true }).expect("room");
    let expected = Opening {
        host_tools: charter().grants.host_tools,
        deliver: false,
        llm: charter().llm,
        system: super::prompt::system(&charter(), &found),
        prompt: bytes(super::prompt::BEGIN),
        tools: charter().grants.tools,
        checkout: charter().checkout,
        budget: BUDGET,
        finish: true,
        families: crate::charter::Families { tools: charter().grants.tools, agents: false },
    };
    assert_eq!(opening, &expected);
    assert_eq!(h.domain.next_deadline(), Some(Time::ZERO.saturating_add(BUDGET.time)));
}

#[test]
fn a_run_looks_for_checks_in_writable_repositories_when_a_change_must_pass_them() {
    let mut h = Harness::new(LIMITS);
    let checkout = Checkout {
        repositories: Box::new([
            Repository { name: bytes(b"temper"), root: Token::new(900), writable: true },
            Repository { name: bytes(b"docs"), root: Token::new(901), writable: false },
        ]),
    };
    let outcome = OutcomeSpec {
        change: Some(ChangeSpec {
            fields: Box::new([
                FieldRule { name: b"title".as_slice().into(), max: 1024 },
                FieldRule { name: b"body".as_slice().into(), max: 1024 },
            ]),
        }),
        verdicts: Box::new([]),
        report: None,
        failure: None,
    };
    let emitted = h.start(1, Charter { checkout, outcome, ..charter() });
    let [Request::Admitted { run, .. }, Request::Read { at, .. }] = &*emitted else {
        panic!("expected a read, got {emitted:?}");
    };
    let run = *run;
    assert_eq!(at.root, Token::new(900));
    let emitted = h.step(Event::Read { owner: run, read: Read::Failed });
    let [Request::Probe { owner, at, deadline }] = &*emitted else {
        panic!("expected a probe, got {emitted:?}");
    };
    assert_eq!((owner, at), (&run, &Place { root: Token::new(900), path: bytes(b".temper/pre-pr") }));
    assert_eq!(*deadline, Time::ZERO.saturating_add(LIMITS.io_timeout));
    let emitted = h.step(Event::Probed { owner: run, executable: true });
    let [Request::Read { at, .. }] = &*emitted else {
        panic!("expected a read, got {emitted:?}");
    };
    assert_eq!(at.root, Token::new(901), "a read-only repository has no checks to look for");
    let emitted = h.step(Event::Read { owner: run, read: Read::Missing });
    let [Request::Open { opening, .. }] = &*emitted else {
        panic!("expected main to open, got {emitted:?}");
    };
    let system = &opening.system;
    let marked = b"- `temper`, which you may change, with checks (`.temper/pre-pr`)\n";
    assert!(skein_lib::bytes::find(system, marked).is_some(), "the checkout says which has checks");
}

#[test]
fn a_run_with_nothing_to_look_for_opens_main_at_once() {
    let mut h = Harness::new(LIMITS);
    let emitted = h.start(1, Charter { checkout: Checkout { repositories: Box::new([]) }, ..charter() });
    let [Request::Admitted { .. }, Request::Open { .. }] = &*emitted else {
        panic!("expected an admitted run and main, got {emitted:?}");
    };
}

#[test]
fn starts_beyond_the_run_or_conversation_slots_are_refused_as_busy() {
    let mut h = Harness::new(Limits { runs: 1, ..LIMITS });
    let _: (Token, Token) = h.admit(1);
    assert_eq!(answered(h.start(2, charter())), (2, Answer::Refused(Refusal::Busy)));

    let mut h = Harness::new(Limits { conversations: 1, ..LIMITS });
    let _: (Token, Token) = h.admit(1);
    assert_eq!(answered(h.start(2, charter())), (2, Answer::Refused(Refusal::Busy)));
    assert_eq!(h.domain.runs(), 1);

    // A charter that can never fit is invalid, room or not.
    let never = Charter { budget: Budget { turns: 0, ..BUDGET }, ..charter() };
    assert_eq!(answered(h.start(3, never)), (3, Answer::Refused(Refusal::Invalid(Invalid::Budget))));
}

#[test]
fn charters_beyond_the_limits_are_refused_as_invalid() {
    let three = Box::new([repository(b"a"), repository(b"b"), repository(b"c")]);
    let twins = Box::new([repository(b"a"), repository(b"a")]);
    let host_tools = Box::new([
        HostTool {
            name: bytes(b"reply"),
            description: b"Host action".as_slice().into(),
            schema: b"{}".as_slice().into(),
            effect: crate::HostEffect::Read,
            timeout: Duration::from_secs(5),
        },
        HostTool {
            name: bytes(b"reply"),
            description: b"Host action".as_slice().into(),
            schema: b"{}".as_slice().into(),
            effect: crate::HostEffect::Read,
            timeout: Duration::from_secs(5),
        },
    ]);
    let llm = Llm { account: 0, endpoint: Endpoint(2), model: bytes(b"model-b"), max_tokens: 512 };
    let models = Box::new([llm.clone(), Llm { endpoint: Endpoint(3), ..llm }]);
    let cases = [
        (Charter { budget: Budget { turns: 101, ..BUDGET }, ..charter() }, Invalid::Budget),
        (Charter { budget: Budget { time: Duration::from_secs(3601), ..BUDGET }, ..charter() }, Invalid::Budget),
        (Charter { budget: Budget { turns: 0, ..BUDGET }, ..charter() }, Invalid::Budget),
        (Charter { budget: Budget { output: 0, ..BUDGET }, ..charter() }, Invalid::Budget),
        (Charter { llm: Llm { max_tokens: 0, ..charter().llm }, ..charter() }, Invalid::Llm),
        (Charter { llm: Llm { max_tokens: 4097, ..charter().llm }, ..charter() }, Invalid::Llm),
        (Charter { models, ..charter() }, Invalid::Llm),
        (Charter { brief: Box::from([b'x'; 4096].as_slice()), ..charter() }, Invalid::TooLarge),
        (Charter { checkout: Checkout { repositories: three }, ..charter() }, Invalid::Checkout),
        (Charter { checkout: Checkout { repositories: twins }, ..charter() }, Invalid::Checkout),
        (Charter { grants: Grants { host_tools, ..charter().grants }, ..charter() }, Invalid::Grants),
        (Charter { outcome: spec(Box::new([])), ..charter() }, Invalid::Outcome),
        (Charter { outcome: spec(Box::new([rule(b"a", 0, 0), rule(b"a", 1, 1)])), ..charter() }, Invalid::Outcome),
        (
            Charter { outcome: spec(Box::new([rule(b"a", 0, 0), rule(b"b", 0, 0), rule(b"c", 0, 0)])), ..charter() },
            Invalid::Outcome,
        ),
        (Charter { outcome: spec(Box::new([rule(b"a", 3, 2)])), ..charter() }, Invalid::Outcome),
        // ItemSpec allowed, with no kind for them to be.
        (
            Charter {
                outcome: spec(Box::new([VerdictRule {
                    items: crate::outcome::ItemSpec { kinds: Box::new([]), ..(rule(b"a", 0, 1)).items },
                    ..rule(b"a", 0, 1)
                }])),
                ..charter()
            },
            Invalid::Outcome,
        ),
    ];
    for (call, (charter, invalid)) in (0_u64..).zip(cases) {
        let mut h = Harness::new(LIMITS);
        assert_eq!(answered(h.start(call, charter)), (call, Answer::Refused(Refusal::Invalid(invalid))));
        assert_eq!(h.domain.runs(), 0);
    }
    // A change alone is an outcome.
    let mut h = Harness::new(LIMITS);
    drop(h.start(
        1,
        Charter {
            outcome: OutcomeSpec {
                change: Some(ChangeSpec {
                    fields: Box::new([
                        FieldRule { name: b"title".as_slice().into(), max: 1024 },
                        FieldRule { name: b"body".as_slice().into(), max: 1024 },
                    ]),
                }),
                verdicts: Box::new([]),
                report: None,
                failure: None,
            },
            ..charter()
        },
    ));
    assert_eq!(h.domain.runs(), 1);
}

fn repository(name: &[u8]) -> Repository {
    Repository { name: bytes(name), root: Token::new(1), writable: true }
}

fn spec(verdicts: Box<[VerdictRule]>) -> OutcomeSpec {
    OutcomeSpec { change: None, verdicts, report: None, failure: None }
}

#[test]
fn a_run_whose_main_conversation_fails_answers_with_its_fault_and_what_it_spent() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    assert!(h.step(Event::Used { conversation, spend: spend(100) }).is_empty(), "within the budget");
    let end = End::Fault(Fault::Provider);
    let emitted = h.step(Event::Ended { conversation, end, spend: spend(100) });
    assert_eq!(answered(emitted), (1, failed(Failure::Model(Fault::Provider), spend(100))));
    assert_eq!(h.domain.next_deadline(), None);
    h.domain.reclaim();
    assert_eq!((h.domain.runs(), h.domain.conversations()), (0, 0));
}

#[test]
fn a_run_whose_main_conversation_runs_out_of_budget_fails_for_budget() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    let end = End::Budget(Exhausted::Turns);
    let emitted = h.step(Event::Ended { conversation, end, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Turns), Spend::ZERO)));
}

#[test]
fn a_main_conversation_refused_at_its_entrance_refuses_the_run() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.admit(1);
    let emitted = h.step(Event::Ended { conversation, end: End::Busy, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, Answer::Refused(Refusal::Busy)));
    let (_, conversation) = h.admit(2);
    let emitted = h.step(Event::Ended { conversation, end: End::Invalid, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (2, Answer::Refused(Refusal::Invalid(Invalid::Conversation))));
}

#[test]
fn an_llm_that_stops_without_finishing_is_nudged_until_its_nudges_run_out() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    let peer = Token::new(100);
    for nudge in 1..=LIMITS.nudges {
        assert!(h.step(Event::Used { conversation, spend: spend(5) }).is_empty(), "within the budget");
        let text = super::prompt::nudge(Stop::EndTurn, nudge, LIMITS.nudges);
        assert_eq!(&*h.step(end_turn(conversation)), &[Request::Say { peer, text }]);
    }
    assert!(h.step(Event::Used { conversation, spend: spend(5) }).is_empty(), "within the budget");
    assert_eq!(&*h.step(end_turn(conversation)), &[Request::Close { peer }]);
    assert_eq!(h.domain.next_deadline(), None);
    // A turn that won the race with the close is spent all the same.
    assert!(h.step(Event::Used { conversation, spend: spend(5) }).is_empty(), "winding down");
    let total = Spend { turns: 4, input: 20, output: 40, cache_read: 0, cache_write: 0 };
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: total });
    let unfinished = Failure::Policy(Policy::Unfinished { nudges: LIMITS.nudges, rejected: 0 });
    assert_eq!(answered(emitted), (1, failed(unfinished, total)));
}

fn end_turn(conversation: Token) -> Event {
    Event::Yielded { conversation, stop: Stop::EndTurn, text: bytes(b"done, I think") }
}

#[test]
fn an_llm_whose_last_stop_shows_a_fault_fails_the_run_with_it() {
    let faults =
        [(Stop::MaxTokens, Fault::Truncated), (Stop::Refusal, Fault::Refused), (Stop::NoCalls, Fault::Malformed)];
    for (stop, fault) in faults {
        let mut h = Harness::new(Limits { nudges: 0, ..LIMITS });
        let (_, conversation) = h.running(1, 100);
        drop(h.step(Event::Yielded { conversation, stop, text: bytes(b"") }));
        let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
        assert_eq!(answered(emitted), (1, failed(Failure::Model(fault), Spend::ZERO)));
    }
}

#[test]
fn an_llm_with_no_input_or_output_left_is_not_nudged() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input) }));
    assert_eq!(&*h.step(end_turn(conversation)), &[Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: spend(BUDGET.input) });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Input), spend(BUDGET.input))));
}

#[test]
fn an_llm_with_no_turn_left_is_not_nudged() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    let all = Spend { turns: BUDGET.turns, ..spend(5) };
    assert!(h.step(Event::Used { conversation, spend: all }).is_empty(), "at the budget");
    let yielded = Event::Yielded { conversation, stop: Stop::EndTurn, text: bytes(b"") };
    assert_eq!(&*h.step(yielded), &[Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: all });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Turns), all)));
}

#[test]
fn spending_past_the_budget_lets_main_finish_its_turn_then_closes_it() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    assert!(h.step(Event::Used { conversation, spend: spend(BUDGET.input) }).is_empty(), "at the budget");
    assert!(h.step(Event::Used { conversation, spend: spend(1) }).is_empty(), "past it: main keeps its turn");
    assert_eq!(&*h.step(Event::Used { conversation, spend: spend(1) }), &[Request::Close { peer: Token::new(100) }]);
    let total = Spend { turns: 3, input: BUDGET.input + 2, output: 30, cache_read: 0, cache_write: 0 };
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: total });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Input), total)));

    // Or it yields, and is closed then.
    let (_, conversation) = h.running(2, 101);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
    assert_eq!(&*h.step(end_turn(conversation)), &[Request::Close { peer: Token::new(101) }], "no nudge past it");
}

#[test]
fn what_an_end_counts_beyond_the_turns_used_is_charged() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    drop(h.step(Event::Used { conversation, spend: spend(10) }));
    let end = End::Fault(Fault::ContextFull);
    let emitted = h.step(Event::Ended { conversation, end, spend: spend(30) });
    assert_eq!(answered(emitted), (1, failed(Failure::Model(Fault::ContextFull), spend(30))));
}

#[test]
fn the_deadline_closes_main_and_fails_the_run_for_time() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    h.after(BUDGET.time);
    assert_eq!(&*h.fire(), &[Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Budget(Exhausted::Time), spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Time), Spend::ZERO)));
}

#[test]
fn the_worst_case_is_bounded_or_refused() {
    let bytes = worst_case(&LIMITS).expect("the test limits fit");
    assert!(bytes > 2 * LIMITS.run_bytes, "every run may hold its bytes");
    assert_eq!(worst_case(&Limits { runs: u32::MAX, run_bytes: u64::MAX, ..LIMITS }), None);
}

// Finishing.

/// When a conversation of a run started at zero expires: the run's deadline.
const EXPIRY: Time = Time::from_nanos(BUDGET.time.as_nanos());

/// What a run's main conversation asks when it finishes with `outcome`, as
/// its call `call`.
fn finish(conversation: Token, call: u64, outcome: Declared) -> Event {
    finish_by(conversation, call, outcome, EXPIRY)
}

/// The same, with the call due by `deadline`.
fn finish_by(conversation: Token, call: u64, outcome: Declared, deadline: Time) -> Event {
    Event::Delegated {
        name: crate::CallName { completion: 1, position: 0 },
        conversation,
        call: Token::new(call),
        ask: Ask::Finish { outcome },
        deadline,
    }
}

fn returned(call: u64, result: Returned) -> Request {
    Request::Return { call: Token::new(call), result }
}

fn verdict(name: &[u8], children: Box<[Item]>) -> Declared {
    Declared::Verdict(Verdict { name: bytes(name), text: bytes(b"Looks good."), items: children, fields: Box::new([]) })
}

fn comment() -> Item {
    let fields = Box::new([
        Field { name: bytes(b"path"), value: bytes(b"a.rs") },
        Field { name: bytes(b"body"), value: bytes(b"Nit.") },
    ]);
    Item { kind: bytes(b"nit"), fields }
}

fn change() -> Change {
    Change {
        fields: Box::new([
            Field { name: b"title".as_slice().into(), value: bytes(b"Fix the parser") },
            Field { name: b"body".as_slice().into(), value: bytes(b"It accepts tabs now.") },
        ]),
    }
}

/// The test charter, finishing with a change whose checks must pass, in two
/// writable repositories.
fn coding() -> Charter {
    let checkout = Checkout {
        repositories: Box::new([
            Repository { name: bytes(b"temper"), root: Token::new(900), writable: true },
            Repository { name: bytes(b"docs"), root: Token::new(901), writable: true },
        ]),
    };
    Charter {
        checkout,
        outcome: OutcomeSpec {
            change: Some(ChangeSpec {
                fields: Box::new([
                    FieldRule { name: b"title".as_slice().into(), max: 1024 },
                    FieldRule { name: b"body".as_slice().into(), max: 1024 },
                ]),
            }),
            verdicts: Box::new([]),
            report: None,
            failure: None,
        },
        ..charter()
    }
}

impl Harness {
    /// Starts a run of `coding()` for call `call` whose repositories both have
    /// checks, and its main conversation as `peer`: the run's token and main's.
    fn coding(&mut self, call: u64, peer: u64) -> (Token, Token) {
        let emitted = self.start(call, coding());
        let [Request::Admitted { run, .. }, Request::Read { .. }] = &*emitted else {
            panic!("expected a read, got {emitted:?}");
        };
        let run = *run;
        drop(self.step(Event::Read { owner: run, read: Read::Missing }));
        drop(self.step(Event::Probed { owner: run, executable: true }));
        drop(self.step(Event::Read { owner: run, read: Read::Missing }));
        let emitted = self.step(Event::Probed { owner: run, executable: true });
        let [Request::Open { conversation, .. }] = &*emitted else {
            panic!("expected main to open, got {emitted:?}");
        };
        let conversation = *conversation;
        drop(self.step(Event::Started { conversation, peer: Token::new(peer) }));
        (run, conversation)
    }

    /// Has main finish with `change()` as call `call`: the owner of the
    /// landing, whose first check is in flight.
    fn land(&mut self, conversation: Token, call: u64) -> Token {
        let emitted = self.step(finish(conversation, call, Declared::Change(change())));
        let [Request::Check { owner, program, deadline, tail }, Request::Checking { worker: _, deadline: until }] =
            &*emitted
        else {
            panic!("expected the first check, got {emitted:?}");
        };
        assert_eq!(program, &Place { root: Token::new(900), path: bytes(b".temper/pre-pr") });
        assert_eq!((*tail, deadline), (LIMITS.check_tail, until));
        assert_eq!(*deadline, self.env.now.saturating_add(LIMITS.check_timeout));
        *owner
    }
}

fn ran(code: u8, output: &[u8]) -> Ran {
    Ran { exit: Exit::Code { code }, output: bytes(output), cut: 0 }
}

#[test]
fn an_outcome_that_does_not_fit_the_spec_is_rejected_and_the_run_goes_on() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    let emitted = h.step(finish(conversation, 7, Declared::Change(change())));
    let problems = Problems { listed: Box::new([Problem::ChangeNotAllowed]), more: 0 };
    assert_eq!(&*emitted, &[returned(7, Returned::Rejected { problems })]);
    let huge = verdict(b"approve", Box::new([]));
    let Declared::Verdict(mut huge) = huge else { unreachable!("a verdict") };
    huge.text = Box::from([b'x'; 2000].as_slice());
    let emitted = h.step(finish(conversation, 8, Declared::Verdict(huge)));
    let problems = Problems { listed: Box::new([Problem::TooLarge { max: LIMITS.outcome_bytes }]), more: 0 };
    assert_eq!(&*emitted, &[returned(8, Returned::Rejected { problems })]);
    // Nudged out, the run fails as unfinished, counting what it rejected.
    for _ in 0..LIMITS.nudges {
        drop(h.step(end_turn(conversation)));
    }
    assert_eq!(&*h.step(end_turn(conversation)), &[Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    let unfinished = Failure::Policy(Policy::Unfinished { nudges: LIMITS.nudges, rejected: 2 });
    assert_eq!(answered(emitted), (1, failed(unfinished, Spend::ZERO)));
}

#[test]
fn a_verdict_that_fits_is_accepted_and_the_run_finishes_with_it() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    let outcome = verdict(b"request", Box::new([comment()]));
    let emitted = h.step(finish(conversation, 7, outcome));
    assert_eq!(&*emitted, &[returned(7, Returned::Accepted), Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: spend(5) });
    let accepted = Answer::Accepted { outcome: verdict(b"request", Box::new([comment()])), spent: spend(5) };
    assert_eq!(answered(emitted), (1, accepted));
}

#[test]
fn a_change_runs_each_repositorys_checks_then_is_pushed_and_accepted() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    let emitted = h.step(Event::Checked { owner, ran: ran(0, b"ok") });
    let [Request::Check { owner: second, program, .. }, Request::Checking { .. }] = &*emitted else {
        panic!("expected the second check, got {emitted:?}");
    };
    assert_eq!((second, program.root), (&owner, Token::new(901)));
    let emitted = h.step(Event::Checked { owner, ran: ran(0, b"ok") });
    assert_eq!(
        &*emitted,
        &[Request::Deliver {
            worker: Token::new(1),
            owner,
            change: change(),
            name: crate::CallName { completion: 1, position: 0 },
            deadline: h.env.now.saturating_add(LIMITS.delivery_timeout).min(EXPIRY)
        }]
    );
    assert_eq!(h.domain.calls(), 1);
    let emitted = h.step(Event::Delivered { owner, push: delivered() });
    assert_eq!(&*emitted, &[returned(7, Returned::Delivered(receipts())), Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    let accepted = Answer::Accepted { outcome: Declared::Change(change()), spent: Spend::ZERO };
    assert_eq!(answered(emitted), (1, accepted));
    h.domain.reclaim();
    assert_eq!((h.domain.runs(), h.domain.conversations(), h.domain.calls()), (0, 0, 0));
}

#[test]
fn a_change_that_fails_its_checks_or_its_push_goes_back_to_the_llm() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    let failing = Ran { exit: Exit::Code { code: 1 }, output: bytes(b"test parse ... FAILED"), cut: 12 };
    let emitted = h.step(Event::Checked { owner, ran: failing });
    let failing = Ran { exit: Exit::Code { code: 1 }, output: bytes(b"test parse ... FAILED"), cut: 12 };
    assert_eq!(&*emitted, &[returned(7, Returned::ChecksFailed { repository: bytes(b"temper"), ran: failing })]);
    let owner = h.land(conversation, 8);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    assert_eq!(
        &*h.step(Event::Delivered {
            owner,
            push: Delivery::Failed(crate::DeliveryFailure::new(crate::DeliveryReason::Unknown))
        }),
        &[returned(
            8,
            Returned::DeliveryFailed { failure: crate::DeliveryFailure::new(crate::DeliveryReason::Unknown) }
        )]
    );
    assert!(h.step(end_turn(conversation)).len() == 1, "the run goes on: a nudge");
}

#[test]
fn a_change_whose_branch_moved_ends_the_run_as_stale() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    let emitted = h.step(Event::Delivered { owner, push: Delivery::Stale });
    assert_eq!(&*emitted, &[returned(7, Returned::Stale), Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, failed(Failure::Stale, Spend::ZERO)));
}

#[test]
fn past_the_budget_the_deadline_fails_the_run_for_the_part_it_went_past() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
    h.after(BUDGET.time);
    assert_eq!(&*h.fire(), &[Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: spend(BUDGET.input + 1) });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Input), spend(BUDGET.input + 1))));
}

#[test]
fn a_guide_that_is_not_text_is_not_there() {
    let mut h = Harness::new(LIMITS);
    let run = h.prepare(1);
    let emitted = h.step(Event::Read { owner: run, read: Read::NotText });
    let [Request::Open { opening, .. }] = &*emitted else { panic!("expected main to open, got {emitted:?}") };
    assert_eq!(opening.system, super::prompt::system(&charter(), &Found::with_capacity(1)));
}

#[test]
fn a_withdrawn_landing_stops_what_is_in_flight_and_returns_once_it_has() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    assert!(h.step(Event::Withdraw { conversation, call: Token::new(6) }).is_empty(), "not the landing call");
    assert_eq!(&*h.step(Event::Withdraw { conversation, call: Token::new(7) }), &[Request::Abort { owner }]);
    assert_eq!(&*h.step(Event::Aborted { owner }), &[returned(7, Returned::Cancelled)]);
    assert!(h.step(Event::Withdraw { conversation, call: Token::new(7) }).is_empty(), "returned already");
    h.domain.reclaim();

    // Withdrawn while pushing, and the push wins the race: it landed.
    let owner = h.land(conversation, 8);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    assert!(
        h.step(Event::Withdraw { conversation, call: Token::new(8) }).is_empty(),
        "submitted delivery is never abandoned"
    );
    let emitted = h.step(Event::Delivered { owner, push: delivered() });
    assert_eq!(&*emitted, &[returned(8, Returned::Delivered(receipts())), Request::Close { peer: Token::new(100) }]);
}

#[test]
fn a_landing_past_its_deadline_is_stopped_and_returns_timed_out_once_it_has() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let minute = Duration::from_secs(60);
    let emitted = h.step(finish_by(conversation, 7, Declared::Change(change()), Time::ZERO.saturating_add(minute)));
    let [Request::Check { owner, deadline, .. }, Request::Checking { .. }] = &*emitted else {
        panic!("expected the first check, got {emitted:?}");
    };
    // A check may not outlive its caller's earlier deadline.
    assert_eq!(*deadline, Time::ZERO.saturating_add(minute).min(Time::ZERO.saturating_add(LIMITS.check_timeout)));
    let owner = *owner;
    h.after(minute);
    assert_eq!(&*h.fire(), &[Request::Abort { owner }]);
    assert!(!h.domain.is_due(h.env.now), "a call's deadline fires once");
    // Checks that pass before the abort lands push nothing.
    assert_eq!(&*h.step(Event::Checked { owner, ran: ran(0, b"") }), &[returned(7, Returned::TimedOut)]);
    assert_eq!(h.step(end_turn(conversation)).len(), 1, "the run goes on: a nudge");
    h.domain.reclaim();

    // Past its deadline while it is pushed.
    let deadline = h.env.now.saturating_add(minute);
    let emitted = h.step(finish_by(conversation, 8, Declared::Change(change()), deadline));
    let [Request::Check { owner, .. }, Request::Checking { .. }] = &*emitted else {
        panic!("expected the first check, got {emitted:?}");
    };
    let owner = *owner;
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    h.after(minute);
    assert!(h.fire().is_empty(), "call expiry retains the submitted host operation");
    assert_eq!(
        &*h.step(Event::Delivered {
            owner,
            push: Delivery::Failed(crate::DeliveryFailure::new(crate::DeliveryReason::TimedOut))
        }),
        &[returned(
            8,
            Returned::DeliveryFailed { failure: crate::DeliveryFailure::new(crate::DeliveryReason::TimedOut) }
        )]
    );
    h.domain.reclaim();
    assert_eq!(h.domain.calls(), 0);
}

#[test]
fn past_the_budget_a_finish_in_the_turn_in_flight_still_counts() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(1, 100);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
    let emitted = h.step(finish(conversation, 7, verdict(b"approve", Box::new([]))));
    assert_eq!(&*emitted, &[returned(7, Returned::Accepted), Request::Close { peer: Token::new(100) }]);
    let total = spend(BUDGET.input + 1);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: total });
    assert_eq!(answered(emitted), (1, Answer::Accepted { outcome: verdict(b"approve", Box::new([])), spent: total }));

    // A refused one closes main, and the run fails for budget.
    let (_, conversation) = h.running(2, 101);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
    let emitted = h.step(finish(conversation, 8, verdict(b"reject", Box::new([]))));
    let problems = Problems { listed: Box::new([Problem::UnknownVerdict]), more: 0 };
    assert_eq!(&*emitted, &[returned(8, Returned::Rejected { problems }), Request::Close { peer: Token::new(101) }]);
}

#[test]
fn a_run_out_of_time_while_it_prepares_answers_once_its_look_has_ended() {
    let mut h = Harness::new(LIMITS);
    let run = h.prepare(1);
    h.after(BUDGET.time);
    assert!(h.fire().is_empty(), "the look is in flight");
    let emitted = h.step(Event::Read { owner: run, read: Read::Missing });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Time), Spend::ZERO)));
}

// A cancel in every state.

fn cancelled() -> Answer {
    failed(Failure::Cancelled, Spend::ZERO)
}

#[test]
fn a_cancel_while_preparing_stops_the_run_once_its_look_has_ended() {
    let mut h = Harness::new(LIMITS);
    let run = h.prepare(1);
    assert!(h.step(Event::Cancel { run }).is_empty(), "the look is in flight");
    assert_eq!(h.domain.next_deadline(), None);
    assert!(h.step(Event::Cancel { run }).is_empty(), "stopping: the ending is decided");
    let emitted = h.step(Event::Read { owner: run, read: Read::Missing });
    assert_eq!(answered(emitted), (1, cancelled()));
    h.domain.reclaim();
    assert_eq!((h.domain.runs(), h.domain.conversations()), (0, 0), "main was never opened, and is gone too");
}

#[test]
fn a_cancel_while_main_opens_closes_main_once_it_starts() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.admit(1);
    assert!(h.step(Event::Cancel { run }).is_empty(), "main has no peer to close yet");
    assert!(h.step(Event::Cancel { run }).is_empty(), "the ending is decided");
    let peer = Token::new(100);
    assert_eq!(&*h.step(Event::Started { conversation, peer }), &[Request::Close { peer }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, cancelled()));

    // Or main is refused, and the cancel is still how the run ends.
    let (run, conversation) = h.admit(2);
    drop(h.step(Event::Cancel { run }));
    let emitted = h.step(Event::Ended { conversation, end: End::Busy, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (2, cancelled()));
}

#[test]
fn a_cancel_while_main_works_closes_it() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.running(1, 100);
    assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(100) }]);
    // The conversation failed before it saw the close: the cancel still wins.
    let emitted = h.step(Event::Ended { conversation, end: End::Fault(Fault::Provider), spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, cancelled()));
}

#[test]
fn a_cancel_while_main_is_over_the_budget_closes_it() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.running(1, 100);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
    assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(100) }]);
    let spent = spend(BUDGET.input + 1);
    let emitted = h.step(Event::Ended { conversation, end: End::Budget(Exhausted::Input), spend: spent });
    assert_eq!(answered(emitted), (1, failed(Failure::Cancelled, spent)));
}

#[test]
fn a_cancel_while_a_change_is_checked_stops_the_checks_once_main_withdraws_its_call() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(100) }]);
    assert_eq!(&*h.step(Event::Withdraw { conversation, call: Token::new(7) }), &[Request::Abort { owner }]);
    // The checks pass before the abort lands: nothing is pushed.
    assert_eq!(&*h.step(Event::Checked { owner, ran: ran(0, b"") }), &[returned(7, Returned::Cancelled)]);
    // A finish that crossed the close is cancelled too.
    let crossed = finish(conversation, 8, verdict(b"approve", Box::new([])));
    assert_eq!(&*h.step(crossed), &[returned(8, Returned::Cancelled)]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, cancelled()));
}

#[test]
fn a_push_that_lands_while_a_cancel_closes_main_wins_over_it() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(100) }]);
    assert!(
        h.step(Event::Withdraw { conversation, call: Token::new(7) }).is_empty(),
        "submitted delivery is never abandoned"
    );
    // The actual host terminal reports a landing while run cancellation settles.
    assert_eq!(
        &*h.step(Event::Delivered { owner, push: delivered() }),
        &[returned(7, Returned::Delivered(receipts()))]
    );
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    let accepted = Answer::Accepted { outcome: Declared::Change(change()), spent: Spend::ZERO };
    assert_eq!(answered(emitted), (1, accepted));

    // Or the actual host operation times out while run cancellation settles.
    // Cancellation does not abandon that submitted operation or invent its result.
    let (run, conversation) = h.coding(2, 101);
    let owner = h.land(conversation, 8);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Cancel { run }));
    drop(h.step(Event::Withdraw { conversation, call: Token::new(8) }));
    assert_eq!(
        &*h.step(Event::Delivered {
            owner,
            push: Delivery::Failed(crate::DeliveryFailure::new(crate::DeliveryReason::TimedOut))
        }),
        &[returned(
            8,
            Returned::DeliveryFailed { failure: crate::DeliveryFailure::new(crate::DeliveryReason::TimedOut) }
        )]
    );
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (2, cancelled()));
}

#[test]
fn a_push_that_lands_after_the_deadline_wins_over_it() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    h.after(BUDGET.time);
    // The run's deadline is main's expiry, and its call's deadline.
    assert_eq!(&*h.fire(), &[Request::Close { peer: Token::new(100) }]);
    assert!(h.fire().is_empty(), "call expiry retains the submitted host operation");
    // The push wins the race with its cancel.
    assert_eq!(
        &*h.step(Event::Delivered { owner, push: delivered() }),
        &[returned(7, Returned::Delivered(receipts()))]
    );
    assert!(h.step(Event::Withdraw { conversation, call: Token::new(7) }).is_empty(), "returned already");
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    let accepted = Answer::Accepted { outcome: Declared::Change(change()), spent: Spend::ZERO };
    assert_eq!(answered(emitted), (1, accepted));

    // Over the budget, too.
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    drop(h.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
    let owner = h.land(conversation, 8);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    h.after(BUDGET.time);
    assert_eq!(&*h.fire(), &[Request::Close { peer: Token::new(100) }]);
    assert!(h.fire().is_empty(), "call expiry retains the submitted host operation");
    assert!(h.step(Event::Withdraw { conversation, call: Token::new(8) }).is_empty(), "stopped already");
    assert_eq!(
        &*h.step(Event::Delivered { owner, push: delivered() }),
        &[returned(8, Returned::Delivered(receipts()))]
    );
    let total = spend(BUDGET.input + 1);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: total });
    assert_eq!(answered(emitted), (1, Answer::Accepted { outcome: Declared::Change(change()), spent: total }));
}

#[test]
fn checks_that_pass_once_the_run_winds_down_push_nothing() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    h.after(BUDGET.time);
    assert_eq!(&*h.fire(), &[Request::Close { peer: Token::new(100) }]);
    // Main's caller deadline equals the run's time budget and has passed;
    // this pre-submission check returns timeout without any host delivery.
    assert_eq!(&*h.step(Event::Checked { owner, ran: ran(0, b"") }), &[returned(7, Returned::TimedOut)]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Time), Spend::ZERO)));
}

#[test]
fn a_cancel_while_the_run_winds_down_changes_nothing() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.running(1, 100);
    h.after(BUDGET.time);
    drop(h.fire());
    assert!(h.step(Event::Cancel { run }).is_empty(), "the ending is decided");
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Time), Spend::ZERO)));

    // Winding down to an accepted outcome, too.
    let (run, conversation) = h.running(2, 101);
    drop(h.step(finish(conversation, 7, verdict(b"approve", Box::new([])))));
    assert!(h.step(Event::Cancel { run }).is_empty(), "the ending is decided");
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    let accepted = Answer::Accepted { outcome: verdict(b"approve", Box::new([])), spent: Spend::ZERO };
    assert_eq!(answered(emitted), (2, accepted));
}

#[test]
fn a_cancel_of_a_run_that_has_answered_changes_nothing() {
    let mut h = Harness::new(LIMITS);
    let (run, conversation) = h.running(1, 100);
    drop(h.step(Event::Ended { conversation, end: End::Fault(Fault::Provider), spend: Spend::ZERO }));
    assert!(h.step(Event::Cancel { run }).is_empty(), "answered, not reclaimed yet");
    h.domain.reclaim();
    assert!(h.step(Event::Cancel { run }).is_empty(), "answered and gone");
    // Its slot taken by another run, the old name still finds nothing.
    let (_, _) = h.running(2, 101);
    assert!(h.step(Event::Cancel { run }).is_empty(), "a stale name");
    assert_eq!(h.domain.runs(), 1);
}

#[test]
fn a_finish_with_no_room_for_its_call_is_busy_and_the_run_goes_on() {
    let mut h = Harness::new(Limits { calls: 1, ..LIMITS });
    let (_, first) = h.coding(1, 100);
    let _: Token = h.land(first, 7);
    let (_, second) = h.coding(2, 101);
    let emitted = h.step(finish(second, 8, Declared::Change(change())));
    assert_eq!(&*emitted, &[returned(8, Returned::Busy)]);
    assert_eq!(h.domain.calls(), 1);
    assert_eq!(h.step(end_turn(second)).len(), 1, "the run goes on: a nudge");
}

// Sub-agents.

fn refused(refusal: AskRefusal) -> Returned {
    Returned::Refused { refusal }
}

fn families(inspect: bool, modify: bool, agents: bool) -> crate::charter::Families {
    crate::charter::Families { tools: Tools { inspect, modify, shell: false }, agents }
}

/// The test charter, granting sub-agents and listing one LLM for them.
fn agents() -> Charter {
    let grants = Grants { agents: true, ..charter().grants };
    let models = Box::new([Llm { account: 0, endpoint: Endpoint(2), model: bytes(b"model-b"), max_tokens: 512 }]);
    Charter { grants, models, ..charter() }
}

fn ask(
    conversation: Token,
    call: u64,
    wanted: crate::charter::Families,
    llm: Option<Box<[u8]>>,
    share: Option<Spend>,
) -> Event {
    let ask = Ask::SubAgent { brief: bytes(b"Find the parser."), families: wanted, llm, share };
    Event::Delegated {
        name: crate::CallName { completion: 1, position: 0 },
        conversation,
        call: Token::new(call),
        ask,
        deadline: EXPIRY,
    }
}

/// What `conversation` asks for a sub-agent that may inspect, as its call
/// `call` due by `deadline`.
fn ask_by(conversation: Token, call: u64, deadline: Time) -> Event {
    let ask =
        Ask::SubAgent { brief: bytes(b"Find it."), families: families(true, false, false), llm: None, share: None };
    Event::Delegated {
        name: crate::CallName { completion: 1, position: 0 },
        conversation,
        call: Token::new(call),
        ask,
        deadline,
    }
}

impl Harness {
    /// Starts a run of `charter` for call `call`, finding no guide, and its
    /// main conversation as `peer`: the run's token and main's.
    fn running_on(&mut self, call: u64, peer: u64, charter: Charter) -> (Token, Token) {
        let emitted = self.start(call, charter);
        let [Request::Admitted { run, .. }, Request::Read { .. }] = &*emitted else {
            panic!("expected a read, got {emitted:?}");
        };
        let run = *run;
        let emitted = self.step(Event::Read { owner: run, read: Read::Missing });
        let [Request::Open { conversation, .. }] = &*emitted else {
            panic!("expected main to open, got {emitted:?}");
        };
        let conversation = *conversation;
        drop(self.step(Event::Started { conversation, peer: Token::new(peer) }));
        (run, conversation)
    }

    /// Has `asker` ask for a sub-agent as `call`, started as `peer`: its
    /// opening and its token.
    fn child(&mut self, asker: Token, call: u64, wanted: crate::charter::Families, peer: u64) -> (Token, Opening) {
        let emitted = self.step(ask(asker, call, wanted, None, None));
        let Ok(one) = Box::<[Request; 1]>::try_from(emitted) else { panic!("expected one request") };
        let [Request::Open { conversation, opening }] = *one else { panic!("expected the sub-agent to open") };
        drop(self.step(Event::Started { conversation, peer: Token::new(peer) }));
        (conversation, opening)
    }
}

#[test]
fn a_sub_agent_answers_with_its_last_message_once_it_has_ended() {
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    drop(h.step(Event::Used { conversation: main, spend: spend(100) }));
    let (child, opening) = h.child(main, 7, families(true, false, false), 101);
    assert_eq!((&opening.llm, opening.finish, opening.families), (&agents().llm, false, families(true, false, false)));
    let left = Budget { turns: BUDGET.turns - 1, input: BUDGET.input - 100, output: BUDGET.output - 10, ..BUDGET };
    assert_eq!(opening.budget, left, "its share is what the run has left");
    assert!(opening.system.starts_with(b"Find the parser."), "its brief is its asker's");
    drop(h.step(Event::Used { conversation: child, spend: spend(50) }));
    let yielded =
        Event::Yielded { conversation: child, stop: Stop::EndTurn, text: bytes(b"The parser is in src/parse.rs.") };
    assert_eq!(&*h.step(yielded), &[Request::Close { peer: Token::new(101) }], "a sub-agent that yields is done");
    let emitted = h.step(Event::Ended { conversation: child, end: End::Closed, spend: spend(50) });
    let answer = Returned::Answered { text: bytes(b"The parser is in"), cut: 14, stop: Stop::EndTurn };
    assert_eq!(&*emitted, &[returned(7, answer)], "its answer, cut at the limit");
    h.domain.reclaim();
    assert_eq!((h.domain.conversations(), h.domain.calls()), (1, 0));
}

#[test]
fn a_sub_agent_runs_on_the_llm_named_for_it_among_the_charters() {
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    let emitted = h.step(ask(main, 7, families(true, false, false), Some(bytes(b"model-b")), None));
    let [Request::Open { opening, .. }] = &*emitted else { panic!("expected the sub-agent to open") };
    assert_eq!(&opening.llm, &agents().models[0]);
    let share = Spend { turns: 2, input: 500, output: 1_000_000, cache_read: 0, cache_write: 0 };
    let emitted = h.step(ask(main, 8, families(true, false, false), None, Some(share)));
    let [Request::Open { opening, .. }] = &*emitted else { panic!("expected the sub-agent to open") };
    let budget =
        Budget { turns: 2, input: 500, output: BUDGET.output, cache_read: 0, cache_write: 0, time: BUDGET.time };
    assert_eq!(opening.budget, budget, "a share asked for, no larger than what is left");
}

#[test]
fn an_ask_the_run_cannot_grant_returns_why_and_the_run_goes_on() {
    // Not granted sub-agents, or wider families than its own.
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, charter());
    assert_eq!(
        &*h.step(ask(main, 7, families(true, false, false), None, None)),
        &[returned(7, refused(AskRefusal::NotGranted))]
    );
    let (_, main) = h.running_on(2, 101, agents());
    assert_eq!(
        &*h.step(ask(main, 8, families(true, true, false), None, None)),
        &[returned(8, refused(AskRefusal::NotGranted))]
    );
    // An LLM the charter does not list.
    assert_eq!(
        &*h.step(ask(main, 9, families(true, false, false), Some(bytes(b"model-z")), None)),
        &[returned(9, refused(AskRefusal::UnknownLlm))]
    );
    // A share with no turn in it.
    let none = Spend { turns: 0, ..spend(10) };
    assert_eq!(
        &*h.step(ask(main, 10, families(true, false, false), None, Some(none))),
        &[returned(10, refused(AskRefusal::Unworkable))]
    );

    // Too deep, and too many.
    let mut h = Harness::new(Limits { depth: 1, ..LIMITS });
    let (_, main) = h.running_on(1, 100, agents());
    let (child, _) = h.child(main, 7, families(true, false, true), 101);
    assert_eq!(
        &*h.step(ask(child, 8, families(true, false, false), None, None)),
        &[returned(8, refused(AskRefusal::TooDeep))]
    );
    let mut h = Harness::new(Limits { run_conversations: 2, ..LIMITS });
    let (_, main) = h.running_on(1, 100, agents());
    drop(h.child(main, 7, families(true, false, false), 101));
    assert_eq!(
        &*h.step(ask(main, 8, families(true, false, false), None, None)),
        &[returned(8, refused(AskRefusal::TooMany))]
    );

    // No room for the call or the conversation.
    let mut h = Harness::new(Limits { calls: 1, run_conversations: 3, ..LIMITS });
    let (_, main) = h.running_on(1, 100, agents());
    drop(h.child(main, 7, families(true, false, false), 101));
    assert_eq!(&*h.step(ask(main, 8, families(true, false, false), None, None)), &[returned(8, Returned::Busy)]);

    // Past the budget.
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    drop(h.step(Event::Used { conversation: main, spend: spend(BUDGET.input + 1) }));
    assert_eq!(
        &*h.step(ask(main, 7, families(true, false, false), None, None)),
        &[returned(7, refused(AskRefusal::Over))]
    );
}

#[test]
fn a_sub_agent_that_ends_without_answering_returns_how_it_ended() {
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    let (child, _) = h.child(main, 7, families(true, false, false), 101);
    let emitted = h.step(Event::Ended { conversation: child, end: End::Fault(Fault::Provider), spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(7, Returned::Unanswered { end: End::Fault(Fault::Provider) })]);
    assert_eq!(h.step(end_turn(main)).len(), 1, "the run goes on: main is nudged");
}

#[test]
fn a_withdrawn_sub_agent_is_closed_and_its_call_returns_once_it_has_ended() {
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    let (child, _) = h.child(main, 7, families(true, false, false), 101);
    assert_eq!(
        &*h.step(Event::Withdraw { conversation: main, call: Token::new(7) }),
        &[Request::Close { peer: Token::new(101) }]
    );
    // Its answer crossed the close: the call is cancelled all the same.
    let yielded = Event::Yielded { conversation: child, stop: Stop::EndTurn, text: bytes(b"late") };
    assert!(h.step(yielded).is_empty(), "closing already");
    let emitted = h.step(Event::Ended { conversation: child, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(7, Returned::Cancelled)]);
    h.domain.reclaim();

    // One withdrawn before it starts is closed once it does.
    let emitted = h.step(ask(main, 8, families(true, false, false), None, None));
    let [Request::Open { conversation: child, .. }] = &*emitted else { panic!("expected the sub-agent to open") };
    let child = *child;
    assert!(h.step(Event::Withdraw { conversation: main, call: Token::new(8) }).is_empty(), "not started yet");
    assert_eq!(
        &*h.step(Event::Started { conversation: child, peer: Token::new(102) }),
        &[Request::Close { peer: Token::new(102) }]
    );
    let emitted = h.step(Event::Ended { conversation: child, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(8, Returned::Cancelled)]);
}

#[test]
fn a_sub_agent_past_its_deadline_is_closed_and_returns_timed_out_once_it_has_ended() {
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    let minute = Duration::from_secs(60);
    let emitted = h.step(ask_by(main, 7, Time::ZERO.saturating_add(minute)));
    let [Request::Open { conversation: child, opening }] = &*emitted else { panic!("expected the sub-agent to open") };
    assert_eq!(opening.budget.time, minute, "its time runs out by its call's deadline");
    let child = *child;
    drop(h.step(Event::Started { conversation: child, peer: Token::new(101) }));
    h.after(minute);
    assert_eq!(&*h.fire(), &[Request::Close { peer: Token::new(101) }]);
    // Its answer crossed the close.
    let yielded = Event::Yielded { conversation: child, stop: Stop::EndTurn, text: bytes(b"late") };
    assert!(h.step(yielded).is_empty(), "closing already");
    let emitted = h.step(Event::Ended { conversation: child, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(7, Returned::TimedOut)]);
    assert_eq!(h.step(end_turn(main)).len(), 1, "the run goes on: main is nudged");
    h.domain.reclaim();

    // One withdrawn first returns as cancelled, its deadline no longer armed.
    let emitted = h.step(ask_by(main, 8, h.env.now.saturating_add(minute)));
    let [Request::Open { conversation: child, .. }] = &*emitted else { panic!("expected the sub-agent to open") };
    let child = *child;
    drop(h.step(Event::Started { conversation: child, peer: Token::new(102) }));
    drop(h.step(Event::Withdraw { conversation: main, call: Token::new(8) }));
    h.after(minute);
    assert!(!h.domain.is_due(h.env.now), "a withdraw cancels its call's deadline");
    let emitted = h.step(Event::Ended { conversation: child, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(8, Returned::Cancelled)]);
}

#[test]
fn a_cancel_closes_the_tree_one_owner_at_a_time() {
    let mut h = Harness::new(LIMITS);
    let (run, main) = h.running_on(1, 100, agents());
    let (child, _) = h.child(main, 7, families(true, false, true), 101);
    let (grandchild, _) = h.child(child, 8, families(true, false, false), 102);
    // The run closes main only; each closing conversation withdraws its calls.
    assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(100) }]);
    assert_eq!(
        &*h.step(Event::Withdraw { conversation: main, call: Token::new(7) }),
        &[Request::Close { peer: Token::new(101) }]
    );
    // A call that crosses its conversation's close returns at once, and opens
    // nothing.
    let crossed = ask(child, 9, families(true, false, false), None, None);
    assert_eq!(&*h.step(crossed), &[returned(9, Returned::Cancelled)]);
    assert_eq!(
        &*h.step(Event::Withdraw { conversation: child, call: Token::new(8) }),
        &[Request::Close { peer: Token::new(102) }]
    );
    let emitted = h.step(Event::Ended { conversation: grandchild, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(8, Returned::Cancelled)]);
    let emitted = h.step(Event::Ended { conversation: child, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(&*emitted, &[returned(7, Returned::Cancelled)]);
    let emitted = h.step(Event::Ended { conversation: main, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, cancelled()));
}

#[test]
fn a_sub_agent_that_spends_past_the_budget_winds_the_run_down() {
    let mut h = Harness::new(LIMITS);
    let (_, main) = h.running_on(1, 100, agents());
    let (child, _) = h.child(main, 7, families(true, false, false), 101);
    let emitted = h.step(Event::Used { conversation: child, spend: spend(BUDGET.input + 1) });
    assert_eq!(&*emitted, &[Request::Close { peer: Token::new(100) }], "main is closed, and closes the rest");
    drop(h.step(Event::Withdraw { conversation: main, call: Token::new(7) }));
    drop(h.step(Event::Ended { conversation: child, end: End::Closed, spend: spend(BUDGET.input + 1) }));
    let emitted = h.step(Event::Ended { conversation: main, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, failed(Failure::Budget(Exhausted::Input), spend(BUDGET.input + 1))));
}

// Facts.

/// The facts the domain holds, oldest first.
fn facts(h: &mut Harness) -> Box<[Fact]> {
    let mut facts = List::with_capacity(LIMITS.facts);
    for _ in 0..LIMITS.facts {
        let Some(fact) = h.domain.pop_fact() else { break };
        facts.push(fact).expect("room for every fact kept");
    }
    facts.into_boxed()
}

#[test]
fn a_run_tells_what_it_did_as_content_free_facts() {
    let mut h = Harness::new(LIMITS);
    let emitted = h.start(1, agents());
    let [Request::Admitted { run, .. }, Request::Read { .. }] = &*emitted else { panic!("expected a read") };
    let run = *run;
    drop(h.step(Event::Read { owner: run, read: Read::Text { text: bytes(b"Be kind."), whole: true } }));
    let main = Token::new(0);
    drop(h.step(Event::Started { conversation: main, peer: Token::new(100) }));
    let (child, _) = h.child(main, 7, families(true, false, false), 101);
    drop(h.step(Event::Yielded { conversation: child, stop: Stop::EndTurn, text: bytes(b"done") }));
    drop(h.step(Event::Ended { conversation: child, end: End::Closed, spend: Spend::ZERO }));
    drop(h.step(finish(main, 8, verdict(b"approve", Box::new([])))));
    drop(h.step(Event::Ended { conversation: main, end: End::Closed, spend: Spend::ZERO }));
    let told = facts(&mut h);
    let expected = [
        Fact::Admitted { run },
        Fact::Prepared { run, guides: 1, checks: 0 },
        Fact::Opened { run, conversation: main, depth: 0 },
        Fact::Called { run, conversation: main, call: Token::new(7), ask: Asked::SubAgent },
        Fact::Opened { run, conversation: child, depth: 1 },
        Fact::Ended { run, conversation: child, end: End::Closed },
        Fact::Returned { run, call: Token::new(7), result: Return::Answered },
        Fact::Called { run, conversation: main, call: Token::new(8), ask: Asked::Finish },
        Fact::Returned { run, call: Token::new(8), result: Return::Accepted },
        Fact::Ended { run, conversation: main, end: End::Closed },
        Fact::Answered { run, answer: Answered::Accepted },
    ];
    assert_eq!(&*told, &expected);
    assert_eq!(h.domain.facts_lost(), 0);
}

#[test]
fn facts_that_do_not_fit_are_dropped_and_counted_and_change_nothing() {
    let mut h = Harness::new(Limits { facts: 2, ..LIMITS });
    let (run, conversation) = h.running(1, 100);
    assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(100) }]);
    let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO });
    assert_eq!(answered(emitted), (1, cancelled()));
    assert_eq!(facts(&mut h).len(), 2);
    assert_eq!(h.domain.facts_lost(), 3, "opened, ended and answered did not fit");
}

#[test]
fn nothing_to_push_is_specific_feedback_and_the_run_can_retry() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    assert_eq!(&*h.step(Event::Delivered { owner, push: Delivery::Nothing }), &[returned(7, Returned::Nothing)]);
    assert_eq!(h.step(end_turn(conversation)).len(), 1, "a retry is nudged");
}

#[test]
fn failed_push_reason_and_diagnostics_return_to_the_finish_caller() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.coding(1, 100);
    let owner = h.land(conversation, 7);
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    drop(h.step(Event::Checked { owner, ran: ran(0, b"") }));
    let failure = crate::DeliveryFailure {
        reason: crate::DeliveryReason::RefusedByTarget,
        diagnostic: crate::Diagnostic::new(b"remote: protected branch", 17),
    };
    assert_eq!(
        &*h.step(Event::Delivered { owner, push: Delivery::Failed(failure) }),
        &[returned(7, Returned::DeliveryFailed { failure })]
    );
    let mut pushed = None;
    for fact in facts(&mut h) {
        match fact {
            Fact::Delivered { push, .. } => pushed = Some(push),
            Fact::Admitted { .. }
            | Fact::Prepared { .. }
            | Fact::Opened { .. }
            | Fact::Ended { .. }
            | Fact::Called { .. }
            | Fact::Returned { .. }
            | Fact::CheckStarted { .. }
            | Fact::CheckFinished { .. }
            | Fact::Answered { .. } => {}
        }
    }
    let pushed = pushed.expect("delivery ended fact");
    assert_eq!(
        pushed,
        crate::DeliveryStatus::Failed(crate::DeliveryReason::RefusedByTarget),
        "facts retain classification, no diagnostic content"
    );
}

fn text_charter(failure: bool) -> Charter {
    let contract = crate::outcome::TextSpec {
        min: u32::from(failure),
        max: 8,
        fields: Box::new([FieldRule { name: bytes(b"source"), max: 4 }]),
    };
    Charter {
        outcome: OutcomeSpec {
            change: None,
            verdicts: Box::new([]),
            report: if failure { None } else { Some(contract.clone()) },
            failure: if failure { Some(contract) } else { None },
        },
        ..charter()
    }
}

fn text_result(failure: bool, text: &[u8]) -> Declared {
    let fields = Box::new([Field { name: bytes(b"source"), value: bytes(b"ref") }]);
    if failure {
        Declared::Failure(crate::outcome::DeclaredFailure { reason: bytes(text), fields })
    } else {
        Declared::Report(crate::outcome::Report { text: bytes(text), fields })
    }
}

#[test]
fn reports_and_declared_failures_settle_once_without_checks_or_push() {
    for (failure, text) in [(false, b"".as_slice()), (true, b"no".as_slice())] {
        let mut h = Harness::new(LIMITS);
        let (run, conversation) = h.running_on(1, 100, text_charter(failure));
        let emitted = h.step(finish(conversation, 7, text_result(failure, text)));
        assert_eq!(&*emitted, &[returned(7, Returned::Accepted), Request::Close { peer: Token::new(100) }]);
        assert!(h.step(Event::Cancel { run: Token::new(999) }).is_empty());
        let emitted = h.step(Event::Ended { conversation, end: End::Closed, spend: spend(5) });
        assert_eq!(answered(emitted), (1, Answer::Accepted { outcome: text_result(failure, text), spent: spend(5) }));
        assert!(h.step(Event::Cancel { run }).is_empty(), "a late cancel emits no second answer");
    }
}

#[test]
fn rejected_failure_reason_can_be_corrected_without_starting_delivery() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running_on(1, 100, text_charter(true));
    let emitted = h.step(finish(conversation, 7, text_result(true, b"")));
    assert_eq!(
        &*emitted,
        &[returned(
            7,
            Returned::Rejected {
                problems: Problems {
                    listed: Box::new([Problem::TextTooShort { form: crate::outcome::Form::Failure, min: 1 }]),
                    more: 0
                }
            }
        )]
    );
    let emitted = h.step(finish(conversation, 8, text_result(true, b"no")));
    assert_eq!(&*emitted, &[returned(8, Returned::Accepted), Request::Close { peer: Token::new(100) }]);
}

#[test]
fn impossible_result_contracts_refuse_before_any_preparation_or_session() {
    let mut c = text_charter(false);
    c.outcome.report.as_mut().unwrap().min = 9;
    let mut h = Harness::new(LIMITS);
    assert_eq!(answered(h.start(1, c)), (1, Answer::Refused(Refusal::Invalid(Invalid::Outcome))));
    assert_eq!((h.domain.runs(), h.domain.conversations()), (0, 0));
    let min = u64::try_from(size_of::<Field>()).unwrap() + 6 + 1;
    let mut h = Harness::new(Limits { outcome_bytes: min - 1, ..LIMITS });
    assert_eq!(answered(h.start(2, text_charter(false))), (2, Answer::Refused(Refusal::Invalid(Invalid::Outcome))));
    assert_eq!((h.domain.runs(), h.domain.conversations()), (0, 0));
}

#[test]
fn unknown_extra_field_ownership_is_checked_before_shape_judgement() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running_on(1, 100, text_charter(false));
    let mut oversized = List::with_capacity(u32::try_from(LIMITS.outcome_bytes).expect("the fixture cap is small"));
    for _ in 0..LIMITS.outcome_bytes {
        oversized.push(b'x').expect("room for exactly the declared cap");
    }
    let value = Declared::Report(crate::outcome::Report {
        text: Box::new([]),
        fields: Box::new([
            Field { name: bytes(b"source"), value: bytes(b"ref") },
            Field { name: bytes(b"opaque"), value: oversized.into_boxed() },
        ]),
    });
    let emitted = h.step(finish(conversation, 7, value));
    assert_eq!(
        &*emitted,
        &[returned(7, Returned::Rejected { problems: crate::outcome::too_large(LIMITS.outcome_bytes) })]
    );
    assert_eq!(
        &*h.step(finish(conversation, 8, text_result(false, b""))),
        &[returned(8, Returned::Accepted), Request::Close { peer: Token::new(100) }]
    );
}

fn receipts() -> crate::Delivered {
    crate::Delivered::new(Box::new([crate::Receipt::new(0, bytes(b"host receipt")).expect("bounded receipt")]))
        .expect("one changed directory")
}

fn delivered() -> Delivery {
    Delivery::Delivered(receipts())
}

fn mid_report() -> Charter {
    let mut charter = text_charter(false);
    charter.checkout.repositories[0].writable = true;
    charter.grants.deliver = Some(ChangeSpec { fields: Box::new([]) });
    charter
}

fn mid_running(harness: &mut Harness) -> (Token, Token) {
    let requests = harness.start(1, mid_report());
    let [Request::Admitted { run, .. }, Request::Read { .. }] = &*requests else { panic!("admitted read") };
    let run = *run;
    let requests = harness.step(Event::Read { owner: run, read: Read::Missing });
    let [Request::Probe { .. }] = requests.as_ref() else {
        panic!("prepare writable check");
    };
    let requests = harness.step(Event::Probed { owner: run, executable: true });
    let [Request::Open { conversation, opening }] = &*requests else { panic!("prepared main") };
    assert!(opening.deliver);
    let conversation = *conversation;
    drop(harness.step(Event::Started { conversation, peer: Token::new(100) }));
    (run, conversation)
}

fn mid_ask(conversation: Token, completion: u32) -> Event {
    Event::Delegated {
        conversation,
        call: Token::new(50),
        name: crate::CallName { completion, position: 2 },
        ask: Ask::Deliver { change: Change { fields: Box::new([]) } },
        deadline: EXPIRY,
    }
}

fn submit_mid(harness: &mut Harness, conversation: Token) -> Token {
    let requests = harness.step(mid_ask(conversation, 3));
    let [Request::Check { owner, .. }, Request::Checking { .. }] = &*requests else { panic!("all writable checks") };
    let owner = *owner;
    let requests = harness.step(Event::Checked { owner, ran: ran(0, b"passed") });
    let [Request::Deliver { name, deadline, .. }] = &*requests else { panic!("actual bounded submission") };
    assert_eq!(*name, crate::CallName { completion: 3, position: 2 });
    assert_eq!(*deadline, harness.env.now.saturating_add(LIMITS.delivery_timeout).min(EXPIRY));
    owner
}

#[test]
fn mid_report_landing_continues_but_an_interrupted_landing_has_its_own_answer() {
    for interrupted in [false, true] {
        let mut harness = Harness::new(LIMITS);
        let (run, conversation) = mid_running(&mut harness);
        let owner = submit_mid(&mut harness, conversation);
        if interrupted {
            drop(harness.step(Event::Cancel { run }));
            assert!(harness.step(Event::Withdraw { conversation, call: Token::new(50) }).is_empty());
        }
        let requests = harness.step(Event::Delivered { owner, push: delivered() });
        assert_eq!(requests.as_ref(), &[returned(50, Returned::Delivered(receipts()))]);
        assert!(
            harness.step(Event::Delivered { owner, push: delivered() }).is_empty(),
            "stale duplicate is inert before reclaim"
        );
        if !interrupted {
            drop(harness.step(finish(conversation, 51, report())));
        }
        let answer = answered(harness.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO })).1;
        if interrupted {
            assert_eq!(
                answer,
                Answer::Delivered {
                    name: crate::CallName { completion: 3, position: 2 },
                    receipts: receipts(),
                    stopped: Failure::Cancelled,
                    spent: Spend::ZERO
                }
            );
        } else {
            assert_eq!(answer, Answer::Accepted { outcome: report(), spent: Spend::ZERO });
        }
        harness.domain.reclaim();
        assert!(
            harness.step(Event::Delivered { owner, push: delivered() }).is_empty(),
            "stale generation is inert after reclaim"
        );
    }
}

#[test]
fn malformed_host_mount_or_zero_origin_never_becomes_successful_delivery() {
    let mut harness = Harness::new(LIMITS);
    let (_, conversation) = mid_running(&mut harness);
    assert_eq!(
        harness.step(mid_ask(conversation, 0)).as_ref(),
        &[returned(50, Returned::Refused { refusal: AskRefusal::Name })]
    );
    let owner = submit_mid(&mut harness, conversation);
    let wrong = crate::Delivered::new(Box::new([crate::Receipt::new(1, bytes(b"unmounted")).expect("sealed ordinal")]))
        .expect("sealed terminal");
    assert_eq!(
        harness.step(Event::Delivered { owner, push: Delivery::Delivered(wrong) }).as_ref(),
        &[returned(
            50,
            Returned::DeliveryFailed { failure: crate::DeliveryFailure::new(crate::DeliveryReason::Broken) }
        )]
    );
    drop(harness.step(finish(conversation, 51, report())));
    let answer = answered(harness.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO })).1;
    assert_eq!(answer, Answer::Accepted { outcome: report(), spent: Spend::ZERO });
}

fn report() -> Declared {
    text_result(false, b"done")
}

#[test]
fn time_and_spend_shutdown_keep_an_already_submitted_mid_landing() {
    for timed in [false, true] {
        let mut harness = Harness::new(LIMITS);
        let (_, conversation) = mid_running(&mut harness);
        let owner = submit_mid(&mut harness, conversation);
        let expected = if timed {
            harness.env.now = EXPIRY;
            for _ in 0_u32..2 {
                if harness.domain.is_due(harness.env.now) {
                    drop(harness.fire());
                }
            }
            Failure::Budget(Exhausted::Time)
        } else {
            drop(harness.step(Event::Used { conversation, spend: spend(BUDGET.input + 1) }));
            Failure::Budget(Exhausted::Input)
        };
        let expected_spend = if timed { Spend::ZERO } else { spend(BUDGET.input + 1) };
        let requests = harness.step(Event::Delivered { owner, push: delivered() });
        if timed {
            assert_eq!(requests.as_ref(), &[returned(50, Returned::Delivered(receipts()))]);
        } else {
            assert_eq!(
                requests.as_ref(),
                &[returned(50, Returned::Delivered(receipts())), Request::Close { peer: Token::new(100) },]
            );
        }
        // Closing the real session waits for the host terminal; its one End
        // then reports the spend it already used, without another delivery.
        let answer = answered(harness.step(Event::Ended { conversation, end: End::Closed, spend: expected_spend })).1;
        assert_eq!(
            answer,
            Answer::Delivered {
                name: crate::CallName { completion: 3, position: 2 },
                receipts: receipts(),
                stopped: expected,
                spent: expected_spend,
            }
        );
    }
}

#[test]
fn generic_non_marker_host_refusal_is_feedback_and_report_can_finish() {
    let mut harness = Harness::new(LIMITS);
    let (_, conversation) = mid_running(&mut harness);
    let owner = submit_mid(&mut harness, conversation);
    let refusal = crate::DeliveryRefusal::new(None, bytes(b"host asks for corrected metadata"))
        .expect("bounded generic correctable feedback");
    assert_eq!(
        harness.step(Event::Delivered { owner, push: Delivery::Refused(refusal.clone()) }).as_ref(),
        &[returned(50, Returned::DeliveryRefused(refusal))]
    );
    drop(harness.step(finish(conversation, 51, report())));
    let answer = answered(harness.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO })).1;
    assert_eq!(answer, Answer::Accepted { outcome: report(), spent: Spend::ZERO });
}

#[test]
fn impossible_delivery_grant_and_zero_host_deadline_refuse_before_preparation() {
    let mut charter = mid_report();
    charter.grants.deliver = Some(ChangeSpec { fields: Box::new([FieldRule { name: bytes(b"ticket"), max: 0 }]) });
    let mut harness = Harness::new(LIMITS);
    assert_eq!(answered(harness.start(1, charter)), (1, Answer::Refused(Refusal::Invalid(Invalid::Grants))));
    assert_eq!((harness.domain.runs(), harness.domain.conversations()), (0, 0));
    let mut harness = Harness::new(Limits { delivery_timeout: Duration::ZERO, ..LIMITS });
    assert_eq!(answered(harness.start(2, mid_report())), (2, Answer::Refused(Refusal::Invalid(Invalid::Checkout))));
    assert_eq!((harness.domain.runs(), harness.domain.conversations()), (0, 0));
}

#[test]
fn caller_only_stop_keeps_submitted_mid_delivery_and_ordinary_continuation() {
    for expired in [false, true] {
        for later_cancel in [false, true] {
            let mut harness = Harness::new(LIMITS);
            let (run, conversation) = mid_running(&mut harness);
            let caller_deadline = harness.env.now.saturating_add(Duration::from_secs(1));
            let mut ask = mid_ask(conversation, 3);
            let Event::Delegated { deadline, .. } = &mut ask else { unreachable!("delegated fixture") };
            *deadline = caller_deadline;
            let requests = harness.step(ask);
            let [Request::Check { owner, .. }, Request::Checking { .. }] = requests.as_ref() else {
                panic!("delivery checks first")
            };
            let owner = *owner;
            let requests = harness.step(Event::Checked { owner, ran: ran(0, b"passed") });
            let [Request::Deliver { deadline, .. }] = requests.as_ref() else {
                panic!("checks submit exactly one delivery")
            };
            assert_eq!(*deadline, caller_deadline);
            if expired {
                harness.env.now = caller_deadline;
                assert!(harness.domain.is_due(harness.env.now));
                assert!(harness.fire().is_empty(), "expiry retains the actual host terminal right");
            } else {
                assert!(harness.step(Event::Withdraw { conversation, call: Token::new(50) }).is_empty());
            }
            assert_eq!(
                harness.step(Event::Delivered { owner, push: delivered() }).as_ref(),
                &[returned(50, Returned::Delivered(receipts()))],
                "caller-only stop neither abandons the host nor closes main"
            );
            if later_cancel {
                assert_eq!(harness.step(Event::Cancel { run }).as_ref(), &[Request::Close { peer: Token::new(100) }]);
            } else {
                drop(harness.step(finish(conversation, 51, report())));
            }
            let answer = answered(harness.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO })).1;
            assert_eq!(
                answer,
                if later_cancel {
                    Answer::Failed { failure: Failure::Cancelled, spent: Spend::ZERO }
                } else {
                    Answer::Accepted { outcome: report(), spent: Spend::ZERO }
                }
            );
        }
    }
}

fn host_ask(conversation: Token, call: u64, deadline: Time) -> Event {
    Event::Delegated {
        conversation,
        call: Token::new(call),
        name: crate::CallName { completion: 7, position: 3 },
        ask: Ask::Host {
            tool: bytes(b"comment"),
            effect: crate::HostEffect::Read,
            input: crate::HostInput::attested(bytes(br#" {"whole":{"opaque":[true,1]},"verbatim":"\u0041"} "#))
                .expect("protocol-attested fixture"),
        },
        deadline,
    }
}

fn host_submission(emitted: &[Request]) -> crate::RelayName {
    let [Request::HostCall { relay, worker, name, input, deadline, tool, effect }] = emitted else {
        panic!("one opaque relay, got {emitted:?}");
    };
    assert_eq!(*worker, Token::new(71));
    assert_eq!(*name, crate::CallName { completion: 7, position: 3 });
    assert_eq!(tool.as_ref(), b"comment");
    assert_eq!(*effect, crate::HostEffect::Read);
    assert_eq!(input.bytes(), br#" {"whole":{"opaque":[true,1]},"verbatim":"\u0041"} "#);
    assert!(*deadline > Time::ZERO);
    *relay
}

#[test]
fn host_busy_and_lost_recover_only_after_actual_terminal_with_same_name_and_bytes() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(71, 99);
    let deadline = h.env.now.saturating_add(Duration::from_secs(30));
    let first = host_submission(&h.step(host_ask(conversation, 41, deadline)));
    assert_eq!(first.attempt, 1);
    assert!(h.step(Event::HostReturned { relay: first, reply: crate::HostReply::Busy }).is_empty());
    h.after(LIMITS.host_backoff);
    let second = host_submission(&h.fire());
    assert_eq!((second.owner, second.attempt), (first.owner, 2));
    assert!(
        h.step(Event::HostReturned {
            relay: first,
            reply: crate::HostReply::Answered(crate::HostAnswer::new(bytes(b"stale"), false).expect("small answer"))
        })
        .is_empty(),
        "stale attempt cannot resolve current attempt"
    );
    assert!(
        h.step(Event::HostReturned { relay: second, reply: crate::HostReply::Unanswered(crate::Unanswered::Lost) })
            .is_empty()
    );
    h.after(LIMITS.host_backoff);
    let third = host_submission(&h.fire());
    assert_eq!(third.attempt, 3);
    let actual = crate::HostAnswer::new(bytes(b"recorded first decision"), true).expect("bounded exact error");
    assert_eq!(
        &*h.step(Event::HostReturned { relay: third, reply: crate::HostReply::Answered(actual.clone()) }),
        &[Request::Return { call: Token::new(41), result: Returned::HostAnswered(actual) }]
    );
    assert!(
        h.step(Event::HostReturned { relay: third, reply: crate::HostReply::Busy }).is_empty(),
        "one logical feedback"
    );
    assert_eq!(h.domain.next_deadline(), Some(Time::ZERO.saturating_add(BUDGET.time)));
}

#[test]
fn host_uncertainty_survives_busy_until_cap_withdrawal_and_caller_expiry() {
    for stop in 0_u32..3 {
        let mut h = Harness::new(LIMITS);
        let (_, conversation) = h.running(71, 99);
        let deadline = h.env.now.saturating_add(Duration::from_secs(30));
        let first = host_submission(&h.step(host_ask(conversation, 41, deadline)));
        assert!(
            h.step(Event::HostReturned { relay: first, reply: crate::HostReply::Unanswered(crate::Unanswered::Lost) })
                .is_empty()
        );
        h.after(LIMITS.host_backoff);
        let second = host_submission(&h.fire());
        assert!(h.step(Event::HostReturned { relay: second, reply: crate::HostReply::Busy }).is_empty());
        let emitted = if stop == 0 {
            h.after(LIMITS.host_backoff);
            let third = host_submission(&h.fire());
            h.step(Event::HostReturned { relay: third, reply: crate::HostReply::Busy })
        } else if stop == 1 {
            h.step(Event::Withdraw { conversation, call: Token::new(41) })
        } else {
            h.env.now = deadline;
            h.fire()
        };
        assert_eq!(&*emitted, &[Request::Return { call: Token::new(41), result: Returned::HostUnknown }]);
        assert_eq!((h.domain.runs(), h.domain.conversations()), (1, 1), "call-only stop retains ordinary run");
    }
}

#[test]
fn host_withdrawal_and_timeout_retain_relay_until_terminal_answer_wins() {
    for stop in 0_u32..3 {
        let mut h = Harness::new(LIMITS);
        let (run, conversation) = h.running(71, 99);
        let deadline = h.env.now.saturating_add(Duration::from_secs(30));
        let relay = host_submission(&h.step(host_ask(conversation, 41, deadline)));
        if stop == 0 {
            assert_eq!(
                &*h.step(Event::Withdraw { conversation, call: Token::new(41) }),
                &[Request::WithdrawHost { relay }]
            );
        } else if stop == 1 {
            h.after(Duration::from_secs(5));
            assert_eq!(&*h.fire(), &[Request::WithdrawHost { relay }]);
        } else {
            assert_eq!(&*h.step(Event::Cancel { run }), &[Request::Close { peer: Token::new(99) }]);
            assert_eq!(
                &*h.step(Event::Withdraw { conversation, call: Token::new(41) }),
                &[Request::WithdrawHost { relay }]
            );
        }
        assert!(h.step(Event::Withdraw { conversation, call: Token::new(41) }).is_empty());
        assert_eq!(h.domain.calls(), 1, "withdrawal did not fabricate terminal");
        let actual = crate::HostAnswer::new(bytes(b"already recorded"), false).expect("small answer");
        assert_eq!(
            &*h.step(Event::HostReturned { relay, reply: crate::HostReply::Answered(actual.clone()) }),
            &[Request::Return { call: Token::new(41), result: Returned::HostAnswered(actual) }]
        );
        if stop == 2 {
            let (_, answer) = answered(h.step(Event::Ended { conversation, end: End::Closed, spend: Spend::ZERO }));
            assert_eq!(answer, failed(Failure::Cancelled, Spend::ZERO));
        }
    }
}

#[test]
fn host_unknown_is_conveyed_after_actual_withdrawn_terminal_when_shutdown_disallows_recovery() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(71, 99);
    let deadline = h.env.now.saturating_add(Duration::from_secs(30));
    let relay = host_submission(&h.step(host_ask(conversation, 41, deadline)));
    assert_eq!(&*h.step(Event::Withdraw { conversation, call: Token::new(41) }), &[Request::WithdrawHost { relay }]);
    assert_eq!(
        &*h.step(Event::HostReturned { relay, reply: crate::HostReply::Unanswered(crate::Unanswered::Withdrawn) }),
        &[Request::Return { call: Token::new(41), result: Returned::HostUnknown }]
    );
}

#[test]
fn host_declarations_are_admitted_as_bounded_unique_contracts_before_io() {
    let baseline = charter();
    assert_eq!(crate::charter::check(&baseline, &LIMITS), Ok(()));
    for invalid in 0_u32..5 {
        let mut declared = charter();
        match invalid {
            0 => declared.grants.host_tools[0].name = bytes(b"finish"),
            1 => {
                declared.grants.host_tools =
                    Box::new([declared.grants.host_tools[0].clone(), declared.grants.host_tools[0].clone()]);
            }
            2 => declared.grants.host_tools[0].timeout = Duration::ZERO,
            3 => declared.grants.host_tools[0].description = Box::new([]),
            4 => declared.grants.host_tools[0].schema = Box::new([]),
            _ => unreachable!("five concrete refusals"),
        }
        let mut h = Harness::new(LIMITS);
        let (_, answer) = answered(h.start(71, declared));
        assert_eq!(answer, Answer::Refused(Refusal::Invalid(Invalid::Grants)));
        assert_eq!((h.domain.runs(), h.domain.conversations(), h.domain.calls()), (0, 0, 0));
    }
    let mut h = Harness::new(Limits { host_attempts: 0, ..LIMITS });
    let (_, answer) = answered(h.start(71, baseline));
    assert_eq!(answer, Answer::Refused(Refusal::Invalid(Invalid::Grants)));
}

#[test]
fn undeclared_host_and_effect_mismatch_are_refused_before_relay() {
    let mut h = Harness::new(LIMITS);
    let (_, conversation) = h.running(71, 99);
    for (tool, effect, problem) in [
        (b"absent".as_slice(), crate::HostEffect::Read, crate::HostProblem::Undeclared),
        (b"comment".as_slice(), crate::HostEffect::Write, crate::HostProblem::Effect),
    ] {
        let event = Event::Delegated {
            conversation,
            call: Token::new(101),
            name: crate::CallName { completion: 1, position: 0 },
            ask: Ask::Host {
                tool: bytes(tool),
                effect,
                input: crate::HostInput::attested(bytes(b"{}")).expect("attested empty object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        };
        assert_eq!(
            &*h.step(event),
            &[Request::Return { call: Token::new(101), result: Returned::HostRejected(problem) }]
        );
    }
    assert_eq!(h.domain.calls(), 0);
}
