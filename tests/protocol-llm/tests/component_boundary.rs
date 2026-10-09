//! Component boundary controls before any peer traffic and across a refresh.

use core::net::SocketAddr;

use skein_io::Request as IoRequest;
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_llm::{self as shared, Credential};
use skein_world::domain::heap::{Counting, Meter};
use smith_domain::{GrantName, llm, run, tools};
use smith_protocol_llm_world::adapter::{
    self as adapter, Component, ComponentLimits, ConfiguredEndpoint, Endpoints, FromDomain, Receiving, ToDomain,
};

#[global_allocator]
static HEAP: Counting = Counting;

fn limits() -> ComponentLimits {
    let client = skein_llm_world::limits();
    let adapter = adapter::Limits {
        client,
        tool_bytes: 32_768,
        rendered_result: client.dialect.string_bytes,
        shell_default: skein_lib::Duration::from_secs(120),
        shell_maximum: skein_lib::Duration::from_secs(1200),
    };
    let decoded_call_bytes = 4096;
    ComponentLimits {
        adapter,
        connection: skein_llm_connection::Limits {
            endpoints: 1,
            connections: 2,
            calls: 2,
            per_endpoint: 2,
            idle_keep: Duration::from_secs(1),
            io: skein_io::Limits {
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
            },
            tls: skein_tls::client::Limits { read: 4096, send: 4096, records: skein_tls::client::MAX_RECORD },
        },
        receiving: Receiving {
            max_completion_bytes: adapter::completion_worst_case(&client, decoded_call_bytes).expect("receiving bound"),
            max_completion_blocks: client.dialect.parts,
            decoded_call_bytes,
            max_failure_bytes: client.dialect.detail_bytes,
        },
        contract_bytes: 4096,
        accounts: 1,
        grant_value_bytes: 128,
        connect: Some(Duration::from_secs(1)),
        handshake: Some(Duration::from_secs(1)),
        head: Some(Duration::from_secs(2)),
        idle: Some(Duration::from_secs(3)),
    }
}

fn endpoints() -> Endpoints {
    let mut roots = skein_tls::RootCertStore::empty();
    roots.add(skein_tls::CertificateDer::from(include_bytes!("../fixtures/root.der").to_vec())).expect("fixture root");
    Endpoints::new(
        Box::new([ConfiguredEndpoint {
            name: llm::Endpoint(42),
            destination: skein_llm_connection::Endpoint {
                limits: limits().adapter.client,
                credential: skein_llm::client::CredentialLimits {
                    access_token: limits().grant_value_bytes,
                    account_id: limits().grant_value_bytes,
                },
                address: SocketAddr::from(([127, 0, 0, 1], 443)),
                transport: skein_llm_connection::Transport::Tls {
                    server_name: skein_tls::Name::new("example.test").expect("server name"),
                    trust: skein_tls::Config::new(roots, &[]).expect("test trust"),
                },
                llm: shared::Endpoint::codex(),
            },
            account: 0,
            reasoning_effort: None,
            cache_key: None,
            identity: smith_protocol_llm::IdentityProfile::Plain,
        }]),
        1,
        1,
    )
    .expect("one endpoint")
}

fn prompt() -> llm::Prompt {
    llm::Prompt {
        endpoint: llm::Endpoint(42),
        model: b"fixture-model".as_slice().into(),
        system: b"testing component".as_slice().into(),
        tools: tools::Grants { inspect: false, modify: false, shell: false },
        served: Box::new([]),
        messages: Box::new([llm::Message {
            role: llm::Role::User,
            content: Box::new([llm::Block::Text { text: b"hello".as_slice().into(), replay: None }]),
        }]),
        max_tokens: 128,
    }
}

fn complete(owner: u64, generation: u64, bounds: Receiving) -> FromDomain {
    FromDomain::Complete {
        owner: Token::new(owner),
        grant: GrantName { account: 0, generation },
        prompt: prompt(),
        timeout: Duration::from_secs(10),
        bounds,
        outcome: run::outcome::OutcomeSpec { change: None, verdicts: Box::new([]), report: None, failure: None },
        deliver: None,
    }
}

fn env(now: u64, limits: &ComponentLimits) -> Env<ComponentLimits> {
    Env { now: Time::from_nanos(now), wall: Wall::from_nanos(1_893_456_000_000_000_000), limits: *limits }
}

fn credential(value: &[u8]) -> Credential {
    Credential { access_token: value.into(), account_id: b"account".as_slice().into() }
}

#[test]
fn a_lapsed_grant_fails_unsent_and_refresh_starts_a_call() {
    let limits = limits();
    let mut component = Component::new(&limits, endpoints()).expect("component");
    let mut up = Queue::with_capacity(adapter::MAX_OUT.above);
    let mut io = Queue::with_capacity(adapter::MAX_OUT.below);
    component
        .grant(GrantName { account: 0, generation: 1 }, credential(b"old"), Time::from_nanos(10))
        .expect("first grant");
    component.from_domain(&env(11, &limits), complete(7, 1, limits.receiving), &mut up, &mut io);
    assert!(io.is_empty(), "lapsed grant never touches io");
    assert!(matches!(
        up.pop(),
        Some(ToDomain::Failed {
            owner,
            failure: llm::Failure::Unauthorized,
            evidence: llm::Evidence::Unsent,
            ..
        }) if owner == Token::new(7)
    ));
    component
        .grant(GrantName { account: 0, generation: 2 }, credential(b"fresh"), Time::from_nanos(100))
        .expect("refresh");
    component.from_domain(&env(12, &limits), complete(8, 2, limits.receiving), &mut up, &mut io);
    assert!(matches!(io.pop(), Some(IoRequest::Connect { .. })), "refreshed grant starts a connection");
    assert!(up.is_empty());
}

#[test]
fn a_grant_refreshed_between_calls_preserves_the_first_start() {
    let limits = limits();
    let mut component = Component::new(&limits, endpoints()).expect("component");
    let mut up = Queue::with_capacity(adapter::MAX_OUT.above);
    let mut io = Queue::with_capacity(adapter::MAX_OUT.below);
    component
        .grant(GrantName { account: 0, generation: 1 }, credential(b"first"), Time::from_nanos(100))
        .expect("first grant");
    component.from_domain(&env(1, &limits), complete(7, 1, limits.receiving), &mut up, &mut io);
    assert!(matches!(io.pop(), Some(IoRequest::Connect { .. })));
    component
        .grant(GrantName { account: 0, generation: 2 }, credential(b"second"), Time::from_nanos(100))
        .expect("refresh");
    component.from_domain(&env(2, &limits), complete(8, 2, limits.receiving), &mut up, &mut io);
    assert!(matches!(io.pop(), Some(IoRequest::Connect { .. })));
    assert!(up.is_empty(), "both calls accepted with distinct generations");
}

#[test]
fn a_connect_deadline_keeps_timed_out_failure_evidence() {
    let limits = limits();
    let mut component = Component::new(&limits, endpoints()).expect("component");
    let mut up = Queue::with_capacity(adapter::MAX_OUT.above);
    let mut io = Queue::with_capacity(adapter::MAX_OUT.below);
    component
        .grant(GrantName { account: 0, generation: 1 }, credential(b"first"), Time::from_nanos(20_000_000_000))
        .expect("grant");
    component.from_domain(&env(0, &limits), complete(7, 1, limits.receiving), &mut up, &mut io);
    assert!(matches!(io.pop(), Some(IoRequest::Connect { .. })));
    component.fire(&env(1_000_000_001, &limits), &mut up, &mut io);
    assert!(matches!(
        up.pop(),
        Some(ToDomain::Failed {
            owner,
            failure: llm::Failure::TimedOut,
            evidence: llm::Evidence::Unsent,
            ..
        }) if owner == Token::new(7)
    ));
}

#[test]
fn every_shared_failure_class_keeps_each_evidence_and_detail() {
    let limits = limits();
    let retry = Duration::from_secs(7);
    let cases = [
        (shared::Failure::Unauthorized, llm::Failure::Unauthorized),
        (shared::Failure::Exhausted { retry_after: retry }, llm::Failure::Exhausted { retry_after: retry }),
        (shared::Failure::RateLimited { retry_after: retry }, llm::Failure::RateLimited { retry_after: retry }),
        (shared::Failure::Overloaded, llm::Failure::Overloaded),
        (shared::Failure::Unavailable, llm::Failure::Unavailable),
        (shared::Failure::ContextTooLong, llm::Failure::ContextTooLong),
        (shared::Failure::Invalid, llm::Failure::Invalid),
        (shared::Failure::Limit, llm::Failure::Limit),
        (shared::Failure::Protocol, llm::Failure::Protocol),
        (shared::Failure::Cancelled, llm::Failure::Cancelled),
        (shared::Failure::TimedOut, llm::Failure::TimedOut),
    ];
    let evidence = [
        (shared::client::Evidence::Unsent, llm::Evidence::Unsent),
        (shared::client::Evidence::Unknown, llm::Evidence::Unknown),
        (shared::client::Evidence::Response, llm::Evidence::Response),
    ];
    for (source, expected) in cases {
        for (sent, expected_sent) in evidence {
            let input = adapter::Input {
                owner: Token::new(71),
                prompt: prompt(),
                endpoint_name: llm::Endpoint(42),
                endpoint: shared::Endpoint::codex(),
                credential: credential(b"secret"),
                application: Box::new([]),
                receiving: limits.receiving,
            };
            let prepared = adapter::prepare(input, &limits.adapter).expect("bounded preparation");
            let event =
                adapter::failed(prepared.context, Token::new(71), source, sent, b"literal detail".as_slice().into())
                    .expect("matching terminal");
            assert!(matches!(
                event,
                smith_domain::Event::Failed { owner, failure, evidence, detail }
                    if owner == Token::new(71)
                        && failure == expected
                        && evidence == expected_sent
                        && detail.as_ref() == b"literal detail"
            ));
        }
    }
}

#[test]
fn a_full_connection_pool_and_two_grant_generations_fit_the_component_bound() {
    let limits = limits();
    let destinations = endpoints();
    let component_bound = adapter::component_worst_case(&limits, &destinations).expect("component worst case");
    let first = complete(7, 1, limits.receiving);
    let second = complete(8, 2, limits.receiving);
    let first_credential = credential(b"first");
    let second_credential = credential(b"second");
    let meter = Meter::new();
    meter.start();
    let mut component = Component::new(&limits, destinations).expect("bounded component");
    let mut up = Queue::with_capacity(adapter::MAX_OUT.above);
    let mut io = Queue::with_capacity(adapter::MAX_OUT.below);
    component
        .grant(GrantName { account: 0, generation: 1 }, first_credential, Time::from_nanos(100))
        .expect("first grant");
    component
        .grant(GrantName { account: 0, generation: 2 }, second_credential, Time::from_nanos(100))
        .expect("second generation");
    component.from_domain(&env(1, &limits), first, &mut up, &mut io);
    component.from_domain(&env(2, &limits), second, &mut up, &mut io);
    let measured = meter.end();
    assert_eq!(io.len(), 2, "the pool holds two concurrent connections");
    let bound = component_bound
        + Queue::<ToDomain>::worst_case(adapter::MAX_OUT.above).expect("domain queue")
        + Queue::<IoRequest>::worst_case(adapter::MAX_OUT.below).expect("io queue");
    assert!(measured.peak() <= bound, "component peak {} exceeds {bound}", measured.peak());
}

#[test]
fn a_tls_handshake_deadline_stops_the_unsent_call() {
    let limits = limits();
    let mut component = Component::new(&limits, endpoints()).expect("component");
    let mut up = Queue::with_capacity(adapter::MAX_OUT.above);
    let mut io = Queue::with_capacity(adapter::MAX_OUT.below);
    component
        .grant(GrantName { account: 0, generation: 1 }, credential(b"first"), Time::from_nanos(20_000_000_000))
        .expect("grant");
    component.from_domain(&env(0, &limits), complete(7, 1, limits.receiving), &mut up, &mut io);
    let owner = match io.pop() {
        Some(IoRequest::Connect { owner, .. }) => owner,
        other => panic!("expected connect, got {other:?}"),
    };
    component.from_below(
        &env(0, &limits),
        skein_io::Event::Connecting { owner, socket: Token::new(99) },
        &mut up,
        &mut io,
    );
    component.from_below(&env(0, &limits), skein_io::Event::Connected { owner }, &mut up, &mut io);
    component.fire(&env(1_000_000_001, &limits), &mut up, &mut io);
    assert!(matches!(
        up.pop(),
        Some(ToDomain::Failed {
            owner,
            failure: llm::Failure::TimedOut,
            evidence: llm::Evidence::Unsent,
            ..
        }) if owner == Token::new(7)
    ));
}

#[test]
fn the_whole_deadline_is_armed_even_without_phase_deadlines() {
    let mut limits = limits();
    limits.connect = None;
    limits.handshake = None;
    limits.head = None;
    limits.idle = None;
    let mut component = Component::new(&limits, endpoints()).expect("component");
    let mut up = Queue::with_capacity(adapter::MAX_OUT.above);
    let mut io = Queue::with_capacity(adapter::MAX_OUT.below);
    component
        .grant(GrantName { account: 0, generation: 1 }, credential(b"first"), Time::from_nanos(20_000_000_000))
        .expect("grant");
    component.from_domain(&env(0, &limits), complete(7, 1, limits.receiving), &mut up, &mut io);
    assert_eq!(component.next_deadline(), Some(Time::from_nanos(10_000_000_000)));
    component.fire(&env(10_000_000_001, &limits), &mut up, &mut io);
    assert!(matches!(
        up.pop(),
        Some(ToDomain::Failed { failure: llm::Failure::TimedOut, evidence: llm::Evidence::Unsent, .. })
    ));
}
