//! Scalar activation bills through root/run/session/tools and actual Clients.
//! The outside oracle prices the actual peer/SDK callbacks with its own literal
//! model table, and sums each callback once. Child-inclusive Turn bills and
//! historical bills never supply that global expected total.
//! Contract: domain/run.md, sections 9, 10, 13 and 14;
//! domain/session.md, section 6; testing-strategy.md, sections 2.3, 6 and 7.

use skein_fake_checkout::Checkout;
use skein_fake_llm_domain::api::{Finish, Line, Message, Part, Query, Role, Script, Turn};
use skein_lib::{Duration, Token};
use skein_world::domain::Span;
use smith_domain::{Transcript, run, session::llm, tools};
use smith_protocol_llm_world::adapter::{self as adapter, Limits};
use smith_protocol_llm_world::{
    Boundary, CompletionObservation, CompletionTerminal, Job, Settings, World,
    wire::{self, Configuration, Observed},
};

const BEGIN: &[u8] = b"Begin the work your brief describes.";
const ID: &[u8] = b"call_0000000000000001";
const CHILD: &[u8] = b"BUDGET-CHILD: actual child result.";
const CHILD_INPUT: &[u8] = br#"{"brief":"@budget-child own task","tools":["inspect"],"agents":true,"llm":"fake-2"}"#;
const GRAND_INPUT: &[u8] = br#"{"brief":"@budget-grand own task","tools":["inspect"],"llm":"fake-3"}"#;
const REPORT: &[u8] = br#"{"report":"Budget work finished.","source":"actual-checkout"}"#;
const READ: &[u8] = br#"{"path":"data.txt"}"#;
const EDIT: &[u8] = br#"{"path":"data.txt","old":"before","new":"after"}"#;
const FIRST: &[u8] = b"Positive-price first activation parked.";
const RESUMED: &[u8] = b"Positive-price actual resumed activation.";
const LAST: &[u8] = b"Positive-price second activation parked.";
const PARK_CHILD_INPUT: &[u8] = br#"{"brief":"@budget-park-child separate task","tools":["inspect"],"llm":"fake-2"}"#;

#[derive(Clone, Copy)]
struct Rate {
    model: &'static [u8],
    input: u64,
    cached: u64,
    output: u64,
    unit: u32,
}

const MODEL_RATES: [Rate; 3] = [
    Rate { model: b"fake-1", input: 2, cached: 3, output: 5, unit: 7 },
    Rate { model: b"fake-2", input: 7, cached: 11, output: 13, unit: 10 },
    Rate { model: b"fake-3", input: 17, cached: 19, output: 23, unit: 29 },
];
const CROSSING_RATES: [Rate; 1] = [Rate { model: b"fake-1", input: 0, cached: 0, output: 1, unit: 1 }];
const PARALLEL_RATES: [Rate; 3] = [
    CROSSING_RATES[0],
    Rate { model: b"fake-2", input: 0, cached: 0, output: 100, unit: 1 },
    Rate { model: b"fake-3", input: 0, cached: 0, output: 1, unit: 1 },
];
const RESUMED_RATES: [Rate; 2] =
    [Rate { model: b"fake-1", input: 31, cached: 37, output: 41, unit: 13 }, MODEL_RATES[1]];

fn charge(usage: skein_llm::Usage, rate: Rate) -> u64 {
    // One combined numerator is rounded once. This oracle uses no domain
    // price method, accumulated spend, child bill or saved Turn value.
    let numerator = (u128::from(usage.input.unwrap_or(0)) + u128::from(usage.cache_write.unwrap_or(0)))
        * u128::from(rate.input)
        + u128::from(usage.cache_read.unwrap_or(0)) * u128::from(rate.cached)
        + u128::from(usage.output.unwrap_or(0)) * u128::from(rate.output);
    let denominator = u128::from(rate.unit);
    u64::try_from(numerator.div_ceil(denominator)).expect("finite story prices fit u64")
}

fn usage(observation: &CompletionObservation) -> skein_llm::Usage {
    let (at, terminal) = observation.terminal.expect("every actual completion settled");
    assert!(at >= observation.started);
    match terminal {
        CompletionTerminal::Completed(usage) => usage,
        CompletionTerminal::Failed | CompletionTerminal::Cancelled => panic!("this story requires an actual winner"),
    }
}

fn total(observations: &[CompletionObservation], rates: &[Rate]) -> run::Spend {
    let mut expected = run::Spend::ZERO;
    for observation in observations {
        let actual = usage(observation);
        let rate = rates.iter().find(|rate| rate.model == observation.model.as_ref()).expect("outside model table");
        expected.turns += 1;
        expected.input += actual.input.unwrap_or(0);
        expected.output += actual.output.unwrap_or(0);
        expected.cache_read += actual.cache_read.unwrap_or(0);
        expected.cache_write += actual.cache_write.unwrap_or(0);
        expected.units += charge(actual, *rate);
    }
    expected
}

fn spent(answer: &run::Answer) -> run::Spend {
    match answer {
        run::Answer::Accepted { spent, .. } | run::Answer::Parked { spent, .. } | run::Answer::Failed { spent, .. } => {
            *spent
        }
        run::Answer::Refused(reason) => panic!("actual story was refused: {reason:?}"),
    }
}

fn conservation(world: &World, rates: &[Rate]) -> run::Spend {
    let expected = total(world.completions(), rates);
    assert_eq!(spent(world.answer()), expected, "one own-completion charge per outside callback");
    assert_eq!(world.prompts().len(), world.completions().len());
    assert_eq!(world.turn_metadata().last().expect("actual main Turn").2, expected);
    assert!(expected.input > 0 && expected.output > 0);
    assert!(world.checked().is_empty() && world.pushes().is_empty() && world.host_submissions().is_empty());
    assert!(world.judged().0 > 0 && world.judged().1 == 1);
    expected
}

fn settings(seed: u64, writable: bool, resume: bool) -> Settings {
    let mut settings = Settings {
        job: Job::Reporting,
        writable,
        resume,
        waiting: Duration::from_secs(1),
        network: Span::millis(1, 1),
        ..Settings::calm(seed)
    };
    settings.budget.spend = 1 << 20;
    settings.limits.run.budget.spend = 1 << 20;
    settings.limits.session.spend = 1 << 20;
    settings
}

fn charter(settings: &Settings, cue: &[u8], rates: &[Rate]) -> run::Charter {
    use run::charter::{Brief, Endpoint, Grants, Llm, Section, Tools};
    use run::outcome::{FieldRule, OutcomeSpec, TextSpec};

    let model = |rate: Rate| Llm {
        prices: run::Prices { input: rate.input, cached: rate.cached, output: rate.output, unit: rate.unit },
        account: 0,
        endpoint: Endpoint(0),
        model: rate.model.into(),
        window: 8192,
        output: 4096,
        dialect: 1,
    };
    run::Charter {
        instructions: cue.into(),
        brief: Brief {
            sections: Box::new([Section {
                title: b"Budget task".as_slice().into(),
                text: b"Use the actual workspace.".as_slice().into(),
            }]),
        },
        grants: Grants {
            wait: true,
            deliver: None,
            tools: Tools { inspect: true, modify: settings.writable, shell: false },
            agents: true,
            host_tools: Box::new([]),
        },
        outcome: OutcomeSpec {
            change: None,
            verdicts: Box::new([]),
            report: Some(TextSpec {
                max: 1024,
                fields: Box::new([FieldRule { name: b"source".as_slice().into(), max: 128 }]),
            }),
            failure: None,
        },
        budget: settings.budget,
        llm: model(rates[0]),
        models: rates[1..].iter().copied().map(model).collect(),
        conventions: Some(run::Conventions {
            guide: b"AGENTS.md".as_slice().into(),
            checks: b".temper/pre-pr".as_slice().into(),
        }),
        resume: settings.resume,
        waiting: settings.waiting,
    }
}

fn workspace(writable: bool) -> (Checkout, run::Workspace) {
    let mut disk = Checkout::new();
    disk.mkdir(b"work");
    disk.write(b"work/AGENTS.md", b"BUDGET-GUIDE: preserve exact task results.\n");
    disk.write(b"work/.git/HEAD", b"HOST-GIT-METADATA");
    disk.write(b"work/data.txt", b"before\n");
    let root = Token::new(disk.root(b"work"));
    (
        disk,
        run::Workspace {
            directories: Box::new([run::Directory {
                name: b"work".as_slice().into(),
                root,
                writable,
                git: true,
                conflicts: Box::new([]),
            }]),
        },
    )
}

fn call(name: &[u8], arguments: &[u8]) -> Line {
    Line::Call { name: name.into(), arguments: arguments.into() }
}

fn calls(lines: Vec<Line>, tokens: u64) -> Turn {
    Turn { lines: lines.into(), finish: Finish::ToolCalls, tokens }
}

fn says(text: &[u8], tokens: u64) -> Turn {
    Turn { lines: Box::new([Line::Text { text: text.into() }]), finish: Finish::Stop, tokens }
}

fn script(cue: &[u8], turns: Vec<Turn>) -> Script {
    Script { cue: cue.into(), turns: turns.into() }
}

fn typed_world(settings: &Settings, cue: &[u8], rates: &[Rate], scripts: Box<[Script]>) -> World {
    let (disk, workspace) = workspace(settings.writable);
    World::with_workspace_scripts_charter(
        *settings,
        None,
        Some(workspace),
        disk,
        scripts,
        charter(settings, cue, rates),
    )
}

fn bounds(settings: &mut Settings) -> Limits {
    let mut bounds = Limits {
        client: skein_llm_world::limits(),
        tool_bytes: 32768,
        rendered_result: skein_llm_world::limits().dialect.string_bytes,
        shell_default: skein_lib::Duration::from_secs(120),
        shell_maximum: skein_lib::Duration::from_secs(1200),
    };
    bounds.client.http.request = 16384;
    bounds.client.dialect.request_bytes = 16384;
    settings.limits.session.completion_bytes =
        adapter::completion_worst_case(&bounds.client, settings.limits.decoded_call_bytes)
            .expect("actual translated reservation");
    settings.limits.session.completion_blocks = bounds.client.dialect.parts;
    bounds
}

fn configuration(index: usize) -> Configuration {
    wire::configurations().into_iter().nth(index).expect("two independently configured native forms")
}

fn native_world(
    mut settings: Settings,
    cue: &[u8],
    rates: &[Rate],
    scripts: Box<[Script]>,
    transcript: Option<Transcript>,
    index: usize,
) -> World {
    let bounds = bounds(&mut settings);
    let (disk, workspace) = workspace(settings.writable);
    World::with_workspace_wire_charter(
        settings,
        transcript,
        Some(workspace),
        disk,
        (configuration(index), bounds),
        scripts,
        charter(&settings, cue, rates),
    )
}

fn native_settled(world: &World) {
    assert_eq!(world.wire_bindings().len(), world.completions().len());
    for (binding, actual) in world.wire_bindings().iter().zip(world.completions()) {
        assert_eq!(binding.owner, actual.owner);
        assert_eq!(binding.started.0, actual.started);
        assert_eq!(binding.accepted_usage, Some(usage(actual)), "SDK counters before adapter translation");
        assert_eq!(
            binding.observed.iter().map(|(_, _, event)| *event).collect::<Vec<_>>(),
            [Observed::Completed(binding.owner), Observed::Reusable, Observed::Close, Observed::Closed],
        );
        assert!(binding.retired.is_some());
        let completed = binding.observed[0].0;
        assert_eq!(actual.terminal.expect("outside actual terminal").0, completed);
        assert!(binding.observed.iter().all(|(at, _, _)| *at >= actual.started));
    }
}

fn query_result(query: &Query, name: &[u8], input: &[u8], output: &[u8]) -> bool {
    query.messages.windows(2).any(|pair| {
        pair[0].role == Role::Assistant
            && pair[1].role == Role::User
            && pair[0].parts.iter().any(|part| {
                let Part::ToolCall { id, name: actual, arguments } = part else { return false };
                actual.as_ref() == name
                    && arguments.as_ref() == input
                    && pair[1].parts.iter().any(|part| {
                        matches!(part, Part::ToolOutput { id: returned, output: text, is_error: false }
                    if id == returned && text.as_ref() == output)
                    })
            })
    })
}

fn model_calls<'a>(world: &'a World, model: &[u8]) -> Vec<&'a CompletionObservation> {
    world.completions().iter().filter(|observation| observation.model.as_ref() == model).collect()
}

fn own_units(calls: &[&CompletionObservation], rate: Rate) -> u64 {
    calls.iter().map(|actual| charge(usage(actual), rate)).sum()
}

fn turn_usage(actual: skein_llm::Usage) -> llm::Usage {
    llm::Usage {
        input_tokens: actual.input,
        output_tokens: actual.output,
        cache_read_tokens: actual.cache_read,
        cache_write_tokens: actual.cache_write,
        reasoning_tokens: actual.reasoning,
    }
}

#[test]
fn two_priced_models_and_a_refused_child_delegate_conserve_global_own_charges() {
    let scripts = Box::new([
        script(
            b"@budget-main",
            vec![
                calls(vec![call(b"sub_agent", CHILD_INPUT)], 3),
                calls(vec![call(b"read", READ)], 5),
                calls(vec![call(b"finish", REPORT)], 7),
            ],
        ),
        script(
            b"@budget-child",
            vec![
                calls(vec![call(b"sub_agent", GRAND_INPUT)], 11),
                calls(vec![call(b"read", READ)], 13),
                says(CHILD, 17),
            ],
        ),
    ]);
    let settings = settings(701, false, false);
    let mut world = typed_world(&settings, b"@budget-main MAIN-INSTRUCTIONS", &MODEL_RATES, scripts);
    world.run(20_000);
    let expected = conservation(&world, &MODEL_RATES);
    assert_eq!(expected.turns, 6);
    assert_eq!(expected.output, 56);
    assert!(expected.cache_read > 0 && expected.cache_write > 0);
    assert_eq!(world.turns().len(), 3, "only main's settled Turns cross the host boundary");
    let main = model_calls(&world, b"fake-1");
    let child = model_calls(&world, b"fake-2");
    assert!(model_calls(&world, b"fake-3").is_empty());
    assert_eq!((main.len(), child.len()), (3, 3));
    let child_bill = own_units(&child, MODEL_RATES[1]);
    let mut inclusive = child_bill;
    for (index, actual) in main.iter().enumerate() {
        inclusive += charge(usage(actual), MODEL_RATES[0]);
        assert_eq!(world.turns()[index].spent, inclusive, "main includes the child's whole subtree bill once");
        assert_eq!(world.turns()[index].usage, turn_usage(usage(actual)));
    }
    assert_eq!(inclusive, expected.units);
    let main_queries = world.prompts().iter().filter(|query| query.model.as_ref() == b"fake-1").collect::<Vec<_>>();
    let child_queries = world.prompts().iter().filter(|query| query.model.as_ref() == b"fake-2").collect::<Vec<_>>();
    assert!(query_result(main_queries[1], b"sub_agent", CHILD_INPUT, CHILD));
    assert!(
        child_queries[1]
            .messages
            .iter()
            .flat_map(|message| &message.parts)
            .any(|part| { matches!(part, Part::ToolOutput { is_error: true, .. }) })
    );
    assert!(child_queries.iter().all(|query| query.system.starts_with(b"@budget-child own task\n\n")));
    assert!(
        matches!(world.answer(), run::Answer::Accepted { outcome: run::outcome::Declared::Report(report), turns: 3, .. }
        if report.text.as_ref() == b"Budget work finished.")
    );
    let mut duplicated = world.completions().to_vec();
    duplicated.push(duplicated[0].clone());
    assert_ne!(
        total(&duplicated, &MODEL_RATES),
        expected,
        "the whole positive conservation oracle detects double charging"
    );
}

#[test]
fn an_unaffordable_completion_never_edits_or_finishes() {
    for finish in [false, true] {
        let mut settings = settings(702, true, false);
        settings.budget.spend = 4;
        let mut lines = vec![call(b"edit", EDIT)];
        if finish {
            lines.push(call(b"finish", REPORT));
        }
        let scripts =
            Box::new([script(b"@budget-crossing", vec![calls(vec![call(b"read", READ)], 3), calls(lines, 7)])]);
        let mut world = typed_world(&settings, b"@budget-crossing", &CROSSING_RATES, scripts);
        world.run(20_000);
        assert!(world.completions().is_empty(), "the maximum cannot fit even the first completion");
        assert_eq!(spent(world.answer()), run::Spend::ZERO);
        assert!(matches!(
            world.answer(),
            run::Answer::Failed { failure: run::Failure::Budget(run::Exhausted::Spend), turns: 0, .. }
        ));
        assert_eq!(world.disk().content(b"work/data.txt"), Some(b"before\n".as_slice()));
        assert!(
            !world
                .boundaries()
                .iter()
                .any(|(_, boundary)| matches!(boundary, Boundary::Io { op: tools::Op::Store { .. } }))
        );
    }
}

#[test]
fn an_affordable_completion_edits_and_finishes_within_the_budget() {
    let settings = settings(702, true, false);
    let scripts = Box::new([script(
        b"@budget-crossing",
        vec![calls(vec![call(b"read", READ)], 3), calls(vec![call(b"edit", EDIT), call(b"finish", REPORT)], 7)],
    )]);
    let mut world = typed_world(&settings, b"@budget-crossing", &CROSSING_RATES, scripts);
    world.run(20_000);
    let expected = conservation(&world, &CROSSING_RATES);
    assert_eq!((expected.turns, expected.units, expected.output), (2, 10, 10));
    assert!(expected.units <= settings.budget.spend);
    assert_eq!(world.disk().content(b"work/data.txt"), Some(b"after\n".as_slice()));
    assert_eq!(world.turns().len(), 2);
    assert!(matches!(
        world.answer(),
        run::Answer::Accepted { outcome: run::outcome::Declared::Report(_), turns: 2, .. }
    ));
}

const COSTLY_INPUT: &[u8] = br#"{"brief":"@budget-costly own task","tools":["inspect"],"llm":"fake-2"}"#;
const CHEAP_INPUT: &[u8] = br#"{"brief":"@budget-cheap own task!","tools":["inspect"],"llm":"fake-3"}"#;
const COSTLY_RESPONSE: &[u8] = b"costly actual winner";
const CHEAP_RESPONSE: &[u8] = b"cheap actual winner!";

fn parallel_scripts() -> Box<[Script]> {
    Box::new([
        script(
            b"@budget-parallel",
            vec![calls(vec![call(b"sub_agent", COSTLY_INPUT), call(b"sub_agent", CHEAP_INPUT)], 1)],
        ),
        script(b"@budget-costly", vec![says(COSTLY_RESPONSE, 2)]),
        script(b"@budget-cheap", vec![says(CHEAP_RESPONSE, 3)]),
    ])
}

fn parallel_story(index: usize) {
    let settings = settings(703, false, false);
    let mut world = native_world(settings, b"@budget-parallel", &PARALLEL_RATES, parallel_scripts(), None, index);
    world.run(100_000);
    native_settled(&world);
    let expected = conservation(&world, &PARALLEL_RATES);
    assert!(expected.units <= settings.budget.spend);
    assert!(!model_calls(&world, b"fake-1").is_empty());
    assert_eq!(model_calls(&world, b"fake-2").len(), 1);
    assert!(!model_calls(&world, b"fake-3").is_empty());
    assert_eq!(world.wire_bindings().len(), world.completions().len());
}

#[test]
fn both_native_forms_account_parallel_children_under_one_budget() {
    for index in 0..2 {
        parallel_story(index);
    }
}

fn parking_scripts() -> Box<[Script]> {
    Box::new([
        script(
            b"@budget-parking-main",
            vec![
                calls(vec![call(b"sub_agent", PARK_CHILD_INPUT)], 3),
                calls(vec![call(b"wait", b"{}")], 5),
                says(FIRST, 7),
                calls(vec![Line::Text { text: RESUMED.into() }, call(b"wait", b"{}")], 11),
                says(LAST, 13),
            ],
        ),
        script(b"@budget-park-child", vec![says(CHILD, 17)]),
    ])
}

fn message(role: Role, parts: Vec<Part>) -> Message {
    Message { role, parts: parts.into() }
}

fn text_part(text: &[u8]) -> Part {
    Part::Text { text: text.into() }
}

fn wait_call() -> Part {
    Part::ToolCall { id: ID.into(), name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() }
}

fn wait_feedback() -> Part {
    Part::ToolOutput { id: ID.into(), output: b"waiting".as_slice().into(), is_error: false }
}

fn parked_prefix() -> Vec<Message> {
    vec![
        message(Role::User, vec![text_part(BEGIN)]),
        message(
            Role::Assistant,
            vec![Part::ToolCall {
                id: ID.into(),
                name: b"sub_agent".as_slice().into(),
                arguments: PARK_CHILD_INPUT.into(),
            }],
        ),
        message(Role::User, vec![Part::ToolOutput { id: ID.into(), output: CHILD.into(), is_error: false }]),
        message(Role::Assistant, vec![wait_call()]),
        message(Role::User, vec![wait_feedback()]),
        message(Role::Assistant, vec![text_part(FIRST)]),
        message(Role::User, vec![text_part(BEGIN)]),
    ]
}

fn exact_history(query: &Query, expected: &[Message]) -> bool {
    query.model.as_ref() == b"fake-1" && query.messages.as_ref() == expected
}

fn positive_history_controls(query: &Query, expected: &[Message]) {
    assert!(exact_history(query, expected), "whole saved history and fresh activation wake");
    let mut changed = query.clone();
    changed.messages[2].parts = Box::new([]);
    assert!(!exact_history(&changed, expected), "dropping the actual child's result breaks the positive oracle");
    let mut changed = query.clone();
    changed.messages[4].parts = vec![text_part(b"rewritten waiting")].into();
    assert!(!exact_history(&changed, expected));
    let mut changed = query.clone();
    changed.messages[5].role = Role::User;
    assert!(!exact_history(&changed, expected));
}

fn saved(world: &World) -> Transcript {
    Transcript {
        version: smith_domain::session::record::VERSION,
        endpoint: llm::Endpoint(0),
        dialect: 1,
        turns: world.turns().to_vec().into(),
    }
}

fn parked_accounting(world: &World, rates: &[Rate], prior_sequence: u32) -> run::Spend {
    native_settled(world);
    let expected = conservation(world, rates);
    assert!(
        matches!(world.answer(), run::Answer::Parked { turns, .. } if *turns == u32::try_from(world.turns().len()).expect("finite main turns"))
    );
    assert_eq!(world.waiting().len(), 1);
    assert_eq!(world.waiting()[0].1, None);
    assert!(world.answered_at() >= world.waiting()[0].0.saturating_add(Duration::from_secs(1)));
    for (index, turn) in world.turns().iter().enumerate() {
        assert_eq!(turn.sequence, prior_sequence + u32::try_from(index).expect("finite main turns") + 1);
    }
    expected
}

fn parking_story(index: usize) {
    let mut first = native_world(
        settings(704, false, true),
        b"@budget-parking-main FIRST-INSTRUCTIONS",
        &MODEL_RATES[..2],
        parking_scripts(),
        None,
        index,
    );
    first.run(100_000);
    let first_total = parked_accounting(&first, &MODEL_RATES[..2], 0);
    assert_eq!((first_total.turns, first_total.output), (4, 32));
    assert_eq!(first.turns().len(), 3);
    let first_main = model_calls(&first, b"fake-1");
    let first_child = model_calls(&first, b"fake-2");
    assert_eq!((first_main.len(), first_child.len()), (3, 1));
    let child_bill = own_units(&first_child, MODEL_RATES[1]);
    assert!(child_bill > 0);
    let mut inclusive = child_bill;
    for (turn, actual) in first.turns().iter().zip(first_main) {
        inclusive += charge(usage(actual), MODEL_RATES[0]);
        assert_eq!(turn.spent, inclusive);
        assert_eq!(turn.usage, turn_usage(usage(actual)));
    }
    assert_eq!(inclusive, first_total.units);
    let history = saved(&first);
    let immutable = history.clone();
    let mut resumed = native_world(
        settings(705, false, true),
        b"@budget-parking-main NEW-INSTRUCTIONS",
        &RESUMED_RATES,
        parking_scripts(),
        Some(history),
        index,
    );
    resumed.run(100_000);
    let expected_prefix = parked_prefix();
    positive_history_controls(&resumed.prompts()[0], &expected_prefix);
    let mut last_prefix = expected_prefix.clone();
    last_prefix.push(message(Role::Assistant, vec![text_part(RESUMED), wait_call()]));
    last_prefix.push(message(Role::User, vec![wait_feedback()]));
    assert!(
        exact_history(&resumed.prompts()[1], &last_prefix),
        "actual continued query keeps all old and new call/result pairs"
    );
    assert!(
        resumed.prompts().iter().all(|query| query.system.starts_with(b"@budget-parking-main NEW-INSTRUCTIONS\n\n"))
    );
    let new_total = parked_accounting(&resumed, &RESUMED_RATES, 3);
    assert_eq!((new_total.turns, new_total.output), (2, 24));
    assert_eq!(resumed.turns().len(), 2);
    assert!(
        model_calls(&resumed, b"fake-2").is_empty(),
        "the historical child is replay data, never a new actual call"
    );
    let mut current = 0;
    for (turn, actual) in resumed.turns().iter().zip(resumed.completions()) {
        current += charge(usage(actual), RESUMED_RATES[0]);
        assert_eq!(turn.spent, current, "only actual new calls at the new prices seed current activation units");
        assert_eq!(turn.usage, turn_usage(usage(actual)));
    }
    assert_eq!(current, new_total.units);
    assert_eq!(saved(&first), immutable, "old Turn.spent and concrete history remain unchanged");
    assert!(immutable.turns.iter().all(|turn| turn.spent > 0));
    assert!(first_total.cache_read > 0 && new_total.cache_read > 0);
    if index == 0 {
        assert_eq!((first_total.cache_write, new_total.cache_write), (0, 0));
    } else {
        assert!(first_total.cache_write > 0 && new_total.cache_write > 0);
    }
}

#[test]
fn both_native_forms_restore_genuine_positive_priced_child_and_wait_history_with_new_activation_rates() {
    for index in 0..2 {
        parking_story(index);
    }
}
