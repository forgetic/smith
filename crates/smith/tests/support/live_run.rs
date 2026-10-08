//! Live outcome policy beside Smith 04's binary and scripted terminal on the
//! shared real ring (testing.md, sections 2.3 and 5). No service state is read.

use super::{Process, start};
use skein_io::kernel::Exit;
use skein_lib::{Duration, Time};
use skein_shell::Clock;
use skein_world::{
    Host, Referee,
    end_to_end::{Mode, Streams},
    real,
};
use smith_local_process_world::{
    process::Proc,
    referee::CheckoutRead,
    terminal::{Action, Terminal},
};
use smith_real_world::{Checkout, Scratch, settled::assert_no_children};
use std::path::PathBuf;

pub struct Expectation {
    pub before: Option<Vec<u8>>,
    pub files: Vec<(Vec<u8>, Vec<u8>)>,
}

struct Judge {
    now: Time,
    deadline: Time,
    checkout: Checkout,
    expectation: Expectation,
    directory: PathBuf,
    trace_prefix: Vec<u8>,
    reviewed: bool,
    minimum_finishes: usize,
}

impl Referee<Process> for Judge {
    fn act(&mut self, _now: Time, procs: &mut [Process]) {
        let exit =
            procs.iter().find_map(|proc| if let Process::Binary(binary) = proc { binary.exit_status() } else { None });
        let finished = procs.iter().any(|proc| matches!(proc, Process::Script(Proc::Terminal(person)) if person.shown().windows(12).any(|bytes| bytes == b"Tool: finish")));
        for proc in procs {
            match proc {
                Process::Script(Proc::Terminal(person)) => {
                    if let Some(exit) = exit {
                        person.finished(exit);
                    }
                }
                Process::Script(Proc::Peer(peer)) if exit.is_some() || finished => peer.shutdown(),
                _ => {}
            }
        }
    }
    fn observe(&mut self, now: Time, procs: &[Process]) {
        self.now = now;
        for proc in procs {
            if let Process::Binary(binary) = proc
                && let Some(exit) = binary.exit_status()
            {
                assert_eq!(exit, Exit::Code(0), "live Smith must exit successfully; diagnostic bytes are withheld");
                assert!(
                    std::str::from_utf8(binary.stderr()).is_ok_and(|text| text
                        .lines()
                        .all(|line| line.starts_with("smith: local host started; worst case ")
                            || line.starts_with("smith: agent started; worst case "))),
                    "live Smith emitted an unexpected diagnostic; credential-bearing bytes are withheld"
                );
            }
        }
        if self.reviewed || !procs.iter().all(Host::is_empty) {
            return;
        }
        let trace = std::fs::read(self.directory.join("agent-trace.jsonl")).expect("live agent trace");
        assert!(trace.starts_with(&self.trace_prefix), "durable trace prefix stays unchanged");
        let records: Vec<serde_json::Value> = std::str::from_utf8(&trace[self.trace_prefix.len()..])
            .expect("trace UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("trace JSONL"))
            .collect();
        assert!(
            records.iter().filter(|record| record["type"] == "call" && record["name"] == "66696e697368").count()
                >= self.minimum_finishes,
            "the shipped live agent actually called finish"
        );
        for (path, expected) in &self.expectation.files {
            assert_eq!(
                std::fs::read(self.directory.join("repo").join(std::str::from_utf8(path).expect("test path")))
                    .expect("live requested file"),
                *expected,
                "the live working file has its prescribed content"
            );
        }
        assert!(
            records.iter().any(|record| record["type"] == "fact"
                && record["fact"].as_str().is_some_and(|fact| fact.starts_with("Run { fact: Returned {")
                    && fact.contains(if self.expectation.before.is_some() {
                        "result: Delivered"
                    } else {
                        "result: Accepted"
                    }))),
            "the live agent received its accepted or delivered finish terminal"
        );
        if let Some(before) = &self.expectation.before {
            let head = self.checkout.head().expect("live committed head");
            assert!(head != *before, "live run creates a new commit");
            let message = self.checkout.message(&head);
            assert!(
                message.windows(15).any(|bytes| bytes == b"Smith-Delivery:"),
                "commit carries the delivery identity"
            );
            let files = self.checkout.files(&head);
            for (path, expected) in &self.expectation.files {
                assert!(
                    files.get(path).is_some_and(|bytes| bytes == expected),
                    "the actual committed file has its prescribed content"
                );
                assert!(
                    std::fs::read(self.directory.join("repo").join(std::str::from_utf8(path).expect("test path")))
                        .is_ok_and(|bytes| bytes == *expected),
                    "the delivered checkout keeps the prescribed content"
                );
            }
            let person = procs
                .iter()
                .find_map(|proc| if let Process::Script(Proc::Terminal(person)) = proc { Some(person) } else { None })
                .expect("live person");
            assert!(
                person.shown().windows(16).any(|bytes| bytes == b"Change delivered"),
                "terminal observed actual delivery"
            );
        }
        self.reviewed = true;
    }
    fn next_deadline(&self) -> Option<Time> {
        Some(self.now.saturating_add(Duration::from_millis(10)).min(self.deadline))
    }
    fn overdue(&self, now: Time) -> Option<String> {
        (now >= self.deadline).then(|| "live binary story did not settle before its deadline; output withheld".into())
    }
    fn passed(&self) -> bool {
        self.reviewed
    }
}

pub fn run(scratch: &Scratch, prompt: &str, change: bool, expectation: Expectation) {
    run_with_peer(scratch, prompt, change, expectation, None, None);
}

/// Exercise a second request as soon as the first finish call is visible.
pub fn run_interactive(scratch: &Scratch, greeting: &str, task: &str, expectation: Expectation) {
    run_with_peer(
        scratch,
        task,
        false,
        expectation,
        None,
        Some(vec![
            Action::Send(format!("{greeting}\n").into_bytes().into()),
            Action::Wait(b"Tool: finish".as_slice().into()),
            Action::Send(format!("{task}\n").into_bytes().into()),
            Action::Wait(b"Tool: shell".as_slice().into()),
            Action::Eof,
        ]),
    );
}

/// Offline control of exactly the live outcome policy and shared binary driver.
pub fn run_fake(scratch: &Scratch, scenario: smith_local_process_world::world::Scenario) {
    super::settings(scratch, &scenario);
    let change = scenario.launch.change;
    let files = if change {
        [
            ("original.txt", "edited\n"),
            ("result.txt", "new\n"),
            ("command.txt", "ran\n"),
            ("checks-ran.txt", "checked\n"),
        ]
        .into_iter()
        .map(|(path, content)| (path.as_bytes().to_vec(), content.as_bytes().to_vec()))
        .collect()
    } else {
        vec![]
    };
    run_with_peer(
        scratch,
        if change { "change it" } else { "hello" },
        change,
        Expectation { before: change.then(|| scratch.checkout().head().expect("initial head")), files },
        Some(scenario.provider()),
        None,
    );
}

fn run_with_peer(
    scratch: &Scratch,
    prompt: &str,
    change: bool,
    expectation: Expectation,
    peer: Option<smith_local_process_world::llm::Peer>,
    commands: Option<Vec<Action>>,
) {
    assert_no_children();
    let clock = Clock::new();
    let now = clock.now().now;
    let budget = if peer.is_some() { Duration::from_secs(5) } else { Duration::from_secs(130) };
    let mut binary = start(
        vec!["local".into(), scratch.path().join("settings.json").into(), scratch.path().into()],
        scratch.path(),
        Mode::Terminal,
    );
    let streams = binary.take_streams().expect("live controlling terminal");
    let descriptors = streams.descriptors();
    let Streams::Terminal { stream } = streams else { unreachable!("live person on terminal") };
    let mut world = real::World::new(Judge {
        now,
        deadline: now.saturating_add(budget),
        checkout: scratch.checkout(),
        expectation,
        directory: scratch.path().into(),
        trace_prefix: std::fs::read(scratch.path().join("agent-trace.jsonl")).unwrap_or_default(),
        reviewed: false,
        minimum_finishes: if commands.is_some() { 2 } else { 1 },
    });
    world.spawn_with_fds(binary.descriptors(), || Process::Binary(binary));
    world.spawn_with_fds(descriptors, || {
        Process::Script(Proc::Terminal(Box::new(Terminal::attached(
            stream,
            commands.unwrap_or_else(|| {
                vec![
                    Action::Send(format!("{prompt}\n").into_bytes().into()),
                    Action::Wait(if change {
                        b"Change delivered".as_slice().into()
                    } else {
                        b"Tool: finish".as_slice().into()
                    }),
                    Action::Eof,
                ]
            }),
        ))))
    });
    if let Some(peer) = peer {
        world.spawn(|| Process::Script(Proc::Peer(Box::new(peer))));
    }
    let outcome = world.run(&clock, budget.saturating_add(Duration::from_secs(1)));
    assert!(outcome.procs.iter().all(Host::is_empty), "live binary and terminal fully drained");
    assert_no_children();
}
