//! The shared harness owns every loop, hosted admission and descriptor lifecycle.
//! This module supplies scenario configuration and the fake machine's filesystem
//! and git face, never process routing (testing.md, sections 2.1, 3.2 and 5).
use crate::{
    process::{self, Launch, Proc},
    referee::{CheckoutRead, Ending, Run},
    terminal::{Action, Terminal},
};
use skein_fake_checkout::git::{Remote, Tree};
use skein_fake_checkout::{Checkout, git};
use skein_fake_llm_domain::api;
use skein_fake_machine::{How, Machine as Filesystem, Opened};
use skein_io::kernel;
use skein_lib::{Duration, Queue, Wall};
use skein_world::{HostedProgram, Memory, Outcome};
use smith_local_domain as local;
use smith_local_shell::{local_host::token_limits, local_tokens};
use smith_local_world::git::History;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Both placements enter the same shell and local policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Spawned,
    InProcess,
}

/// Credential state at the next shell startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authentication {
    SignIn,
    Refresh,
    Refused,
    Proactive,
}

/// Files surviving separate invocations of the actual shared shell.
pub struct Files {
    path: PathBuf,
}

impl Files {
    #[must_use]
    pub fn new() -> Self {
        let index = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("smith-local-process-{}-{index}", std::process::id()));
        std::fs::create_dir_all(&path).expect("fixture directory");
        Self { path }
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Default for Files {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn scripts() -> Box<[api::Script]> {
    let mut turns = Vec::new();
    for answer in ["First answer", "Second answer", "Third answer"] {
        turns.push(api::Turn {
            lines: Box::new([api::Line::Call { name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() }]),
            finish: api::Finish::ToolCalls,
            tokens: 1,
        });
        turns.push(api::Turn {
            lines: Box::new([api::Line::Call {
                name: b"finish".as_slice().into(),
                arguments: format!("{{\"form\":\"report\",\"text\":\"{answer}\",\"fields\":{{}}}}").into_bytes().into(),
            }]),
            finish: api::Finish::ToolCalls,
            tokens: 1,
        });
    }
    Box::new([api::Script { cue: b"@local-shell".as_slice().into(), turns: turns.into() }])
}

fn change_scripts() -> Box<[api::Script]> {
    let turns = [
        ("wait", "{}"),
        ("write", "{\"path\":\"result.txt\",\"content\":\"new\\n\"}"),
        (
            "finish",
            "{\"form\":\"change\",\"text\":\"\",\"fields\":{\"title\":\"Updated result\",\"body\":\"Created the result file\"}}",
        ),
    ];
    Box::new([api::Script {
        cue: b"@local-shell".as_slice().into(),
        turns: turns
            .into_iter()
            .map(|(name, arguments)| api::Turn {
                lines: Box::new([api::Line::Call {
                    name: name.as_bytes().into(),
                    arguments: arguments.as_bytes().into(),
                }]),
                finish: api::Finish::ToolCalls,
                tokens: 1,
            })
            .collect(),
    }])
}

// Shared scenario data: any tier can select these same provider turns.
fn tool_scripts() -> Box<[api::Script]> {
    let turns = [
        ("wait", "{}"),
        ("read", r#"{"path":"original.txt"}"#),
        ("search", r#"{"pattern":"original","path":"original.txt"}"#),
        ("edit", r#"{"path":"original.txt","old":"original","new":"edited"}"#),
        ("write", r#"{"path":"result.txt","content":"new\n"}"#),
        ("shell", r#"{"command":"printf 'ran\n' > command.txt"}"#),
        (
            "finish",
            r#"{"form":"change","text":"","fields":{"title":"Updated result","body":"Created the result file"}}"#,
        ),
    ];
    Box::new([api::Script {
        cue: b"@local-shell".as_slice().into(),
        turns: turns
            .into_iter()
            .map(|(name, arguments)| api::Turn {
                lines: Box::new([api::Line::Call {
                    name: name.as_bytes().into(),
                    arguments: arguments.as_bytes().into(),
                }]),
                finish: api::Finish::ToolCalls,
                tokens: 1,
            })
            .collect(),
    }])
}

/// Checkout observations shared only by the machine and outside referee.
#[derive(Clone)]
pub struct CheckoutView(Rc<RefCell<Option<(Checkout, History, u64)>>>);
impl CheckoutRead for CheckoutView {
    fn head(&self) -> Option<Vec<u8>> {
        self.0.borrow().as_ref().map(|(_, _, head)| format!("{head:040x}").into_bytes())
    }
    fn message(&self, commit: &[u8]) -> Vec<u8> {
        let commit = u64::from_str_radix(std::str::from_utf8(commit).expect("head"), 16).expect("object");
        self.0.borrow().as_ref().expect("checkout").1.commit_message(commit).to_vec()
    }
    fn files(&self, commit: &[u8]) -> Tree {
        let commit = u64::from_str_radix(std::str::from_utf8(commit).expect("head"), 16).expect("object");
        self.0.borrow().as_ref().expect("checkout").1.tree(commit)
    }
}
/// The fake disk and checkout, outside every process heap.
pub struct Machine {
    files: Filesystem,
    master: Opened,
    checkout: CheckoutView,
    commands: Vec<Box<[Box<[u8]>]>>,
}
impl Machine {
    fn current_tree(&mut self) -> Tree {
        let root = self.files.open(self.master, b"repo", How::Directory).expect("repo");
        let mut pending = vec![(root, Vec::<u8>::new())];
        let mut tree = Tree::new();
        while let Some((directory, prefix)) = pending.pop() {
            for (kind, name) in self.files.list(directory, 32, 4096).expect("bounded fixture tree") {
                if name.as_ref() == b".git" {
                    continue;
                }
                let path = [prefix.as_slice(), name.as_ref()].concat();
                match kind {
                    skein_fake_machine::Is::Directory => {
                        let opened = self.files.open(directory, &name, How::Directory).expect("directory");
                        pending.push((opened, [path.as_slice(), b"/"].concat()));
                    }
                    skein_fake_machine::Is::File => {
                        let opened = self.files.open(directory, &name, How::Read).expect("file");
                        tree.insert(path, self.files.read(opened, 0, 8192).expect("fixture bytes"));
                        self.files.close(opened);
                    }
                    _ => panic!("fixture uses regular files only"),
                }
            }
            self.files.close(directory);
        }
        tree
    }

    fn git_reply(&mut self, args: &[Box<[u8]>]) -> (Box<[u8]>, u8) {
        let tree = self.current_tree();
        let mut shared = self.checkout.0.borrow_mut();
        let (checkout, history, head) = shared.as_mut().expect("git fixture");
        checkout.replace_tree(b"work", &tree);
        let verb = args.first().expect("git verb").as_ref();
        let bytes = match verb {
            b"rev-parse" if args.len() == 2 => format!("{head:040x}\n").into_bytes(),
            b"rev-parse" => return (Box::new([]), 1),
            b"status" => {
                let previous = history.tree(*head);
                let mut changed = Vec::new();
                for (path, bytes) in &tree {
                    if previous.get(path) != Some(bytes) {
                        changed.extend_from_slice(b" M ");
                        changed.extend_from_slice(path);
                        changed.push(0);
                    }
                }
                for path in previous.keys() {
                    if !tree.contains_key(path) {
                        changed.extend_from_slice(b" D ");
                        changed.extend_from_slice(path);
                        changed.push(0);
                    }
                }
                changed
            }
            b"add" => Vec::new(),
            b"-c" => {
                *head = git::commit(history, checkout, b"work", *head, args.last().expect("commit message"))
                    .expect("local commit parent")
                    .expect("changed tree");
                Vec::new()
            }
            b"log" => {
                let pattern = &args[3];
                let trailer = &pattern[b"--grep=^".len()..pattern.len() - 1];
                let mut at = Some(*head);
                let mut found = Vec::new();
                while let Some(commit) = at {
                    let message = history.commit_message(commit);
                    if message.split(|byte| *byte == b'\n').any(|line| line == trailer) {
                        found = format!("{commit:040x}\0").into_bytes();
                        found.extend_from_slice(message);
                        found.push(b'\n');
                        break;
                    }
                    at = history.parent(commit);
                }
                found
            }
            b"push" => panic!("push was not configured"),
            _ => panic!("unexpected git command {args:?}"),
        };
        (bytes.into(), 0)
    }
}
impl skein_world::Machine for Machine {
    fn open_root(&mut self, path: &[u8]) -> Result<skein_sim::Handle, kernel::Error> {
        if let Some(arguments) = path.strip_prefix(b".smith-test/git/") {
            assert!(self.commands.len() < 128, "bounded git commands");
            let arguments: Box<[Box<[u8]>]> = serde_json::from_slice(arguments).expect("exact git arguments");
            let (bytes, code) = self.git_reply(&arguments);
            self.commands.push(arguments);
            let name = format!("git-{}", self.commands.len()).into_bytes();
            let temporary =
                self.files.open(self.master, b".smith-test", How::Directory).expect("fake result namespace");
            self.files.make_directory(temporary, &name).expect("fresh result directory");
            let root = self.files.open(temporary, &name, How::Directory).expect("result root");
            self.files.close(temporary);
            let file = self.files.open(root, b"result", How::Create { mode: 0o600 }).expect("result file");
            let mut result = vec![code];
            result.extend_from_slice(&bytes);
            self.files.write(file, 0, &result).expect("bounded result");
            self.files.close(file);
            Ok(skein_sim::Handle::new(root.raw()))
        } else {
            self.files
                .open(self.master, path, How::Directory)
                .map(|root| skein_sim::Handle::new(root.raw()))
                .map_err(|_| kernel::Error::NotFound)
        }
    }
    fn close_root(&mut self, root: skein_sim::Handle) {
        self.files.close(Opened::new(root.raw()));
    }
    fn step(&mut self, call: skein_sim::Call, answers: &mut Queue<skein_sim::Answer>) {
        skein_fake_machine::step(&mut self.files, call, answers);
    }
}

/// A reusable story and its immutable hosted-launch settings.
pub struct Scenario {
    pub launch: Launch,
    pub commands: Vec<Action>,
    pub ending: Ending,
    pub interrupt_on_query: bool,
    pub authentication: Option<Authentication>,
}
/// Simulated configuration and the settled shared-harness result.
pub struct World {
    pub scenario: Scenario,
    machine: Option<Machine>,
    checkout: CheckoutView,
    memory: Memory,
    outcome: Option<Outcome<Proc, Machine>>,
    commit_message: Vec<u8>,
    first_post: Option<Wall>,
    saved_before_query: bool,
}
impl World {
    #[must_use]
    pub fn new(seed: u64, files: &Files, placement: Placement, answer: &[u8]) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![Action::Send(b"hello\n".as_slice().into()), Action::Wait(answer.into()), Action::Eof],
            Ending::Report(answer.to_vec()),
            (false, true, false),
        )
    }
    #[must_use]
    pub fn interrupted(seed: u64, files: &Files, placement: Placement) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![
                Action::Send(b"hello\n".as_slice().into()),
                Action::Wait(b"Run cancelled".as_slice().into()),
                Action::Eof,
            ],
            Ending::Cancelled,
            (true, true, false),
        )
    }
    #[must_use]
    pub fn without_facts(seed: u64, files: &Files, placement: Placement, answer: &[u8]) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![Action::Send(b"hello\n".as_slice().into()), Action::Wait(answer.into()), Action::Eof],
            Ending::Report(answer.to_vec()),
            (false, false, false),
        )
    }
    #[must_use]
    pub fn changed(seed: u64, files: &Files, placement: Placement) -> Self {
        Self::configured(
            seed,
            files,
            placement,
            vec![
                Action::Send(b"change it\n".as_slice().into()),
                Action::Wait(b"Change delivered".as_slice().into()),
                Action::Eof,
            ],
            Ending::Change { before: format!("{:040x}", 1).into_bytes() },
            (false, true, true),
        )
    }
    fn configured(
        seed: u64,
        files: &Files,
        placement: Placement,
        commands: Vec<Action>,
        ending: Ending,
        behavior: (bool, bool, bool),
    ) -> Self {
        let (interrupt_on_query, keep_facts, change) = behavior;
        let mut disk = Filesystem::new();
        let mut items = vec![skein_fake_machine::Item::directory(b".smith-test")];
        if change {
            items.push(skein_fake_machine::Item::directory(b"repo"));
            items.push(skein_fake_machine::Item::file(b"repo/original.txt", b"original\n"));
        }
        let master = disk.lay(&items);
        let checkout = CheckoutView(Rc::new(RefCell::new(if change {
            let mut history = History::new(Tree::from([(b"original.txt".to_vec(), b"original\n".to_vec())]));
            let mut checkout = Checkout::new();
            git::clone_repository(&mut history, &mut checkout, b"repo", b"work").expect("clone fixture");
            git::check_out(&history, &mut checkout, b"work", 1).expect("checkout fixture");
            Some((checkout, history, 1))
        } else {
            None
        })));
        let token_directory = files.path.join("tokens");
        let tokens = local_tokens::Tokens::new(&token_directory, token_limits()).expect("token directory");
        if tokens.load(0).expect("token record").is_none() {
            let saved = skein_oauth::SavedToken {
                key: 0,
                generation: 1,
                access_token: b"token".as_slice().into(),
                refresh_token: Some(b"refresh".as_slice().into()),
                metadata: None,
                expires_at: Wall::from_nanos(
                    skein_tls_world::pki::VALID.as_nanos() + Duration::from_secs(7200).as_nanos(),
                ),
            };
            tokens
                .save(0, &skein_oauth::encode_record(&saved, &token_limits()).expect("token fixture"))
                .expect("durable token");
        }
        Self {
            scenario: Scenario {
                launch: Launch {
                    seed,
                    tls: false,
                    trust_der: None,
                    change,
                    tools: false,
                    in_process: placement == Placement::InProcess,
                    keep_facts,
                    authenticated: false,
                    state_directory: files.path.join("chat"),
                    token_directory,
                    root_path: ".".into(),
                },
                commands,
                ending,
                interrupt_on_query,
                authentication: None,
            },
            machine: Some(Machine { files: disk, master, checkout: checkout.clone(), commands: Vec::new() }),
            checkout,
            memory: Memory::Unchecked,
            outcome: None,
            commit_message: Vec::new(),
            first_post: None,
            saved_before_query: false,
        }
    }
    #[must_use]
    pub fn authenticated(seed: u64, files: &Files, placement: Placement, mode: Authentication) -> Self {
        let mut world = Self::new(
            seed,
            files,
            placement,
            if mode == Authentication::Refused { b"account is unavailable" } else { b"First answer" },
        );
        world.scenario.launch.authenticated = true;
        world.scenario.authentication = Some(mode);
        if mode == Authentication::Refused {
            world.scenario.ending = Ending::Unavailable;
        }
        let tokens =
            local_tokens::Tokens::new(&world.scenario.launch.token_directory, token_limits()).expect("private store");
        if mode == Authentication::SignIn {
            std::fs::remove_file(world.scenario.launch.token_directory.join("0.json")).expect("no saved token");
        } else {
            let saved = skein_oauth::SavedToken {
                key: 0,
                generation: 1,
                access_token: if mode == Authentication::Proactive {
                    b"access-new".as_slice().into()
                } else {
                    b"old".as_slice().into()
                },
                refresh_token: Some(if mode == Authentication::Refused {
                    b"wrong".as_slice().into()
                } else {
                    b"refresh-old".as_slice().into()
                }),
                metadata: None,
                expires_at: Wall::from_nanos(
                    skein_tls_world::pki::VALID.as_nanos()
                        + if mode == Authentication::Proactive { Duration::from_secs(61).as_nanos() } else { 0 },
                ),
            };
            tokens
                .save(0, &skein_oauth::encode_record(&saved, &token_limits()).expect("prior token"))
                .expect("expired grant");
        }
        world
    }
    /// Use the shared per-process construction, iteration and drop meter.
    pub fn check_memory(&mut self) {
        self.memory = Memory::Checked;
    }
    pub fn settle(&mut self) {
        let launch = &self.scenario.launch;
        let mut config = skein_sim::Config::calm();
        config.wall = skein_tls_world::pki::VALID;
        if launch.seed >= 100 {
            config.faults.latency = 300;
            config.faults.latency_max = Duration::from_millis(1);
            config.faults.short_recv = 500;
            config.faults.short_send = 500;
            config.faults.short_read = 500;
            config.faults.short_write = 500;
            config.faults.cancel_race = 500;
        }
        let mut machine = self.machine.take().expect("one invocation");
        let opened = machine.files.open_handles();
        assert_eq!(opened, 1);
        let terminal_root = machine.files.open(machine.master, b".", How::Directory).expect("terminal root");
        let referee = Run::new(
            launch.seed,
            self.scenario.ending.clone(),
            Box::new(self.checkout.clone()),
            launch.token_directory.clone(),
            self.scenario.interrupt_on_query,
            launch.keep_facts,
        );
        let mut world = skein_world::World::new(launch.seed, config, referee, self.memory).with_machine(machine);
        world.host_roots(
            HostedProgram {
                program: b"smith-local".as_slice().into(),
                make: process::make_local,
                instances: 1,
                operations: 2048,
            },
            process::local_roots,
        );
        world.host_roots(
            HostedProgram {
                program: b"smith-agent".as_slice().into(),
                make: process::make_agent,
                instances: 8,
                operations: process::lower_configuration().limits.routes + 3,
            },
            process::agent_roots,
        );
        world.host_roots(
            HostedProgram {
                program: b"/usr/bin/git".as_slice().into(),
                make: crate::git::make,
                instances: 1,
                operations: 4,
            },
            crate::git::roots,
        );
        // The terminal owns a fresh handle, never the machine's master.
        world.spawn_root(skein_sim::Handle::new(terminal_root.raw()), |root| {
            Proc::Terminal(Box::new(Terminal::configured(root, self.scenario.commands.clone(), launch.arguments())))
        });
        world.spawn(|| Proc::Peer(Box::new(self.scenario.provider())));
        if launch.authenticated {
            world.spawn(|| {
                Proc::Issuer(Box::new(process::IssuerProcess::configured(if launch.tls {
                    skein_fake_peers::Transport::Tls
                } else {
                    skein_fake_peers::Transport::Plaintext
                })))
            });
            world.spawn(|| Proc::Browser(Box::default()));
        }
        self.outcome = Some(world.run());
        let head = self.checkout.head();
        self.commit_message = head.as_ref().map_or_else(Vec::new, |head| self.checkout.message(head));
        // The shared referee's observed values are retained by the peers.
        self.first_post = self.issuer().and_then(process::IssuerProcess::first_post);
        self.saved_before_query = self.scenario.launch.authenticated && !self.queries().is_empty();
    }
    fn procs(&self) -> &[Proc] {
        &self.outcome.as_ref().expect("settled world").procs
    }
    fn terminal(&self) -> &Terminal {
        self.procs()
            .iter()
            .find_map(|p| if let Proc::Terminal(p) = p { Some(p.as_ref()) } else { None })
            .expect("terminal")
    }
    fn peer(&self) -> &crate::llm::Peer {
        self.procs().iter().find_map(|p| if let Proc::Peer(p) = p { Some(p.as_ref()) } else { None }).expect("peer")
    }
    fn issuer(&self) -> Option<&process::IssuerProcess> {
        self.procs().iter().find_map(|p| if let Proc::Issuer(p) = p { Some(p.as_ref()) } else { None })
    }
    fn browser(&self) -> Option<&process::BrowserProcess> {
        self.procs().iter().find_map(|p| if let Proc::Browser(p) = p { Some(p.as_ref()) } else { None })
    }
    #[must_use]
    pub fn commit_message(&self) -> &[u8] {
        &self.commit_message
    }
    #[must_use]
    pub fn commit_files(&self) -> Tree {
        self.checkout.head().as_ref().map_or_else(Tree::new, |head| self.checkout.files(head))
    }
    #[must_use]
    pub fn git_commands(&self) -> &[Box<[Box<[u8]>]>] {
        &self.outcome.as_ref().expect("settled world").machine.commands
    }
    #[must_use]
    pub fn first_token_post(&self) -> Option<Wall> {
        self.first_post
    }
    #[must_use]
    pub fn oauth_posts(&self) -> u64 {
        self.issuer().map_or(0, |issuer| crate::oauth::posts(issuer.peer()))
    }
    #[must_use]
    pub fn page_visits(&self) -> u32 {
        self.browser().map_or(0, process::BrowserProcess::pages)
    }
    #[must_use]
    pub fn browser_replied(&self) -> bool {
        self.browser().is_some_and(process::BrowserProcess::replied)
    }
    #[must_use]
    pub fn saved_before_query(&self) -> bool {
        self.saved_before_query
    }
    #[must_use]
    pub fn shown(&self) -> &[u8] {
        self.terminal().shown()
    }
    #[must_use]
    pub fn queries(&self) -> Vec<api::Query> {
        crate::llm::queries(self.peer()).cloned().collect()
    }
    #[must_use]
    pub fn facts(&self) -> &[local::Fact] {
        self.procs().iter().find_map(|p| if let Proc::Local(p) = p { Some(p.facts()) } else { None }).expect("local")
    }
    #[must_use]
    pub fn exit(&self) -> Option<kernel::Exit> {
        self.terminal().exit()
    }
    #[must_use]
    pub fn trace(&self) -> Vec<String> {
        self.outcome.as_ref().expect("settled world").trace.iter().map(|entry| format!("{entry:?}")).collect()
    }
    #[must_use]
    pub fn steps(&self) -> u32 {
        self.outcome.as_ref().expect("settled world").iterations
    }
    #[must_use]
    pub fn heaps(&self) -> &[(u64, u64)] {
        self.outcome.as_ref().expect("settled world").heap.as_ref().expect("memory checked")
    }
    #[must_use]
    pub fn seen(&self) -> crate::referee::Seen {
        crate::referee::seen(
            self.procs(),
            self.saved_before_query,
            self.git_commands().iter().any(|args| args[0].as_ref() == b"push"),
        )
    }
    #[must_use]
    pub fn turns(&mut self) -> usize {
        let store = smith_local_shell::local_store::Store::new(
            self.scenario.launch.state_directory.clone(),
            process::lower_configuration().channel_endpoints,
            1 << 20,
        )
        .expect("outside durable store");
        let local::Event::Loaded { transcript, .. } = store.load().expect("durable history") else { unreachable!() };
        transcript.map_or(0, |history| history.turns.len())
    }
}
impl CheckoutRead for World {
    fn head(&self) -> Option<Vec<u8>> {
        self.checkout.head()
    }
    fn message(&self, commit: &[u8]) -> Vec<u8> {
        self.checkout.message(commit)
    }
    fn files(&self, commit: &[u8]) -> Tree {
        self.checkout.files(commit)
    }
}
impl Scenario {
    /// Provider scripts and credentials are scenario data, shared across tiers.
    #[must_use]
    pub fn provider(&self) -> crate::llm::Peer {
        smith_agent_process_world::fake::configured_transport(
            if self.launch.tools {
                tool_scripts()
            } else if self.launch.change {
                change_scripts()
            } else {
                scripts()
            },
            skein_llm::Credential {
                access_token: if self.launch.authenticated {
                    b"access-new".as_slice().into()
                } else {
                    b"token".as_slice().into()
                },
                account_id: b"acc".as_slice().into(),
            },
            if self.interrupt_on_query {
                Duration::from_secs(3600)
            } else if self.authentication == Some(Authentication::Proactive) {
                Duration::from_secs(2)
            } else {
                Duration::ZERO
            },
            if self.launch.tls { skein_fake_peers::Transport::Tls } else { skein_fake_peers::Transport::Plaintext },
        )
    }
}
