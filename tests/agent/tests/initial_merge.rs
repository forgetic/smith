//! A shared fake-git merge genuinely precedes Start. The parent refuses the
//! actual unresolved tree, then commits the LLM's checked resolution with both
//! original parents. No marker fixture or anticipated outcome stands in for git.
//! Contract: domain/run.md, sections 3.2, 8.1–8.4 and 13; testing-strategy.md, section 4.3.

use std::collections::BTreeMap;

use skein_fake_checkout::{
    Checkout,
    git::{self, CommitFailure, Created, Fault, Pushed, Remote, Tree, Want},
};
use skein_fake_llm_domain::api::{Finish, Line, Part, Script, Turn};
use skein_lib::Token;
use smith_agent_world::{Boundary, Job, Settings, World};
use smith_domain::{run, session::llm, tools};

const MARKERS: &[u8] = b"<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs\n";
const READ_CONFLICT: &[u8] = br#"{"path":"conflict.txt"}"#;
// Existing typed fixture rendering uses numeric byte-array Debug, rather than
// an ASCII file body. This is the complete literal continuation expectation.
const READ_RESULT: &[u8] = b"Owned { outcome: Read { content: [60, 60, 60, 60, 60, 60, 60, 32, 111, 117, 114, 115, 10, 111, 117, 114, 115, 10, 61, 61, 61, 61, 61, 61, 61, 10, 116, 104, 101, 105, 114, 115, 10, 62, 62, 62, 62, 62, 62, 62, 32, 116, 104, 101, 105, 114, 115, 10], skipped: 0, lines: 5, total: 5, cut: false } }";

#[derive(Clone, Debug)]
struct Commit {
    parent: Option<u64>,
    merging: Option<u64>,
    tree: Tree,
}

// Only the shared kit's store boundary lives here. This fixture neither merges
// nor checks markers, and keeps no Smith call, check or delivery state.
#[derive(Debug)]
struct Store {
    commits: BTreeMap<u64, Commit>,
}

impl Store {
    fn new() -> Self {
        let mut commits = BTreeMap::new();
        for (id, parent, value) in [(1, None, b"base\n".as_slice()), (2, Some(1), b"ours\n"), (3, Some(1), b"theirs\n")]
        {
            let tree = [
                (b"conflict.txt".to_vec(), value.to_vec()),
                (b"AGENTS.md".to_vec(), b"Resolve the actual conflict.txt before delivery.".to_vec()),
                (b".temper/pre-pr".to_vec(), b"#!checks".to_vec()),
                (b"src/lib.rs".to_vec(), b"pub fn answer() -> u32 { 43 }\n".to_vec()),
            ]
            .into();
            assert!(commits.insert(id, Commit { parent, merging: None, tree }).is_none());
        }
        Self { commits }
    }
}

impl Remote for Store {
    fn heads(&mut self, remote: &[u8]) -> Result<Vec<u64>, Fault> {
        assert_eq!(remote, b"merge-story");
        Ok(vec![2, 3])
    }

    fn fetch(&mut self, _remote: &[u8], _want: Want<'_>) -> Result<u64, Fault> {
        Err(Fault::Refused)
    }

    fn create(&mut self, _remote: &[u8], _branch: &[u8], _commit: u64) -> Result<Created, Fault> {
        Err(Fault::Refused)
    }

    fn push(&mut self, _remote: &[u8], _branch: &[u8], _commit: u64, _expected: Option<u64>) -> Result<Pushed, Fault> {
        Err(Fault::Refused)
    }

    fn parent(&self, commit: u64) -> Option<u64> {
        self.commits[&commit].parent
    }

    fn merge_parent(&self, commit: u64) -> Option<u64> {
        self.commits[&commit].merging
    }

    fn tree(&self, commit: u64) -> Tree {
        self.commits[&commit].tree.clone()
    }

    fn store(&mut self, parent: u64, merging: Option<u64>, tree: Tree) -> Option<u64> {
        assert!(self.commits.contains_key(&parent));
        if let Some(merging) = merging {
            assert!(self.commits.contains_key(&merging));
        }
        if merging.is_none() && tree == self.tree(parent) {
            return None;
        }
        let id = self.commits.last_key_value().expect("finite initial graph").0 + 1;
        assert!(self.commits.insert(id, Commit { parent: Some(parent), merging, tree }).is_none());
        Some(id)
    }
}

fn call(name: &[u8], arguments: &[u8]) -> Line {
    Line::Call { name: name.into(), arguments: arguments.into() }
}

fn calls(lines: Vec<Line>) -> Turn {
    Turn { lines: lines.into(), finish: Finish::ToolCalls, tokens: 1 }
}

fn scripts() -> Box<[Script]> {
    Box::new([
        Script {
            cue: b"@midreport".as_slice().into(),
            turns: Box::new([
                calls(vec![
                    Line::Text { text: b"Try the supplied initial merge.".as_slice().into() },
                    call(b"deliver", br#"{"ticket":"initial-merge"}"#),
                ]),
                calls(vec![call(b"sub_agent", br#"{"brief":"@initial-reader","tools":["inspect"]}"#)]),
                calls(vec![
                    call(b"read", br#"{"path":"conflict.txt"}"#),
                    call(b"read", br#"{"path":"../notes/data.txt"}"#),
                ]),
                calls(vec![call(b"write", br#"{"path":"conflict.txt","content":"resolved by the actual LLM"}"#)]),
                calls(vec![call(b"edit", br#"{"path":"../notes/data.txt","old":"old","new":"delivered-plain"}"#)]),
                calls(vec![
                    Line::Text { text: b"Deliver the checked resolution.".as_slice().into() },
                    call(b"deliver", br#"{"ticket":"resolved-merge"}"#),
                ]),
                calls(vec![call(
                    b"finish",
                    br#"{"report":"Initial merge resolved and plain files delivered.","source":"workspace"}"#,
                )]),
            ]),
        },
        Script {
            cue: b"@initial-reader".as_slice().into(),
            turns: Box::new([
                calls(vec![call(b"read", br#"{"path":"conflict.txt"}"#)]),
                Turn {
                    lines: Box::new([Line::Text {
                        text: b"Initial markers observed without git writes.".as_slice().into(),
                    }]),
                    finish: Finish::Stop,
                    tokens: 1,
                },
            ]),
        },
    ])
}

struct Fixture {
    world: World,
    store: Store,
    readonly_git: Tree,
    readonly_plain: Tree,
    actual_conflicts: Vec<Vec<u8>>,
    roots: [Token; 4],
}

fn setup() -> Fixture {
    let mut store = Store::new();
    let mut disk = Checkout::new();
    git::clone_repository(&mut store, &mut disk, b"merge-story", b"work").expect("shared clone imports both parents");
    git::check_out(&store, &mut disk, b"work", 2).expect("actual local ours");
    let merged = git::merge(&store, &mut disk, b"work", 3).expect("actual merge in progress before Start");
    assert_eq!(merged.conflicts, [b"conflict.txt".to_vec()]);
    assert_eq!(disk.content(b"work/conflict.txt"), Some(MARKERS));
    assert_eq!(store.commits.len(), 3, "merge makes no commit");
    for name in [b"archive".as_slice(), b"notes", b"reference"] {
        disk.mkdir(name);
        disk.write(&[name, b"/AGENTS.md"].concat(), &[b"guide-", name].concat());
        disk.write(&[name, b"/.temper/pre-pr"].concat(), b"#!checks");
        disk.write(&[name, b"/src/lib.rs"].concat(), b"pub fn answer() -> u32 { 43 }\n");
        disk.write(&[name, b"/data.txt"].concat(), b"old");
    }
    disk.write(b"archive/.git/HEAD", b"read-only-git");
    disk.write(b"archive/other-conflict.txt", b"read-only unresolved information");
    let readonly_git = disk.tree(b"archive");
    let readonly_plain = disk.tree(b"reference");
    let roots = [b"work".as_slice(), b"archive", b"notes", b"reference"].map(|name| Token::new(disk.root(name)));
    let actual_conflicts = merged.conflicts;
    let workspace = run::Workspace {
        directories: Box::new([
            run::Directory {
                name: b"work".as_slice().into(),
                root: roots[0],
                writable: true,
                git: true,
                conflicts: actual_conflicts.iter().map(|path| path.as_slice().into()).collect(),
            },
            run::Directory {
                name: b"archive".as_slice().into(),
                root: roots[1],
                writable: false,
                git: true,
                conflicts: Box::new([b"other-conflict.txt".as_slice().into()]),
            },
            run::Directory {
                name: b"notes".as_slice().into(),
                root: roots[2],
                writable: true,
                git: false,
                conflicts: Box::new([]),
            },
            run::Directory {
                name: b"reference".as_slice().into(),
                root: roots[3],
                writable: false,
                git: false,
                conflicts: Box::new([]),
            },
        ]),
    };
    let mut settings = Settings { job: Job::MidReport, ..Settings::calm(1003) };
    settings.limits.run.directories = 4;
    settings.limits.session.tools.repos = 4;
    let mut world = World::with_workspace_scripts(settings, None, Some(workspace), disk, scripts());
    world.enable_parent_deliveries();
    Fixture { world, store, readonly_git, readonly_plain, actual_conflicts, roots }
}

fn submitted(world: &mut World, count: usize) -> Token {
    for _ in 0..10_000 {
        assert!(!world.drive(1));
        if world.delivery_submissions().len() == count {
            return world.delivery_submissions()[count - 1].owner;
        }
    }
    panic!("actual initial-merge delivery {count} never arrived");
}

fn contains(bytes: &[u8], part: &[u8]) -> bool {
    bytes.windows(part.len()).any(|bytes| bytes == part)
}

fn conflict_read(turn: &smith_domain::Turn) -> bool {
    let calls = turn
        .messages
        .iter()
        .filter(|message| message.role == llm::Role::Assistant)
        .flat_map(|message| &message.content)
        .filter_map(|block| {
            if let llm::Block::ToolCall { id, name, input, call: llm::Decoded::Historical, .. } = block
                && name.as_ref() == b"read"
                && input.as_ref() == READ_CONFLICT
            {
                return Some(id);
            }
            None
        })
        .collect::<Vec<_>>();
    let [id] = calls.as_slice() else { return false };
    turn.messages
        .iter()
        .filter(|message| message.role == llm::Role::User)
        .flat_map(|message| &message.content)
        .filter(|block| {
            matches!(block,
            llm::Block::ToolResult { id: returned_id, result: llm::Returned::Owned {
                outcome: tools::Outcome::Read { content, skipped: 0, lines: 5, total: 5, cut: false }
            }} if returned_id == *id && content.as_ref() == MARKERS)
        })
        .count()
        == 1
}

fn assert_conflict_read(world: &World, root: Token) {
    let turn = world.turns().iter().find(|turn| turn.sequence == 3).expect("actual main read completion");
    assert!(conflict_read(turn), "actual main call/result pair contains all five unresolved lines: {turn:?}");
    for field in 0..3 {
        let mut corrupted = turn.clone();
        for block in corrupted.messages.iter_mut().flat_map(|message| &mut message.content) {
            if let llm::Block::ToolResult {
                id,
                result: llm::Returned::Owned { outcome: tools::Outcome::Read { content, .. } },
            } = block
                && content.as_ref() == MARKERS
            {
                match field {
                    0 => content[0] = b'X',
                    1 => *id = b"invented-read-result".as_slice().into(),
                    2 => {}
                    _ => unreachable!("three independent record corruptions"),
                }
            }
            if let llm::Block::ToolCall { input, .. } = block
                && field == 2
                && input.as_ref() == READ_CONFLICT
            {
                *input = br#"{"path":"invented-file"}"#.as_slice().into();
            }
        }
        assert!(!conflict_read(&corrupted), "content/result identity/call path corruption {field}");
    }
    let reads = world
        .boundaries()
        .iter()
        .filter(|(at, boundary)| {
            matches!(boundary,
        Boundary::Io { op: tools::Op::Load { at: place, .. } }
            if place.root == root && place.path.as_ref() == b"conflict.txt"
                && *at > world.delivery_submissions()[0].at && *at < world.delivery_submissions()[1].at)
        })
        .count();
    assert_eq!(reads, 2, "child and main issue actual loads after refusal and before resolved delivery");
    assert!(
        world
            .prompts()
            .iter()
            .filter(|query| query.system.starts_with(b"## Task\n\n@midreport"))
            .flat_map(|query| &query.messages)
            .flat_map(|message| &message.parts)
            .any(|part| matches!(part,
            Part::ToolOutput { output, is_error: false, .. } if output.as_ref() == READ_RESULT)),
        "the typed fixture renders byte content through Debug, preserving the actual complete read"
    );
}

#[test]
fn initial_merge_refusal_resolution_checks_and_two_parent_commit() {
    let Fixture { mut world, mut store, readonly_git, readonly_plain, actual_conflicts, roots } = setup();
    let first = submitted(&mut world, 1);
    assert_eq!(world.checked(), [true, true]);
    assert_eq!(world.delivery_submissions()[0].name, run::CallName { activation: 1, completion: 1, position: 1 });
    let unresolved = git::commit_merging(&mut store, world.delivery_checkout(first), b"work", 2, 3);
    assert_eq!(unresolved, Err(CommitFailure::Unresolved { files: actual_conflicts.clone() }));
    assert_eq!(store.commits.len(), 3, "failed actual commit creates no object");
    let Err(CommitFailure::Unresolved { files }) = unresolved else { panic!("actual remaining marker paths") };
    let refusal = run::DeliveryRefusal::new(
        Some(run::Marker::new(0, files[0].as_slice().into()).expect("actual shared-git relative conflict")),
        b"initial merge still has actual markers".as_slice().into(),
    )
    .expect("bounded actual host refusal");
    world.return_delivery(first, run::Delivery::Refused(refusal.clone())).expect("one actual refusal terminal");
    let second = submitted(&mut world, 2);
    assert_eq!(world.delivery_submissions()[1].name, run::CallName { activation: 1, completion: 6, position: 1 });
    assert_eq!(world.checked(), [true, true, true, true], "both writable checks rerun after actual resolution");
    assert_eq!(world.disk().content(b"work/conflict.txt"), Some(b"resolved by the actual LLM".as_slice()));
    assert_eq!(world.disk().content(b"notes/data.txt"), Some(b"delivered-plain".as_slice()));
    let commit = git::commit_merging(&mut store, world.delivery_checkout(second), b"work", 2, 3)
        .expect("actual shared merge commit after checked resolution");
    assert_eq!(store.parent(commit), Some(2));
    assert_eq!(store.merge_parent(commit), Some(3));
    assert_eq!(store.tree(commit).get(b"conflict.txt".as_slice()), Some(&b"resolved by the actual LLM".to_vec()));
    assert!(!world.disk().exists(b"work/.git/MERGE_HEAD"));
    assert_eq!(world.disk().content(b"work/.git/temper-head"), Some(commit.to_le_bytes().as_slice()));
    assert_eq!(world.disk().tree(b"archive"), readonly_git);
    assert_eq!(world.disk().tree(b"reference"), readonly_plain);
    assert!(!world.disk().exists(b"notes/.git") && !world.disk().exists(b"reference/.git"));
    let receipt_text = format!("work: merge commit {commit}, parents 2 and 3").into_bytes();
    let receipts = run::Delivered::new(Box::new([
        run::Receipt::new(0, receipt_text.clone().into()).expect("actual merge commit receipt"),
        run::Receipt::new(2, b"notes: delivered-plain files kept".as_slice().into())
            .expect("actual changed plain-directory receipt"),
    ]))
    .expect("exact complete changed writable directory set");
    world
        .return_delivery(second, run::Delivery::Delivered(receipts.clone()))
        .expect("actual merge/plain landing terminal");
    world.run(10_000);
    assert_eq!(world.pushes(), [run::Delivery::Refused(refusal), run::Delivery::Delivered(receipts)]);
    assert!(matches!(world.answer(), run::Answer::Accepted { outcome: run::outcome::Declared::Report(report), .. }
        if report.text.as_ref() == b"Initial merge resolved and plain files delivered."));
    let checks = world
        .boundaries()
        .iter()
        .filter_map(|(_, boundary)| match boundary {
            Boundary::Check { program } => Some(program.root),
            Boundary::Read { .. } | Boundary::Probe { .. } | Boundary::Io { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(checks, [roots[0], roots[2], roots[0], roots[2]]);
    for query in world.prompts() {
        assert!(contains(&query.system, b"with initial merge conflicts: `conflict.txt`"));
        assert!(
            contains(&query.system, b"with initial merge conflicts: `other-conflict.txt`"),
            "readonly git conflict is informative"
        );
        assert!(contains(&query.system, b"`notes`, which you may change, a plain directory"));
        assert!(contains(&query.system, b"`reference`, which you may only read, a plain directory"));
    }
    let child =
        world.prompts().iter().find(|query| query.system.starts_with(b"@initial-reader")).expect("actual child opened");
    assert!(child.tools.iter().all(|tool| [b"read".as_slice(), b"list", b"search"].contains(&tool.name.as_ref())));
    let main =
        world.prompts().iter().filter(|query| query.system.starts_with(b"## Task\n\n@midreport")).collect::<Vec<_>>();
    assert!(main.iter().any(|query| query.messages.iter().flat_map(|message| &message.parts).any(|part|
        matches!(part, Part::ToolOutput { output, is_error: true, .. }
            if output.as_ref() == b"delivery-refused explanation=\"initial merge still has actual markers\" marker-directory=0 marker-path=\"conflict.txt\""))),
        "actual marker refusal reaches the LLM unchanged");
    assert_conflict_read(&world, roots[0]);
    assert!(main.iter().any(|query| query.messages.iter().flat_map(|message| &message.parts).any(|part|
        matches!(part, Part::ToolOutput { output, is_error: false, .. }
            if contains(output, &receipt_text) && contains(output, b"receipt directory=0") && contains(output, b"receipt directory=2")))),
        "complete actual git/plain receipt names reach the continuation");
}
