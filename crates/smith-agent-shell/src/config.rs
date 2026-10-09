//! JSON configuration read once before the agent opens its channel.
//! It retains endpoint names and destinations, limits, command environment,
//! and future trace settings; no credential is read here.
//! Contract: protocol/agent.md, sections 4 and 6.

use std::fs;
use std::io::Read;
use std::net::{SocketAddr, ToSocketAddrs};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use serde::Deserialize;
use skein_channel::StreamMode;
use skein_http::Header;
use skein_lib::{Duration, List};
use skein_llm as shared;
use skein_llm_connection as connection;
use skein_tls as tls;
use smith_agent_service as service;
use smith_domain::{self as domain, run, tools};
use smith_protocol_channel as channel;
use smith_protocol_llm as llm;

use crate::trace::{Capture, TraceConfig};
use smith_agent_service::profile::{ACCOUNTS, ENDPOINTS, ProfileError};

const CONFIG_BYTES: u64 = 1 << 20;

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
    #[serde(default = "tls_transport")]
    transport: String,
    #[serde(default)]
    server_name: Option<String>,
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
    if u64::try_from(bytes.len()).expect("byte length fits u64") > CONFIG_BYTES {
        return Err(format!("configuration is larger than {CONFIG_BYTES} bytes"));
    }
    parse(&bytes)
}

/// Validate generated child configuration before the local host spawns it.
pub fn parse(bytes: &[u8]) -> Result<Configuration, String> {
    if u64::try_from(bytes.len()).expect("byte length fits u64") > CONFIG_BYTES {
        return Err(format!("configuration is larger than {CONFIG_BYTES} bytes"));
    }
    let document: Document = serde_json::from_slice(bytes).map_err(|error| format!("configuration JSON: {error}"))?;
    build(document)
}

fn build(document: Document) -> Result<Configuration, String> {
    if document.profile != "standard" {
        return Err("configuration profile must be standard".into());
    }
    if document.endpoints.len() > usize::try_from(ENDPOINTS).expect("endpoint count fits usize") {
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
    let mut limits = service::profile::standard_limits(document.memory_bytes).map_err(profile_refusal)?;
    limits.machine.stop_grace = Duration::from_millis(document.grace_ms);
    let endpoints = build_endpoints(document.endpoints)?;
    let mut environment = Vec::with_capacity(document.environment.len());
    let mut environment_bytes = 0_u64;
    for variable in document.environment {
        if variable.name.is_empty() || variable.name.contains(['=', '\0']) || variable.value.contains('\0') {
            return Err("invalid command environment variable".into());
        }
        let entry_bytes = variable
            .name
            .len()
            .checked_add(variable.value.len())
            .and_then(|bytes| bytes.checked_add(1))
            .ok_or("environment is too large")?;
        environment_bytes = environment_bytes
            .checked_add(u64::try_from(entry_bytes).map_err(|_| "environment is too large")?)
            .ok_or("environment is too large")?;
        environment
            .push(tools::Var { name: variable.name.into_bytes().into(), value: variable.value.into_bytes().into() });
    }
    if environment_bytes > u64::from(limits.machine.env_bytes) {
        return Err("command environment exceeds the profile".into());
    }
    let llm_endpoints =
        llm::Endpoints::new(endpoints.llm, ENDPOINTS, ACCOUNTS).map_err(|error| format!("LLM endpoints: {error:?}"))?;
    limits.memory = document.memory_bytes;
    let config = service::Config {
        limits,
        domain: domain::Config { endpoints: endpoints.domain },
        channel_endpoints: endpoints.channel,
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

struct PreparedEndpoints {
    domain: Box<[run::charter::Endpoint]>,
    channel: channel::Endpoints,
    llm: Box<[llm::ConfiguredEndpoint]>,
}

fn build_endpoints(endpoints: Vec<Endpoint>) -> Result<PreparedEndpoints, String> {
    let mut domain_endpoints = Vec::with_capacity(endpoints.len());
    let mut channel_endpoints = List::with_capacity(ENDPOINTS);
    let mut llm_endpoints = Vec::with_capacity(endpoints.len());
    let mut names = std::collections::BTreeSet::new();
    let mut numbers = std::collections::BTreeSet::new();
    for endpoint in endpoints {
        if endpoint.name.is_empty()
            || endpoint.name.len()
                > usize::try_from(smith_charter::CEILINGS.llm_endpoint).expect("endpoint name bound fits usize")
        {
            return Err("endpoint name is empty or too long".into());
        }
        if endpoint.account >= ACCOUNTS {
            return Err("endpoint account is outside the profile".into());
        }
        if !names.insert(endpoint.name.clone()) || !numbers.insert(endpoint.number) {
            return Err("duplicate endpoint name or number".into());
        }
        let address = resolve(&endpoint.address)?;
        let transport = match endpoint.transport.as_str() {
            "tls" => connection::Transport::Tls {
                server_name: tls::Name::new(
                    endpoint.server_name.as_deref().ok_or("TLS endpoint requires server_name")?,
                )
                .ok_or("invalid TLS server name")?,
                trust: trust(endpoint.trust_der.as_deref())?,
            },
            "plaintext" => {
                if !address.ip().is_loopback() {
                    return Err("plaintext endpoint address must be loopback".into());
                }
                if endpoint.server_name.is_some() || endpoint.trust_der.is_some() {
                    return Err("plaintext endpoint cannot name TLS server_name or trust_der".into());
                }
                connection::Transport::Plaintext
            }
            _ => return Err("endpoint transport must be tls or plaintext".into()),
        };
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
            destination: connection::Endpoint { address, transport, llm: llm_endpoint },
            account: endpoint.account,
            reasoning_effort: endpoint.reasoning_effort.map(|value| value.into_bytes().into()),
            cache_key: endpoint.cache_key.map(|value| value.into_bytes().into()),
            identity,
        });
    }
    Ok(PreparedEndpoints {
        domain: domain_endpoints.into_boxed_slice(),
        channel: channel::Endpoints::new(channel_endpoints),
        llm: llm_endpoints.into_boxed_slice(),
    })
}

fn tls_transport() -> String {
    "tls".into()
}

fn resolve(address: &str) -> Result<SocketAddr, String> {
    let mut resolved = address.to_socket_addrs().map_err(|error| format!("endpoint address {address:?}: {error}"))?;
    resolved.next().ok_or_else(|| format!("endpoint address {address:?} has no result"))
}

// Startup trust material and decoded configuration are bounded together.
/// Bound on startup trust material and decoded configuration, in bytes.
pub const TRUST_BYTES: u64 = 16_777_216;

/// Load the bounded startup trust roots used by agent and local endpoints.
pub fn trust(der: Option<&str>) -> Result<tls::Config, String> {
    let mut roots = tls::RootCertStore::empty();
    if let Some(path) = der {
        #[expect(clippy::disallowed_types, reason = "10-product uses skein startup reads and trust roots")]
        let file: fs::File = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|error| format!("trust certificate {path:?}: {error}"))?;
        let metadata = file.metadata().map_err(|error| format!("trust certificate metadata: {error}"))?;
        if !metadata.is_file() || metadata.len() > 65_536 {
            return Err("trust certificate must be a regular file of at most 65536 bytes".into());
        }
        let mut bytes = Vec::new();
        file.take(65_537).read_to_end(&mut bytes).map_err(|error| format!("trust certificate read: {error}"))?;
        if bytes.len() > 65_536 {
            return Err("trust certificate exceeds 65536 bytes".into());
        }
        roots.add(tls::CertificateDer::from(bytes)).map_err(|error| format!("trust certificate: {error}"))?;
    } else {
        let native = rustls_native_certs::load_native_certs();
        if native.certs.len() > 512 || native.certs.iter().any(|cert| cert.len() > 8192) {
            return Err("native trust store exceeds its certificate bounds".into());
        }
        for cert in native.certs {
            roots.add(cert).map_err(|error| format!("native trust certificate: {error}"))?;
        }
    }
    tls::Config::new(roots, &[]).map_err(|error| format!("TLS trust: {error:?}"))
}

fn profile_refusal(error: ProfileError) -> String {
    match error {
        ProfileError::Receiving => "LLM receiving bound overflows".into(),
        ProfileError::ChannelSchema(error) => format!("channel schema: {error:?}"),
        ProfileError::ChannelVersion => "channel version 1 is absent".into(),
        ProfileError::ChannelFrame => "channel frame size overflows".into(),
        ProfileError::IoRoutes => "IO route count overflows".into(),
    }
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
    fn named_endpoint_resolves_and_its_tls_trust_builds_before_start() {
        let mut document = document();
        document.endpoints.push(Endpoint {
            name: "test".into(),
            number: 42,
            dialect: 1,
            account: 0,
            provider: "codex".into(),
            address: "127.0.0.1:443".into(),
            transport: "tls".into(),
            server_name: Some("example.test".into()),
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
    fn plaintext_configuration_names_its_transport_and_admits_only_loopback() {
        for address in ["127.0.0.1:8080", "[::1]:8080"] {
            let json = format!(
                r#"{{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,
                "endpoints":[{{"name":"local","number":1,"dialect":1,"account":0,"provider":"codex",
                "address":"{address}","transport":"plaintext"}}],"environment":[]}}"#
            );
            let configuration = parse(json.as_bytes()).expect("loopback plaintext configuration");
            service::Service::new(configuration.service, 1).expect("loopback plaintext service");
        }
        for address in ["192.0.2.1:8080", "[2001:db8::1]:8080"] {
            let json = format!(
                r#"{{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,
                "endpoints":[{{"name":"local","number":1,"dialect":1,"account":0,"provider":"codex",
                "address":"{address}","transport":"plaintext"}}],"environment":[]}}"#
            );
            assert_eq!(parse(json.as_bytes()).err().as_deref(), Some("plaintext endpoint address must be loopback"));
        }
    }

    #[test]
    fn ambiguous_or_unknown_endpoint_transports_refuse_at_configuration() {
        for extra in [
            r#""transport":"plaintext","server_name":"example.test""#,
            r#""transport":"plaintext","trust_der":"missing.der""#,
            r#""transport":"unknown""#,
            r#""transport":"tls""#,
        ] {
            let json = format!(
                r#"{{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,
                "endpoints":[{{"name":"local","number":1,"dialect":1,"account":0,"provider":"codex",
                "address":"127.0.0.1:8080",{extra}}}],"environment":[]}}"#
            );
            assert!(parse(json.as_bytes()).is_err(), "invalid transport {extra}");
        }
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
