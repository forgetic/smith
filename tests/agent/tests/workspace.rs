//! Optional workspace and mixed mounts on the typed fake LLM world.
//! Contract: domain/run.md, sections 3 and 8; domain/tools.md, sections 2–4.

use skein_fake_checkout::Checkout;
use skein_fake_llm_domain::api::{Finish, Line, Part, Query, Script, Turn};
use skein_lib::Token;
use smith_agent_world::{Boundary, Job, Settings, World};
use smith_domain::run;

fn call(name: &[u8], arguments: &[u8]) -> Line {
    Line::Call { name: name.into(), arguments: arguments.into() }
}

fn calls(lines: Vec<Line>, tokens: u64) -> Turn {
    Turn { lines: lines.into(), finish: Finish::ToolCalls, tokens }
}

fn says(text: &[u8], tokens: u64) -> Turn {
    Turn { lines: Box::new([Line::Text { text: text.into() }]), finish: Finish::Stop, tokens }
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
