//! Actual optional-workspace starts, native parked transcripts and mixed mounts.
//! All bodies come from the root's Turns; all file effects use the shared checkout.
//! Contract: domain/run.md, sections 3, 6, 8 and 13; domain/tools.md, sections 2–4.

use skein_fake_checkout::Checkout;
use skein_fake_llm_domain::api::{Finish, Line, Message, Part, Query, Role, Script, Turn};
use skein_lib::{Duration, Token};
use skein_world::domain::Span;
use smith_domain::{Transcript, run};
use smith_protocol_llm_world::adapter::{self as adapter, Limits};
use smith_protocol_llm_world::{
    Boundary, Job, Settings, World,
    wire::{self, Observed},
};

const BEGIN: &[u8] = b"Begin the work your brief describes.";
const ID: &[u8] = b"call_0000000000000001";
const INPUT: &[u8] = br#"{"task":"one durable effect"}"#;
const ANSWER: &[u8] = b"opaque host answer: first decision";
const FIRST: &[u8] = b"Host answered; first activation parked.";
const RESUMED: &[u8] = b"History restored without another host effect.";
const LAST: &[u8] = b"Second activation parked.";

fn call(name: &[u8], arguments: &[u8]) -> Line {
    Line::Call { name: name.into(), arguments: arguments.into() }
}

fn calls(lines: Vec<Line>, tokens: u64) -> Turn {
    Turn { lines: lines.into(), finish: Finish::ToolCalls, tokens }
}

fn says(text: &[u8], tokens: u64) -> Turn {
    Turn { lines: Box::new([Line::Text { text: text.into() }]), finish: Finish::Stop, tokens }
}

fn native_script() -> Box<[Script]> {
    Box::new([Script {
        cue: b"@hosttools".as_slice().into(),
        turns: Box::new([
            calls(vec![call(b"host_action", INPUT)], 17),
            calls(vec![call(b"wait", b"{}")], 11),
            says(FIRST, 3),
            calls(vec![Line::Text { text: RESUMED.into() }, call(b"wait", b"{}")], 7),
            says(LAST, 5),
        ]),
    }])
}

fn text(bytes: &[u8]) -> Part {
    Part::Text { text: bytes.into() }
}

fn message(role: Role, parts: Vec<Part>) -> Message {
    Message { role, parts: parts.into() }
}

fn tool(name: &[u8], arguments: &[u8]) -> Part {
    Part::ToolCall { id: ID.into(), name: name.into(), arguments: arguments.into() }
}

fn result(output: &[u8]) -> Part {
    Part::ToolOutput { id: ID.into(), output: output.into(), is_error: false }
}

fn part_bytes(part: &Part) -> usize {
    match part {
        Part::Text { text } | Part::Opaque { bytes: text } => text.len(),
        Part::ToolCall { name, arguments, .. } => name.len() + arguments.len(),
        Part::ToolOutput { output, .. } => output.len(),
    }
}

fn accounting(world: &World, prefixes: &[Vec<Message>], outputs: &[u64], prior: u32, cache_write: bool) {
    assert_eq!(world.prompts().len(), prefixes.len());
    assert_eq!(world.turns().len(), prefixes.len());
    let mut spent = run::Spend::ZERO;
    for (index, ((query, turn), expected)) in world.prompts().iter().zip(world.turns()).zip(prefixes).enumerate() {
        assert_eq!(query.messages.as_ref(), expected, "whole native history at completion {index}");
        let (last, earlier) = expected.split_last().expect("user-ending prompt");
        let last_bytes = last.parts.iter().map(part_bytes).sum::<usize>();
        let (fresh, cached) = if earlier.is_empty() {
            (query.system.len() + last_bytes, 0)
        } else {
            (
                last_bytes,
                query.system.len() + earlier.iter().flat_map(|message| &message.parts).map(part_bytes).sum::<usize>(),
            )
        };
        let fresh_tokens = u64::try_from(fresh / 4).expect("bounded fixture");
        let input = if cache_write { 0 } else { fresh_tokens };
        let read = u64::try_from(cached / 4).expect("bounded fixture");
        let write = if cache_write { Some(fresh_tokens) } else { None };
        assert_eq!(
            [
                turn.usage.input_tokens,
                turn.usage.output_tokens,
                turn.usage.cache_read_tokens,
                turn.usage.cache_write_tokens
            ],
            [Some(input), Some(outputs[index]), Some(read), write],
            "all four independently calculated SDK usage fields"
        );
        spent.turns += 1;
        spent.input += input;
        spent.output += outputs[index];
        spent.cache_read += read;
        spent.cache_write += write.unwrap_or(0);
        assert_eq!(world.turn_metadata()[index], (spent.turns, None, spent));
        assert_eq!(turn.sequence, prior + spent.turns);
        assert_eq!(turn.version, smith_domain::session::record::VERSION);
        assert_eq!(turn.spent, 0);
        assert!(
            turn.messages.iter().flat_map(|message| &message.content).all(|block| {
                !matches!(block, smith_domain::session::llm::Block::ToolCall { call, .. }
                if !matches!(call, smith_domain::session::llm::Decoded::Historical))
            }),
            "persisted calls are concrete history"
        );
    }
    assert!(
        matches!(world.answer(), run::Answer::Parked { turns, spent: actual } if *turns == spent.turns && *actual == spent)
    );
}

fn no_workspace(world: &World, settings: &Settings) {
    assert!(world.boundaries().is_empty(), "no Read, Probe, workspace IO or Check");
    assert!(world.disk().files().is_empty(), "no unused seeded checkout");
    assert_eq!(format!("{:?}", world.disk()), format!("{:?}", Checkout::new()), "no unused roots or programs either");
    assert!(world.checked().is_empty() && world.pushes().is_empty() && world.delivery_submissions().is_empty());
    assert_eq!(world.waiting().len(), 1);
    assert!(world.answered_at() >= world.waiting()[0].0.saturating_add(settings.waiting));
    for query in world.prompts() {
        assert_eq!(
            query.tools.iter().map(|tool| tool.name.as_ref()).collect::<Vec<_>>(),
            [b"host_action".as_slice(), b"wait", b"finish", b"sub_agent"]
        );
        assert!(!contains(&query.system, b".temper/pre-pr"));
    }
    assert_eq!(world.wire_bindings().len(), world.turns().len());
    for binding in world.wire_bindings() {
        assert_eq!(
            binding.observed.iter().map(|(_, _, seen)| *seen).collect::<Vec<_>>(),
            [Observed::Completed(binding.owner), Observed::Reusable, Observed::Close, Observed::Closed]
        );
        assert!(binding.retired.is_some(), "physical owners retire only after actual Closed");
    }
}

#[test]
fn no_workspace_host_answer_wait_park_and_native_transcript_resume() {
    for index in 0..2 {
        let mut bounds = Limits {
            client: skein_llm_world::limits(),
            tool_bytes: 32768,
            rendered_result: skein_llm_world::limits().dialect.string_bytes,
            shell_default: skein_lib::Duration::from_secs(120),
            shell_maximum: skein_lib::Duration::from_secs(1200),
        };
        bounds.client.http.request = 16384;
        bounds.client.dialect.request_bytes = 16384;
        let mut settings = Settings {
            job: Job::HostTools,
            waiting: Duration::from_secs(1),
            network: Span::millis(1, 1),
            ..Settings::calm(1001)
        };
        settings.limits.session.completion_bytes =
            adapter::completion_worst_case(&bounds.client, settings.limits.decoded_call_bytes)
                .expect("actual Client completion reservation");
        settings.limits.session.completion_blocks = bounds.client.dialect.parts;
        let configuration = wire::configurations().into_iter().nth(index).expect("two native fixtures");
        let mut first =
            World::with_workspace_wire(settings, None, None, Checkout::new(), configuration, bounds, native_script());
        first.run(100_000);
        let first_prefix = vec![message(Role::User, vec![text(BEGIN)])];
        let mut second_prefix = first_prefix.clone();
        second_prefix.extend([
            message(Role::Assistant, vec![tool(b"host_action", INPUT)]),
            message(Role::User, vec![result(ANSWER)]),
        ]);
        let mut third_prefix = second_prefix.clone();
        third_prefix.extend([
            message(Role::Assistant, vec![tool(b"wait", b"{}")]),
            message(Role::User, vec![result(b"waiting")]),
        ]);
        accounting(&first, &[first_prefix, second_prefix, third_prefix.clone()], &[17, 11, 3], 0, true);
        no_workspace(&first, &settings);
        assert_eq!(first.host_submissions().len(), 1);
        assert_eq!(first.host_submissions()[0].name, run::CallName { activation: 1, completion: 1, position: 0 });
        assert_eq!(first.host_submissions()[0].input.bytes(), INPUT);
        assert_eq!(first.host_decisions(), 1);
        assert!(
            matches!(first.host_terminals(), [(_, _, run::HostReply::Answered(answer))] if answer.text() == ANSWER && !answer.error())
        );
        let actual = &first.turns()[0];
        let saved = Transcript {
            version: actual.version,
            endpoint: actual.endpoint,
            dialect: actual.dialect,
            turns: first.turns().into(),
        };
        assert_eq!(saved.turns.last().expect("persisted actual tail").sequence, 3);
        settings.resume = true;
        let configuration = wire::configurations().into_iter().nth(index).expect("same native fixture");
        let mut resumed = World::with_workspace_wire(
            settings,
            Some(saved),
            None,
            Checkout::new(),
            configuration,
            bounds,
            native_script(),
        );
        resumed.run(100_000);
        third_prefix.extend([message(Role::Assistant, vec![text(FIRST)]), message(Role::User, vec![text(BEGIN)])]);
        let mut continued = third_prefix.clone();
        continued.extend([
            message(Role::Assistant, vec![text(RESUMED), tool(b"wait", b"{}")]),
            message(Role::User, vec![result(b"waiting")]),
        ]);
        accounting(&resumed, &[third_prefix, continued], &[7, 5], 3, true);
        no_workspace(&resumed, &settings);
        assert!(resumed.host_submissions().is_empty() && resumed.host_terminals().is_empty());
        assert_eq!(resumed.host_decisions(), 0, "restored history performs no second host effect");
        assert_eq!(first.turn_metadata()[2].2.output, 31);
        assert_eq!(resumed.turn_metadata()[1].2.output, 12, "history is not recharged");
    }
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|bytes| bytes == part)
}

fn mixed_disk() -> (Checkout, run::Workspace) {
    let mut disk = Checkout::new();
    let mut directories = Vec::new();
    for (name, writable, git) in [
        (b"work".as_slice(), true, true),
        (b"archive", false, true),
        (b"notes", true, false),
        (b"reference", false, false),
    ] {
        disk.mkdir(name);
        disk.write(&[name, b"/AGENTS.md"].concat(), &[b"guide-", name].concat());
        disk.write(&[name, b"/.temper/pre-pr"].concat(), b"#!checks");
        disk.write(&[name, b"/src/lib.rs"].concat(), b"pub fn answer() -> u32 { 43 }\n");
        disk.write(&[name, b"/data.txt"].concat(), b"old");
        if git {
            disk.write(&[name, b"/.git/HEAD"].concat(), b"host-owned");
        }
        let root = Token::new(disk.root(name));
        directories.push(run::Directory { name: name.into(), root, writable, git, conflicts: Box::new([]) });
    }
    (disk, run::Workspace { directories: directories.into() })
}

fn mixed_scripts() -> Box<[Script]> {
    Box::new([
        Script {
            cue: b"@midreport".as_slice().into(),
            turns: vec![
                calls(vec![call(b"edit", br#"{"path":"../notes/data.txt","old":"old","new":"premature"}"#)], 1),
                calls(vec![call(b"read", br#"{"path":"../notes/data.txt"}"#)], 1),
                calls(vec![call(b"edit", br#"{"path":"../notes/data.txt","old":"old","new":"main"}"#)], 1),
                calls(vec![call(b"edit", br#"{"path":"../archive/data.txt","old":"old","new":"forbidden"}"#)], 1),
                calls(vec![call(b"write", br#"{"path":".git/HEAD","content":"forbidden"}"#)], 1),
                calls(vec![call(b"sub_agent", br#"{"brief":"@mixed-read","tools":["inspect"]}"#)], 1),
                calls(vec![call(b"sub_agent", br#"{"brief":"@mixed-write","tools":["inspect","modify"]}"#)], 1),
                calls(vec![call(b"deliver", br#"{"ticket":"mixed-actual-tree"}"#)], 1),
                calls(vec![call(b"finish", br#"{"report":"Mixed directories handled.","source":"workspace"}"#)], 1),
            ]
            .into(),
        },
        Script {
            cue: b"@mixed-read".as_slice().into(),
            turns: Box::new([
                calls(vec![call(b"read", br#"{"path":"../reference/data.txt"}"#)], 1),
                calls(vec![call(b"edit", br#"{"path":"../notes/data.txt","old":"main","new":"forbidden"}"#)], 1),
                says(b"Read-only child done.", 1),
            ]),
        },
        Script {
            cue: b"@mixed-write".as_slice().into(),
            turns: Box::new([
                calls(vec![call(b"edit", br#"{"path":"../notes/data.txt","old":"main","new":"unread-child"}"#)], 1),
                calls(
                    vec![call(b"read", br#"{"path":"../notes/data.txt"}"#), call(b"read", br#"{"path":"data.txt"}"#)],
                    1,
                ),
                calls(vec![call(b"edit", br#"{"path":"../notes/data.txt","old":"main","new":"child"}"#)], 1),
                calls(vec![call(b"edit", br#"{"path":"data.txt","old":"old","new":"child-git"}"#)], 1),
                calls(vec![call(b"edit", br#"{"path":"../reference/data.txt","old":"old","new":"forbidden"}"#)], 1),
                says(b"Writable child done.", 1),
            ]),
        },
    ])
}

fn feedback_has(query: &Query, part: &[u8]) -> bool {
    query
        .messages
        .iter()
        .flat_map(|message| &message.parts)
        .any(|item| matches!(item, Part::ToolOutput { output, is_error: true, .. } if contains(output, part)))
}

#[test]
fn mixed_git_plain_and_readonly_mounts_share_discovery_and_child_authority() {
    let (disk, workspace) = mixed_disk();
    let readonly_git = disk.tree(b"archive");
    let readonly_plain = disk.tree(b"reference");
    let git_head = disk.content(b"work/.git/HEAD").expect("host-owned git metadata").to_vec();
    let roots = workspace.directories.iter().map(|directory| directory.root).collect::<Vec<_>>();
    let mut settings = Settings { job: Job::MidReport, ..Settings::calm(1002) };
    settings.limits.run.directories = 4;
    settings.limits.session.tools.repos = 4;
    let mut world = World::with_workspace_scripts(settings, None, Some(workspace), disk, mixed_scripts());
    world.enable_parent_deliveries();
    for _ in 0..10_000 {
        assert!(!world.drive(1));
        if !world.delivery_submissions().is_empty() {
            break;
        }
    }
    assert_eq!(world.delivery_submissions().len(), 1, "actual parent delivery reached");
    assert_eq!(world.checked(), [true, true]);
    assert_eq!(world.disk().content(b"work/data.txt"), Some(b"child-git".as_slice()));
    assert_eq!(world.disk().content(b"notes/data.txt"), Some(b"child".as_slice()));
    assert_eq!(world.disk().tree(b"archive"), readonly_git);
    assert_eq!(world.disk().tree(b"reference"), readonly_plain);
    assert_eq!(world.disk().content(b"work/.git/HEAD"), Some(git_head.as_slice()));
    assert!(!world.disk().exists(b"notes/.git") && !world.disk().exists(b"reference/.git"));
    let owner = world.delivery_submissions()[0].owner;
    let receipts = run::Delivered::new(Box::new([
        run::Receipt::new(0, b"work: checked git files kept".as_slice().into()).expect("git receipt"),
        run::Receipt::new(2, b"notes: changed plain files kept".as_slice().into()).expect("plain receipt"),
    ]))
    .expect("exact changed writable ordinals");
    world.return_delivery(owner, run::Delivery::Delivered(receipts)).expect("one actual terminal");
    world.run(10_000);
    assert!(matches!(world.answer(), run::Answer::Accepted { outcome: run::outcome::Declared::Report(_), .. }));
    let discovery = world
        .boundaries()
        .iter()
        .filter_map(|(_, boundary)| match boundary {
            Boundary::Read { at } => Some((b"read".as_slice(), at.root, at.path.as_ref())),
            Boundary::Probe { at } => Some((b"probe".as_slice(), at.root, at.path.as_ref())),
            Boundary::Check { .. } | Boundary::Io { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        discovery,
        [
            (b"read".as_slice(), roots[0], b"AGENTS.md".as_slice()),
            (b"probe", roots[0], b".temper/pre-pr"),
            (b"read", roots[1], b"AGENTS.md"),
            (b"read", roots[2], b"AGENTS.md"),
            (b"probe", roots[2], b".temper/pre-pr"),
            (b"read", roots[3], b"AGENTS.md"),
        ]
    );
    let checks = world
        .boundaries()
        .iter()
        .filter_map(|(_, boundary)| match boundary {
            Boundary::Check { program } => Some(program.root),
            Boundary::Read { .. } | Boundary::Probe { .. } | Boundary::Io { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(checks, [roots[0], roots[2]], "readonly executable files never become checks");
    for query in world.prompts() {
        for name in [b"work".as_slice(), b"archive", b"notes", b"reference"] {
            assert!(contains(&query.system, &[b"guide-", name].concat()), "same actual guides in main and children");
        }
        for metadata in [
            b"`work`, which you may change, a git working tree".as_slice(),
            b"`archive`, which you may only read, a git working tree",
            b"`notes`, which you may change, a plain directory",
            b"`reference`, which you may only read, a plain directory",
        ] {
            assert!(contains(&query.system, metadata), "identical kind and write metadata in main and children");
        }
    }
    let main =
        world.prompts().iter().filter(|query| query.system.starts_with(b"## Task\n\n@midreport")).collect::<Vec<_>>();
    assert!(main.iter().any(|query| feedback_has(query, b"NotRead")));
    assert!(main.iter().any(|query| feedback_has(query, b"ReadOnly")));
    assert!(main.iter().any(|query| feedback_has(query, b"Protected")));
    let readonly = world.prompts().iter().filter(|query| query.system.starts_with(b"@mixed-read")).collect::<Vec<_>>();
    assert_eq!(readonly.len(), 3);
    assert!(readonly.iter().all(|query| query.tools.iter().all(|tool| {
        ![b"edit".as_slice(), b"write", b"finish", b"wait", b"host_action"].contains(&tool.name.as_ref())
    })));
    assert!(readonly.iter().any(|query| feedback_has(query, b"UnknownTool")));
    let writable = world.prompts().iter().filter(|query| query.system.starts_with(b"@mixed-write")).collect::<Vec<_>>();
    assert!(writable.iter().any(|query| feedback_has(query, b"NotRead")), "main's knowledge is not child's");
    assert!(writable.iter().any(|query| feedback_has(query, b"ReadOnly")), "child cannot widen mount authority");
}
