use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use skein_world::Host;
use smith::agent_shell::{Agent, Resources};
use smith::{config, trace};
use smith_agent_process_world as fixture;

#[derive(Clone)]
struct Errors(Arc<Mutex<Vec<u8>>>);

impl Errors {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("error output lock").clone()).expect("UTF-8 error lines")
    }
}

impl Write for Errors {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("error output lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn reading_startup_configuration_uses_supplied_streams_and_logs_the_terminal_once() {
    let path = std::env::temp_dir().join(format!("smith-agent-library-read-{}", std::process::id()));
    std::fs::write(
        &path,
        r#"{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,
        "endpoints":[],"environment":[]}"#,
    )
    .expect("configuration fixture");
    let mut sim = skein_sim::Sim::new(1, skein_sim::Config::calm());
    let pid = sim.spawn_process();
    let input = sim.open_inherited_read(pid);
    let output = sim.open_inherited_write(pid);
    let signals = sim.open_signal_source(pid);
    let errors = Errors::new();
    let mut agent = Agent::read(
        &path,
        Resources { input, output, signals, seed: 1, roots: Some(Box::new([])) },
        Box::new(errors.clone()),
    )
    .expect("read shared startup");
    std::fs::remove_file(path).expect("remove configuration fixture");
    sim.peer_close(pid, input);
    for _ in 0..1000 {
        sim.reap(pid, agent.completions());
        agent.iterate(sim.now(), sim.wall());
        sim.submit(pid, agent.submissions());
        if agent.result().is_some() {
            break;
        }
        if !agent.work_pending(sim.now())
            && sim.ready(pid) == 0
            && let Some(at) = [sim.next_due(), agent.next_deadline()].into_iter().flatten().min()
        {
            sim.advance_to(at);
        }
    }
    assert_eq!(agent.result(), Some(Err("the run could not answer: Some(ChannelEnded)")));
    assert!(agent.is_empty());
    sim.assert_quiescent(pid);
    sim.assert_no_open_fds(pid);
    let before = errors.text();
    assert!(before.starts_with("smith: agent started; worst case "));
    assert_eq!(before.lines().last(), Some("smith: the run could not answer: Some(ChannelEnded)"));
    agent.iterate(sim.now(), sim.wall());
    assert_eq!(errors.text(), before, "terminal diagnostics appear once");
}

#[test]
fn shared_startup_reports_configuration_refusal_without_adopting_the_streams() {
    let errors = Errors::new();
    let result = Agent::read(
        std::path::Path::new(""),
        Resources {
            input: skein_io::kernel::Fd::new(100),
            output: skein_io::kernel::Fd::new(101),
            signals: skein_io::kernel::Fd::new(102),
            seed: 1,
            roots: Some(Box::new([])),
        },
        Box::new(errors.clone()),
    );
    assert!(result.is_err());
    let text = errors.text();
    assert!(text.starts_with("smith: configuration metadata:"));
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn the_shared_agent_pass_answers_the_host_and_writes_its_trace() {
    let path = std::env::temp_dir().join(format!("smith-agent-library-trace-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut sim_config = skein_sim::Config::calm();
    sim_config.wall = skein_tls_world::pki::VALID;
    let mut sim = skein_sim::Sim::new(7, sim_config);
    let pid = sim.spawn_process();
    let peer_pid = sim.spawn_process();
    let input = sim.open_inherited_read(pid);
    let output = sim.open_inherited_write(pid);
    let signals = sim.open_signal_source(pid);
    let errors = Errors::new();
    let mut configuration = fixture::configuration();
    configuration.capture_prompts = true;
    let mut agent = Agent::new(
        config::Configuration {
            service: configuration,
            memory: u64::MAX,
            trace: Some(trace::TraceConfig { path: path.clone(), capture: trace::Capture::Everything }),
        },
        Resources { input, output, signals, seed: 7, roots: Some(Box::new([])) },
        Box::new(errors.clone()),
    )
    .expect("shared agent startup");
    let mut host = fixture::host();
    host.play(fixture::start(&fixture::charter()));
    let mut peer = fixture::fake::peer();
    let mut machine = skein_fake_machine::Machine::new();
    let mut pending = None;
    let mut closed = false;
    for _ in 0..10_000 {
        sim.reap(peer_pid, peer.completions());
        peer.iterate(sim.now(), sim.wall());
        sim.submit(peer_pid, peer.submissions());
        sim.reap(pid, agent.completions());
        agent.iterate(sim.now(), sim.wall());
        sim.submit(pid, agent.submissions());
        skein_fake_machine::serve(&mut machine, &mut sim);
        if pending.is_none() {
            pending = host.pop_output().map(|bytes| (bytes, 0));
        }
        if let Some((bytes, offset)) = &mut pending {
            *offset += sim.peer_feed(pid, input, &bytes[*offset..]);
            if *offset == bytes.len() {
                pending = None;
            }
        }
        host.feed(&sim.peer_drain(pid, output, 4096)).expect("valid host channel");
        if host.observed().iter().any(|frame| frame.kind == 0x0110) && !closed {
            sim.peer_close(pid, input);
            closed = true;
        }
        if agent.result().is_some() {
            break;
        }
        if !agent.work_pending(sim.now())
            && !peer.work_pending(sim.now())
            && sim.ready(pid) == 0
            && sim.ready(peer_pid) == 0
            && let Some(at) = [sim.next_due(), agent.next_deadline(), peer.next_deadline()].into_iter().flatten().min()
        {
            sim.advance_to(at);
        }
    }
    assert_eq!(agent.result(), Some(Ok(())));
    assert!(fixture::fake::replied(&peer));
    assert!(agent.is_empty());
    sim.assert_quiescent(pid);
    sim.assert_no_open_fds(pid);
    assert!(!errors.text().contains("could not answer"), "successful invocation logs no failure");
    drop(agent);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let records = loop {
        let text = std::fs::read_to_string(&path).expect("trace file");
        if text.lines().any(|line| line.contains("\"type\":\"prompt\""))
            && text.lines().any(|line| line.contains("\"type\":\"fact\""))
            && text.lines().any(|line| line.contains("\"type\":\"completion\""))
        {
            break text;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "shared trace writer records prompts, facts and completions: {text}"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(!records.contains("acctoken"), "trace excludes credential values");
    assert!(!records.contains("616363746f6b656e"), "trace excludes hex credential values");
    std::fs::remove_file(path).expect("remove trace fixture");
}
