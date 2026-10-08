//! The scripted browser follows actual HTTP redirects; Skein owns the issuer.
//! It keeps GET and callback state and received observations, never local
//! service state. Contract: testing.md, sections 4 and 5.
pub use skein_fake_peers::oauth::Peer;
use skein_io::{self as io, kernel};
use skein_lib::stream::{Down, OutputDown, OutputOutcome, OutputUp, Read, Up};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_tls_world::pki;
use skein_world::Host;

/// Outside token POST count, with no issuer state access.
#[must_use]
pub fn posts(peer: &Peer) -> u64 {
    u64::try_from(
        peer.observations()
            .iter()
            .filter(|observation| matches!(observation, skein_fake_peers::oauth::Observation::Post { .. }))
            .count(),
    )
    .expect("bounded observations")
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
    redirect: Option<Box<[u8]>>,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
}

impl Browser {
    #[must_use]
    pub fn new(url: &[u8]) -> Self {
        let (address, request) = get(url);
        let limits = io::Limits {
            // The redirect connects before the retired authorization socket is reclaimed.
            sockets: 2,
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
            redirect: None,
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
                        if bytes.starts_with(b"HTTP/1.1 302") {
                            let head = std::str::from_utf8(&bytes).expect("redirect head");
                            let location = head
                                .split("\r\n")
                                .find_map(|line| {
                                    line.strip_prefix("Location: ").or_else(|| line.strip_prefix("location: "))
                                })
                                .expect("issuer Location");
                            self.redirect = Some(location.as_bytes().into());
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
                }
                io::Event::Closed { .. } => {
                    self.socket = None;
                    if let Some(url) = self.redirect.take() {
                        let (address, request) = get(&url);
                        self.request = Some(request);
                        self.requests
                            .push(io::Request::Connect { owner: Token::new(1), addr: kernel::Addr::from(address) });
                    } else {
                        self.ended = true;
                    }
                }
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

fn get(url: &[u8]) -> (std::net::SocketAddr, Box<[u8]>) {
    let text = std::str::from_utf8(url).expect("loopback URI").strip_prefix("http://").expect("plaintext HTTP");
    let (authority, path) = text.split_once('/').expect("GET path");
    let address = authority.parse().expect("loopback address");
    let request = format!("GET /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n")
        .into_bytes()
        .into_boxed_slice();
    (address, request)
}
