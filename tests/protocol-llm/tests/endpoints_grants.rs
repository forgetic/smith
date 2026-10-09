//! Startup endpoint mapping and two-generation credential table.

use core::net::SocketAddr;

use skein_lib::Time;
use skein_llm::{self as shared, Credential};
use smith_domain::{GrantName, llm};
use smith_protocol_llm::{ConfiguredEndpoint, EndpointError, Endpoints, GrantError, Grants};

fn destination(name: u32, account: u32, provider: shared::Provider) -> ConfiguredEndpoint {
    let mut roots = skein_tls::RootCertStore::empty();
    roots
        .add(skein_tls::CertificateDer::from(include_bytes!("../fixtures/root.der").to_vec()))
        .expect("fixture TLS root");
    let llm = match provider {
        shared::Provider::OpenAiCodex => shared::Endpoint::codex(),
        shared::Provider::Anthropic => shared::Endpoint::anthropic(),
    };
    ConfiguredEndpoint {
        name: llm::Endpoint(name),
        destination: skein_llm_connection::Endpoint {
            limits: skein_llm_world::limits(),
            credential: skein_llm::client::CredentialLimits { access_token: 64, account_id: 64 },
            address: SocketAddr::from(([127, 0, 0, 1], 443)),
            transport: skein_llm_connection::Transport::Tls {
                server_name: skein_tls::Name::new("example.test").expect("server name"),
                trust: skein_tls::Config::new(roots, &[]).expect("test trust"),
            },
            llm,
        },
        account,
        reasoning_effort: Some(b"medium".as_slice().into()),
        cache_key: Some(b"run-cache".as_slice().into()),
        identity: smith_protocol_llm::IdentityProfile::Plain,
    }
}

#[test]
fn numeric_names_resolve_the_complete_destination_and_options() {
    let endpoints = Endpoints::new(
        Box::new([destination(42, 1, shared::Provider::OpenAiCodex), destination(7, 0, shared::Provider::Anthropic)]),
        2,
        2,
    )
    .expect("two named endpoints");
    let codex = endpoints.resolve(llm::Endpoint(42)).expect("configured name");
    assert_eq!(codex.index, 0);
    assert_eq!(codex.account, 1);
    assert_eq!(codex.provider, shared::Provider::OpenAiCodex);
    assert_eq!(endpoints.resolve(llm::Endpoint(7)).expect("second name").index, 1);
    let mut prompt = shared::Prompt {
        model: b"model".as_slice().into(),
        instructions: Box::new([]),
        tools: Box::new([]),
        choice: shared::ToolChoice::Auto,
        messages: Box::new([]),
        reasoning_effort: None,
        cache_key: None,
        max_output_tokens: Some(32),
    };
    assert_eq!(endpoints.apply(llm::Endpoint(42), &mut prompt), Ok(0));
    assert_eq!(prompt.reasoning_effort.as_deref(), Some(b"medium".as_slice()));
    assert_eq!(prompt.cache_key.as_deref(), Some(b"run-cache".as_slice()));
    assert_eq!(endpoints.apply(llm::Endpoint(3), &mut prompt), Err(EndpointError::Unknown));
    let (destinations, options) = endpoints.into_parts();
    assert_eq!(destinations.len(), 2);
    assert_eq!(options.get(&42).expect("named options").index, 0);
    assert_eq!(destinations.get(1).expect("second destination").llm.provider, shared::Provider::Anthropic);
}

#[test]
fn duplicate_names_and_out_of_range_accounts_fail_at_startup() {
    assert!(matches!(
        Endpoints::new(
            Box::new([
                destination(9, 0, shared::Provider::Anthropic),
                destination(9, 0, shared::Provider::OpenAiCodex),
            ]),
            2,
            1,
        ),
        Err(EndpointError::Duplicate)
    ));
    assert!(matches!(
        Endpoints::new(Box::new([destination(9, 1, shared::Provider::Anthropic)]), 1, 1),
        Err(EndpointError::Account)
    ));
    assert!(matches!(
        Endpoints::new(Box::new([destination(9, 0, shared::Provider::Anthropic)]), 0, 1),
        Err(EndpointError::TooMany)
    ));
}

fn credential(value: &[u8], account_id: &[u8]) -> Credential {
    Credential { access_token: value.into(), account_id: account_id.into() }
}

#[test]
fn a_refresh_preserves_the_first_call_then_evicts_the_oldest_generation() {
    let mut grants = Grants::new(1, 64).expect("bounded table");
    let first = GrantName { account: 0, generation: 1 };
    let second = GrantName { account: 0, generation: 2 };
    let third = GrantName { account: 0, generation: 3 };
    grants.grant(first, credential(b"first", b"account-id"), Time::from_nanos(100)).expect("first grant");
    let started = grants.read(first, Time::from_nanos(10)).expect("call reads its starting value");
    grants.grant(second, credential(b"second", b"account-id"), Time::from_nanos(200)).expect("refresh");
    assert_eq!(started.access_token.as_ref(), b"first");
    assert_eq!(grants.read(second, Time::from_nanos(20)).expect("next call").access_token.as_ref(), b"second");
    assert_eq!(grants.read(first, Time::from_nanos(20)).expect("overlapping first").access_token.as_ref(), b"first");
    grants.grant(third, credential(b"third", b"account-id"), Time::from_nanos(300)).expect("next refresh");
    assert!(matches!(grants.read(first, Time::from_nanos(20)), Err(GrantError::Missing)));
    assert!(matches!(grants.read(second, Time::from_nanos(201)), Err(GrantError::Lapsed)));
    assert_eq!(grants.read(third, Time::from_nanos(201)).expect("new value").access_token.as_ref(), b"third");
    assert!(matches!(grants.grant(second, credential(b"stale", b""), Time::from_nanos(400)), Err(GrantError::Stale)));
    assert!(Grants::worst_case(1, 64).expect("bounded footprint") >= 128);
    assert!(!format!("{grants:?}").contains("third"), "debug hides bearer values");
}

#[test]
fn configured_claude_identity_precedes_domain_instructions_only_for_anthropic() {
    let mut anthropic = destination(8, 0, shared::Provider::Anthropic);
    anthropic.identity = smith_protocol_llm::IdentityProfile::ClaudeCode;
    let endpoints = Endpoints::new(Box::new([anthropic]), 1, 1).expect("provider supports identity");
    let mut prompt = shared::Prompt {
        model: b"model".as_slice().into(),
        instructions: b"domain system".as_slice().into(),
        tools: Box::new([]),
        choice: shared::ToolChoice::Auto,
        messages: Box::new([]),
        reasoning_effort: None,
        cache_key: None,
        max_output_tokens: Some(32),
    };
    assert_eq!(endpoints.apply(llm::Endpoint(8), &mut prompt), Ok(0));
    assert!(prompt.instructions.starts_with(shared::anthropic::identity::CLAUDE_CODE_SYSTEM_IDENTITY));
    assert!(prompt.instructions.ends_with(b"domain system"));
    let mut codex = destination(9, 0, shared::Provider::OpenAiCodex);
    codex.identity = smith_protocol_llm::IdentityProfile::ClaudeCode;
    assert!(matches!(Endpoints::new(Box::new([codex]), 1, 1), Err(EndpointError::Identity)));
}
