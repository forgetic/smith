//! End to end at the run child domain: runs started by a scripted host, with a
//! scripted partner playing their conversations, in a domain world.
//!

use std::collections::BTreeMap;

use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use smith_domain_run as run;
use smith_domain_run::{Answer, Budget, Exhausted, Failure, Fault, Invalid, Limits, Policy, Refusal};
use smith_run_world::host;
use smith_run_world::partner::Script;
use smith_run_world::{Checkouts, Settings, Span, World, noisy};

const ITERATIONS: u32 = 1_000_000;

fn settled(settings: &Settings) -> World {
    let mut world = World::new(*settings);
    world.run(ITERATIONS);
    world
}

fn answers(world: &World) -> Vec<&Answer> {
    world.answers().map(|answer| answer.expect("every start is answered")).collect()
}

/// A retained host decision outlives the crashed domain. The host writes its
/// own wake message from the answer it actually recorded.
struct CrashHost {
    starts: Vec<run::Event>,
    decision: Option<run::HostAnswer>,
}

impl CrashHost {
    fn new() -> Self {
        let calm = Settings::calm(606);
        let script = host::Script { jobs: 2, window: Duration::ZERO, host_tools: 1000, ..calm.host };
        let mut host = host::Host::new(script, 606);
        let mut starts = Vec::new();
        host.fire(Time::ZERO, &mut starts);
        assert_eq!(starts.len(), 2);
        Self { starts, decision: None }
    }

    fn start(&mut self, activation: u64) -> run::Event {
        let start = self.starts.remove(0);
        let run::Event::Start { charter, workspace, .. } = start else { panic!("scripted host starts a run") };
        run::Event::Start {
            messages: Box::default(),
            reply_to: skein_lib::ReplyTo::new(Token::new(88)),
            host_run: Token::new(91),
            activation,
            window: run::Window { turns: u32::MAX, bytes: u64::MAX, largest_turn: 1 },
            charter: run::Charter { resume: true, ..charter },
            workspace,
            transcript: None,
            resumed: false,
        }
    }

    fn answer(&mut self, relay: run::RelayName) -> run::Event {
        let answer = run::HostAnswer::new(b"host accepted change".as_slice().into(), false).expect("bounded answer");
        self.decision = Some(answer.clone());
        run::Event::HostReturned { relay, reply: run::HostReply::Answered(answer) }
    }

    fn waking_text(&self) -> Box<[u8]> {
        let answer = self.decision.as_ref().expect("the host retained its answer");
        let mut text = b"host: the previous host call was answered: ".to_vec();
        text.extend_from_slice(answer.text());
        text.into_boxed_slice()
    }
}

fn crash_take(
    domain: &mut run::Domain,
    env: &Env<run::Limits>,
    out: &mut Queue<run::Request>,
    event: run::Event,
) -> Vec<run::Request> {
    while domain.pop_fact().is_some() {}
    run::step(domain, env, event, out);
    let mut requests = Vec::new();
    while let Some(request) = out.pop() {
        requests.push(request);
    }
    requests
}

fn crash_open(
    domain: &mut run::Domain,
    env: &Env<run::Limits>,
    out: &mut Queue<run::Request>,
    start: run::Event,
    activation: u64,
) -> (Token, Token) {
    let started = crash_take(domain, env, out, start);
    let Some(run::Request::Read { owner: run, .. }) =
        started.iter().find(|request| matches!(request, run::Request::Read { .. }))
    else {
        panic!("admitted run prepares its main: {started:?}")
    };
    let run = *run;
    let mut prepared = crash_take(domain, env, out, run::Event::Read { owner: run, read: run::Read::Missing });
    for _ in 0..8_u32 {
        let next = match prepared.as_slice() {
            [run::Request::Read { owner, .. }] => Some(run::Event::Read { owner: *owner, read: run::Read::Missing }),
            [run::Request::Probe { owner, .. }] => Some(run::Event::Probed { owner: *owner, executable: false }),
            [run::Request::Open { .. }] => None,
            _ => panic!("preparation request: {prepared:?}"),
        };
        let Some(next) = next else { break };
        prepared = crash_take(domain, env, out, next);
    }
    let Some(run::Request::Open { conversation, opening }) =
        prepared.iter().find(|request| matches!(request, run::Request::Open { .. }))
    else {
        panic!("main opens: {prepared:?}")
    };
    assert_eq!(opening.activation, activation);
    assert!(opening.transcript.is_none());
    let conversation = *conversation;
    assert!(crash_take(domain, env, out, run::Event::Started { conversation, peer: Token::new(99) }).is_empty());
    (run, conversation)
}

fn reserved_start(host: &mut CrashHost, spend: u64) -> run::Event {
    let mut start = host.start(1);
    let run::Event::Start { charter, .. } = &mut start else { panic!("scripted host starts a run") };
    charter.budget.spend = spend;
    charter.grants.agents = true;
    start
}

#[test]
fn a_completion_whose_maximum_does_not_fit_is_not_made_and_the_run_ends_for_its_budget() {
    let mut host = CrashHost::new();
    let limits = Settings::calm(606).run;
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(run::MAX_OUT);
    let mut domain = run::Domain::new(&limits);
    let (_, main) = crash_open(&mut domain, &env, &mut out, reserved_start(&mut host, 100), 1);
    assert_eq!(run::reserve(&mut domain, main, Token::new(801), 101), Err(Exhausted::Spend));
    let ended = crash_take(
        &mut domain,
        &env,
        &mut out,
        run::Event::Ended { conversation: main, end: run::End::Budget(Exhausted::Spend), spend: run::Spend::ZERO },
    );
    assert!(matches!(ended.as_slice(), [run::Request::Answer {
        answer: Answer::Failed { failure: Failure::Budget(Exhausted::Spend), spent, .. }, ..
    }] if *spent == run::Spend::ZERO));
}

#[test]
fn sub_agents_reserve_from_the_runs_budget_and_spend_never_passes_it() {
    let mut host = CrashHost::new();
    let limits = Settings::calm(606).run;
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(run::MAX_OUT);
    let mut domain = run::Domain::new(&limits);
    let (_, main) = crash_open(&mut domain, &env, &mut out, reserved_start(&mut host, 100), 1);
    let asked = crash_take(
        &mut domain,
        &env,
        &mut out,
        run::Event::Delegated {
            conversation: main,
            call: Token::new(7),
            name: run::CallName { activation: 1, completion: 1, position: 0 },
            ask: run::Ask::SubAgent {
                brief: b"review".as_slice().into(),
                families: run::charter::Families {
                    tools: run::charter::Tools { inspect: false, modify: false, shell: false },
                    agents: false,
                },
                llm: None,
                share: None,
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    let [run::Request::Open { conversation: child, .. }] = asked.as_slice() else {
        panic!("the run opened its child: {asked:?}")
    };
    let child = *child;
    assert!(
        crash_take(&mut domain, &env, &mut out, run::Event::Started { conversation: child, peer: Token::new(101) })
            .is_empty()
    );

    let first = Token::new(801);
    let second = Token::new(802);
    assert_eq!(run::reserve(&mut domain, main, first, 60), Ok(()));
    assert_eq!(run::reserve(&mut domain, child, second, 50), Err(Exhausted::Spend));
    assert!(run::settle_reservation(&mut domain, main, first, 20));
    assert!(
        crash_take(
            &mut domain,
            &env,
            &mut out,
            run::Event::Priced { conversation: main, own_spent: 20, subtree_spent: 20 },
        )
        .is_empty()
    );
    assert_eq!(run::reserve(&mut domain, child, second, 50), Ok(()));
    assert!(run::settle_reservation(&mut domain, child, second, 30));
    assert!(
        crash_take(
            &mut domain,
            &env,
            &mut out,
            run::Event::Priced { conversation: child, own_spent: 30, subtree_spent: 30 },
        )
        .is_empty()
    );
    let child_spend = run::Spend { turns: 1, units: 30, ..run::Spend::ZERO };
    let returned = crash_take(
        &mut domain,
        &env,
        &mut out,
        run::Event::Ended { conversation: child, end: run::End::Closed, spend: child_spend },
    );
    assert!(matches!(returned.as_slice(), [run::Request::Return { spent: 30, .. }]));
    assert_eq!(run::reserve(&mut domain, main, Token::new(803), 51), Err(Exhausted::Spend));
    let ended = crash_take(
        &mut domain,
        &env,
        &mut out,
        run::Event::Ended {
            conversation: main,
            end: run::End::Budget(Exhausted::Spend),
            spend: run::Spend { turns: 1, units: 20, ..run::Spend::ZERO },
        },
    );
    assert!(matches!(ended.as_slice(), [run::Request::Answer {
        answer: Answer::Failed { failure: Failure::Budget(Exhausted::Spend), spent, .. }, ..
    }] if spent.units == 50 && spent.units <= 100));
}

#[test]
fn answered_host_call_before_turn_crash_wakes_as_host_text_with_new_call_namespace() {
    let mut host = CrashHost::new();
    let limits = Settings::calm(606).run;
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(run::MAX_OUT);
    let mut first = run::Domain::new(&limits);
    let (_, conversation) = crash_open(&mut first, &env, &mut out, host.start(1), 1);
    let old_name = run::CallName { activation: 1, completion: 1, position: 0 };
    let called = crash_take(
        &mut first,
        &env,
        &mut out,
        run::Event::Delegated {
            conversation,
            call: Token::new(101),
            name: old_name,
            ask: run::Ask::Host {
                tool: b"comment".as_slice().into(),
                effect: run::HostEffect::Read,
                input: run::HostInput::attested(b"{}".as_slice().into()).expect("object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    let [run::Request::HostCall { relay, name, .. }] = called.as_slice() else { panic!("one host call: {called:?}") };
    assert_eq!(*name, old_name);
    let returned = crash_take(&mut first, &env, &mut out, host.answer(*relay));
    assert!(matches!(returned.as_slice(), [run::Request::Return { result: run::Returned::HostAnswered(_), .. }]));
    assert!(!returned.iter().any(|request| matches!(request, run::Request::Turn { .. })));
    drop(first);

    let mut resumed = run::Domain::new(&limits);
    let (run, conversation) = crash_open(&mut resumed, &env, &mut out, host.start(2), 2);
    let notice = host.waking_text();
    assert!(
        crash_take(
            &mut resumed,
            &env,
            &mut out,
            run::Event::Message { label: Box::from(&b"host"[..]), run, name: Token::new(5), text: notice[6..].into() },
        )
        .is_empty()
    );
    let waking = crash_take(
        &mut resumed,
        &env,
        &mut out,
        run::Event::Yielded { conversation, stop: run::Stop::EndTurn, text: b"ready".as_slice().into() },
    );
    assert!(matches!(waking.as_slice(), [run::Request::Say { peer, text }]
        if *peer == Token::new(99) && text == &notice));
    assert!(!waking.iter().any(|request| matches!(request, run::Request::Return { .. } | run::Request::Turn { .. })));

    let stale = crash_take(
        &mut resumed,
        &env,
        &mut out,
        run::Event::Delegated {
            conversation,
            call: Token::new(103),
            name: old_name,
            ask: run::Ask::Host {
                tool: b"comment".as_slice().into(),
                effect: run::HostEffect::Read,
                input: run::HostInput::attested(b"{}".as_slice().into()).expect("object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    assert!(matches!(
        stale.as_slice(),
        [run::Request::Return { result: run::Returned::Refused { refusal: run::AskRefusal::Name }, .. }]
    ));

    let new_name = run::CallName { activation: 2, completion: 1, position: 0 };
    let fresh = crash_take(
        &mut resumed,
        &env,
        &mut out,
        run::Event::Delegated {
            conversation,
            call: Token::new(102),
            name: new_name,
            ask: run::Ask::Host {
                tool: b"comment".as_slice().into(),
                effect: run::HostEffect::Read,
                input: run::HostInput::attested(b"{}".as_slice().into()).expect("object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    assert!(matches!(fresh.as_slice(), [run::Request::HostCall { name, .. }] if *name == new_name));
}

fn failure(answer: &Answer) -> Failure {
    match answer {
        Answer::Failed { failure, .. } => *failure,
        Answer::Refused(refusal) => panic!("the run was refused: {refusal:?}"),
        Answer::Parked { .. } => panic!("this source partner never requests waiting"),
        Answer::Accepted { outcome, .. } => panic!("the run finished with {outcome:?}"),
    }
}

#[test]
fn an_llm_that_keeps_stopping_is_nudged_until_the_run_fails_as_unfinished() {
    let calm = Settings::calm(1);
    let world = settled(&Settings { partner: Script { yields: 1000, ..calm.partner }, ..calm });
    for answer in answers(&world) {
        assert_eq!(failure(answer), Failure::Policy(Policy::Unfinished { nudges: calm.run.nudges, rejected: 0 }));
    }
    let stats = world.stats();
    assert_eq!((stats.opens, stats.says, stats.closes), (4, 4 * calm.run.nudges, 4));
    assert_eq!(stats.partner.nudged, 4 * calm.run.nudges);
}

#[test]
fn an_llm_that_fails_fails_its_run_with_the_fault() {
    let calm = Settings::calm(2);
    let world = settled(&Settings { partner: Script { faults: 1000, ..calm.partner }, ..calm });
    for answer in answers(&world) {
        assert!(
            [Failure::Model(Fault::Provider), Failure::Model(Fault::ContextFull)].contains(&failure(answer)),
            "{answer:?}"
        );
    }
    assert_eq!(world.stats().closes, 0, "the conversations ended on their own");
}

#[test]
fn an_llm_whose_last_stop_shows_a_fault_fails_the_run_with_it() {
    let calm = Settings::calm(3);
    let script = Script { yields: 1000, odd_stops: 1000, ..calm.partner };
    let world = settled(&Settings { run: Limits { nudges: 0, ..calm.run }, partner: script, ..calm });
    let faults = [Fault::Truncated, Fault::Refused, Fault::Malformed].map(Failure::Model);
    for answer in answers(&world) {
        assert!(faults.contains(&failure(answer)), "{answer:?}");
    }
}

#[test]
fn an_llm_that_works_on_runs_out_of_turns() {
    let calm = Settings::calm(4);
    let world = settled(&Settings { partner: Script { yields: 0, ..calm.partner }, ..calm });
    for answer in answers(&world) {
        assert_eq!(failure(answer), Failure::Budget(Exhausted::Turns));
    }
    assert!(world.stats().partner.ceilings >= 4, "each conversation kept to its share of turns");
}

#[test]
fn an_llm_that_spends_past_the_tokens_fails_its_run_for_budget() {
    let calm = Settings::calm(5);
    let host = host::Script { tokens_min: 5_000, tokens_max: 20_000, ..calm.host };
    let world = settled(&Settings { host, partner: Script { yields: 0, ..calm.partner }, ..calm });
    let tokens = [Exhausted::Spend, Exhausted::Spend, Exhausted::Spend, Exhausted::Spend].map(Failure::Budget);
    for answer in answers(&world) {
        assert!(tokens.contains(&failure(answer)), "{answer:?}");
    }
}

#[test]
fn a_run_out_of_time_fails_for_time() {
    let calm = Settings::calm(6);
    let host = host::Script { time: Span::millis(10_000, 20_000), ..calm.host };
    let script = Script { turn: Span::millis(3_000, 8_000), yields: 0, ..calm.partner };
    let world = settled(&Settings { host, partner: script, ..calm });
    for answer in answers(&world) {
        assert_eq!(failure(answer), Failure::Budget(Exhausted::Time));
    }
}

#[test]
fn a_cancelled_run_closes_its_conversation_and_answers_as_cancelled() {
    let calm = Settings::calm(7);
    let host = host::Script { cancels: 1000, cancel: Span::millis(0, 30_000), ..calm.host };
    let world = settled(&Settings { host, partner: Script { yields: 0, ..calm.partner }, ..calm });
    for answer in answers(&world) {
        assert_eq!(failure(answer), Failure::Cancelled);
    }
    let stats = world.stats();
    assert_eq!((stats.cancels, stats.closes), (4, 4));
}

#[test]
fn a_cancel_that_comes_before_main_starts_closes_main_once_it_does() {
    let calm = Settings::calm(8);
    let host = host::Script { cancels: 1000, cancel: Span::millis(0, 0), ..calm.host };
    // The checkout is read at once, and the cancel is back within two trips to
    // the host; main's start takes two hops longer than that.
    let checkout = Checkouts { io: Span::millis(0, 0), ..calm.checkout };
    let world = settled(&Settings { host, hop: Span::millis(50, 100), checkout, ..calm });
    for answer in answers(&world) {
        assert_eq!(failure(answer), Failure::Cancelled);
    }
    let first = |what: &str| world.trace().iter().position(|line| line.contains(what)).expect("it happened");
    assert!(first("run <- Cancel {") < first("run <- Started {"), "the first cancel came before the first start");
    let stats = world.stats();
    assert_eq!((stats.cancels, stats.closes, stats.partner.closed), (4, 4, 4));
}

#[test]
fn a_cancel_while_the_run_reads_its_checkout_answers_once_the_read_has_ended() {
    let calm = Settings::calm(13);
    let host = host::Script { cancels: 1000, cancel: Span::millis(0, 0), ..calm.host };
    let checkout = Checkouts { io: Span::millis(500, 1_000), ..calm.checkout };
    let world = settled(&Settings { host, checkout, ..calm });
    for answer in answers(&world) {
        assert_eq!(failure(answer), Failure::Cancelled);
    }
    let stats = world.stats();
    assert_eq!((stats.cancels, stats.opens, stats.reads), (4, 0, 4), "each run stopped at its first read");
}

#[test]
fn a_run_reads_its_checkouts_guides_and_looks_for_checks() {
    let calm = Settings::calm(14);
    let checkout = Checkouts { guides: 1000, checks: 1000, ..calm.checkout };
    let world = settled(&Settings { partner: Script { yields: 1000, ..calm.partner }, checkout, ..calm });
    let stats = world.stats();
    assert_eq!(stats.opens, 4);
    assert!(stats.reads >= 4 && stats.probes > 0, "{stats:?}");
}

#[test]
fn starts_beyond_the_run_slots_are_refused_as_busy() {
    let calm = Settings::calm(8);
    let settings = Settings {
        run: Limits { runs: 1, ..calm.run },
        host: host::Script { window: Duration::ZERO, ..calm.host },
        partner: Script { yields: 1000, ..calm.partner },
        ..calm
    };
    let world = settled(&settings);
    let busy = answers(&world).into_iter().filter(|answer| **answer == Answer::Refused(Refusal::Busy)).count();
    assert_eq!(busy, 3);
}

#[test]
fn charters_beyond_the_limits_are_refused_as_invalid() {
    let calm = Settings::calm(9);
    let run = Limits { budget: Budget { turns: 10, ..calm.run.budget }, ..calm.run };
    let world = settled(&Settings { run, ..calm });
    for answer in answers(&world) {
        assert_eq!(answer, &Answer::Refused(Refusal::Invalid(Invalid::Budget)));
    }
    assert_eq!(world.stats().opens, 0);
}

#[test]
fn a_main_conversation_refused_at_its_entrance_refuses_its_run() {
    let calm = Settings::calm(10);
    let world = settled(&Settings { partner: Script { conversations: 0, ..calm.partner }, ..calm });
    for answer in answers(&world) {
        assert_eq!(answer, &Answer::Refused(Refusal::Busy));
    }
    let world = settled(&Settings { partner: Script { invalid: 1000, ..calm.partner }, ..calm });
    for answer in answers(&world) {
        assert_eq!(answer, &Answer::Refused(Refusal::Invalid(Invalid::Conversation)));
    }
}

/// A world where the LLM finishes, with outcomes that fit, and changes land
/// after their checks fail a time or two.
fn finishing(seed: u64) -> Settings {
    let calm = Settings::calm(seed);
    Settings {
        host: host::Script { writable: 1000, changes: 1000, verdicts: 1000, ..calm.host },
        partner: Script { finishes: 300, yields: 0, ..calm.partner },
        checkout: Checkouts { checks: 1000, check_failures: 2, ..calm.checkout },
        ..calm
    }
}

#[test]
fn an_llm_that_finishes_with_what_fits_has_its_run_accepted() {
    let world = settled(&finishing(15));
    for answer in answers(&world) {
        assert!(matches!(answer, Answer::Accepted { .. }), "{answer:?}");
    }
    assert_eq!(world.stats().partner.accepted, 4);
}

#[test]
fn changes_land_after_their_checks_pass_and_their_push_goes_through() {
    let settings = finishing(16);
    let world = settled(&Settings { partner: Script { changes: 1000, ..settings.partner }, ..settings });
    let stats = world.stats();
    assert_eq!(stats.partner.accepted, 4, "{stats:?}");
    assert!(stats.partner.checks_failed > 0, "{stats:?}");
    assert!(world.trace().iter().any(|line| line.contains("Checking {")), "the run tells the host of its checks");
    assert_eq!(stats.host.notices, stats.checks * 2, "the host hears each check start and end");
}

#[test]
fn a_change_whose_branch_moved_ends_its_run_as_stale() {
    let settings = finishing(19);
    let world = settled(&Settings {
        host: host::Script { moved: 1000, ..settings.host },
        partner: Script { changes: 1000, ..settings.partner },
        ..settings
    });
    let stats = world.stats();
    assert!(stats.partner.moved > 0 && stats.partner.accepted == 0, "{stats:?}");
    for answer in answers(&world) {
        assert!(matches!(answer, Answer::Failed { failure: Failure::Stale, .. }), "{answer:?}");
    }
}

#[test]
fn an_outcome_that_does_not_fit_is_rejected_and_the_llm_tries_again() {
    let settings = finishing(17);
    let world = settled(&Settings { partner: Script { good: 300, changes: 0, ..settings.partner }, ..settings });
    let stats = world.stats();
    assert!(stats.partner.rejected > 0, "{stats:?}");
    for answer in answers(&world) {
        assert!(matches!(answer, Answer::Accepted { .. } | Answer::Failed { .. }), "{answer:?}");
    }
}

#[test]
fn a_check_deadline_terminal_returns_feedback_without_submitting_to_host() {
    let settings = finishing(18);
    let world = settled(&Settings {
        host: host::Script { time: Span::millis(4_000, 4_000), ..settings.host },
        partner: Script { changes: 1000, finishes: 1000, turn: Span::millis(100, 100), ..settings.partner },
        checkout: Checkouts {
            check: Span::millis(8_000, 8_000),
            check_failures: 0,
            io: Span::millis(1, 1),
            ..settings.checkout
        },
        races: 0,
        // Actual Root Opened/Started is synchronous. This focused story
        // adds no artificial session hop, so the IO deadline really ties
        // the run alarm; noisy worlds retain their independent hop races.
        hop: Span::millis(0, 0),
        ..settings
    });
    let stats = world.stats();
    // IO supplies its deadline terminal before alarms fire (programming-model.md, section 2).
    // The stopped check returns feedback; there is no live check to abort or delivery to submit.
    assert!(stats.checks == 4 && stats.partner.checks_failed == 4, "{stats:?}");
    assert_eq!(stats.aborts, 0, "{stats:?}");
    assert_eq!(stats.pushes, 0, "{stats:?}");
    for answer in answers(&world) {
        assert!(matches!(answer, Answer::Failed { failure: Failure::Budget(Exhausted::Time), .. }), "{answer:?}");
    }
}

#[test]
fn a_seed_replays_to_the_same_run() {
    let trace = skein_world::domain::assert_replays(12, 13, |seed| {
        let world = settled(&noisy(seed));
        (world.trace().to_vec(), (world.stats(), world.now()))
    });
    assert!(trace.len() > 50, "the runs did something");
}

/// Adds what `world` counted of the states cancels and deadlines found runs in.
fn tally(world: &World, cancels: &mut BTreeMap<&'static str, u32>, deadlines: &mut BTreeMap<&'static str, u32>) {
    for (cell, count) in world.cancel_cells() {
        *cancels.entry(*cell).or_insert(0) += count;
    }
    for (cell, count) in world.deadline_cells() {
        *deadlines.entry(*cell).or_insert(0) += count;
    }
}

/// Cancels at random moments, from the host (once, twice, late) and handed
/// to the run right before or after any of its events, find runs in every
/// state, and each answers once (checked by `World::run`).
#[test]
fn cancels_find_runs_in_every_state() {
    let (mut cancels, mut deadlines) = (BTreeMap::new(), BTreeMap::new());
    for seed in 0..250 {
        tally(&settled(&noisy(seed)), &mut cancels, &mut deadlines);
    }
    // Cancelled twice while the run reads a slow checkout.
    let calm = Settings::calm(20);
    let host = host::Script { cancels: 1000, cancel: Span::millis(0, 1_000), recancels: 1000, ..calm.host };
    let checkout = Checkouts { io: Span::millis(3_000, 5_000), ..calm.checkout };
    tally(&settled(&Settings { host, checkout, ..calm }), &mut cancels, &mut deadlines);
    // Cancelled as changes are checked and pushed, and as the LLM spends past
    // its budget.
    for seed in 21..24 {
        let landing = finishing(seed);
        let run = Limits { runs: 16, conversations: 16, calls: 16, ..landing.run };
        let host = host::Script {
            jobs: 16,
            cancels: 1000,
            cancel: Span::millis(2_000, 30_000),
            tokens_min: 5_000,
            tokens_max: 40_000,
            ..landing.host
        };
        let partner = Script { changes: 1000, ..landing.partner };
        let checkout = Checkouts { check: Span::millis(2_000, 8_000), check_failures: 3, ..landing.checkout };
        let settings = Settings { run, host, partner, checkout, inject: 20, ..landing };
        tally(&settled(&settings), &mut cancels, &mut deadlines);
    }
    // Grant one new operation at a time, so cancellation can find its retained
    // phase rather than an unrelated completion or a denied charter.
    for seed in 1000..1020 {
        let calm = Settings::calm(seed);
        let host = host::Script {
            cancels: 1000,
            cancel: Span::millis(100, 2_000),
            time: Span::millis(60_000, 60_000),
            ..calm.host
        };
        let partner = Script { waits: 1000, yields: 0, turn: Span::millis(10, 50), ..calm.partner };
        tally(&settled(&Settings { host, partner, ..calm }), &mut cancels, &mut deadlines);

        let host = host::Script {
            cancels: 1000,
            cancel: Span::millis(100, 6_000),
            host_tools: 1000,
            relay: Span::millis(8_000, 8_000),
            relay_busy: 500,
            relay_lost: 500,
            time: Span::millis(60_000, 60_000),
            ..calm.host
        };
        let partner = Script { host_calls: 1000, yields: 0, turn: Span::millis(10, 50), ..calm.partner };
        tally(&settled(&Settings { host, partner, ..calm }), &mut cancels, &mut deadlines);

        let host = host::Script {
            cancels: 1000,
            cancel: Span::millis(30, 600),
            host_tools: 1000,
            relay: Span::millis(1, 1),
            relay_busy: 1000,
            relay_lost: 0,
            time: Span::millis(60_000, 60_000),
            ..calm.host
        };
        tally(&settled(&Settings { host, partner, ..calm }), &mut cancels, &mut deadlines);
    }
    let cells = [
        "preparing",
        "stopping",
        "opening",
        "working",
        "waiting",
        "relay sending",
        "relay withdrawing",
        "relay backoff",
        "landing",
        "over",
        "winding",
        "answered",
        "gone",
    ];
    for cell in cells {
        assert!(cancels.get(cell).is_some_and(|count| *count > 0), "no cancel found a run {cell}: {cancels:?}");
    }
}

/// Deadlines short against a slow checkout and slow conversations fall due in
/// every state a run's deadline runs in.
#[test]
fn deadlines_find_runs_in_every_state_they_run_in() {
    let (mut cancels, mut deadlines) = (BTreeMap::new(), BTreeMap::new());
    for seed in 0..50 {
        let calm = Settings::calm(seed);
        let host = host::Script { time: Span::millis(1_000, 6_000), ..calm.host };
        let checkout = Checkouts { io: Span::millis(0, 2_000), ..calm.checkout };
        let partner = Script { turn: Span::millis(100, 2_000), ..calm.partner };
        let settings = Settings { host, checkout, partner, hop: Span::millis(0, 1_500), ..calm };
        tally(&settled(&settings), &mut cancels, &mut deadlines);
    }
    // And as changes are checked and pushed, and as the LLM spends past its
    // budget.
    for seed in 50..56 {
        let landing = finishing(seed);
        let run = Limits { runs: 16, conversations: 16, calls: 16, ..landing.run };
        let host = host::Script {
            jobs: 16,
            time: Span::millis(2_000, 15_000),
            tokens_min: 2_000,
            tokens_max: 8_000,
            ..landing.host
        };
        let partner = Script { changes: 1000, turn: Span::millis(100, 2_000), ..landing.partner };
        let checkout = Checkouts { check: Span::millis(2_000, 8_000), check_failures: 3, ..landing.checkout };
        let settings = Settings { run, host, partner, checkout, hop: Span::millis(0, 1_500), ..landing };
        tally(&settled(&settings), &mut cancels, &mut deadlines);
    }
    for seed in 2000..2020 {
        let calm = Settings::calm(seed);
        let partner = Script { waits: 1000, yields: 0, turn: Span::millis(10, 50), ..calm.partner };
        let host = host::Script { time: Span::millis(100, 500), ..calm.host };
        tally(&settled(&Settings { host, partner, ..calm }), &mut cancels, &mut deadlines);

        let partner = Script { host_calls: 1000, yields: 0, turn: Span::millis(10, 50), ..calm.partner };
        let host = host::Script {
            host_tools: 1000,
            relay: Span::millis(8_000, 8_000),
            time: Span::millis(100, 6_000),
            ..calm.host
        };
        tally(&settled(&Settings { host, partner, ..calm }), &mut cancels, &mut deadlines);
        let host = host::Script {
            host_tools: 1000,
            relay: Span::millis(1, 1),
            relay_busy: 1000,
            time: Span::millis(30, 300),
            ..calm.host
        };
        tally(&settled(&Settings { host, partner, ..calm }), &mut cancels, &mut deadlines);
    }
    for cell in [
        "preparing",
        "opening",
        "working",
        "waiting",
        "relay sending",
        "relay withdrawing",
        "relay backoff",
        "landing",
        "over",
    ] {
        assert!(deadlines.get(cell).is_some_and(|count| *count > 0), "no deadline found a run {cell}: {deadlines:?}");
    }
}

/// A world where the LLM asks for sub-agents and finishes now and then.
fn asking(seed: u64) -> Settings {
    let calm = Settings::calm(seed);
    Settings {
        host: host::Script { agents: 1000, ..calm.host },
        partner: Script { asks: 300, finishes: 50, yields: 100, parallel: 3, ..calm.partner },
        ..calm
    }
}

#[test]
fn sub_agents_answer_their_askers_and_end_before_their_calls_return() {
    let world = settled(&asking(30));
    let stats = world.stats();
    assert!(stats.children > 4 && stats.partner.answered > 0, "{stats:?}");
}

#[test]
fn read_only_sub_agents_asked_in_one_turn_run_side_by_side() {
    let stats = settled(&asking(34)).stats();
    assert!(stats.peak > 2 && stats.partner.answered > 2, "{stats:?}");
}

#[test]
fn asks_the_run_cannot_grant_are_refused_and_the_llm_goes_on() {
    let settings = asking(31);
    let world = settled(&Settings { partner: Script { bad_asks: 1000, ..settings.partner }, ..settings });
    let stats = world.stats();
    assert!(stats.partner.ask_refused > 0 && stats.children == 0, "{stats:?}");
}

#[test]
fn sub_agents_with_a_small_share_come_back_unanswered_when_it_runs_out() {
    let settings = asking(32);
    let partner = Script { shares: 1000, yields: 0, ..settings.partner };
    let stats = settled(&Settings { partner, ..settings }).stats();
    assert!(stats.partner.unanswered > 0, "{stats:?}");
}

#[test]
fn a_cancelled_run_closes_its_sub_agents_down_the_tree() {
    for seed in 0..128 {
        let settings = asking(seed);
        let host = host::Script { cancels: 1000, cancel: Span::millis(5_000, 15_000), ..settings.host };
        let partner = Script { turn: Span::millis(1_000, 10_000), ..settings.partner };
        let world = settled(&Settings { host, partner, ..settings });
        let stats = world.stats();
        if stats.children > 0 && stats.partner.withdrawn > 0 {
            for answer in answers(&world) {
                assert!(
                    matches!(answer, Answer::Failed { failure: Failure::Cancelled, .. } | Answer::Accepted { .. }),
                    "{answer:?}"
                );
            }
            return;
        }
    }
    panic!("no cancellation found a live child");
}

/// A bounded native queue holds pending inputs and preserves every observation.
#[test]
fn a_fact_queue_at_its_bound_holds_the_step_and_loses_nothing() {
    let settings = asking(40);
    let settings = Settings { checkout: Checkouts { checks: 1000, ..settings.checkout }, ..settings };
    let roomy = settled(&Settings { run: Limits { facts: 100_000, ..settings.run }, ..settings });
    let tight = settled(&Settings {
        run: Limits { facts: smith_domain_run::max_facts(&settings.run), ..settings.run },
        ..settings
    });
    assert_eq!(roomy.fact_holds(), 0);
    assert!(tight.fact_holds() > 0, "actual pending work was held for the native fact drain");
    for world in [&roomy, &tight] {
        let told = world.facts();
        let stats = world.stats();
        assert_eq!(told.get("opened"), Some(&stats.opens));
        assert_eq!(told.get("admitted"), told.get("answered"), "every admitted run answers once");
        assert!(told.get("called").is_some_and(|called| *called == told["returned"]), "every call returns");
        assert_eq!(answers(world).len(), usize::try_from(settings.host.jobs).expect("bounded scripted jobs"));
    }
}

#[test]
fn empty_reports_and_declared_failures_are_terminal_results_without_delivery() {
    use smith_domain_run::outcome::Declared;
    for failure in [false, true] {
        let calm = Settings::calm(41);
        let host = host::Script {
            changes: 0,
            verdicts: 0,
            reports: if failure { 0 } else { 1000 },
            failures: if failure { 1000 } else { 0 },
            ..calm.host
        };
        let partner = Script {
            finishes: 1000,
            yields: 0,
            changes: 0,
            reports: if failure { 0 } else { 1000 },
            failures: if failure { 1000 } else { 0 },
            ..calm.partner
        };
        let settings = Settings { host, partner, ..calm };
        let world = settled(&settings);
        for answer in answers(&world) {
            match answer {
                Answer::Accepted { outcome: Declared::Report(report), .. } => {
                    assert!(!failure);
                    assert!(report.text.is_empty(), "the host's zero minimum is honored");
                    assert_eq!(&*report.fields[0].value, b"README.md");
                }
                Answer::Accepted { outcome: Declared::Failure(declared), .. } => {
                    assert!(failure);
                    assert_eq!(&*declared.reason, b"Required access is unavailable.");
                }
                Answer::Accepted { outcome: Declared::Change(_) | Declared::Verdict(_), .. }
                | Answer::Failed { .. }
                | Answer::Refused(_)
                | Answer::Parked { .. } => {
                    panic!("report/failure finish must produce its actual text result: {answer:?}")
                }
            }
        }
        assert_eq!((world.stats().checks, world.stats().pushes), (0, 0));
        assert_eq!(world.stats().partner.accepted, 4);
        let silent = settled(&Settings {
            run: Limits { facts: smith_domain_run::max_facts(&settings.run), ..settings.run },
            ..settings
        });
        assert_eq!(silent.trace(), world.trace(), "facts never decide text-result behavior");
        assert_eq!(answers(&silent), answers(&world));
        assert_eq!(silent.facts(), world.facts());
        skein_world::domain::assert_replays(41, 42, |seed| {
            let world = settled(&Settings { seed, ..settings });
            (world.trace().to_vec(), (world.stats(), world.now()))
        });
    }
}

#[test]
fn an_empty_failure_reason_is_accepted_in_the_run_world() {
    let calm = Settings::calm(43);
    let host = host::Script { changes: 0, verdicts: 0, failures: 1000, ..calm.host };
    let partner = Script { finishes: 1000, yields: 0, changes: 0, failures: 1000, good: 500, ..calm.partner };
    let world = settled(&Settings { host, partner, ..calm });
    assert_eq!(world.stats().partner.rejected, 0);
    assert_eq!(world.stats().partner.accepted, 4);
    assert_eq!((world.stats().checks, world.stats().pushes), (0, 0));
    assert!(answers(&world).iter().all(|answer| matches!(
        answer,
        Answer::Accepted { outcome: smith_domain_run::outcome::Declared::Failure(_), .. }
    )));
}
