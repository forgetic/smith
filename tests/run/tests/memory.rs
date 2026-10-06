//! Memory stays within the worst case (programming-model.md, section 6.3), measured by
//! a counting allocator: the run child domain with every run holding a charter
//! of exactly its byte limit, and every conversation started and spending.

use std::mem::size_of;

use skein_lib::{Duration, Env, Queue, ReplyTo, Time, Token, Wall};
use skein_world::domain::heap::{self, Meter};
use smith_domain_run::charter::{Endpoint, Families, Grants, HostTool, Llm, Tools};
use smith_domain_run::outcome::{Change, ChangeSpec, Declared, Field, FieldRule, ItemRule, OutcomeSpec, VerdictRule};
use smith_domain_run::{
    Answer, Ask, Budget, Charter, Domain, End, Event, Exit, Invalid, Limits, MAX_OUT, Ran, Read, Refusal, Request,
    Spend, Stop, worst_case,
};
use smith_domain_run::{Directory, Workspace};

#[global_allocator]
static HEAP: heap::Counting = heap::Counting;

fn bytes(len: u64) -> Box<[u8]> {
    vec![b'x'; usize::try_from(len).expect("a test length fits")].into_boxed_slice()
}

fn size(of: usize) -> u64 {
    u64::try_from(of).expect("a size fits")
}

/// Redistribute the original aggregate filler across maximum Section cells and
/// simultaneously nonempty instructions, titles and bodies, without adding bytes.
fn context(held: u64) -> (Box<[u8]>, smith_domain_run::Brief) {
    let count = usize::try_from(LIMITS.brief_sections).expect("bounded receiving count");
    let payload = held.checked_sub(size(size_of::<smith_domain_run::Section>() * count)).expect("room for cells");
    let instructions = bytes(payload / 4);
    let mut remainder = payload - size(instructions.len());
    let mut sections = Vec::with_capacity(count);
    for section in 0..count {
        let remaining = size(count - section);
        let title = bytes(remainder / remaining / 3);
        let text = bytes(remainder / remaining - size(title.len()));
        remainder -= size(title.len() + text.len());
        sections.push(smith_domain_run::Section { title, text });
    }
    assert_eq!(remainder, 0, "every original filler byte is owned exactly once");
    (instructions, smith_domain_run::Brief { sections: sections.into_boxed_slice() })
}

fn context_bytes(charter: &Charter) -> u64 {
    size(charter.instructions.len())
        + size(size_of::<smith_domain_run::Section>() * charter.brief.sections.len())
        + charter.brief.sections.iter().map(|section| size(section.title.len() + section.text.len())).sum::<u64>()
}

const BUDGET: Budget = Budget { turns: 10, spend: 1000, time: Duration::from_secs(3600) };

const LIMITS: Limits = Limits {
    runs: 1,
    conversations: 2,
    run_bytes: 1024,
    brief_sections: 4,
    directories: 1,
    directory_name_bytes: 64,
    conflicts: 1,
    conflict_path_bytes: 64,
    host_tools: 1,
    host_input_bytes: 65_536,
    host_reply_bytes: 65_536,
    host_timeout: Duration::from_secs(60),
    host_backoff: Duration::from_millis(50),
    host_attempts: 3,
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
    messages: 8,
    message_bytes: 4096,
    waiting: skein_lib::Duration::from_secs(300),
};

/// A charter that holds exactly `held` bytes, as the run counts them: one of
/// every charter part held in a box and a workspace attaining every receiving
/// directory/name/conflict/path cap; instructions, maximum Section cells and
/// titles/bodies occupy the unchanged aggregate remainder.
fn workspace() -> Workspace {
    Workspace {
        directories: Box::new([Directory {
            name: bytes(u64::from(LIMITS.directory_name_bytes)),
            root: Token::new(1),
            writable: true,
            git: true,
            conflicts: Box::new([bytes(u64::from(LIMITS.conflict_path_bytes))]),
        }]),
    }
}

fn charter(held: u64) -> Charter {
    let parts = (size(size_of::<Directory>())
        + u64::from(LIMITS.directory_name_bytes)
        + size(size_of::<Box<[u8]>>())
        + u64::from(LIMITS.conflict_path_bytes))
        + (size(size_of::<HostTool>()) + 14)
        + 1
        + (size(size_of::<Llm>()) + 1);
    let rule = size(size_of::<VerdictRule>()) + 1 + size(size_of::<ItemRule>()) + 1 + size(size_of::<FieldRule>()) + 1;
    let change_rules = 2 * size(size_of::<FieldRule>()) + 5 + 4;
    let convention_paths = size(b"AGENTS.md".len() + b".temper/pre-pr".len());
    let (instructions, brief) = context(held - parts - rule - change_rules - convention_paths);
    Charter {
        instructions,
        brief,

        grants: Grants {
            wait: true,
            deliver: None,
            tools: Tools { inspect: true, modify: true, shell: true },

            agents: true,
            host_tools: Box::new([HostTool {
                name: bytes(1),
                description: b"Host action".as_slice().into(),
                schema: b"{}".as_slice().into(),
                effect: smith_domain_run::HostEffect::Read,
                timeout: Duration::from_secs(5),
            }]),
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
        llm: Llm {
            prices: smith_domain_run::Prices { input: 1, cached: 0, output: 0, unit: 1 },
            account: 0,
            endpoint: Endpoint(0),
            model: bytes(1),
            max_tokens: 1,
            dialect: 1,
        },
        models: Box::new([Llm {
            prices: smith_domain_run::Prices { input: 1, cached: 0, output: 0, unit: 1 },
            account: 0,
            endpoint: Endpoint(0),
            model: bytes(1),
            max_tokens: 1,
            dialect: 1,
        }]),
        conventions: Some(smith_domain_run::Conventions {
            guide: b"AGENTS.md".as_slice().into(),
            checks: b".temper/pre-pr".as_slice().into(),
        }),
        resume: false,
        waiting: skein_lib::Duration::from_secs(30),
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

/// Observe and release a receiving request without retaining or allocating bytes.
fn observed_request(request: Request, selected: Option<&smith_domain_run::Conventions>) -> Asked {
    match request {
        Request::Open { conversation, .. } => Asked::Open { conversation },
        Request::Read { owner, at, .. } => {
            if let Some(selected) = selected {
                assert_eq!(at.path, selected.guide);
                assert_eq!(at.path.len(), smith_domain_run::Conventions::PATH_CAPACITY);
            }
            Asked::Read { owner }
        }
        Request::Probe { owner, at, .. } => {
            if let Some(selected) = selected {
                assert_eq!(at.path, selected.checks);
                assert_eq!(at.path.len(), smith_domain_run::Conventions::PATH_CAPACITY);
            }
            Asked::Probe { owner }
        }
        Request::Check { owner, program, .. } => {
            if let Some(selected) = selected {
                assert_eq!(program.path, selected.checks);
                assert_eq!(program.path.len(), smith_domain_run::Conventions::PATH_CAPACITY);
            }
            Asked::Check { owner }
        }
        Request::Answer { answer, to: _ } => Asked::Answer { answer },
        Request::HostCall { .. }
        | Request::WithdrawHost { .. }
        | Request::Turn { .. }
        | Request::Waiting { .. }
        | Request::MessageBounced { .. }
        | Request::Admitted { .. }
        | Request::Say { .. }
        | Request::Close { .. }
        | Request::Abort { .. }
        | Request::Checking { .. }
        | Request::Deliver { .. }
        | Request::Return { .. } => Asked::Other,
    }
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
    fill_selected(limits, None);
}

/// Preserve the exact aggregate payload while selecting maximum owning paths.
fn selected_charter(run_bytes: u64, selected: Option<&smith_domain_run::Conventions>) -> Charter {
    let mut charter = charter(run_bytes);
    if let Some(selected) = selected {
        let previous = charter.conventions.as_ref().expect("explicit legacy fixture policy");
        let previous_paths = size(previous.guide.len() + previous.checks.len());
        let selected_paths = size(selected.guide.len() + selected.checks.len());
        // The same exact aggregate cap is attained: move byte room
        // from the brief into the two maximum owning paths.
        let (instructions, brief) = context(context_bytes(&charter) + previous_paths - selected_paths);
        charter.instructions = instructions;
        charter.brief = brief;
        charter.conventions = Some(selected.clone());
    }
    charter
}

fn fill_selected(limits: Limits, selected: Option<&smith_domain_run::Conventions>) {
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
            asked.push(observed_request(request, selected));
        }
        meter.check(measured, bound, &limits);
        asked
    };
    let spend = Spend {
        units: 0,
        units_overflow: false,
        usage_overflow: false,
        turns: 1,
        input: 1,
        output: 1,
        cache_read: 1,
        cache_write: 1,
    };
    let expiry = Time::ZERO.saturating_add(limits.budget.time);
    for run in 0..limits.runs {
        let worker = Token::new(u64::from(run));
        let start = Event::Start {
            workspace: Some(workspace()),
            reply_to: ReplyTo::new(worker),
            worker,
            activation: 1,
            charter: selected_charter(limits.run_bytes, selected),
            transcript: None,
        };
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
        let families = Families { tools: Tools { inspect: true, modify: false, shell: false }, agents: false };
        let ask = Ask::SubAgent { brief: bytes(10), families, llm: Some(bytes(1)), share: None };
        let delegated = Event::Delegated {
            name: smith_domain_run::CallName { activation: 1, completion: 1, position: 0 },
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
        let change = full_change(limits.outcome_bytes);
        let ask = Ask::Finish { outcome: Declared::Change(change) };
        let call = Token::new(u64::from(run) + 1_000_000);
        let finish = Event::Delegated {
            conversation,
            call,
            ask,
            deadline: expiry,
            name: smith_domain_run::CallName { activation: 1, completion: 1, position: 0 },
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

    refuse_oversized_charter(limits, &env, &mut out);
}

/// The positive full-cap run above precedes this exact one-byte-over control.
fn refuse_oversized_charter(limits: Limits, env: &Env<Limits>, out: &mut Queue<Request>) {
    // A byte more is refused.
    let mut domain = Domain::new(&Limits { runs: 1, conversations: 2, ..limits });
    let worker = Token::new(0);
    let start = Event::Start {
        workspace: Some(workspace()),
        reply_to: ReplyTo::new(worker),
        worker,
        activation: 1,
        charter: charter(limits.run_bytes + 1),
        transcript: None,
    };
    smith_domain_run::step(&mut domain, env, start, out);
    let Some(Request::Answer { to: _, answer }) = out.pop() else { panic!("expected an answer") };
    assert_eq!(answer, Answer::Refused(Refusal::Invalid(Invalid::TooLarge)));
}

#[test]
fn a_domain_with_every_run_full_stays_within_its_worst_case() {
    fill(LIMITS);
    fill(Limits { runs: 64, conversations: 128, calls: 128, run_bytes: 65_536, guide_bytes: 32_768, ..LIMITS });
    fill(Limits { runs: 1000, conversations: 2000, calls: 2000, run_bytes: 2048, guide_bytes: 16, ..LIMITS });
}

fn full_change(outcome_bytes: u64) -> Change {
    Change {
        fields: Box::new([
            Field { name: b"title".as_slice().into(), value: bytes(1) },
            Field {
                name: b"body".as_slice().into(),
                value: bytes(outcome_bytes - 2 * size(size_of::<Field>()) - 5 - 4 - 1),
            },
        ]),
    }
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
            Request::Turn { .. }
            | Request::Waiting { .. }
            | Request::MessageBounced { .. }
            | Request::Admitted { .. }
            | Request::Checking { .. }
            | Request::Close { .. } => {}
            unexpected @ (Request::HostCall { .. }
            | Request::WithdrawHost { .. }
            | Request::Answer { .. }
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
    let limits = Limits {
        directories: smith_domain_run::MAX_DIRECTORIES,
        directory_name_bytes: 256,
        conflicts: 64,
        conflict_path_bytes: 4096,
        run_bytes: 65_536,
        brief_sections: 4,
        ..LIMITS
    };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(MAX_OUT);
    let meter = Meter::new();
    let mut domain = Domain::new(&limits);
    let charter = full_delivery_charter(limits);
    let worker = Token::new(10);
    let (owner, _) = delivery_memory_step(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Event::Start {
            workspace: Some(full_delivery_workspace()),
            reply_to: ReplyTo::new(worker),
            worker,
            activation: 1,
            charter,
            transcript: None,
        },
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
            name: smith_domain_run::CallName { activation: 1, completion: 1, position: 0 },
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

/// Complete caller-owned charter at the aggregate cap, with all directories
/// writable and every exact receiving receipt slot exercised by the real path.
fn full_delivery_workspace() -> Workspace {
    let directories: Box<[Directory]> = (0..smith_domain_run::MAX_DIRECTORIES)
        .map(|directory| Directory {
            name: Box::new([u8::try_from(directory + 64).expect("64 distinct names")]),
            root: Token::new(u64::from(directory)),
            writable: true,
            git: true,
            conflicts: Box::new([]),
        })
        .collect();
    Workspace { directories }
}

fn full_delivery_charter(limits: Limits) -> Charter {
    let parts = u64::from(smith_domain_run::MAX_DIRECTORIES) * (size(size_of::<Directory>()) + 1)
        + 1
        + size(b"AGENTS.md".len() + b".temper/pre-pr".len());
    let (instructions, brief) = context(limits.run_bytes - parts);
    Charter {
        instructions,
        brief,

        grants: Grants {
            wait: true,
            deliver: Some(ChangeSpec { fields: Box::new([]) }),
            tools: Tools { inspect: true, modify: true, shell: true },

            agents: false,
            host_tools: Box::new([]),
        },
        outcome: OutcomeSpec {
            change: None,
            verdicts: Box::new([]),
            report: Some(smith_domain_run::outcome::TextSpec { min: 0, max: 1, fields: Box::new([]) }),
            failure: None,
        },
        budget: BUDGET,
        llm: Llm {
            prices: smith_domain_run::Prices { input: 1, cached: 0, output: 0, unit: 1 },
            account: 0,
            endpoint: Endpoint(0),
            model: bytes(1),
            max_tokens: 1,
            dialect: 1,
        },
        models: Box::new([]),
        conventions: Some(smith_domain_run::Conventions {
            guide: b"AGENTS.md".as_slice().into(),
            checks: b".temper/pre-pr".as_slice().into(),
        }),
        resume: false,
        waiting: skein_lib::Duration::from_secs(30),
    }
}

fn host_memory_take(
    domain: &mut Domain,
    env: &Env<Limits>,
    out: &mut Queue<Request>,
    meter: &Meter,
    event: Option<Event>,
) -> (Option<Token>, Option<smith_domain_run::RelayName>) {
    meter.start();
    match event {
        Some(event) => smith_domain_run::step(domain, env, event, out),
        None => smith_domain_run::fire(domain, env, out),
    }
    let measured = meter.end();
    let mut token = None;
    let mut relay = None;
    while let Some(request) = out.pop() {
        match request {
            Request::HostCall { relay: name, input, tool, .. } => {
                assert_eq!(input.bytes().len(), usize::try_from(env.limits.host_input_bytes).expect("cap fits"));
                assert_eq!(tool.len(), 1);
                relay = Some(name);
            }
            Request::Return { result: smith_domain_run::Returned::HostAnswered(answer), .. } => {
                assert_eq!(answer.text().len(), usize::try_from(env.limits.host_reply_bytes).expect("cap fits"));
            }
            Request::Read { owner, .. } => token = Some(owner),
            Request::Open { conversation, opening } => {
                assert_eq!(opening.host_tools.len(), 1);
                token = Some(conversation);
            }
            Request::Turn { .. }
            | Request::Waiting { .. }
            | Request::MessageBounced { .. }
            | Request::Admitted { .. }
            | Request::WithdrawHost { .. }
            | Request::Close { .. }
            | Request::Answer { .. } => {}
            unexpected @ (Request::Return { .. }
            | Request::Say { .. }
            | Request::Probe { .. }
            | Request::Abort { .. }
            | Request::Check { .. }
            | Request::Checking { .. }
            | Request::Deliver { .. }) => panic!("unexpected host memory output {unexpected:?}"),
        }
    }
    meter.check(measured, worst_case(&env.limits).expect("bounded limits"), &"full immutable host relay path");
    (token, relay)
}

#[test]
fn complete_declaration_and_maximum_opaque_input_answer_retries_reach_the_measured_ownership_bound() {
    let limits = Limits { run_bytes: 4096, ..LIMITS };
    let mut env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(MAX_OUT);
    let meter = Meter::new();
    let mut domain = Domain::new(&limits);
    let mut charter = charter(limits.run_bytes);
    let mut mounted = Some(workspace());
    mounted.as_mut().expect("explicit host fixture workspace").directories[0].writable = false;
    // Read-only host-tool work has a feasible verdict contract. Move the exact
    // retired Change field cells/names and brief into the schema, preserving the
    // aggregate maximum and this fixture's original read-only workspace control.
    let retired = charter.outcome.change.take().expect("default Change fixture fields");
    let mut retired_bytes = 0_u64;
    for field in &retired.fields {
        retired_bytes += size(size_of::<FieldRule>()) + size(field.name.len());
    }
    drop(retired);
    let retired_context = context_bytes(&charter);
    let tool = &mut charter.grants.host_tools[0];
    tool.schema = bytes(size(tool.schema.len()) + retired_context + retired_bytes);
    charter.instructions = Box::new([]);
    charter.brief = smith_domain_run::Brief { sections: Box::new([]) };
    let (run, _) = host_memory_take(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Some(Event::Start {
            workspace: mounted,
            reply_to: ReplyTo::new(Token::new(88)),
            worker: Token::new(91),
            activation: 1,
            charter,
            transcript: None,
        }),
    );
    let run = run.expect("real admitted run reads");
    let (conversation, _) = host_memory_take(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Some(Event::Read { owner: run, read: smith_domain_run::Read::Missing }),
    );
    let conversation = conversation.expect("main opens with complete declaration");
    host_memory_take(&mut domain, &env, &mut out, &meter, Some(Event::Started { conversation, peer: Token::new(99) }));
    let mut input = vec![b' '; usize::try_from(limits.host_input_bytes).expect("cap fits")];
    input[0] = b'{';
    let last = input.len() - 1;
    input[last] = b'}';
    let input = smith_domain_run::HostInput::attested(input.into()).expect("maximum protocol-attested object");
    let (_, relay) = host_memory_take(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Some(Event::Delegated {
            conversation,
            call: Token::new(101),
            name: smith_domain_run::CallName { activation: 1, completion: 1, position: 0 },
            ask: smith_domain_run::Ask::Host { tool: bytes(1), effect: smith_domain_run::HostEffect::Read, input },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        }),
    );
    let mut relay = relay.expect("maximum body relayed");
    for reply in
        [smith_domain_run::HostReply::Unanswered(smith_domain_run::Unanswered::Lost), smith_domain_run::HostReply::Busy]
    {
        host_memory_take(&mut domain, &env, &mut out, &meter, Some(Event::HostReturned { relay, reply }));
        env.now = env.now.saturating_add(limits.host_backoff);
        let (_, recovered) = host_memory_take(&mut domain, &env, &mut out, &meter, None);
        relay = recovered.expect("retained maximum body is copied into retry output");
    }
    let answer =
        smith_domain_run::HostAnswer::new(bytes(u64::from(limits.host_reply_bytes)), false).expect("maximum answer");
    host_memory_take(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Some(Event::HostReturned { relay, reply: smith_domain_run::HostReply::Answered(answer) }),
    );
    host_memory_take(&mut domain, &env, &mut out, &meter, Some(Event::Cancel { run }));
    host_memory_take(
        &mut domain,
        &env,
        &mut out,
        &meter,
        Some(Event::Ended { conversation, end: smith_domain_run::End::Closed, spend: smith_domain_run::Spend::ZERO }),
    );
    domain.reclaim();
    assert_eq!((domain.runs(), domain.conversations(), domain.calls()), (0, 0, 0));
}

// The parent owns the decision after a crash. The next activation gets a text
// message because the answered call's completion never became a Turn.
#[test]
#[expect(clippy::too_many_lines, reason = "one crash story keeps the two activations and their host evidence together")]
fn answered_host_call_before_turn_crash_wakes_as_text_with_a_new_call_namespace() {
    fn take(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>, event: Event) -> Vec<Request> {
        smith_domain_run::step(domain, env, event, out);
        let mut requests = Vec::new();
        while let Some(request) = out.pop() {
            requests.push(request);
        }
        requests
    }

    fn open(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>, activation: u64) -> (Token, Token) {
        let worker = Token::new(91);
        let mut selected = charter(env.limits.run_bytes);
        selected.resume = true;
        let started = take(
            domain,
            env,
            out,
            Event::Start {
                reply_to: ReplyTo::new(Token::new(88)),
                worker,
                activation,
                charter: selected,
                workspace: Some(workspace()),
                transcript: None,
            },
        );
        let Some(Request::Read { owner: run, .. }) =
            started.iter().find(|request| matches!(request, Request::Read { .. }))
        else {
            panic!("admitted run prepares its main")
        };
        let run = *run;
        let probe = take(domain, env, out, Event::Read { owner: run, read: Read::Missing });
        let [Request::Probe { owner, .. }] = probe.as_slice() else { panic!("guide lookup leads to check probe") };
        let prepared = take(domain, env, out, Event::Probed { owner: *owner, executable: false });
        let Some(Request::Open { conversation, opening }) =
            prepared.iter().find(|request| matches!(request, Request::Open { .. }))
        else {
            panic!("main opens without fabricated history")
        };
        assert_eq!(opening.activation, activation);
        assert!(opening.transcript.is_none());
        let conversation = *conversation;
        assert!(take(domain, env, out, Event::Started { conversation, peer: Token::new(99) }).is_empty());
        (run, conversation)
    }

    let limits = Limits { run_bytes: 4096, ..LIMITS };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(MAX_OUT);
    let mut first = Domain::new(&limits);
    let (_, conversation) = open(&mut first, &env, &mut out, 1);
    let old_name = smith_domain_run::CallName { activation: 1, completion: 1, position: 0 };
    let called = take(
        &mut first,
        &env,
        &mut out,
        Event::Delegated {
            conversation,
            call: Token::new(101),
            name: old_name,
            ask: Ask::Host {
                tool: b"x".as_slice().into(),
                effect: smith_domain_run::HostEffect::Read,
                input: smith_domain_run::HostInput::attested(b"{}".as_slice().into()).expect("object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    let [Request::HostCall { relay, name, .. }] = called.as_slice() else { panic!("one host operation") };
    assert_eq!(*name, old_name);
    let answer = smith_domain_run::HostAnswer::new(b"host accepted change".as_slice().into(), false)
        .expect("bounded host answer");
    let returned = take(
        &mut first,
        &env,
        &mut out,
        Event::HostReturned { relay: *relay, reply: smith_domain_run::HostReply::Answered(answer) },
    );
    assert!(matches!(
        returned.as_slice(),
        [Request::Return { result: smith_domain_run::Returned::HostAnswered(_), .. }]
    ));
    assert!(!returned.iter().any(|request| matches!(request, Request::Turn { .. })));
    drop(first); // Crash before the answered completion reaches a Turn.

    let mut resumed = Domain::new(&limits);
    let (run, conversation) = open(&mut resumed, &env, &mut out, 2);
    let notice = b"host: the previous host call was answered: host accepted change";
    assert!(take(
        &mut resumed,
        &env,
        &mut out,
        Event::Message { run, name: Token::new(5), text: notice.as_slice().into() },
    )
    .is_empty());
    let waking = take(
        &mut resumed,
        &env,
        &mut out,
        Event::Yielded { conversation, stop: Stop::EndTurn, text: b"ready".as_slice().into() },
    );
    assert!(
        matches!(waking.as_slice(), [Request::Say { peer, text }] if *peer == Token::new(99) && text.as_ref() == notice)
    );
    assert!(!waking.iter().any(|request| matches!(request, Request::Return { .. } | Request::Turn { .. })));

    let stale = take(
        &mut resumed,
        &env,
        &mut out,
        Event::Delegated {
            conversation,
            call: Token::new(103),
            name: old_name,
            ask: Ask::Host {
                tool: b"x".as_slice().into(),
                effect: smith_domain_run::HostEffect::Read,
                input: smith_domain_run::HostInput::attested(b"{}".as_slice().into()).expect("object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    assert!(matches!(
        stale.as_slice(),
        [Request::Return {
            result: smith_domain_run::Returned::Refused { refusal: smith_domain_run::AskRefusal::Name },
            ..
        }]
    ));

    let new_name = smith_domain_run::CallName { activation: 2, completion: 1, position: 0 };
    assert_ne!(old_name, new_name);
    let fresh = take(
        &mut resumed,
        &env,
        &mut out,
        Event::Delegated {
            conversation,
            call: Token::new(102),
            name: new_name,
            ask: Ask::Host {
                tool: b"x".as_slice().into(),
                effect: smith_domain_run::HostEffect::Read,
                input: smith_domain_run::HostInput::attested(b"{}".as_slice().into()).expect("object"),
            },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(30)),
        },
    );
    assert!(matches!(fresh.as_slice(), [Request::HostCall { name, .. }] if *name == new_name));
}

#[test]
fn maximum_selected_read_probe_and_check_paths_attain_the_full_run_ownership_bound() {
    let capacity = smith_domain_run::Conventions::PATH_CAPACITY;
    let selected = smith_domain_run::Conventions {
        guide: vec![b'g'; capacity].into_boxed_slice(),
        checks: vec![b'c'; capacity].into_boxed_slice(),
    };
    fill_selected(Limits { run_bytes: 16_384, ..LIMITS }, Some(&selected));
}
