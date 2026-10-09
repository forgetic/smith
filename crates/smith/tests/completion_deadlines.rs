//! Standard completion timing through the actual adapter and shared HTTP/SSE
//! pool, with virtual clocks. Session-world stories retain outer cancellation.
//! Contract: domain/session.md, sections 4, 6 and 10; protocol/llm.md, section 6.

use std::collections::BTreeMap;

use skein_io::{Event as IoEvent, Request as IoRequest};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Duration, Env, Intake, Queue, Time, Token, Wall};
use smith_domain::{GrantName, llm, run, tools};
use smith_protocol_llm::{Component, ComponentLimits, ConfiguredEndpoint, Endpoints, FromDomain, ToDomain};

fn configuration() -> smith::config::Configuration {
    smith::config::parse(
        br#"{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,"endpoints":[],"environment":[]}"#,
    )
    .expect("production standard profile")
}

struct Binding {
    owner: Token,
    read: Option<Read>,
    response: Intake,
}

struct Peer {
    component: Component,
    env: Env<ComponentLimits>,
    above: Queue<ToDomain>,
    below: Queue<IoRequest>,
    bindings: BTreeMap<Token, Binding>,
    terminals: Vec<ToDomain>,
}

impl Peer {
    fn new() -> Self {
        let limits = configuration().service.limits.llm;
        let endpoints = Endpoints::new(
            Box::new([ConfiguredEndpoint {
                name: llm::Endpoint(0),
                destination: skein_llm_connection::Endpoint {
                    address: "127.0.0.1:443".parse().expect("fixture address"),
                    transport: skein_llm_connection::Transport::Plaintext,
                    llm: skein_llm::Endpoint::codex(),
                },
                account: 0,
                reasoning_effort: None,
                cache_key: None,
                identity: smith_protocol_llm::IdentityProfile::Plain,
            }]),
            limits.connection.endpoints,
            limits.connection.connections,
        )
        .expect("fixture endpoint");
        let mut component = Component::new(&limits, endpoints).expect("production component");
        component
            .grant(
                GrantName { account: 0, generation: 1 },
                skein_llm::Credential {
                    access_token: b"fake".as_slice().into(),
                    account_id: b"fake".as_slice().into(),
                },
                Time::ZERO.saturating_add(Duration::from_secs(1000)),
            )
            .expect("fixture grant");
        Self {
            component,
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            above: Queue::with_capacity(smith_protocol_llm::MAX_OUT.above),
            below: Queue::with_capacity(smith_protocol_llm::MAX_OUT.below),
            bindings: BTreeMap::new(),
            terminals: Vec::new(),
        }
    }

    fn start(&mut self, owner: u64) {
        self.component.from_domain(
            &self.env,
            FromDomain::Complete {
                owner: Token::new(owner),
                grant: GrantName { account: 0, generation: 1 },
                prompt: llm::Prompt {
                    endpoint: llm::Endpoint(0),
                    model: b"fixture".as_slice().into(),
                    system: b"timed reasoning".as_slice().into(),
                    tools: tools::Grants { inspect: true, modify: false, shell: false },
                    served: Box::new([]),
                    messages: Box::new([llm::Message {
                        role: llm::Role::User,
                        content: Box::new([llm::Block::Text { text: b"begin".as_slice().into(), replay: None }]),
                    }]),
                    max_tokens: 4096,
                },
                timeout: configuration().service.limits.domain.session.call_timeout,
                bounds: self.env.limits.receiving,
                outcome: run::outcome::OutcomeSpec {
                    change: None,
                    verdicts: Box::new([]),
                    report: None,
                    failure: None,
                },
                deliver: None,
            },
            &mut self.above,
            &mut self.below,
        );
        self.pump();
    }

    fn pump(&mut self) {
        for _ in 0..10_000 {
            while let Some(request) = self.below.pop() {
                match request {
                    IoRequest::Connect { owner, .. } => {
                        let socket = Token::new(100 + u64::try_from(self.bindings.len()).expect("few bindings"));
                        self.bindings
                            .insert(socket, Binding { owner, read: None, response: Intake::with_capacity(16_384) });
                        self.component.from_below(
                            &self.env,
                            IoEvent::Connecting { owner, socket },
                            &mut self.above,
                            &mut self.below,
                        );
                        self.component.from_below(
                            &self.env,
                            IoEvent::Connected { owner },
                            &mut self.above,
                            &mut self.below,
                        );
                    }
                    IoRequest::Stream { stream, down } => {
                        let binding = self.bindings.get_mut(&stream).expect("known stream");
                        match down {
                            Down::Demand { read, room } => {
                                binding.read = if room == 0 { Some(read) } else { None };
                                if room > 0 {
                                    self.component.from_below(
                                        &self.env,
                                        IoEvent::Stream { owner: binding.owner, up: Up::Room },
                                        &mut self.above,
                                        &mut self.below,
                                    );
                                }
                            }
                            Down::Send(_) | Down::Finish => {}
                        }
                    }
                    IoRequest::Close { entity } | IoRequest::Abort { entity } => {
                        let binding = self.bindings.remove(&entity).expect("one physical close");
                        self.component.from_below(
                            &self.env,
                            IoEvent::Closed { owner: binding.owner },
                            &mut self.above,
                            &mut self.below,
                        );
                    }
                    other => panic!("unexpected request: {other:?}"),
                }
            }
            let mut delivered = false;
            for binding in self.bindings.values_mut() {
                if let Some(read) = binding.read
                    && let Some(bytes) = binding.response.meet(read)
                {
                    binding.read = None;
                    delivered = true;
                    self.component.from_below(
                        &self.env,
                        IoEvent::Stream { owner: binding.owner, up: Up::Bytes(bytes) },
                        &mut self.above,
                        &mut self.below,
                    );
                }
            }
            while let Some(event) = self.above.pop() {
                if !matches!(event, ToDomain::Text { .. }) {
                    self.terminals.push(event);
                }
            }
            self.component.fire(&self.env, &mut self.above, &mut self.below);
            self.component.reclaim();
            if !delivered && !self.component.has_work() && self.above.is_empty() && self.below.is_empty() {
                return;
            }
        }
        panic!("bounded fixture did not settle");
    }

    fn advance(&mut self, seconds: u64) {
        self.env.now = Time::ZERO.saturating_add(Duration::from_secs(seconds));
        self.pump();
    }

    fn bytes(&mut self, socket: u64, bytes: Vec<u8>) {
        assert!(
            self.bindings.contains_key(&Token::new(socket)),
            "live binding {socket}, terminals={:?}",
            self.terminals
        );
        self.bindings
            .get_mut(&Token::new(socket))
            .expect("live binding")
            .response
            .append(&bytes)
            .expect("bounded response intake");
        self.pump();
    }

    fn cancel(&mut self, owner: u64) {
        self.component.from_domain(
            &self.env,
            FromDomain::Cancel { owner: Token::new(owner) },
            &mut self.above,
            &mut self.below,
        );
        self.pump();
    }
}

fn body() -> Vec<u8> {
    let item = serde_json::json!({"type":"function_call","id":"item","call_id":"call","name":"read","arguments":"{\"path\":\"README.md\"}"});
    let mut body = Vec::new();
    for event in [
        serde_json::json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"item","call_id":"call","name":"read","arguments":""}}),
        serde_json::json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"path\":\"README.md\"}"}),
        serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}),
        serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[item],"usage":{"input_tokens":1,"output_tokens":1}}}),
    ] {
        body.extend_from_slice(b"data: ");
        body.extend_from_slice(&serde_json::to_vec(&event).expect("provider event"));
        body.extend_from_slice(b"\n\n");
    }
    body
}

fn head() -> Vec<u8> {
    head_for(body().len())
}

fn head_for(bytes: usize) -> Vec<u8> {
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {bytes}\r\n\r\n").into_bytes()
}

#[test]
fn standard_head_and_reasoning_waits_exceed_a_minute_and_complete_once() {
    let mut peer = Peer::new();
    assert_eq!(configuration().service.limits.domain.session.call_timeout, Duration::from_secs(300));
    assert_eq!(peer.env.limits.head, Some(Duration::from_secs(300)));
    assert_eq!(peer.env.limits.idle, Some(Duration::from_secs(300)));
    peer.start(1);
    peer.start(2);
    peer.bytes(100, head());
    // First call waits after its head; its sibling waits for the head itself.
    peer.advance(120);
    assert!(peer.terminals.is_empty(), "both production waits admit reasoning over 60 seconds: {:?}", peer.terminals);
    peer.bytes(100, body());
    let mut response = head();
    response.extend_from_slice(&body());
    peer.bytes(101, response);
    assert_eq!(peer.terminals.len(), 2);
    assert!(peer.terminals.iter().all(|event| matches!(event, ToDomain::Completed { .. })));
    peer.advance(301);
    assert_eq!(peer.terminals.len(), 2, "expired former timers do not repeat completed terminals");
}

#[test]
fn standard_absolute_deadline_and_cancel_settle_once_without_ending_the_sibling() {
    let mut peer = Peer::new();
    peer.start(1);
    let mut heartbeat = vec![b' '; 513];
    heartbeat[0] = b':';
    heartbeat[511] = b'\n';
    heartbeat[512] = b'\n';
    peer.bytes(100, head_for(body().len() + 2 * heartbeat.len()));
    peer.advance(100);
    peer.bytes(100, heartbeat.to_vec());
    peer.start(2);
    peer.bytes(101, head());
    peer.advance(200);
    peer.bytes(100, heartbeat.to_vec());
    peer.advance(299);
    assert!(peer.terminals.is_empty());
    peer.advance(300);
    assert!(
        matches!(peer.terminals.as_slice(), [ToDomain::Failed { owner, failure: llm::Failure::TimedOut, evidence: llm::Evidence::Response, .. }] if *owner == Token::new(1)),
        "the whole-call cap wins even after activity rearms idleness"
    );
    peer.bytes(101, body());
    assert!(matches!(&peer.terminals[1], ToDomain::Completed { owner, .. } if *owner == Token::new(2)));
    peer.advance(401);
    assert_eq!(peer.terminals.len(), 2);

    let mut peer = Peer::new();
    peer.start(1);
    peer.start(2);
    peer.bytes(100, head());
    peer.bytes(101, head());
    peer.advance(120);
    peer.cancel(1);
    peer.cancel(1);
    peer.bytes(101, body());
    assert_eq!(peer.terminals.len(), 2);
    assert!(matches!(&peer.terminals[0], ToDomain::Cancelled { owner } if *owner == Token::new(1)));
    assert!(matches!(&peer.terminals[1], ToDomain::Completed { owner, .. } if *owner == Token::new(2)));
    peer.advance(301);
    assert_eq!(peer.terminals.len(), 2);
}

#[test]
fn standard_session_outer_budget_cancels_a_slow_call_and_preserves_its_sibling() {
    use smith_session_world::{Settings, World, spec};
    let calm = Settings::calm(903);
    let limits = configuration().service.limits.domain.session;
    let mut world = World::new(Settings {
        agent: limits,
        provider: skein_fake_llm_domain::Config {
            latency_min: Duration::from_secs(120),
            latency_max: Duration::from_secs(120),
            tool_rounds: 0,
            ..calm.provider
        },
        ..calm
    });
    let ordinary = spec(b"slow reasoning");
    let expired = world.submit(
        Time::ZERO,
        smith_domain::session::Spec {
            budget: smith_domain::session::Budget { time: Duration::from_secs(90), ..ordinary.budget },
            ..spec(b"slow reasoning")
        },
    );
    let sibling = world.submit(Time::ZERO, ordinary);
    world.run(100_000);
    assert_eq!(
        world.session(expired).ended.expect("expired terminal").end,
        smith_domain::session::End::Budget { spent: smith_domain::session::Dimension::Time }
    );
    assert_eq!(world.session(sibling).ended.expect("sibling terminal").end, smith_domain::session::End::Closed);
    assert_eq!(world.session(sibling).yields.len(), 1);
    assert_eq!(world.stats().timeouts, 0, "neither call reaches the 300 second completion ceiling");
    assert_eq!((world.stats().cancels, world.stats().late_answers), (1, 1));
    assert_eq!(world.sessions().count(), 2, "the shared referee independently enforces one terminal per session");
}
