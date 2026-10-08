//! A hosted HTTP/TLS face for Skein's record-level fake OAuth issuer. The
//! peer retains issuer and socket state, never a simulator or application
//! policy. Its inputs are kernel completions and an observed page visit.

use std::collections::{BTreeMap, VecDeque};

use skein_fake_oauth as fake;
use skein_io::{self as io, kernel};
use skein_lib::stream::{Down, OutputDown, OutputOutcome, OutputUp, Read, Up};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_oauth as oauth;
use skein_tls_world::{pki, server::Server};
use skein_world::Host;

struct Connection {
    socket: Token,
    tls: Server,
    outgoing: VecDeque<u8>,
    header: Option<Box<[u8]>>,
    wanted: u32,
    reading: bool,
    ended: bool,
    replied: bool,
    right: Option<Token>,
    next_right: u64,
}

/// Fake issuer and HTTP/TLS sockets driven through the Host boundary.
pub struct Peer {
    issuer: fake::Issuer,
    endpoint: Box<[u8]>,
    io: io::Io,
    env: Env<io::Limits>,
    listener: Option<Token>,
    connections: BTreeMap<Token, Connection>,
    next: u64,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    stopped: bool,
    lose: bool,
    worst: u64,
}

impl Peer {
    #[must_use]
    pub fn new(issuer: fake::Issuer, limits: &fake::Limits, address: kernel::Addr, endpoint: Box<[u8]>) -> Self {
        let io_limits = io::Limits {
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
        let mut requests = Queue::with_capacity(256);
        requests.push(io::Request::Listen { owner: Token::new(1), addr: address });
        Self {
            issuer,
            endpoint,
            io: io::Io::new(&io_limits),
            env: Env { now: Time::ZERO, wall: pki::VALID, limits: io_limits },
            listener: None,
            connections: BTreeMap::new(),
            next: 2001,
            completions: Queue::with_capacity(256),
            submissions: Queue::with_capacity(256),
            events: Queue::with_capacity(256),
            requests,
            stopped: false,
            lose: false,
            worst: fake::worst_case(limits).expect("fake issuer bound") + 8_000_000,
        }
    }

    /// The fake page supplies a redirect in response to the terminal's observed visit.
    pub fn authorize(&mut self, url: Box<[u8]>, now: Time) -> fake::Request {
        let mut out = Queue::with_capacity(fake::MAX_OUT);
        self.issuer.step(fake::Event::Authorize { url, now }, &mut out);
        out.pop().expect("issuer answers the visit")
    }

    /// Content-free observation of token POSTs processed by the issuer.
    #[must_use]
    pub fn posts(&self) -> u64 {
        self.issuer.posts()
    }

    /// Lose the next response after the issuer consumes and rotates its refresh token.
    pub fn lose_response(&mut self) {
        self.lose = true;
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
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
                self.listener = Some(listener);
                if self.stopped {
                    self.requests.push(io::Request::Close { entity: listener });
                }
            }
            io::Event::Accepted { socket, .. } => {
                assert!(self.connections.len() < 4, "bounded fake issuer clients");
                let owner = Token::new(self.next);
                self.next += 1;
                self.connections.insert(
                    owner,
                    Connection {
                        socket,
                        tls: Server::new(pki::Server::plain().config()),
                        outgoing: VecDeque::new(),
                        header: None,
                        wanted: 5,
                        reading: false,
                        ended: false,
                        replied: false,
                        right: None,
                        next_right: 1,
                    },
                );
                self.requests.push(io::Request::Bind { socket, owner });
            }
            io::Event::Stream { owner, up: Up::Bytes(bytes) } => {
                let connection = self.connections.get_mut(&owner).expect("announced issuer connection");
                connection.reading = false;
                match connection.header.take() {
                    None => {
                        connection.wanted = u32::from(u16::from_be_bytes([bytes[3], bytes[4]]));
                        connection.header = Some(bytes);
                    }
                    Some(header) => {
                        let mut record = header.into_vec();
                        record.extend_from_slice(&bytes);
                        connection.tls.receive(&record);
                        assert!(connection.tls.failed.is_none(), "issuer TLS record accepted");
                        connection.outgoing.extend(connection.tls.transmit());
                        connection.wanted = 5;
                        if !connection.replied
                            && let Some((content_type, body)) = post_body(&connection.tls.received)
                        {
                            connection.replied = true;
                            let request = oauth::HttpRequest {
                                id: 1,
                                endpoint: self.endpoint.clone(),
                                content_type,
                                body,
                                deadline: self.env.now.saturating_add(Duration::from_secs(20)),
                            };
                            let mut out = Queue::with_capacity(fake::MAX_OUT);
                            self.issuer
                                .step(fake::Event::Post { request, now: self.env.now, wall: self.env.wall }, &mut out);
                            let Some(fake::Request::Http(answer)) = out.pop() else {
                                panic!("issuer returns one HTTP response")
                            };
                            if self.lose {
                                self.lose = false;
                                connection.ended = true;
                                self.requests.push(io::Request::Abort { entity: connection.socket });
                            } else {
                                let mut response = format!(
                                    "HTTP/1.1 {} Token\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    answer.status,
                                    answer.body.len()
                                )
                                .into_bytes();
                                response.extend_from_slice(&answer.body);
                                connection.tls.write(&response);
                                connection.outgoing.extend(connection.tls.transmit());
                            }
                        }
                    }
                }
            }
            io::Event::Stream { owner, up: Up::End | Up::Failed(_) } => {
                if let Some(connection) = self.connections.get_mut(&owner) {
                    connection.ended = true;
                    connection.reading = false;
                    self.requests.push(io::Request::Close { entity: connection.socket });
                }
            }
            io::Event::Output { owner, up: OutputUp::Settled { right, outcome: OutputOutcome::Granted } } => {
                let connection = self.connections.get_mut(&owner).expect("issuer output owner");
                assert_eq!(connection.right.take(), Some(right));
                self.requests.push(io::Request::Output {
                    stream: connection.socket,
                    down: OutputDown::Send { right, bytes: connection.outgoing.drain(..).collect::<Vec<_>>().into() },
                });
            }
            io::Event::Closed { owner } => {
                self.connections.remove(&owner);
            }
            io::Event::Output { .. } if self.stopped => {}
            _ => {}
        }
    }
}

impl Host for Peer {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
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
        for connection in self.connections.values_mut() {
            if !connection.reading && !connection.ended {
                connection.reading = true;
                self.requests.push(io::Request::Stream {
                    stream: connection.socket,
                    down: Down::Demand { read: Read::Fill(connection.wanted), room: 0 },
                });
            }
            if connection.right.is_none() && !connection.outgoing.is_empty() && !connection.ended {
                let right = Token::new(connection.next_right);
                connection.next_right += 1;
                connection.right = Some(right);
                self.requests.push(io::Request::Output {
                    stream: connection.socket,
                    down: OutputDown::Room {
                        right,
                        bytes: u32::try_from(connection.outgoing.len()).expect("bounded TLS flight"),
                    },
                });
            }
        }
        while self.io.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &self.env, request, &mut self.submissions);
        }
        self.io.reclaim();
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
            || !self.completions.is_empty()
            || !self.events.is_empty()
            || !self.requests.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.stopped && self.io.is_empty() && self.completions.is_empty() && self.submissions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.worst
    }
    fn operations(&self) -> u32 {
        io::operations(&self.env.limits).expect("issuer operations")
    }
}

fn post_body(bytes: &[u8]) -> Option<(&'static [u8], Box<[u8]>)> {
    let split = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    assert!(split <= 4096, "bounded issuer request head");
    let header = std::str::from_utf8(&bytes[..split]).expect("HTTP request text");
    assert!(header.starts_with("POST "), "token endpoint only accepts POST");
    let mut length = None;
    let mut json = false;
    for line in header.split("\r\n").skip(1) {
        let (name, value) = line.split_once(':').expect("header field");
        if name.eq_ignore_ascii_case("Content-Length") {
            length = Some(value.trim().parse::<usize>().expect("body length"));
        }
        if name.eq_ignore_ascii_case("Content-Type") {
            json = value.trim() == "application/json";
        }
    }
    let length = length.expect("token POST has a length");
    assert!(length <= 16_384, "bounded issuer request body");
    let body = bytes.get(split + 4..split + 4 + length)?;
    Some((if json { b"application/json" } else { b"application/x-www-form-urlencoded" }, body.into()))
}

/// A browser's cleartext loopback GET, independent of the world's kernel.
pub struct Browser {
    io: io::Io,
    env: Env<io::Limits>,
    request: Option<Box<[u8]>>,
    socket: Option<Token>,
    body: bool,
    ended: bool,
    seen: bool,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
}

impl Browser {
    #[must_use]
    pub fn new(redirect: fake::Request) -> Self {
        let fake::Request::Redirect { uri, state, code } = redirect else {
            panic!("the issuer redirects an accepted page")
        };
        let text = std::str::from_utf8(&uri).expect("loopback URI").strip_prefix("http://").expect("loopback http");
        let (authority, path) = text.split_once('/').expect("loopback path");
        let address: std::net::SocketAddr = authority.parse().expect("loopback address");
        let request = format!(
            "GET /{path}?state={}&code={} HTTP/1.1\r\nHost: {authority}\r\n\r\n",
            encoded(&state),
            encoded(&code)
        )
        .into_bytes()
        .into_boxed_slice();
        let limits = io::Limits {
            sockets: 1,
            refusals: 1,
            intake: 4096,
            receive: 4096,
            output: 8192,
            sends: 2,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(1),
        };
        let mut requests = Queue::with_capacity(64);
        requests.push(io::Request::Connect { owner: Token::new(1), addr: kernel::Addr::from(address) });
        Self {
            io: io::Io::new(&limits),
            env: Env { now: Time::ZERO, wall: pki::VALID, limits },
            request: Some(request),
            socket: None,
            body: false,
            ended: false,
            seen: false,
            completions: Queue::with_capacity(64),
            submissions: Queue::with_capacity(64),
            events: Queue::with_capacity(64),
            requests,
        }
    }
    #[must_use]
    pub fn saw_reply(&self) -> bool {
        self.seen
    }
}

impl Host for Browser {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
        while self.io.is_ready() {
            io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
        }
        while let Some(complete) = self.completions.pop() {
            io::up(&mut self.io, &self.env, complete, &mut self.events, &mut self.submissions);
        }
        for _ in 0..8 {
            if self.io.is_ready() {
                io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
            }
            if self.io.is_due(now) {
                io::fire(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
            }
        }
        while let Some(event) = self.events.pop() {
            match event {
                io::Event::Connecting { socket, .. } => self.socket = Some(socket),
                io::Event::Connected { .. } => {
                    let socket = self.socket.expect("browser socket");
                    self.requests.push(io::Request::Output {
                        stream: socket,
                        down: OutputDown::Room {
                            right: Token::new(1),
                            bytes: u32::try_from(self.request.as_ref().expect("one GET").len()).expect("bounded GET"),
                        },
                    });
                    self.requests.push(io::Request::Stream {
                        stream: socket,
                        down: Down::Demand {
                            read: Read::Scan {
                                until: skein_lib::stream::Delimiter::new(b"\r\n\r\n").expect("header delimiter"),
                                max: 4096,
                            },
                            room: 0,
                        },
                    });
                }
                io::Event::Output { up: OutputUp::Settled { right, outcome: OutputOutcome::Granted }, .. } => {
                    self.requests.push(io::Request::Output {
                        stream: self.socket.expect("browser socket"),
                        down: OutputDown::Send { right, bytes: self.request.take().expect("one GET output") },
                    });
                }
                io::Event::Stream { up: Up::Bytes(bytes), .. } => {
                    let socket = self.socket.expect("browser socket");
                    if self.body {
                        self.seen = bytes.as_ref() == b"Redirect received\n";
                        self.requests.push(io::Request::Close { entity: socket });
                    } else {
                        assert!(bytes.starts_with(b"HTTP/1.1 200"));
                        self.body = true;
                        self.requests.push(io::Request::Stream {
                            stream: socket,
                            down: Down::Demand { read: Read::Fill(18), room: 0 },
                        });
                    }
                }
                io::Event::Closed { .. } => self.ended = true,
                io::Event::Stream { up: Up::End | Up::Failed(_), .. } | io::Event::Failed { .. } => {
                    if let Some(socket) = self.socket {
                        self.requests.push(io::Request::Close { entity: socket });
                    }
                }
                _ => {}
            }
        }
        while self.io.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &self.env, request, &mut self.submissions);
        }
        self.io.reclaim();
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
            || !self.requests.is_empty()
            || !self.events.is_empty()
            || !self.completions.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.ended
            && self.io.is_empty()
            && self.requests.is_empty()
            && self.events.is_empty()
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        io::worst_case(&self.env.limits).expect("browser IO bound") + 65_536
    }
    fn operations(&self) -> u32 {
        io::operations(&self.env.limits).expect("browser operations")
    }
}

fn encoded(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            write!(&mut out, "%{byte:02X}").expect("String write");
        }
    }
    out
}
