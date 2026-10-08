//! Smith's shipped-binary stories: the shared person, browser, peers and scratch
//! over Skein's real loop (testing.md, sections 2.3 and 5). Only outside
//! observations are kept; no local or agent state is available.

use skein_io::kernel::{Complete, Exit, Submit};
use skein_lib::{Duration, Queue, Time, Wall};
use skein_shell::Clock;
use skein_world::{
    Host, Referee,
    end_to_end::{Binary, Command, Mode, Streams},
    real,
};
use smith_local_process_world::{
    process::{self, Proc},
    referee::{self, Seen},
    terminal::Terminal,
    world::Scenario,
};
use smith_real_world::{
    Scratch,
    settled::{Before, assert_no_children},
};
use std::path::Path;

pub enum Process {
    Binary(Binary),
    Script(Proc),
}
impl Process {
    fn host(&self) -> &dyn Host {
        match self {
            Self::Binary(p) => p,
            Self::Script(p) => p,
        }
    }
    fn host_mut(&mut self) -> &mut dyn Host {
        match self {
            Self::Binary(p) => p,
            Self::Script(p) => p,
        }
    }
}
impl Host for Process {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.host_mut().iterate(now, wall);
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        self.host_mut().completions()
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        self.host_mut().submissions()
    }
    fn work_pending(&self, now: Time) -> bool {
        self.host().work_pending(now)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.host().next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.host().is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.host().worst_case()
    }
    fn operations(&self) -> u32 {
        self.host().operations()
    }
}
struct Judge {
    now: Time,
    deadline: Time,
    saved: bool,
    tokens: std::path::PathBuf,
}
impl Referee<Process> for Judge {
    fn act(&mut self, _now: Time, procs: &mut [Process]) {
        let exit = procs.iter().find_map(|p| if let Process::Binary(p) = p { p.exit_status() } else { None });
        let url = procs.iter().find_map(|p| {
            if let Process::Script(Proc::Terminal(p)) = p {
                let shown = p.shown();
                shown.windows(9).position(|s| s == b"Sign in: ").and_then(|at| {
                    let rest = &shown[at + 9..];
                    rest.iter().position(|b| *b == b'\n').map(|end| rest[..end].trim_ascii_end().to_vec())
                })
            } else {
                None
            }
        });
        let finished_turn = procs.iter().any(
            |p| matches!(p, Process::Script(Proc::Terminal(p)) if p.shown().windows(12).any(|b| b == b"Tool: finish")),
        );
        for proc in procs {
            match proc {
                Process::Script(Proc::Terminal(p)) => {
                    if let Some(exit) = exit {
                        p.finished(exit);
                    }
                }
                Process::Script(Proc::Peer(p)) if exit.is_some() || finished_turn => p.shutdown(),
                Process::Script(Proc::Issuer(p)) if exit.is_some() => p.shutdown(),
                Process::Script(Proc::Browser(p)) => {
                    if p.pages() == 0
                        && let Some(url) = &url
                    {
                        p.visit(url);
                    }
                    if exit.is_some() {
                        p.shutdown();
                    }
                }
                _ => {}
            }
        }
    }
    fn observe(&mut self, now: Time, procs: &[Process]) {
        self.now = now;
        if now >= self.deadline {
            for proc in procs {
                match proc {
                    Process::Binary(p) => eprintln!(
                        "binary exit={:?} empty={} stderr={}",
                        p.exit_status(),
                        p.is_empty(),
                        String::from_utf8_lossy(p.stderr())
                    ),
                    Process::Script(Proc::Terminal(p)) => {
                        eprintln!("person empty={} shown={}", p.is_empty(), String::from_utf8_lossy(p.shown()))
                    }
                    Process::Script(Proc::Peer(p)) => {
                        eprintln!("queries={:?}", smith_local_process_world::llm::queries(p).count())
                    }
                    _ => {}
                }
            }
        }
        let authenticated = procs.iter().any(|p| matches!(p, Process::Script(Proc::Issuer(_))));
        if authenticated && !self.saved && procs.iter().any(|p| matches!(p, Process::Script(Proc::Peer(p)) if smith_local_process_world::llm::queries(p).next().is_some())) {
            let store = smith::local_tokens::Tokens::new(&self.tokens, smith::local_host::token_limits()).expect("private tokens");
            assert_eq!(store.load(0).expect("token read").expect("saved before grant use").access_token.as_ref(), b"access-new");
            self.saved = true;
        }
        for proc in procs {
            if let Process::Binary(p) = proc
                && let Some(exit) = p.exit_status()
            {
                assert_eq!(exit, Exit::Code(0), "shipped local failed: {}", String::from_utf8_lossy(p.stderr()));
            }
        }
    }
    fn next_deadline(&self) -> Option<Time> {
        Some(self.now.saturating_add(Duration::from_millis(1)).min(self.deadline))
    }
    fn overdue(&self, now: Time) -> Option<String> {
        (now >= self.deadline).then(|| "shipped Smith binary story did not settle".into())
    }
    fn passed(&self) -> bool {
        true
    }
}

pub fn start(arguments: Vec<std::ffi::OsString>, directory: &Path, mode: Mode) -> Binary {
    Binary::start(
        Command {
            program: env!("CARGO_BIN_EXE_smith").into(),
            arguments,
            environment: vec![],
            directory: directory.into(),
        },
        mode,
        65_536,
    )
    .unwrap_or_else(|why| panic!("shipped Smith end to end requires usable io_uring and binary startup: {why:?}"))
}

pub fn settings(scratch: &Scratch, scenario: &Scenario) {
    let trust = scratch.path().join("root.der");
    if !scenario.launch.authenticated {
        let tokens =
            smith::local_tokens::Tokens::new(&scenario.launch.token_directory, smith::local_host::token_limits())
                .expect("private tokens");
        let mut saved = tokens.load(0).expect("token load").expect("fixture token");
        saved.expires_at =
            Wall::from_nanos(Clock::new().now().wall.as_nanos().saturating_add(Duration::from_secs(7200).as_nanos()));
        tokens
            .save(0, &skein_oauth::encode_record(&saved, &smith::local_host::token_limits()).expect("token encoding"))
            .expect("fresh fixture grant");
    }
    let accounts = process::accounts_with(scenario.launch.authenticated, true, Some(&trust));
    let accounts: Vec<_> = accounts.iter().map(|account| {
        let oauth = account.oauth.as_ref().map(|o| serde_json::json!({"authorization_url":o.authorization_url,"token_endpoint":o.token_endpoint,"client_id":o.client_id,"redirect_uri":o.redirect_uri,"scope":o.scope,"address":o.address,"server_name":o.server_name,"trust_der":o.trust_der,"json":o.json}));
        serde_json::json!({"number":account.number,"account_id":account.account_id,"oauth":oauth})
    }).collect();
    let mut source = serde_json::json!({
        "agent":{"profile":"standard","memory_bytes":1099511627776_u64,"grace_ms":10,
            "endpoints":[{"name":"fake","number":0,"dialect":0,"account":0,"provider":"codex","address":"127.0.0.1:34443","server_name":"skein.test","trust_der":trust,"identity":"plain"}],
            "environment":[],"trace":{"path":scratch.path().join("agent-trace.jsonl"),"capture":"calls"}},
        "chat":"chat","instructions":"@local-shell Assist",
        "models":[{"endpoint":"fake","name":"fake","max_tokens":1024,"input_price":0,"cached_price":0,"output_price":0,"price_unit":1}],
        "budget":{"turns":8,"spend":1,"seconds":60},"waiting_seconds":30,
        "contract":{"form":"report","max":128},"token_directory":scenario.launch.token_directory,
        "accounts":accounts
    });
    if scenario.launch.change {
        source["directories"] =
            serde_json::json!([{"name":"repo","path":scratch.path().join("repo"),"writable":true,"git":true}]);
        source["contract"] = serde_json::json!({"form":"change","checks_must_pass":scenario.launch.tools,"fields":[{"name":"title","max":256},{"name":"body","max":1024}]});
    }
    if scenario.launch.tools {
        source["conventions"] = serde_json::json!({"guide":".smith-test/guide","checks":".smith-test/check"});
    }
    std::fs::write(scratch.path().join("settings.json"), serde_json::to_vec(&source).expect("settings"))
        .expect("settings fixture");
}

pub fn run(scratch: &Scratch, mut scenario: Scenario) -> Seen {
    assert_no_children();
    scenario.launch.tls = true;
    settings(scratch, &scenario);
    let before = Before::new(scratch.path());
    let clock = Clock::new();
    let now = clock.now().now;
    let mut binary = start(
        vec!["local".into(), scratch.path().join("settings.json").into(), scratch.path().into()],
        scratch.path(),
        Mode::Terminal,
    );
    let streams = binary.take_streams().expect("scripted terminal");
    let descriptors = streams.descriptors();
    let Streams::Terminal { stream } = streams else { unreachable!("person on terminal") };
    let mut world = real::World::new(Judge {
        now,
        deadline: now.saturating_add(Duration::from_secs(5)),
        saved: false,
        tokens: scenario.launch.token_directory.clone(),
    });
    world.spawn_with_fds(binary.descriptors(), || Process::Binary(binary));
    world.spawn_with_fds(descriptors, || {
        Process::Script(Proc::Terminal(Box::new(Terminal::attached(stream, scenario.commands.clone()))))
    });
    world.spawn(|| Process::Script(Proc::Peer(Box::new(scenario.provider()))));
    if scenario.launch.authenticated {
        world.spawn(|| {
            Process::Script(Proc::Issuer(Box::new(process::IssuerProcess::configured(
                skein_fake_peers::Transport::Tls,
            ))))
        });
        world.spawn(|| Process::Script(Proc::Browser(Box::default())));
    }
    let outcome = world.run(&clock, Duration::from_secs(6));
    assert!(outcome.procs.iter().all(Host::is_empty), "binary and scripted processes drained");
    let scripts: Vec<_> =
        outcome.procs.into_iter().filter_map(|p| if let Process::Script(p) = p { Some(p) } else { None }).collect();
    let seen = referee::seen(&scripts, scenario.launch.authenticated, false);
    referee::review_binary(
        &seen,
        &if scenario.launch.change { scratch.checkout() } else { smith_real_world::Checkout::default() },
        scenario.ending.clone(),
    )
    .assert_passed(scenario.launch.seed);
    before.assert_binary_expected(&scenario, &seen);
    let trace = std::fs::read_to_string(scratch.path().join("agent-trace.jsonl")).expect("agent's trace");
    let lines: Vec<serde_json::Value> =
        trace.lines().map(|line| serde_json::from_str(line).expect("trace JSONL")).collect();
    assert!(
        lines.iter().any(|record| record["type"] == "call" && record["name"] == "66696e697368"),
        "agent actually called finish"
    );
    assert!(!trace.contains("access-new") && !trace.contains("refresh-new"), "trace excludes credentials");
    assert_no_children();
    seen
}
