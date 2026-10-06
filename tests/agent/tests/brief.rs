//! Ordered host sections through actual main/child sessions and native restore.
//! Expected system bodies are handwritten outside literals; concrete history,
//! child results, usage and lower settlement come from the real composed routes.
//! Contract: domain/run.md, sections 3.1, 3.3, 5.3, 6.2 and 13;
//! testing-strategy.md, sections 2.3, 6 and 7.

use skein_fake_checkout::Checkout;
use skein_fake_llm_domain::api::{Finish, Line, Message, Part, Query, Role, Script, Turn};
use skein_lib::{Duration, Token};
use skein_world::domain::Span;
use smith_agent_world::{
    Boundary, Job, Settings, World,
    wire::{self, Observed},
};
use smith_domain::{Transcript, run, session::llm, tools};
use smith_protocol_llm::{self as adapter, Limits};

const BEGIN: &[u8] = b"Begin the work your brief describes.";
const ID: &[u8] = b"call_0000000000000001";
const MAIN_INSTRUCTIONS: &str = "@brief-main PARENT-INSTRUCTIONS: coordinate this task.\nSeparate role line.";
const NATIVE_INSTRUCTIONS: &str = "@brief-native PARENT-INSTRUCTIONS: native first activation.";
const RESUME_INSTRUCTIONS: &str = "@brief-native RESUME-INSTRUCTIONS: use the saved history.\n";
const CHILD_INPUT: &[u8] = br#"{"brief":"@brief-child CHILD-TASK-ONLY: inspect both mounts.","tools":["inspect"]}"#;
const CHILD_RESULT: &[u8] = b"CHILD-RESULT: parent-change in work; unchanged in archive.";
const FINISH_INPUT: &[u8] = br#"{"report":"The child inspected the changed workspace.","source":"workspace"}"#;
const FIRST: &[u8] = b"Native structured brief first activation parked.";
const RESUMED: &[u8] = b"Native structured brief history restored.";
const LAST: &[u8] = b"Native structured brief second activation parked.";

// These literals do not call the production renderer or reproduce its algorithm.
// Duplicate Zulu titles and the intervening Alpha title must stay in host order.
const SECTIONS: &str = "## Zulu PARENT-TITLE-Z\n\nPARENT-BODY-ONE: café\nline one\n\n\
## Alpha PARENT-TITLE-A\n\nPARENT-BODY-TWO: λ ends with LF\n\n\
## Zulu PARENT-TITLE-Z\n\nPARENT-BODY-THREE: 東京\nthird line\n\n";
const GUIDES: &str = "## AGENTS.md in `work`\n\nWORK-GUIDE: inspect data.txt.\n\n\
## AGENTS.md in `archive`\n\nARCHIVE-GUIDE: keep every byte.\n\n";
const MAIN_MECHANICS: &str = "## Tools\n\n\
You can read, list and search the files in the checkout.\n\
You can write and edit files in its writable repositories.\n\n\
## Checkout\n\n\
- `work`, which you may change, a git working tree\n\
- `archive`, which you may only read, a plain directory\n\n\
## Sub-agents\n\n\
You can ask for a sub-agent: an LLM of its own, working on a brief you write, with tools no wider than \
yours and a share of the budget. Its last message comes back to you as the result. It runs on the \
run's main LLM unless you name one of these: `fake-2`.\n\n\
## Finishing\n\n\
When the work is done, call `finish` with its result. A result outside the host's contract returns \
typed feedback; fix it and call `finish` again. Stopping without `finish` does not finish the run.\n\n\
Report: text from 0 through 1024 bytes, with these host-required fields:\n\
- `source`: nonempty, at most 128 bytes\n\
Extra fields are allowed within the aggregate result byte limit; no field name may repeat.\n";
const CHILD_MECHANICS: &str = "## Tools\n\n\
You can read, list and search the files in the checkout.\n\n\
## Checkout\n\n\
- `work`, which you may change, a git working tree\n\
- `archive`, which you may only read, a plain directory\n\n\
## Answering\n\n\
When you are done, end your turn with your answer: your last message goes, as it is, to the LLM that asked for \
you, and you are done.\n";

fn charter(settings: &Settings, instructions: &str) -> run::Charter {
    use run::charter::{Brief, Endpoint, Grants, Llm, Section, Tools};
    use run::outcome::{FieldRule, OutcomeSpec, TextSpec};
    let llm = Llm {
        prices: run::Prices { input: 0, cached: 0, output: 0, unit: 1 },
        dialect: 1,
        account: 0,
        endpoint: Endpoint(0),
        model: b"fake-1".as_slice().into(),
        max_tokens: 4096,
    };
    run::Charter {
        instructions: instructions.as_bytes().into(),
        brief: Brief {
            sections: Box::new([
                Section {
                    title: b"Zulu PARENT-TITLE-Z".as_slice().into(),
                    text: "PARENT-BODY-ONE: café\nline one".as_bytes().into(),
                },
                Section {
                    title: b"Alpha PARENT-TITLE-A".as_slice().into(),
                    text: "PARENT-BODY-TWO: λ ends with LF\n".as_bytes().into(),
                },
                Section {
                    title: b"Zulu PARENT-TITLE-Z".as_slice().into(),
                    text: "PARENT-BODY-THREE: 東京\nthird line".as_bytes().into(),
                },
            ]),
        },
        grants: Grants {
            deliver: None,
            tools: Tools { inspect: true, modify: true, shell: false },
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
        models: Box::new([Llm { model: b"fake-2".as_slice().into(), ..llm.clone() }]),
        llm,
        conventions: Some(run::Conventions {
            guide: b"AGENTS.md".as_slice().into(),
            checks: b".temper/pre-pr".as_slice().into(),
        }),
        resume: settings.resume,
        waiting: settings.waiting,
    }
}

fn workspace() -> (Checkout, run::Workspace) {
    let mut disk = Checkout::new();
    disk.mkdir(b"work");
    disk.mkdir(b"archive");
    disk.write(b"work/AGENTS.md", b"WORK-GUIDE: inspect data.txt.\n");
    disk.write(b"archive/AGENTS.md", b"ARCHIVE-GUIDE: keep every byte.");
    disk.write(b"work/.temper/pre-pr", b"#!checks");
    disk.write(b"archive/.temper/pre-pr", b"#!readonly-check-must-not-run");
    disk.write(b"work/.git/HEAD", b"host-owned-git-metadata");
    disk.write(b"work/data.txt", b"old");
    disk.write(b"archive/data.txt", b"unchanged");
    let mounted = run::Workspace {
        directories: Box::new([
            run::Directory {
                name: b"work".as_slice().into(),
                root: Token::new(disk.root(b"work")),
                writable: true,
                git: true,
                conflicts: Box::new([]),
            },
            run::Directory {
                name: b"archive".as_slice().into(),
                root: Token::new(disk.root(b"archive")),
                writable: false,
                git: false,
                conflicts: Box::new([]),
            },
        ]),
    };
    (disk, mounted)
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

fn typed_scripts() -> Box<[Script]> {
    Box::new([
        Script {
            cue: b"@brief-main".as_slice().into(),
            turns: Box::new([
                calls(vec![call(b"read", br#"{"path":"data.txt"}"#)], 2),
                calls(vec![call(b"edit", br#"{"path":"data.txt","old":"old","new":"parent-change"}"#)], 3),
                calls(vec![call(b"write", br#"{"path":"../archive/data.txt","content":"forbidden-main"}"#)], 5),
                calls(vec![call(b"sub_agent", CHILD_INPUT)], 7),
                calls(vec![call(b"finish", FINISH_INPUT)], 11),
            ]),
        },
        Script {
            cue: b"@brief-child".as_slice().into(),
            turns: Box::new([
                calls(
                    vec![call(b"read", br#"{"path":"data.txt"}"#), call(b"read", br#"{"path":"../archive/data.txt"}"#)],
                    13,
                ),
                calls(vec![call(b"edit", br#"{"path":"data.txt","old":"parent-change","new":"forbidden-child"}"#)], 17),
                says(CHILD_RESULT, 19),
            ]),
        },
    ])
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|bytes| bytes == part)
}

fn feedback_has(query: &Query, part: &[u8]) -> bool {
    query
        .messages
        .iter()
        .flat_map(|message| &message.parts)
        .any(|item| matches!(item, Part::ToolOutput { output, is_error: true, .. } if contains(output, part)))
}

fn expected_main(instructions_literal: &str) -> Vec<u8> {
    [instructions_literal, SECTIONS, GUIDES, MAIN_MECHANICS].concat().into_bytes()
}

fn typed_openings(world: &World) {
    let main_system = expected_main("@brief-main PARENT-INSTRUCTIONS: coordinate this task.\nSeparate role line.\n\n");
    let child_system =
        ["@brief-child CHILD-TASK-ONLY: inspect both mounts.\n\n", GUIDES, CHILD_MECHANICS].concat().into_bytes();
    let main = world.prompts().iter().filter(|query| query.system.starts_with(b"@brief-main")).collect::<Vec<_>>();
    let child = world.prompts().iter().filter(|query| query.system.starts_with(b"@brief-child")).collect::<Vec<_>>();
    assert_eq!(main.len(), 5, "actual main turns, including child result and accepted finish");
    assert_eq!(child.len(), 3, "actual child reads, denied edit and terminal answer");
    assert_eq!(main.len() + child.len(), world.prompts().len());
    for query in &main {
        assert_eq!(query.system.as_ref(), main_system, "literal complete ordered main system");
        assert_eq!(query.model.as_ref(), b"fake-1");
    }
    for query in &child {
        assert_eq!(query.system.as_ref(), child_system, "literal task, actual guides and narrowed mechanics");
        for sentinel in [
            b"PARENT-INSTRUCTIONS".as_slice(),
            b"PARENT-TITLE-Z",
            b"PARENT-TITLE-A",
            b"PARENT-BODY-ONE",
            b"PARENT-BODY-TWO",
            b"PARENT-BODY-THREE",
        ] {
            assert!(!contains(&query.system, sentinel), "parent payload must not implicitly reach child: {sentinel:?}");
        }
        assert_eq!(
            query.tools.iter().map(|tool| tool.name.as_ref()).collect::<Vec<_>>(),
            [b"read".as_slice(), b"list", b"search"]
        );
    }
    assert!(main.iter().any(|query| feedback_has(query, b"ReadOnly")));
    assert!(child.iter().any(|query| feedback_has(query, b"UnknownTool")));
    assert!(main[4].messages.iter().flat_map(|message| &message.parts).any(|part| {
        matches!(part, Part::ToolOutput { output, is_error: false, .. } if output.as_ref() == CHILD_RESULT)
    }));
}

#[test]
fn ordered_sections_and_instructions_reach_main_but_child_receives_only_its_own_task() {
    let settings = Settings { job: Job::Reporting, network: Span::millis(1, 1), ..Settings::calm(1101) };
    let (disk, mounted) = workspace();
    let readonly = disk.tree(b"archive");
    let roots = mounted.directories.iter().map(|directory| directory.root).collect::<Vec<_>>();
    let mut world = World::with_workspace_scripts_charter(
        settings,
        None,
        Some(mounted),
        disk,
        typed_scripts(),
        charter(&settings, MAIN_INSTRUCTIONS),
    );
    world.run(10_000);
    typed_openings(&world);
    assert_eq!(world.disk().content(b"work/data.txt"), Some(b"parent-change".as_slice()));
    assert_eq!(world.disk().content(b"work/.git/HEAD"), Some(b"host-owned-git-metadata".as_slice()));
    assert_eq!(world.disk().tree(b"archive"), readonly);
    let stores = world
        .boundaries()
        .iter()
        .filter_map(|(_, boundary)| {
            if let Boundary::Io { op: tools::Op::Store { at, content, .. } } = boundary {
                Some((at.root, at.path.as_ref(), content.as_ref()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(stores, [(roots[0], b"data.txt".as_slice(), b"parent-change".as_slice())]);
    let loads = world
        .boundaries()
        .iter()
        .filter_map(|(_, boundary)| {
            if let Boundary::Io { op: tools::Op::Load { at, .. } } = boundary {
                Some((at.root, at.path.as_ref()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        loads.iter().filter(|(root, path)| *root == roots[0] && *path == b"data.txt").count(),
        3,
        "main's read and edit reload, then child's real read"
    );
    assert_eq!(loads.iter().filter(|(root, path)| *root == roots[1] && *path == b"data.txt").count(), 1);
    assert_eq!(world.turns().len(), 5);
    assert_eq!(world.turns().iter().map(|turn| turn.sequence).collect::<Vec<_>>(), [1, 2, 3, 4, 5]);
    let delegated = &world.turns()[3];
    let child_call = delegated
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .find_map(|block| match block {
            llm::Block::ToolCall { id, name, input, call: llm::Decoded::Historical, .. }
                if name.as_ref() == b"sub_agent" && input.as_ref() == CHILD_INPUT =>
            {
                Some(id)
            }
            llm::Block::Opaque { .. }
            | llm::Block::Refusal { .. }
            | llm::Block::Text { .. }
            | llm::Block::ToolCall { .. }
            | llm::Block::ToolResult { .. } => None,
        })
        .expect("genuine persisted raw child-task call");
    assert_eq!(
        delegated
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .filter(|block| {
                matches!(block, llm::Block::ToolResult { id, result: llm::Returned::Text {
                text, error: false, replay: None
            }} if id == child_call && text.as_ref() == CHILD_RESULT)
            })
            .count(),
        1,
        "actual child result is paired once with its caller's concrete Turn"
    );
    assert!(matches!(world.answer(), run::Answer::Accepted {
        outcome: run::outcome::Declared::Report(report), turns: 5, ..
    } if report.text.as_ref() == b"The child inspected the changed workspace."
        && report.fields.as_ref() == [run::outcome::Field {
            name: b"source".as_slice().into(), value: b"workspace".as_slice().into()
        }]));
    assert!(world.checked().is_empty() && world.pushes().is_empty() && world.host_submissions().is_empty());
    assert!(world.judged().0 > 0 && world.judged().1 > 0);
}

fn native_scripts() -> Box<[Script]> {
    Box::new([Script {
        cue: b"@brief-native".as_slice().into(),
        turns: Box::new([
            calls(vec![call(b"wait", b"{}")], 17),
            says(FIRST, 3),
            calls(vec![Line::Text { text: RESUMED.into() }, call(b"wait", b"{}")], 11),
            says(LAST, 5),
        ]),
    }])
}

fn text(text: &[u8]) -> Part {
    Part::Text { text: text.into() }
}

fn message(role: Role, parts: Vec<Part>) -> Message {
    Message { role, parts: parts.into() }
}

fn wait_call() -> Part {
    Part::ToolCall { id: ID.into(), name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() }
}

fn wait_result() -> Part {
    Part::ToolOutput { id: ID.into(), output: b"waiting".as_slice().into(), is_error: false }
}

fn part_bytes(part: &Part) -> usize {
    match part {
        Part::Text { text } | Part::Opaque { bytes: text } => text.len(),
        Part::ToolCall { name, arguments, .. } => name.len() + arguments.len(),
        Part::ToolOutput { output, .. } => output.len(),
    }
}

fn native_accounting(
    world: &World,
    system: &[u8],
    prefixes: &[Vec<Message>],
    outputs: &[u64],
    prior: u32,
    cache_write: bool,
) {
    assert_eq!(world.prompts().len(), prefixes.len());
    assert_eq!(world.turns().len(), prefixes.len());
    let mut spent = run::Spend::ZERO;
    for (index, ((query, turn), prefix)) in world.prompts().iter().zip(world.turns()).zip(prefixes).enumerate() {
        assert_eq!(query.system.as_ref(), system, "whole native structured opening at completion {index}");
        assert_eq!(query.model.as_ref(), b"fake-1");
        assert_eq!(query.messages.as_ref(), prefix, "whole native history and concrete feedback at completion {index}");
        let (last, earlier) = prefix.split_last().expect("handwritten user-ending native history");
        let last_bytes = last.parts.iter().map(part_bytes).sum::<usize>();
        let (fresh, cached) = if earlier.is_empty() {
            (system.len() + last_bytes, 0)
        } else {
            (
                last_bytes,
                system.len() + earlier.iter().flat_map(|message| &message.parts).map(part_bytes).sum::<usize>(),
            )
        };
        let input = u64::try_from(fresh / 4).expect("bounded outside fixture");
        let read = u64::try_from(cached / 4).expect("bounded outside fixture");
        let write = if cache_write { input } else { 0 };
        assert_eq!(
            [
                turn.usage.input_tokens,
                turn.usage.output_tokens,
                turn.usage.cache_read_tokens,
                turn.usage.cache_write_tokens
            ],
            [input, outputs[index], read, write],
            "all four usage fields independently priced from literal system and native query"
        );
        spent.turns += 1;
        spent.input += input;
        spent.output += outputs[index];
        spent.cache_read += read;
        spent.cache_write += write;
        assert_eq!(world.turn_metadata()[index], (spent.turns, None, spent));
        assert_eq!(turn.sequence, prior + spent.turns);
        assert_eq!(turn.version, 2);
        assert_eq!(turn.endpoint, llm::Endpoint(0));
        assert_eq!(turn.dialect, 1);
        assert_eq!(turn.spent, 0);
    }
    assert!(matches!(world.answer(), run::Answer::Parked { turns, spent: actual }
        if *turns == spent.turns && *actual == spent));
}

fn native_settled(world: &World, settings: &Settings, roots: &[Token]) {
    assert_eq!(world.waiting().len(), 1);
    assert_eq!(world.waiting()[0].1, None);
    assert!(world.answered_at() >= world.waiting()[0].0.saturating_add(settings.waiting));
    assert!(world.checked().is_empty() && world.pushes().is_empty() && world.host_submissions().is_empty());
    assert!(world.host_terminals().is_empty() && world.delivery_submissions().is_empty());
    let discovery = world
        .boundaries()
        .iter()
        .map(|(_, boundary)| match boundary {
            Boundary::Read { at } => (b"read".as_slice(), at.root, at.path.as_ref()),
            Boundary::Probe { at } => (b"probe".as_slice(), at.root, at.path.as_ref()),
            Boundary::Io { .. } | Boundary::Check { .. } => panic!("Wait-only native script does no workspace effects"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        discovery,
        [(b"read".as_slice(), roots[0], b"AGENTS.md".as_slice()), (b"read", roots[1], b"AGENTS.md"),]
    );
    assert_eq!(world.wire_bindings().len(), world.turns().len());
    for binding in world.wire_bindings() {
        assert_eq!(binding.receiving.max_completion_bytes, settings.limits.session.completion_bytes);
        assert_eq!(binding.receiving.max_completion_blocks, settings.limits.session.completion_blocks);
        assert_eq!(binding.receiving.max_failure_bytes, settings.limits.session.failure_bytes);
        assert_eq!(binding.receiving.decoded_call_bytes, settings.limits.decoded_call_bytes);
        assert_eq!(
            binding.observed.iter().map(|(_, _, seen)| *seen).collect::<Vec<_>>(),
            [Observed::Completed(binding.owner), Observed::Reusable, Observed::Close, Observed::Closed]
        );
        assert!(binding.retired.is_some(), "actual lower Closed precedes physical retirement");
    }
}

fn recorded_wait(turn: &smith_domain::Turn, resumed: bool) {
    let call = turn
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .find_map(|block| {
            if let llm::Block::ToolCall { id, name, input, call: llm::Decoded::Historical, .. } = block {
                assert_eq!(id.as_ref(), ID);
                assert_eq!(name.as_ref(), b"wait");
                assert_eq!(input.as_ref(), b"{}");
                Some(id)
            } else {
                None
            }
        })
        .expect("genuine actual historical Wait call");
    assert_eq!(
        turn.messages
            .iter()
            .flat_map(|message| &message.content)
            .filter(|block| {
                matches!(block, llm::Block::ToolResult { id, result: llm::Returned::Text {
            text, error: false, replay: None
        }} if id == call && text.as_ref() == b"waiting")
            })
            .count(),
        1,
        "actual settled Wait feedback saved once"
    );
    if resumed {
        assert!(
            turn.messages
                .iter()
                .flat_map(|message| &message.content)
                .any(|block| { matches!(block, llm::Block::Text { text, .. } if text.as_ref() == RESUMED) })
        );
    } else {
        assert!(matches!(turn.messages[0].content.as_ref(), [llm::Block::Text { text, replay: None }]
            if text.as_ref() == BEGIN));
    }
}

#[test]
fn structured_brief_native_wait_park_and_genuine_transcript_restore_in_both_wire_forms() {
    for index in 0..2 {
        let mut bounds = Limits { client: skein_llm_world::limits(), tool_bytes: 32768, result_bytes: 32768 };
        bounds.client.http.request = 16384;
        bounds.client.dialect.request_bytes = 16384;
        let mut settings = Settings {
            job: Job::Waiting,
            waiting: Duration::from_secs(1),
            network: Span::millis(1, 1),
            ..Settings::calm(1102)
        };
        settings.limits.session.completion_bytes =
            adapter::completion_worst_case(&bounds.client, settings.limits.decoded_call_bytes)
                .expect("real Client receiving reservation before original Start");
        settings.limits.session.completion_blocks = bounds.client.dialect.parts;
        let (disk, mounted) = workspace();
        let roots = mounted.directories.iter().map(|directory| directory.root).collect::<Vec<_>>();
        let mut first = World::with_workspace_wire_charter(
            settings,
            None,
            Some(mounted),
            disk,
            (wire::configurations().into_iter().nth(index).expect("two native wire forms"), bounds),
            native_scripts(),
            charter(&settings, NATIVE_INSTRUCTIONS),
        );
        first.run(100_000);
        let first_system = expected_main("@brief-native PARENT-INSTRUCTIONS: native first activation.\n\n");
        let opening = vec![message(Role::User, vec![text(BEGIN)])];
        let mut waited = opening.clone();
        waited.extend([message(Role::Assistant, vec![wait_call()]), message(Role::User, vec![wait_result()])]);
        native_accounting(&first, &first_system, &[opening, waited.clone()], &[17, 3], 0, index == 1);
        native_settled(&first, &settings, &roots);
        recorded_wait(&first.turns()[0], false);
        assert!(matches!(first.turns()[1].messages[0].content.as_ref(), [llm::Block::Text { text, .. }]
            if text.as_ref() == FIRST));
        let actual = &first.turns()[0];
        let saved = Transcript {
            version: actual.version,
            endpoint: actual.endpoint,
            dialect: actual.dialect,
            turns: first.turns().into(),
            after: Box::new([]),
        };
        assert_eq!(saved.turns.last().expect("actual persisted transcript tail").sequence, 2);
        settings.resume = true;
        let (disk, mounted) = workspace();
        let roots = mounted.directories.iter().map(|directory| directory.root).collect::<Vec<_>>();
        let mut restored = World::with_workspace_wire_charter(
            settings,
            Some(saved),
            Some(mounted),
            disk,
            (wire::configurations().into_iter().nth(index).expect("same native wire form"), bounds),
            native_scripts(),
            charter(&settings, RESUME_INSTRUCTIONS),
        );
        restored.run(100_000);
        let restored_system = expected_main("@brief-native RESUME-INSTRUCTIONS: use the saved history.\n\n");
        waited.extend([message(Role::Assistant, vec![text(FIRST)]), message(Role::User, vec![text(BEGIN)])]);
        let mut continued = waited.clone();
        continued.extend([
            message(Role::Assistant, vec![text(RESUMED), wait_call()]),
            message(Role::User, vec![wait_result()]),
        ]);
        native_accounting(&restored, &restored_system, &[waited, continued], &[11, 5], 2, index == 1);
        native_settled(&restored, &settings, &roots);
        recorded_wait(&restored.turns()[0], true);
        assert!(matches!(restored.turns()[1].messages[0].content.as_ref(), [llm::Block::Text { text, .. }]
            if text.as_ref() == LAST));
        assert_eq!(first.turn_metadata()[1].2.output, 20);
        assert_eq!(restored.turn_metadata()[1].2.output, 16, "new activation charges only its actual completions");
    }
}
