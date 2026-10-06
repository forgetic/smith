//! Scalar activation bills through real root/run/session/tools and actual Clients.
//! The outside oracle prices the actual peer/SDK callbacks with its own literal
//! model table, and sums each callback once. Child-inclusive Turn bills and
//! historical bills never supply that global expected total.
//! Contract: domain/run.md, sections 9, 10, 13 and 14;
//! domain/session.md, section 6; testing-strategy.md, sections 2.3, 6 and 7.

use skein_fake_checkout::Checkout;
use skein_fake_llm_domain::api::{Finish, Line, Part, Query, Role, Script, Turn};
use skein_lib::{Duration, Token};
use skein_world::domain::Span;
use smith_agent_world::{CompletionObservation, CompletionTerminal, Job, Settings, World};
use smith_domain::{run, session::llm};

const CHILD: &[u8] = b"BUDGET-CHILD: actual nested result.";
const GRANDCHILD: &[u8] = b"BUDGET-GRANDCHILD: actual leaf result.";
const CHILD_INPUT: &[u8] = br#"{"brief":"@budget-child own task","tools":["inspect"],"agents":true,"llm":"fake-2"}"#;
const GRAND_INPUT: &[u8] = br#"{"brief":"@budget-grand own task","tools":["inspect"],"llm":"fake-3"}"#;
const REPORT: &[u8] = br#"{"report":"Budget work finished.","source":"actual-checkout"}"#;
const READ: &[u8] = br#"{"path":"data.txt"}"#;

#[derive(Clone, Copy)]
struct Rate {
    model: &'static [u8],
    input: u64,
    cached: u64,
    output: u64,
    unit: u32,
}

const NESTED_RATES: [Rate; 3] = [
    Rate { model: b"fake-1", input: 2, cached: 3, output: 5, unit: 7 },
    Rate { model: b"fake-2", input: 7, cached: 11, output: 13, unit: 10 },
    Rate { model: b"fake-3", input: 17, cached: 19, output: 23, unit: 29 },
];
fn charge(usage: llm::Usage, rate: Rate) -> u64 {
    // One combined numerator is rounded once. This oracle uses no domain
    // price method, accumulated spend, child bill or saved Turn value.
    let numerator = (u128::from(usage.input_tokens) + u128::from(usage.cache_write_tokens)) * u128::from(rate.input)
        + u128::from(usage.cache_read_tokens) * u128::from(rate.cached)
        + u128::from(usage.output_tokens) * u128::from(rate.output);
    let denominator = u128::from(rate.unit);
    u64::try_from(numerator.div_ceil(denominator)).expect("finite story prices fit u64")
}

fn usage(observation: &CompletionObservation) -> llm::Usage {
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
        expected.input += actual.input_tokens;
        expected.output += actual.output_tokens;
        expected.cache_read += actual.cache_read_tokens;
        expected.cache_write += actual.cache_write_tokens;
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
    assert_eq!(spent(world.answer()), expected, "one own-completion charge per genuine outside callback");
    assert_eq!(world.prompts().len(), world.completions().len());
    assert_eq!(world.turn_metadata().last().expect("actual main Turn").2, expected);
    assert!(world.turns().iter().all(|turn| !turn.spend_overflow));
    assert!(!expected.units_overflow && !expected.usage_overflow);
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
        max_tokens: 4096,
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
                min: 0,
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

fn turn_usage(actual: llm::Usage) -> llm::Usage {
    llm::Usage {
        input_tokens: actual.input_tokens,
        output_tokens: actual.output_tokens,
        cache_read_tokens: actual.cache_read_tokens,
        cache_write_tokens: actual.cache_write_tokens,
    }
}

#[test]
fn three_priced_models_and_seven_actual_nested_completions_conserve_global_own_charges() {
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
        script(b"@budget-grand", vec![says(GRANDCHILD, 19)]),
    ]);
    let settings = settings(701, false, false);
    let mut world = typed_world(&settings, b"@budget-main MAIN-INSTRUCTIONS", &NESTED_RATES, scripts);
    world.run(20_000);
    let expected = conservation(&world, &NESTED_RATES);
    assert_eq!(expected.turns, 7);
    assert_eq!(expected.output, 75);
    assert!(expected.cache_read > 0 && expected.cache_write > 0);
    assert_eq!(world.turns().len(), 3, "only main's settled Turns cross the host boundary");
    let main = model_calls(&world, b"fake-1");
    let child = model_calls(&world, b"fake-2");
    let grandchild = model_calls(&world, b"fake-3");
    assert_eq!((main.len(), child.len(), grandchild.len()), (3, 3, 1));
    let child_bill = own_units(&child, NESTED_RATES[1]) + own_units(&grandchild, NESTED_RATES[2]);
    let mut inclusive = child_bill;
    for (index, actual) in main.iter().enumerate() {
        inclusive += charge(usage(actual), NESTED_RATES[0]);
        assert_eq!(world.turns()[index].spent, inclusive, "main includes the genuine child's whole subtree bill once");
        assert_eq!(world.turns()[index].usage, turn_usage(usage(actual)));
    }
    assert_eq!(inclusive, expected.units);
    let main_queries = world.prompts().iter().filter(|query| query.model.as_ref() == b"fake-1").collect::<Vec<_>>();
    let child_queries = world.prompts().iter().filter(|query| query.model.as_ref() == b"fake-2").collect::<Vec<_>>();
    assert!(query_result(main_queries[1], b"sub_agent", CHILD_INPUT, CHILD));
    assert!(query_result(child_queries[1], b"sub_agent", GRAND_INPUT, GRANDCHILD));
    assert!(child_queries.iter().all(|query| query.system.starts_with(b"@budget-child own task\n\n")));
    assert!(
        world
            .prompts()
            .iter()
            .filter(|query| query.model.as_ref() == b"fake-3")
            .all(|query| query.system.starts_with(b"@budget-grand own task\n\n"))
    );
    assert!(
        matches!(world.answer(), run::Answer::Accepted { outcome: run::outcome::Declared::Report(report), turns: 3, .. }
        if report.text.as_ref() == b"Budget work finished.")
    );
    let mut duplicated = world.completions().to_vec();
    duplicated.push(duplicated[0].clone());
    assert_ne!(
        total(&duplicated, &NESTED_RATES),
        expected,
        "the whole positive conservation oracle detects double charging"
    );
}
