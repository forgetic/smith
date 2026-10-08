//! Smith's LLM component routes io and TLS to skein's fake byte peer in both dialects.
//! Contract: protocol/llm.md, sections 6 and 10.

use core::net::Ipv4Addr;

use skein_fake_llm_domain::{self as fake, api};
use skein_fake_llm_protocol::{documents, provider};
use skein_io::kernel::Addr;
use skein_io::{Event as IoEvent, Request as IoRequest};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Duration, Env, Intake, Queue, Time, Token};
use skein_llm::{Credential, Provider};
use skein_tls_world::{drive::Wire, pki, server::Server};
use smith_domain::{GrantName, llm, run, tools};
use smith_protocol_llm::{
    self as adapter, Component, ComponentLimits, ConfiguredEndpoint, Endpoints, FromDomain, MAX_OUT, Receiving,
    ToDomain,
};

fn limits() -> ComponentLimits {
    let client = skein_llm_world::limits();
    let decoded_call_bytes = 4096;
    ComponentLimits {
        adapter: adapter::Limits { client, tool_bytes: 32768, result_bytes: 32768 },
        connection: skein_llm_connection::Limits {
            endpoints: 1,
            connections: 1,
            per_endpoint: 1,
            idle_keep: Duration::from_secs(10),
            io: skein_io::Limits {
                sockets: 1,
                refusals: 1,
                intake: 19_000,
                receive: 1024,
                output: 19_000,
                sends: 2,
                accepts: 1,
                backlog: 1,
                close_timeout: Duration::from_secs(1),
                retry: Duration::from_millis(10),
            },
            tls: skein_tls::client::Limits { read: 4096, send: 4096, records: skein_tls::client::MAX_RECORD },
            llm: client,
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
        connect: None,
        handshake: None,
        head: None,
        idle: None,
    }
}

#[expect(clippy::too_many_lines, reason = "the protocol story drives the component, io and fake peer")]
fn run(dialect: Provider, calls: bool) {
    let mut call = skein_llm_world::call(7);
    match dialect {
        Provider::OpenAiCodex => {}
        Provider::Anthropic => {
            call.endpoint = skein_llm::Endpoint::anthropic();
            call.credential = Credential::anthropic(b"fake-token".as_slice().into());
            call.prompt.cache_key = None;
        }
    }
    call.prompt.output_ceiling(dialect, 4096).expect("provider output ceiling");
    call.prompt.instructions = b"connection-world".as_slice().into();
    let peer_credential = Credential {
        access_token: call.credential.access_token.clone(),
        account_id: call.credential.account_id.clone(),
    };
    let peer_limits = skein_llm_world::fake::limits(&skein_llm_world::limits());
    let provider_kind = match dialect {
        Provider::OpenAiCodex => documents::Provider::OpenAi,
        Provider::Anthropic => documents::Provider::Anthropic,
    };
    let mut service = provider::Service::new(
        provider::Config { provider: provider_kind, path: call.endpoint.target.clone(), headers: Box::new([]) },
        &peer_limits,
    )
    .expect("fake service");
    let mut peer = provider::Server::new(Token::new(2), &peer_limits).expect("fake server");
    let fake_limits = skein_llm_world::fake::config();
    let lines = if calls {
        Box::new([api::Line::Call {
            name: b"read".as_slice().into(),
            arguments: br#"{"path":"README.md"}"#.as_slice().into(),
        }])
    } else {
        Box::new([api::Line::Text { text: b"scripted answer".as_slice().into() }])
    };
    let scripts = Box::new([api::Script {
        cue: b"connection-world".as_slice().into(),
        turns: Box::new([api::Turn {
            lines,
            finish: if calls { api::Finish::ToolCalls } else { api::Finish::Stop },
            tokens: 2,
        }]),
    }]);
    let mut domain = fake::Domain::try_scripted(&fake_limits, 3, scripts).expect("scripted domain");
    let endpoints = Endpoints::new(
        Box::new([ConfiguredEndpoint {
            name: llm::Endpoint(1),
            destination: skein_llm_connection::Endpoint {
                address: Addr::from((Ipv4Addr::LOCALHOST, 443)),
                transport: skein_llm_connection::Transport::Tls {
                    server_name: skein_tls::Name::new("skein.test").expect("test name"),
                    trust: pki::client(&[]),
                },
                llm: call.endpoint,
            },
            account: 0,
            reasoning_effort: None,
            cache_key: None,
            identity: smith_protocol_llm::IdentityProfile::Plain,
        }]),
        1,
        1,
    )
    .expect("one endpoint");
    let mut component = Component::new(&limits(), endpoints).expect("component");
    component
        .grant(GrantName { account: 0, generation: 1 }, call.credential, Time::from_nanos(10_000_000_000))
        .expect("grant");
    let env = Env { now: Time::ZERO, wall: pki::VALID, limits: limits() };
    let peer_env = Env { now: Time::ZERO, wall: pki::VALID, limits: peer_limits };
    let fake_env = Env { now: Time::ZERO, wall: pki::VALID, limits: fake_limits };
    let mut up = Queue::with_capacity(MAX_OUT.above);
    let mut io = Queue::with_capacity(MAX_OUT.below);
    let mut peer_up = Queue::with_capacity(provider::MAX_UP);
    let mut peer_down = Queue::with_capacity(provider::MAX_DOWN);
    let mut replies = Queue::with_capacity(fake::MAX_OUT);
    let mut to_peer = Intake::with_capacity(32768);
    let mut peer_demand: Option<(Read, u32)> = None;
    let mut grant: Option<u32> = None;
    let mut wire = Wire::new(Server::new(pki::Server::plain().config()));
    let mut received = 0_usize;
    let mut owner = None;
    let mut completed = 0_u32;
    provider::start(&mut peer, &mut service, &peer_credential, &peer_env, &mut peer_up, &mut peer_down);
    component.from_domain(
        &env,
        FromDomain::Complete {
            owner: Token::new(7),
            grant: GrantName { account: 0, generation: 1 },
            prompt: llm::Prompt {
                endpoint: llm::Endpoint(1),
                model: call.prompt.model,
                system: b"connection-world".as_slice().into(),
                tools: tools::Grants { inspect: calls, modify: false, shell: false },
                served: Box::new([]),
                messages: Box::new([llm::Message {
                    role: llm::Role::User,
                    content: Box::new([llm::Block::Text { text: b"hi".as_slice().into(), replay: None }]),
                }]),
                max_tokens: 4096,
            },
            timeout: Duration::from_secs(10),
            bounds: limits().receiving,
            outcome: run::outcome::OutcomeSpec { change: None, verdicts: Box::new([]), report: None, failure: None },
            deliver: None,
        },
        &mut up,
        &mut io,
    );
    for _ in 0_u32..40_000 {
        if let Some(event) = up.pop() {
            match event {
                ToDomain::Text { owner, bytes } => {
                    assert_eq!(owner, Token::new(7));
                    assert!(bytes > 0);
                }
                ToDomain::Completed { owner, completion } => {
                    assert_eq!(owner, Token::new(7));
                    assert!(!completion.content.is_empty());
                    if calls {
                        assert!(completion.content.iter().any(|block| matches!(
                            block,
                            llm::Said::ToolCall { name, call: llm::Decoded::Owned { call: tools::Call::Read { .. } }, .. }
                                if name.as_ref() == b"read"
                        )));
                    } else {
                        assert!(completion.content.iter().any(|block| matches!(
                            block,
                            llm::Said::Text { text, .. } if text.as_ref() == b"scripted answer"
                        )));
                    }
                    completed += 1;
                }
                ToDomain::Failed { .. } | ToDomain::Cancelled { .. } => {
                    panic!("unexpected component event: {event:?}");
                }
            }
        }
        if let Some(request) = io.pop() {
            match request {
                IoRequest::Connect { owner: token, .. } => {
                    assert!(owner.replace(token).is_none());
                    component.from_below(
                        &env,
                        IoEvent::Connecting { owner: token, socket: Token::new(100) },
                        &mut up,
                        &mut io,
                    );
                    component.from_below(&env, IoEvent::Connected { owner: token }, &mut up, &mut io);
                }
                IoRequest::Stream { stream, down } => {
                    assert_eq!(stream, Token::new(100));
                    wire.take(down);
                }
                IoRequest::Close { entity } | IoRequest::Abort { entity } => {
                    assert_eq!(entity, Token::new(100));
                    component.from_below(
                        &env,
                        IoEvent::Closed { owner: owner.expect("connected owner") },
                        &mut up,
                        &mut io,
                    );
                }
                other @ (IoRequest::Listen { .. }
                | IoRequest::Bind { .. }
                | IoRequest::Reject { .. }
                | IoRequest::Output { .. }
                | IoRequest::Spawn { .. }
                | IoRequest::Signal { .. }) => panic!("unexpected io request: {other:?}"),
            }
        }
        if wire.server.received.len() > received {
            to_peer.append(&wire.server.received[received..]).expect("bounded request intake");
            received = wire.server.received.len();
        }
        if let Some(answer) = wire.answer() {
            component.from_below(
                &env,
                IoEvent::Stream { owner: owner.expect("connected owner"), up: answer },
                &mut up,
                &mut io,
            );
        }
        if let Some(event) = peer_up.pop() {
            match event {
                provider::Event::Domain(input) => fake::step(&mut domain, &fake_env, input, &mut replies),
                provider::Event::Close | provider::Event::Closed => {}
            }
        }
        if let Some(reply) = replies.pop() {
            provider::down(&mut peer, &mut service, &peer_credential, &peer_env, reply, &mut peer_up, &mut peer_down);
            domain.reclaim();
            service.reclaim();
        }
        if let Some(down) = peer_down.pop() {
            match down {
                Down::Demand { read: Read::Nothing, room: 0 } => peer_demand = None,
                Down::Demand { read, room } => {
                    assert!(peer_demand.is_none(), "one demand at a time");
                    peer_demand = Some((read, room));
                }
                Down::Send(bytes) => {
                    assert!(bytes.len() <= usize::try_from(grant.take().expect("room grant")).expect("room fits"));
                    wire.server.write(&bytes);
                    wire.pull();
                }
                Down::Finish => wire.server.close_notify(),
            }
        }
        if let Some((read, room)) = peer_demand {
            let answer = match to_peer.meet(read) {
                Some(bytes) => Some(Up::Bytes(bytes)),
                None if room > 0 => {
                    grant = Some(room);
                    Some(Up::Room)
                }
                None => None,
            };
            if let Some(answer) = answer {
                peer_demand = None;
                provider::up(
                    &mut peer,
                    &mut service,
                    &peer_credential,
                    &peer_env,
                    answer,
                    &mut peer_up,
                    &mut peer_down,
                );
            }
        }
        if peer.has_work() {
            provider::resume(&mut peer, &mut service, &peer_credential, &peer_env, &mut peer_up, &mut peer_down);
        }
        if domain.is_due(fake_env.now) {
            fake::fire(&mut domain, &fake_env, &mut replies);
        }
        if component.has_work() {
            component.fire(&env, &mut up, &mut io);
        }
        if completed == 1 && !component.has_work() && io.is_empty() && up.is_empty() && !peer.has_work() {
            break;
        }
    }
    assert_eq!(service.count(), 1, "the independent fake decoded the call");
    assert_eq!(completed, 1);
}

#[test]
fn codex_fake_peer_over_tls() {
    run(Provider::OpenAiCodex, false);
}

#[test]
fn anthropic_fake_peer_over_tls() {
    run(Provider::Anthropic, false);
}

#[test]
fn codex_tool_input_is_typed_through_the_owned_connection() {
    run(Provider::OpenAiCodex, true);
}

#[test]
fn anthropic_tool_input_is_typed_through_the_owned_connection() {
    run(Provider::Anthropic, true);
}
