//! JSON configuration read once before the agent opens its channel.
//! It retains endpoint names and destinations, limits, command environment,
//! and future trace settings; no credential is read here.
//! Contract: protocol/agent.md, sections 4 and 6.

use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;

use serde::Deserialize;
use skein_channel::StreamMode;
use skein_http::{self as http, Header};
use skein_io as io;
use skein_lib::{Duration, List};
use skein_llm::{self as shared, openai};
use skein_llm_connection as connection;
use skein_tls as tls;
use smith_agent_service as service;
use smith_domain::{self as domain, run, tools};
use smith_protocol_channel as channel;
use smith_protocol_llm as llm;
use smith_protocol_machine as machine;

use crate::limits;
use crate::trace::{Capture, TraceConfig};

const CONFIG_BYTES: u64 = 1 << 20;
const ENDPOINTS: u32 = 3;
const ACCOUNTS: u32 = 4;

/// Parsed startup configuration and the service it can build.
pub struct Configuration {
    pub service: service::Config,
    pub memory: u64,
    pub trace: Option<TraceConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    profile: String,
    memory_bytes: u64,
    grace_ms: u64,
    endpoints: Vec<Endpoint>,
    environment: Vec<Variable>,
    #[serde(default)]
    trace: Option<TraceDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceDocument {
    path: String,
    capture: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    name: String,
    number: u32,
    dialect: u32,
    account: u32,
    provider: String,
    address: String,
    server_name: String,
    #[serde(default)]
    authority: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    trust_der: Option<String>,
    #[serde(default)]
    headers: Vec<HeaderField>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    cache_key: Option<String>,
    #[serde(default)]
    identity: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderField {
    name: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variable {
    name: String,
    value: String,
}

/// Read one JSON document and reject unknown keys, oversized input and
/// invalid transport settings before any channel byte is read.
pub fn read(path: &Path) -> Result<Configuration, String> {
    let size = fs::metadata(path).map_err(|error| format!("configuration metadata: {error}"))?.len();
    if size > CONFIG_BYTES {
        return Err(format!("configuration is larger than {CONFIG_BYTES} bytes"));
    }
    let bytes = fs::read(path).map_err(|error| format!("configuration read: {error}"))?;
    if bytes.len() as u64 > CONFIG_BYTES {
        return Err(format!("configuration is larger than {CONFIG_BYTES} bytes"));
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(|error| format!("configuration JSON: {error}"))?;
    build(document)
}

fn build(document: Document) -> Result<Configuration, String> {
    if document.profile != "standard" {
        return Err("configuration profile must be standard".into());
    }
    if document.endpoints.len() > ENDPOINTS as usize {
        return Err("too many endpoints".into());
    }
    if document.grace_ms == 0 {
        return Err("grace_ms must be positive".into());
    }
    let trace = match document.trace {
        Some(trace) => {
            if trace.path.is_empty() {
                return Err("trace path is empty".into());
            }
            let capture = match trace.capture.as_str() {
                "none" => Capture::None,
                "calls" => Capture::Calls,
                "everything" => Capture::Everything,
                _ => return Err("trace capture must be none, calls or everything".into()),
            };
            Some(TraceConfig { path: trace.path.into(), capture })
        }
        None => None,
    };
    let mut limits = standard_limits(document.memory_bytes)?;
    limits.machine.stop_grace = Duration::from_millis(document.grace_ms);
    let mut domain_endpoints = Vec::with_capacity(document.endpoints.len());
    let mut channel_endpoints = List::with_capacity(ENDPOINTS);
    let mut llm_endpoints = Vec::with_capacity(document.endpoints.len());
    let mut names = std::collections::HashSet::new();
    let mut numbers = std::collections::HashSet::new();
    for endpoint in document.endpoints {
        if endpoint.name.is_empty() || endpoint.name.len() > smith_charter::CEILINGS.llm_endpoint as usize {
            return Err("endpoint name is empty or too long".into());
        }
        if endpoint.account >= ACCOUNTS {
            return Err("endpoint account is outside the profile".into());
        }
        if !names.insert(endpoint.name.clone()) || !numbers.insert(endpoint.number) {
            return Err("duplicate endpoint name or number".into());
        }
        let address = resolve(&endpoint.address)?;
        let server_name = tls::Name::new(&endpoint.server_name).ok_or("invalid TLS server name")?;
        let trust = trust(endpoint.trust_der.as_deref())?;
        let mut llm_endpoint = match endpoint.provider.as_str() {
            "codex" => shared::Endpoint::codex(),
            "anthropic" => shared::Endpoint::anthropic(),
            _ => return Err("endpoint provider must be codex or anthropic".into()),
        };
        if let Some(authority) = endpoint.authority {
            if authority.is_empty() || authority.contains(['\r', '\n']) {
                return Err("invalid endpoint HTTP authority".into());
            }
            llm_endpoint.authority = authority.into_bytes().into();
        }
        if let Some(target) = endpoint.target {
            if !target.starts_with('/') || target.contains(['\r', '\n']) {
                return Err("invalid endpoint HTTP target".into());
            }
            llm_endpoint.target = target.into_bytes().into();
        }
        let mut headers = Vec::with_capacity(endpoint.headers.len());
        for header in endpoint.headers {
            if header.name.is_empty() || header.name.contains(['\r', '\n', ':']) || header.value.contains(['\r', '\n'])
            {
                return Err("invalid endpoint HTTP header".into());
            }
            headers.push(Header { name: header.name.into_bytes().into(), value: header.value.into_bytes().into() });
        }
        llm_endpoint.headers = headers.into_boxed_slice();
        let identity = match endpoint.identity.as_deref() {
            None | Some("plain") => llm::IdentityProfile::Plain,
            Some("claude-code") => llm::IdentityProfile::ClaudeCode,
            Some(_) => return Err("endpoint identity must be plain or claude-code".into()),
        };
        domain_endpoints.push(run::charter::Endpoint(endpoint.number));
        channel_endpoints
            .push(channel::Endpoint {
                name: endpoint.name.into_bytes().into(),
                number: endpoint.number,
                dialect: endpoint.dialect,
                account: endpoint.account,
            })
            .map_err(|_| "too many channel endpoints")?;
        llm_endpoints.push(llm::ConfiguredEndpoint {
            name: domain::llm::Endpoint(endpoint.number),
            destination: connection::Endpoint { address, server_name, trust, llm: llm_endpoint },
            account: endpoint.account,
            reasoning_effort: endpoint.reasoning_effort.map(|value| value.into_bytes().into()),
            cache_key: endpoint.cache_key.map(|value| value.into_bytes().into()),
            identity,
        });
    }
    let mut environment = Vec::with_capacity(document.environment.len());
    let mut environment_bytes = 0_u64;
    for variable in document.environment {
        if variable.name.is_empty() || variable.name.contains(['=', '\0']) || variable.value.contains('\0') {
            return Err("invalid command environment variable".into());
        }
        environment_bytes = environment_bytes
            .checked_add((variable.name.len() + variable.value.len() + 1) as u64)
            .ok_or("environment is too large")?;
        environment
            .push(tools::Var { name: variable.name.into_bytes().into(), value: variable.value.into_bytes().into() });
    }
    if environment_bytes > u64::from(limits.machine.env_bytes) {
        return Err("command environment exceeds the profile".into());
    }
    let llm_endpoints = llm::Endpoints::new(llm_endpoints.into_boxed_slice(), ENDPOINTS, ACCOUNTS)
        .map_err(|error| format!("LLM endpoints: {error:?}"))?;
    limits.memory = document.memory_bytes;
    let config = service::Config {
        limits,
        domain: domain::Config { endpoints: domain_endpoints.into_boxed_slice() },
        channel_endpoints: channel::Endpoints::new(channel_endpoints),
        llm_endpoints,
        environment: environment.into_boxed_slice(),
        stream_mode: StreamMode::Two,
        capture_prompts: match &trace {
            Some(trace) => trace.capture == Capture::Everything,
            None => false,
        },
    };
    let reserve = if trace.is_some() { crate::trace::MEMORY_RESERVE } else { 0 };
    let complete = service::worst_case(&config.limits)
        .and_then(|bytes| bytes.checked_add(reserve))
        .ok_or("agent plus trace memory calculation overflowed")?;
    if complete > document.memory_bytes {
        return Err("agent plus trace exceeds memory_bytes".into());
    }
    Ok(Configuration { service: config, memory: document.memory_bytes, trace })
}

fn resolve(address: &str) -> Result<SocketAddr, String> {
    let mut resolved = address.to_socket_addrs().map_err(|error| format!("endpoint address {address:?}: {error}"))?;
    resolved.next().ok_or_else(|| format!("endpoint address {address:?} has no result"))
}

fn trust(der: Option<&str>) -> Result<tls::Config, String> {
    let mut roots = tls::RootCertStore::empty();
    match der {
        Some(path) => {
            let bytes = fs::read(path).map_err(|error| format!("trust certificate {path:?}: {error}"))?;
            roots.add(tls::CertificateDer::from(bytes)).map_err(|error| format!("trust certificate: {error}"))?;
        }
        None => {
            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                roots.add(cert).map_err(|error| format!("native trust certificate: {error}"))?;
            }
        }
    }
    tls::Config::new(roots, &[]).map_err(|error| format!("TLS trust: {error:?}"))
}

fn standard_limits(memory: u64) -> Result<service::Limits, String> {
    let client = shared::client::Limits {
        http: http::client::Limits { request: 4096, head: 4096, headers: 32, read: 256, send: 31 },
        sse: http::sse::Limits { line: 4096, event: 8192, field: 128, chunk: 128 },
        dialect: openai::Limits {
            request_bytes: 8192,
            document_bytes: 8192,
            string_bytes: 4096,
            depth: 32,
            tokens: 1024,
            parts: 16,
            input_bytes: 2048,
            opaque_bytes: 2048,
            answer_bytes: 8192,
            detail_bytes: 256,
        },
        error_bytes: 4096,
    };
    let decoded_call_bytes = 4096;
    let completion = llm::completion_worst_case(&client, decoded_call_bytes).ok_or("LLM receiving bound overflows")?;
    let bodies = smith_channel::CEILINGS;
    let schema = smith_channel::schema(&bodies).map_err(|error| format!("channel schema: {error:?}"))?;
    let version = schema.version(1).ok_or("channel version 1 is absent")?;
    let mut largest = 0;
    for kind in &version.kinds {
        largest = largest.max(kind.largest);
    }
    let output_bytes = largest.checked_add(8).ok_or("channel frame size overflows")?;
    let io = io::Limits {
        sockets: 32,
        refusals: 4,
        intake: 19_000,
        receive: 4096,
        output: output_bytes.max(19_000),
        sends: 8,
        accepts: 1,
        backlog: 2,
        close_timeout: Duration::from_secs(5),
        retry: Duration::from_millis(10),
    };
    let mut domain = limits::LIMITS;
    domain.session.completion_bytes = completion;
    let queue = domain::max_out(&domain).max(256);
    let routes = io::operations(&io).and_then(|count| count.checked_add(64)).ok_or("IO route count overflows")?;
    Ok(service::Limits {
        domain,
        channel: channel::Limits {
            bodies,
            charter: smith_charter::CEILINGS,
            transcript: smith_transcript::CEILINGS,
            channel: skein_channel::Limits {
                chunk: 4096,
                credential: 0,
                skip: 4096,
                output_bytes,
                output_frames: 4,
                kinds: 17,
            },
            endpoints: ENDPOINTS,
            calls: 16,
            turns: 64,
            fact_reserve_frames: 1,
            fact_reserve_bytes: 128,
            grants: 8,
        },
        llm: llm::ComponentLimits {
            adapter: llm::Limits { client, tool_bytes: 32_768, result_bytes: 32_768 },
            connection: connection::Limits {
                endpoints: ENDPOINTS,
                connections: 6,
                per_endpoint: 2,
                idle_keep: Duration::from_secs(15),
                io,
                tls: tls::client::Limits { read: 4096, send: 4096, records: tls::client::MAX_RECORD },
                llm: client,
            },
            receiving: llm::Receiving {
                max_completion_bytes: completion,
                max_completion_blocks: client.dialect.parts,
                decoded_call_bytes,
                max_failure_bytes: domain.session.failure_bytes,
            },
            contract_bytes: 4096,
            accounts: ACCOUNTS,
            grant_value_bytes: 2048,
            connect: Some(Duration::from_secs(10)),
            handshake: Some(Duration::from_secs(10)),
            head: Some(Duration::from_secs(60)),
            idle: Some(Duration::from_secs(30)),
        },
        machine: machine::Limits {
            operations: 16,
            roots: 2,
            path_bytes: 4096,
            file_bytes: 65_536,
            entries: 64,
            entry_bytes: 4096,
            processes: 8,
            output_bytes: 1024,
            search_hits: 16,
            search_bytes: 1024,
            search_line_bytes: 4096,
            env_bytes: 8192,
            stop_grace: Duration::from_millis(250),
        },
        io,
        file_slots: 32,
        file_read: 65_536,
        file_entries: 64,
        file_bytes: 65_536,
        file_timeout: Duration::from_secs(30),
        queue,
        routes,
        memory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document {
            profile: "standard".into(),
            memory_bytes: u64::MAX,
            grace_ms: 250,
            endpoints: vec![],
            environment: vec![],
            trace: None,
        }
    }

    #[test]
    fn standard_profile_builds_a_complete_bounded_service() {
        let configuration = build(document()).expect("standard profile");
        let bound = service::worst_case(&configuration.service.limits).expect("checked bound");
        assert!(bound > 0);
        let service = service::Service::new(configuration.service, 1).expect("service");
        assert!(service::done(&service).is_none());
    }

    #[test]
    fn named_endpoint_resolves_and_its_tls_trust_builds_before_start() {
        let mut document = document();
        document.endpoints.push(Endpoint {
            name: "test".into(),
            number: 42,
            dialect: 1,
            account: 0,
            provider: "codex".into(),
            address: "127.0.0.1:443".into(),
            server_name: "example.test".into(),
            authority: None,
            target: None,
            trust_der: Some(format!("{}/../../tests/protocol-llm/fixtures/root.der", env!("CARGO_MANIFEST_DIR"))),
            headers: vec![],
            reasoning_effort: None,
            cache_key: None,
            identity: None,
        });
        let configuration = build(document).expect("resolved endpoint");
        service::Service::new(configuration.service, 1).expect("LLM and channel endpoints");
    }

    #[test]
    fn bad_profile_and_zero_grace_refuse_before_a_channel_is_opened() {
        let mut bad = document();
        bad.profile = "unknown".into();
        assert!(build(bad).is_err());
        let mut bad = document();
        bad.grace_ms = 0;
        assert!(build(bad).is_err());
    }

    #[test]
    fn malformed_json_refuses_missing_required_configuration_fields() {
        assert!(serde_json::from_slice::<Document>(br#"{"profile":"standard"}"#).is_err());
        assert!(serde_json::from_slice::<Document>(br#"{"profile":"standard","memory_bytes":1,"grace_ms":1,"endpoints":[],"environment":[],"credential":"bad"}"#).is_err());
    }
}
