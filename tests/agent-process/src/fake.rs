//! Scenario configuration and observations of Skein's independent LLM peer.
//! It keeps no transport state or service state; the peer owns both byte
//! routing and its script. Contract: testing.md, sections 4 and 5.
use skein_fake_llm_domain::{self as domain, api};
use skein_fake_llm_protocol::{documents, provider};
use skein_fake_peers::{Limits, Transport, llm};
use skein_lib::Duration;

/// Bounded loopback transport and observation settings for Smith's peers.
#[must_use]
pub fn limits() -> Limits {
    Limits {
        io: skein_io::Limits { sockets: 8, intake: 32_768, output: 65_536, ..super::limits().io },
        connections: 4,
        queue: 256,
        plaintext: 32_768,
        ciphertext: 65_536,
        observations: 256,
        observation_bytes: 4_194_304,
    }
}

/// The one-turn agent fixture's provider response, sent on plaintext loopback.
#[must_use]
pub fn peer() -> llm::Peer {
    configured(
        Box::new([api::Script {
            cue: Box::new([]),
            turns: Box::new([api::Turn {
                lines: Box::new([api::Line::Text { text: b"Hello".as_slice().into() }]),
                finish: api::Finish::Stop,
                tokens: 1,
            }]),
        }]),
        skein_llm::Credential { access_token: b"token".as_slice().into(), account_id: b"acc".as_slice().into() },
        Duration::ZERO,
    )
}

/// Build a seeded plaintext loopback peer for a local scenario.
#[must_use]
pub fn configured(scripts: Box<[api::Script]>, credential: skein_llm::Credential, latency: Duration) -> llm::Peer {
    let mut config = skein_llm_world::fake::config();
    config.latency_min = latency;
    config.latency_max = latency;
    let mut protocol = skein_llm_world::fake::limits(&skein_llm_world::limits());
    protocol.documents.openai.request_bytes = 32_768;
    protocol.documents.openai.document_bytes = 32_768;
    protocol.documents.openai.string_bytes = 16_384;
    protocol.documents.openai.tokens = 4096;
    protocol.documents.openai.parts = 256;
    llm::Peer::new(
        (std::net::Ipv4Addr::LOCALHOST, 443).into(),
        Transport::Plaintext,
        limits(),
        provider::Config {
            provider: documents::Provider::OpenAi,
            path: skein_llm_world::call(1).endpoint.target,
            headers: Box::new([]),
        },
        credential,
        protocol,
        domain::Domain::try_scripted(&config, 17, scripts).expect("bounded scenario scripts"),
        config,
    )
    .expect("configured independent peer")
}

/// Decoded outside requests emitted by the independent byte peer.
pub fn queries(peer: &llm::Peer) -> impl Iterator<Item = &api::Query> {
    peer.observations().iter().filter_map(|observation| match observation {
        llm::Observation::Query { query, .. } => Some(query),
        llm::Observation::Accepted { .. } | llm::Observation::Answered { .. } | llm::Observation::Closed { .. } => None,
    })
}

/// Whether the actual script domain emitted a successful terminal.
#[must_use]
pub fn replied(peer: &llm::Peer) -> bool {
    peer.observations()
        .iter()
        .any(|observation| matches!(observation, llm::Observation::Answered { result: Ok(()), .. }))
}
