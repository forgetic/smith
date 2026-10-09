//! A whole agent service on Skein's simulated kernel, with a scripted host
//! speaking the real channel schema. The fake machine owns any filesystem and
//! child-process effects. Contract: protocol/agent.md, sections 4–6.

use skein_channel::{Role, StreamMode, frame_writer};
use skein_fake_channel::ScriptedPeer;
use skein_io::{self as io, kernel};
use skein_lib::{Duration, List, Writer};
use smith_agent_service as agent;
use smith_protocol_channel as channel;
use smith_protocol_llm as llm;
use smith_protocol_machine as machine;

pub mod fake;

/// The bounded diagnostic writer observed outside each hosted agent.
#[derive(Clone, Default)]
pub struct AgentErrors {
    observed: std::rc::Rc<std::cell::RefCell<Vec<u8>>>,
    pipe: Option<skein_world::PipeWriter>,
}

impl AgentErrors {
    /// Observe the bytes accepted by skein's shared diagnostic pipe writer.
    #[must_use]
    pub fn pipe(fd: kernel::Fd) -> (Self, skein_world::PipeCapture) {
        let (writer, capture) = skein_world::PipeWriter::new(fd, 4096);
        (Self { observed: std::rc::Rc::default(), pipe: Some(writer) }, capture)
    }

    /// Snapshot the agent's words without reading its service state.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.observed.borrow().clone()
    }
}

impl std::io::Write for AgentErrors {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = match &mut self.pipe {
            Some(pipe) => std::io::Write::write(pipe, bytes)?,
            None => bytes.len(),
        };
        let mut output = self.observed.borrow_mut();
        assert!(output.len().checked_add(count).is_some_and(|size| size <= 4096), "finite agent diagnostics");
        output.extend_from_slice(bytes.get(..count).expect("accepted diagnostic prefix"));
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.pipe {
            Some(pipe) => std::io::Write::flush(pipe),
            None => Ok(()),
        }
    }
}

/// Adopt scenario-specific service settings and mounts in Start order.
#[must_use]
pub fn configured(
    configuration: agent::Config,
    seed: u64,
    inherited: &skein_world::Inherited,
    errors: AgentErrors,
) -> smith_agent_shell::Agent {
    smith_agent_shell::Agent::new(
        smith_agent_shell::config::Configuration { service: configuration, memory: u64::MAX, trace: None },
        smith_agent_shell::Resources {
            input: inherited.pipes.iter().find(|(child, _)| *child == 0).expect("stdin").1,
            output: inherited.pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1,
            error: Some(inherited.pipes.iter().find(|(child, _)| *child == 2).expect("stderr").1),
            signals: inherited.signal,
            seed,
            roots: Some(inherited.roots.iter().map(|(_, fd)| *fd).collect()),
        },
        Box::new(errors),
    )
    .expect("configured shipped agent")
}

/// Bounded agent-process settings shared by its direct and hosted worlds.
#[must_use]
pub fn limits() -> agent::Limits {
    let mut client = skein_llm_world::limits();
    // The full answer allowance coexists with its provider document wrappers.
    client.dialect.document_bytes = 32_768;
    client.dialect.tokens = 4096;
    client.sse.line = 32_768;
    client.sse.event = 65_536;
    let completion = llm::completion_worst_case(&client, 4096).expect("receiving bound");
    let mut domain = smith_agent_world::LIMITS;
    domain.session.completion_bytes = completion;
    let channel_bodies = smith_channel::CEILINGS;
    let schema = smith_channel::schema(&channel_bodies).expect("bounded channel schema");
    let version = schema.version(2).expect("version two");
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
        domain,
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
                kinds: 18,
            },
            endpoints: 1,
            calls: 8,
            turns: 8,
            fact_reserve_frames: 1,
            fact_reserve_bytes: 128,
            grants: 8,
        },
        llm: llm::ComponentLimits {
            adapter: llm::Limits { client, tool_bytes: 32_768, rendered_result: client.dialect.string_bytes },
            connection: skein_llm_connection::Limits {
                endpoints: 1,
                connections: 2,
                calls: 2,
                per_endpoint: 2,
                idle_keep: Duration::from_secs(1),
                io: llm_io,
                tls: skein_tls::client::Limits { read: 4096, send: 4096, records: skein_tls::client::MAX_RECORD },
            },
            receiving: llm::Receiving {
                max_completion_bytes: completion,
                max_completion_blocks: client.dialect.parts,
                decoded_call_bytes: 4096,
                max_failure_bytes: smith_agent_world::LIMITS.session.failure_bytes,
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
        queue: smith_domain::max_out(&domain).max(256),
        routes: 66,
        memory: u64::MAX,
    }
}

/// Build the same agent service for a direct or spawned process world.
#[must_use]
pub fn service(seed: u64) -> agent::Service {
    agent::Service::new(configuration(), seed).expect("bounded agent")
}

/// The shared lower configuration for direct, spawned and colocated process worlds.
#[must_use]
pub fn configuration() -> agent::Config {
    let mut endpoints = List::with_capacity(1);
    endpoints
        .push(channel::Endpoint { name: Box::default(), number: 0, dialect: 0, account: 0 })
        .expect("one endpoint");
    let llm_endpoints = llm::Endpoints::new(
        Box::new([llm::ConfiguredEndpoint {
            name: smith_domain::llm::Endpoint(0),
            destination: skein_llm_connection::Endpoint {
                limits: limits().llm.adapter.client,
                credential: skein_llm::client::CredentialLimits {
                    access_token: limits().llm.grant_value_bytes,
                    account_id: limits().llm.grant_value_bytes,
                },
                address: kernel::Addr::from((std::net::Ipv4Addr::LOCALHOST, 443)),
                transport: skein_llm_connection::Transport::Plaintext,
                llm: skein_llm_world::call(7).endpoint,
            },
            account: 0,
            reasoning_effort: None,
            cache_key: None,
            identity: llm::IdentityProfile::Plain,
        }]),
        1,
        1,
    )
    .expect("one LLM endpoint");
    agent::Config {
        limits: limits(),
        domain: smith_domain::Config { endpoints: Box::new([smith_domain::run::charter::Endpoint(0)]) },
        channel_endpoints: channel::Endpoints::new(endpoints),
        llm_endpoints,
        environment: Box::new([]),
        stream_mode: StreamMode::Two,
        capture_prompts: false,
    }
}

/// The scripted channel initiator used by agent startup stories.
#[must_use]
pub fn host() -> ScriptedPeer {
    let limits = limits().channel;
    let schema = smith_channel::schema(&limits.bodies).expect("schema");
    ScriptedPeer::new(schema, Role::Initiator, limits.channel, 2, Box::new([])).expect("scripted host")
}

/// One activation Start sent by the scripted host, ending with an Answer.
#[must_use]
pub fn start(charter: &[u8]) -> skein_channel::Frame {
    let limits = smith_channel::CEILINGS;
    let window = smith_channel::Window::new(&limits, smith_channel::WindowParts { turns: 8, bytes: 1_000_000_000 })
        .expect("window");
    let mut grants = List::with_capacity(1);
    grants
        .push(
            smith_channel::Grant::new(
                &limits,
                smith_channel::GrantParts {
                    account: 0,
                    generation: 1,
                    valid: Duration::from_secs(7200),
                    value: smith_channel::GrantValue::new(
                        &limits,
                        smith_channel::GrantValueParts { credential: Box::from(&b"\0\x03acctoken"[..]) },
                    )
                    .expect("credential envelope"),
                },
            )
            .expect("grant"),
        )
        .expect("one grant");
    let record = smith_channel::Start::new(
        &limits,
        smith_channel::StartParts {
            activation: 1,
            charter: Box::from(charter),
            workspace: None,
            transcript: List::with_capacity(0),
            answered: List::with_capacity(0),
            grants,
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

/// A concrete charter within the process limits, with one report contract.
#[must_use]
pub fn charter() -> Box<[u8]> {
    let smallest = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin");
    let source = smith_charter::Charter::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(smallest))
        .expect("golden charter");
    let mut parts = source.into_parts();
    parts.budget = smith_charter::Budget::new(
        &smith_charter::CEILINGS,
        smith_charter::BudgetParts { turns: 1, spend: 1, time: Duration::from_secs(60) },
    )
    .expect("workable budget");
    parts.contract = smith_charter::Contract::new(
        &smith_charter::CEILINGS,
        smith_charter::ContractParts {
            report: Some(
                smith_charter::TextRule::new(
                    &smith_charter::CEILINGS,
                    smith_charter::TextRuleParts { max: 128, fields: List::with_capacity(0) },
                )
                .expect("report rule"),
            ),
            verdicts: List::with_capacity(0),
            change: None,
            failure: None,
        },
    )
    .expect("outcome contract");
    let mut llm = parts.main.into_parts();
    llm.model = Box::from(*b"fake");
    llm.max_tokens = 1024;
    llm.prices = smith_charter::Prices::new(
        &smith_charter::CEILINGS,
        smith_charter::PricesParts { input: 0, cached: 0, output: 0, unit: 1 },
    )
    .expect("prices");
    parts.main = smith_charter::Llm::new(&smith_charter::CEILINGS, llm).expect("model");
    let record = smith_charter::Charter::new(&smith_charter::CEILINGS, parts).expect("charter");
    let mut writer = Writer::new(usize::try_from(record.measure()).expect("charter size"));
    record.encode(&mut writer).expect("measured charter");
    writer.finish()
}

pub use world::World;

mod world;

#[cfg(test)]
mod memory {
    use skein_world::domain::heap::{Counting, Meter};

    use super::{agent, limits, service};

    #[global_allocator]
    static HEAP: Counting = Counting;

    #[test]
    fn composed_service_start_fits_the_checked_bound() {
        let bound = agent::worst_case(&limits(), &super::configuration().llm_endpoints).expect("checked service bound");
        let meter = Meter::new();
        meter.start();
        let agent = service(7);
        let peak = meter.check(meter.end(), bound, &"service construction");
        assert!(peak > 0, "the counting allocator saw live service memory");
        drop(agent);
    }
}
