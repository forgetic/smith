//! A whole agent service on Skein's simulated kernel, with a scripted host
//! speaking the real channel schema. The fake machine owns any filesystem and
//! child-process effects. Contract: protocol/agent.md, sections 4–6.

use skein_channel::{Role, StreamMode, frame_writer};
use skein_fake_channel::ScriptedPeer;
use skein_io::{self as io, kernel};
use skein_lib::{Duration, List, Writer};
use skein_sim::{Config as SimConfig, Pid, Sim};
use smith_agent_service as agent;
use smith_protocol_channel as channel;
use smith_protocol_llm as llm;
use smith_protocol_machine as machine;

fn limits() -> agent::Limits {
    let client = skein_llm_world::limits();
    let channel_bodies = smith_channel::CEILINGS;
    let schema = smith_channel::schema(&channel_bodies).expect("bounded channel schema");
    let version = schema.version(1).expect("version one");
    let largest = version.kinds.iter().map(|kind| kind.largest).max().expect("kinds");
    let llm_io = io::Limits {
        sockets: 3,
        refusals: 1,
        intake: 19_000,
        receive: 1024,
        output: 19_000,
        sends: 2,
        accepts: 1,
        backlog: 2,
        close_timeout: Duration::from_secs(1),
        retry: Duration::from_millis(10),
    };
    agent::Limits {
        domain: smith_agent_world::LIMITS,
        channel: channel::Limits {
            bodies: channel_bodies,
            charter: smith_charter::CEILINGS,
            transcript: smith_transcript::CEILINGS,
            channel: skein_channel::Limits {
                chunk: 4096,
                credential: 0,
                skip: 4096,
                output_bytes: largest.checked_add(8).expect("largest frame"),
                output_frames: 4,
                kinds: 17,
            },
            endpoints: 1,
            calls: 8,
            turns: 8,
            fact_reserve_frames: 1,
            fact_reserve_bytes: 128,
            grants: 8,
        },
        llm: llm::ComponentLimits {
            adapter: llm::Limits { client, tool_bytes: 32_768, result_bytes: 32_768 },
            connection: skein_llm_connection::Limits {
                endpoints: 1,
                connections: 2,
                per_endpoint: 2,
                idle_keep: Duration::from_secs(1),
                io: llm_io,
                tls: skein_tls::client::Limits { read: 4096, send: 4096, records: skein_tls::client::MAX_RECORD },
                llm: client,
            },
            receiving: llm::Receiving {
                max_completion_bytes: llm::completion_worst_case(&client, 4096).expect("receiving bound"),
                max_completion_blocks: client.dialect.parts,
                decoded_call_bytes: 4096,
                max_failure_bytes: client.dialect.detail_bytes,
            },
            contract_bytes: 4096,
            accounts: 1,
            grant_value_bytes: 128,
            connect: Some(Duration::from_secs(1)),
            handshake: Some(Duration::from_secs(1)),
            head: Some(Duration::from_secs(2)),
            idle: Some(Duration::from_secs(3)),
        },
        machine: machine::Limits {
            operations: 2,
            roots: 1,
            path_bytes: 64,
            file_bytes: 16,
            entries: 2,
            entry_bytes: 512,
            processes: 2,
            output_bytes: 64,
            search_hits: 8,
            search_bytes: 256,
            search_line_bytes: 512,
            env_bytes: 512,
            stop_grace: Duration::from_millis(10),
        },
        io: io::Limits { sockets: 16, ..llm_io },
        file_slots: 4,
        file_read: 64,
        file_entries: 2,
        file_bytes: 512,
        file_timeout: Duration::from_secs(1),
        queue: smith_domain::max_out(&smith_agent_world::LIMITS).max(256),
        routes: 66,
        memory: u64::MAX,
    }
}

fn service(seed: u64) -> agent::Service {
    agent::Service::new(
        agent::Config {
            limits: limits(),
            domain: smith_domain::Config { endpoints: Box::new([]) },
            channel_endpoints: channel::Endpoints::new(List::with_capacity(1)),
            llm_endpoints: llm::Endpoints::new(Box::new([]), 1, 1).expect("empty endpoint table"),
            environment: Box::new([]),
            stream_mode: StreamMode::Two,
            capture_prompts: false,
        },
        seed,
    )
    .expect("bounded agent")
}

fn host() -> ScriptedPeer {
    let limits = limits().channel;
    let schema = smith_channel::schema(&limits.bodies).expect("schema");
    ScriptedPeer::new(schema, Role::Initiator, limits.channel, 1, Box::new([])).expect("scripted host")
}

fn start(charter: &[u8]) -> skein_channel::Frame {
    let limits = smith_channel::CEILINGS;
    let window =
        smith_channel::Window::new(&limits, smith_channel::WindowParts { turns: 1, bytes: 1_000_000 }).expect("window");
    let record = smith_channel::Start::new(
        &limits,
        smith_channel::StartParts {
            activation: 1,
            charter: Box::from(charter),
            workspace: None,
            transcript: List::with_capacity(0),
            answered: List::with_capacity(0),
            grants: List::with_capacity(0),
            window,
        },
    )
    .expect("Start record");
    let mut body = Writer::new(usize::try_from(record.measure()).expect("body size"));
    record.encode(&mut body).expect("measured body");
    let body = body.finish();
    let mut frame = frame_writer(0x0100, u32::try_from(body.len()).expect("body length")).expect("Start frame");
    frame.put(&body).expect("measured frame");
    frame.finish().expect("full frame")
}

/// The service's inherited descriptors and scripted peer on one simulated
/// process. No wall clock, operating-system channel, or random scheduling is
/// consulted by the driver.
pub struct World {
    sim: Sim,
    pid: Pid,
    service: agent::Service,
    host: ScriptedPeer,
    input: kernel::Fd,
    output: kernel::Fd,
    signals: kernel::Fd,
    pending: Option<(Box<[u8]>, usize)>,
    input_closed: bool,
}

impl World {
    #[must_use]
    pub fn new(seed: u64, charter: &[u8]) -> World {
        let mut sim = Sim::new(seed, SimConfig::calm());
        let pid = sim.spawn_process();
        let input = sim.open_inherited_read(pid);
        let output = sim.open_inherited_write(pid);
        let signals = sim.open_signal_source(pid);
        let mut service = service(seed);
        service.adopt_streams(input, output, signals).expect("three inherited descriptors");
        let mut host = host();
        host.play(start(charter));
        World { sim, pid, service, host, input, output, signals, pending: None, input_closed: false }
    }

    pub fn signal(&mut self) {
        self.sim.deliver_service_signal(self.pid, self.signals, kernel::ServiceSignal::Terminate);
    }

    pub fn step(&mut self) {
        self.sim.reap(self.pid, self.service.completions());
        agent::iterate(&mut self.service, self.sim.now(), self.sim.wall());
        self.sim.submit(self.pid, self.service.submissions());
        if self.pending.is_none() {
            self.pending = self.host.pop_output().map(|bytes| (bytes, 0));
        }
        if let Some((bytes, offset)) = &mut self.pending {
            let sent = self.sim.peer_feed(self.pid, self.input, &bytes[*offset..]);
            *offset += sent;
            if *offset == bytes.len() {
                self.pending = None;
            }
        }
        let received = self.sim.peer_drain(self.pid, self.output, 4096);
        self.host.feed(&received).expect("valid agent channel frames");
        if self.has_answer() && !self.input_closed {
            self.sim.peer_close(self.pid, self.input);
            self.input_closed = true;
        }
        if !agent::work_pending(&self.service, self.sim.now()) && self.sim.ready(self.pid) == 0 {
            if let Some(at) = self.sim.next_due().or_else(|| agent::next_deadline(&self.service)) {
                self.sim.advance_to(at);
            }
        }
    }

    #[must_use]
    pub fn has_answer(&self) -> bool {
        self.host.observed().iter().any(|frame| frame.kind == 0x0110)
    }

    #[must_use]
    pub fn done(&self) -> Option<bool> {
        agent::done(&self.service)
    }

    #[must_use]
    pub fn observed(&self) -> &[skein_fake_channel::Observed] {
        self.host.observed()
    }

    pub fn settle(&mut self) -> bool {
        for _ in 0..10_000 {
            self.step();
            if let Some(answered) = self.done() {
                return answered;
            }
        }
        panic!("agent did not settle: {}", self.sim.render_trace());
    }
}
