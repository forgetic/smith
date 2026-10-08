//! Skein's independent provider codec and fake LLM domain over TLS sockets.
//! This Host keeps bounded connection and script state, no simulator. Its
//! query observations are the referee's only view of model requests.

use std::collections::{BTreeMap, VecDeque};

use skein_fake_llm_domain::{self as fake, api};
use skein_fake_llm_protocol::{documents, provider};
use skein_io::{self as io, kernel};
use skein_lib::stream::{Down, OutputDown, OutputOutcome, OutputUp, Read, Up};
use skein_lib::{Duration, Env, Intake, Queue, Time, Token, Wall};
use skein_llm::Credential;
use skein_tls_world::{pki, server::Server};
use skein_world::Host;

struct Connection {
    socket: Token,
    tls: Server,
    server: provider::Server,
    intake: Intake,
    outgoing: VecDeque<u8>,
    header: Option<Box<[u8]>>,
    wanted: u32,
    reading: bool,
    ended: bool,
    closing: bool,
    right: Option<(Token, usize)>,
    next_right: u64,
    demand: Option<(Read, u32)>,
    grant: u32,
    above: Queue<provider::Event>,
    below: Queue<Down>,
}

/// Hosted fake LLM whose protocol and conversation policy are owned by Skein.
pub struct Peer {
    io: io::Io,
    env: Env<io::Limits>,
    service: provider::Service,
    provider_env: Env<provider::Limits>,
    fake: fake::Domain,
    fake_env: Env<fake::Config>,
    credential: Credential,
    replies: Queue<fake::Request>,
    queries: Vec<api::Query>,
    connections: BTreeMap<Token, Connection>,
    listener: Option<Token>,
    next: u64,
    stopped: bool,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    worst: u64,
}

impl Peer {
    #[must_use]
    pub fn new(scripts: Box<[api::Script]>, credential: Credential, latency: Duration) -> Self {
        let limits = io::Limits {
            sockets: 8,
            refusals: 1,
            intake: 19_000,
            receive: 4096,
            output: 32_768,
            sends: 4,
            accepts: 1,
            backlog: 2,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(1),
        };
        let mut provider_limits = skein_llm_world::fake::limits(&skein_llm_world::limits());
        provider_limits.documents.openai.request_bytes = 32_768;
        provider_limits.documents.openai.document_bytes = 32_768;
        provider_limits.documents.openai.string_bytes = 16_384;
        provider_limits.documents.openai.tokens = 4096;
        provider_limits.documents.openai.parts = 256;
        let mut config = skein_llm_world::fake::config();
        config.latency_min = latency;
        config.latency_max = latency;
        let service = provider::Service::new(
            provider::Config {
                provider: documents::Provider::OpenAi,
                path: skein_llm_world::call(1).endpoint.target,
                headers: Box::new([]),
            },
            &provider_limits,
        )
        .expect("independent provider");
        let mut requests = Queue::with_capacity(256);
        requests.push(io::Request::Listen {
            owner: Token::new(1),
            addr: kernel::Addr::from((std::net::Ipv4Addr::LOCALHOST, 443)),
        });
        let worst = io::worst_case(&limits).expect("IO bound")
            + provider::worst_case(&provider_limits).expect("provider bound") * 4
            + fake::worst_case(&config).expect("script bound")
            + 16_000_000;
        Self {
            io: io::Io::new(&limits),
            env: Env { now: Time::ZERO, wall: pki::VALID, limits },
            service,
            provider_env: Env { now: Time::ZERO, wall: pki::VALID, limits: provider_limits },
            fake: fake::Domain::try_scripted(&config, 17, scripts).expect("bounded scripts"),
            fake_env: Env { now: Time::ZERO, wall: pki::VALID, limits: config },
            credential,
            replies: Queue::with_capacity(fake::MAX_OUT),
            queries: Vec::new(),
            connections: BTreeMap::new(),
            listener: None,
            next: 2000,
            stopped: false,
            completions: Queue::with_capacity(256),
            submissions: Queue::with_capacity(256),
            events: Queue::with_capacity(256),
            requests,
            worst,
        }
    }

    #[must_use]
    pub fn queries(&self) -> &[api::Query] {
        &self.queries
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.fake = fake::Domain::try_scripted(&self.fake_env.limits, 17, Box::new([])).expect("empty peer script");
        while self.replies.pop().is_some() {}
        if let Some(listener) = self.listener.take() {
            self.requests.push(io::Request::Close { entity: listener });
        }
        for connection in self.connections.values() {
            self.requests.push(io::Request::Abort { entity: connection.socket });
        }
    }

    fn event(&mut self, event: io::Event) {
        match event {
            io::Event::Listening { listener, .. } => {
                if self.stopped {
                    self.requests.push(io::Request::Close { entity: listener });
                } else {
                    self.listener = Some(listener);
                }
            }
            io::Event::Accepted { socket, .. } => {
                if self.stopped {
                    self.requests.push(io::Request::Reject { socket });
                    return;
                }
                assert!(self.connections.len() < 4, "bounded fake LLM connections");
                let owner = Token::new(self.next);
                self.next = self.next.checked_add(1).expect("finite fixture");
                let mut connection = Connection {
                    socket,
                    tls: Server::new(pki::Server::plain().config()),
                    server: provider::Server::new(owner, &self.provider_env.limits).expect("provider connection"),
                    intake: Intake::with_capacity(32_768),
                    outgoing: VecDeque::new(),
                    header: None,
                    wanted: 5,
                    reading: false,
                    ended: false,
                    closing: false,
                    right: None,
                    next_right: 1,
                    demand: None,
                    grant: 0,
                    above: Queue::with_capacity(64),
                    below: Queue::with_capacity(256),
                };
                provider::start(
                    &mut connection.server,
                    &mut self.service,
                    &self.credential,
                    &self.provider_env,
                    &mut connection.above,
                    &mut connection.below,
                );
                self.connections.insert(owner, connection);
                self.requests.push(io::Request::Bind { socket, owner });
            }
            io::Event::Stream { owner, up: Up::Bytes(bytes) } => {
                let connection = self.connections.get_mut(&owner).expect("announced connection");
                connection.reading = false;
                match connection.header.take() {
                    None => {
                        assert_eq!(bytes.len(), 5);
                        connection.wanted = u32::from(u16::from_be_bytes([bytes[3], bytes[4]]));
                        assert!(connection.wanted > 0 && connection.wanted <= 18_432);
                        connection.header = Some(bytes);
                    }
                    Some(header) => {
                        let mut record = header.into_vec();
                        record.extend_from_slice(&bytes);
                        connection.tls.receive(&record);
                        assert!(connection.tls.failed.is_none(), "TLS accepted");
                        let plaintext = std::mem::take(&mut connection.tls.received);
                        connection.intake.append(&plaintext).expect("bounded plaintext intake");
                        connection.outgoing.extend(connection.tls.transmit());
                        connection.wanted = 5;
                    }
                }
            }
            io::Event::Stream { owner, up: up @ (Up::End | Up::Failed(_)) } => {
                if let Some(connection) = self.connections.get_mut(&owner) {
                    connection.reading = true;
                    connection.closing = true;
                    if !connection.ended {
                        provider::up(
                            &mut connection.server,
                            &mut self.service,
                            &self.credential,
                            &self.provider_env,
                            up,
                            &mut connection.above,
                            &mut connection.below,
                        );
                    }
                }
            }
            io::Event::Output { owner, up: OutputUp::Settled { right, outcome } } => {
                let connection = self.connections.get_mut(&owner).expect("announced output");
                let (wanted, count) = connection.right.take().expect("one native right");
                assert_eq!(wanted, right);
                match outcome {
                    OutputOutcome::Granted => self.requests.push(io::Request::Output {
                        stream: connection.socket,
                        down: OutputDown::Send {
                            right,
                            bytes: connection.outgoing.drain(..count).collect::<Vec<_>>().into(),
                        },
                    }),
                    OutputOutcome::Cancelled | OutputOutcome::Failed(_) => {
                        assert!(self.stopped || connection.ended || connection.closing)
                    }
                }
            }
            io::Event::Closed { owner } => {
                if let Some(mut connection) = self.connections.remove(&owner) {
                    provider::closed(
                        &mut connection.server,
                        &mut self.service,
                        &self.provider_env,
                        &mut connection.above,
                        &mut connection.below,
                    );
                }
            }
            _ => {}
        }
    }

    fn route(&mut self, owner: Token) {
        let connection = self.connections.get_mut(&owner).expect("announced provider");
        for _ in 0..128 {
            if let Some(event) = connection.above.pop() {
                match event {
                    provider::Event::Domain(event) => {
                        let fake::Event::Call { query, .. } = &event;
                        assert!(self.queries.len() < 64, "bounded query observations");
                        self.queries.push(query.clone());
                        fake::step(&mut self.fake, &self.fake_env, event, &mut self.replies);
                    }
                    provider::Event::Close => {
                        connection.closing = true;
                    }
                    provider::Event::Closed => {}
                }
            } else if let Some(down) = connection.below.pop() {
                match down {
                    Down::Demand { read: Read::Nothing, room: 0 } => connection.demand = None,
                    Down::Demand { read, room } => {
                        assert!(connection.demand.is_none(), "one plaintext demand");
                        connection.demand = Some((read, room));
                    }
                    Down::Send(bytes) => {
                        assert!(bytes.len() <= usize::try_from(connection.grant).expect("bounded room"));
                        connection.grant = 0;
                        connection.tls.write(&bytes);
                        connection.outgoing.extend(connection.tls.transmit());
                    }
                    Down::Finish => unreachable!("provider closes through its owner"),
                }
            } else if connection.server.has_work() {
                provider::resume(
                    &mut connection.server,
                    &mut self.service,
                    &self.credential,
                    &self.provider_env,
                    &mut connection.above,
                    &mut connection.below,
                );
            } else if let Some((read, room)) = connection.demand {
                let up = if room > 0 && connection.outgoing.len() < 32_768 {
                    connection.grant = room;
                    Some(Up::Room)
                } else if room == 0 {
                    connection.intake.meet(read).map(Up::Bytes)
                } else {
                    None
                };
                if let Some(up) = up {
                    connection.demand = None;
                    provider::up(
                        &mut connection.server,
                        &mut self.service,
                        &self.credential,
                        &self.provider_env,
                        up,
                        &mut connection.above,
                        &mut connection.below,
                    );
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        assert!(connection.outgoing.len() <= 65_536, "bounded TLS output");
    }
}

impl Host for Peer {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
        self.provider_env.now = now;
        self.provider_env.wall = wall;
        self.fake_env.now = now;
        self.fake_env.wall = wall;
        while self.io.is_ready() {
            io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
        }
        while let Some(complete) = self.completions.pop() {
            io::up(&mut self.io, &self.env, complete, &mut self.events, &mut self.submissions);
        }
        for _ in 0..16 {
            if self.io.is_ready() {
                io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
            }
            if self.io.is_due(now) {
                io::fire(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
            }
        }
        while let Some(event) = self.events.pop() {
            self.event(event);
        }
        let owners: Vec<_> = self.connections.keys().copied().collect();
        for owner in owners {
            self.route(owner);
        }
        if self.fake.is_due(now) {
            fake::fire(&mut self.fake, &self.fake_env, &mut self.replies);
        }
        while let Some(reply) = self.replies.pop() {
            if let Ok((owner, reply)) = self.service.target(reply) {
                let connection = self.connections.get_mut(&owner).expect("live provider target");
                provider::down(
                    &mut connection.server,
                    &mut self.service,
                    &self.credential,
                    &self.provider_env,
                    reply,
                    &mut connection.above,
                    &mut connection.below,
                );
                self.route(owner);
            }
        }
        for connection in self.connections.values_mut() {
            if !connection.reading && !connection.ended && !connection.closing && connection.intake.room() >= 18_432 {
                connection.reading = true;
                self.requests.push(io::Request::Stream {
                    stream: connection.socket,
                    down: Down::Demand { read: Read::Fill(connection.wanted), room: 0 },
                });
            }
            if connection.right.is_none() && !connection.outgoing.is_empty() && !connection.ended {
                let right = Token::new(connection.next_right);
                connection.next_right = connection.next_right.checked_add(1).expect("finite fixture");
                let count = connection.outgoing.len().min(19_000);
                connection.right = Some((right, count));
                self.requests.push(io::Request::Output {
                    stream: connection.socket,
                    down: OutputDown::Room { right, bytes: u32::try_from(count).expect("bounded output") },
                });
            }
        }
        for connection in self.connections.values_mut() {
            if connection.closing && !connection.ended && connection.right.is_none() && connection.outgoing.is_empty() {
                connection.ended = true;
                self.requests.push(io::Request::Close { entity: connection.socket });
            }
        }
        while self.io.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &self.env, request, &mut self.submissions);
        }
        self.io.reclaim();
        self.service.reclaim();
        self.fake.reclaim();
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        self.io.is_ready()
            || self.io.is_due(now)
            || self.fake.is_due(now)
            || !self.requests.is_empty()
            || !self.events.is_empty()
            || !self.completions.is_empty()
            || !self.replies.is_empty()
            || self.connections.values().any(|connection| {
                !connection.above.is_empty() || !connection.below.is_empty() || connection.server.has_work()
            })
    }
    fn next_deadline(&self) -> Option<Time> {
        [self.io.next_deadline(), self.fake.next_deadline()].into_iter().flatten().min()
    }
    fn is_empty(&self) -> bool {
        self.stopped
            && self.io.is_empty()
            && self.connections.is_empty()
            && self.requests.is_empty()
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.worst
    }
    fn operations(&self) -> u32 {
        io::operations(&self.env.limits).expect("peer operations")
    }
}
