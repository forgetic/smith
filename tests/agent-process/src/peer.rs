//! A simulated socket endpoint that serves Skein's fake LLM response over
//! actual TLS records. The fake machine remains the process world's authority
//! for any file or child effects.

use std::collections::VecDeque;
use std::net::Ipv4Addr;

use skein_io::{self as io, kernel};
use skein_lib::stream::{Down, OutputDown, OutputOutcome, OutputUp, Read, Up};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_tls_world::{pki, server::Server};

const LISTENER: Token = Token::new(1);
const SERVER: Token = Token::new(2);

pub struct Peer {
    io: io::Io,
    env: Env<io::Limits>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    submissions: Queue<kernel::Submit>,
    completions: Queue<kernel::Complete>,
    listener: Option<Token>,
    socket: Option<Token>,
    tls: Server,
    outgoing: VecDeque<u8>,
    right: Option<Token>,
    next_right: u64,
    header: Option<Box<[u8]>>,
    wanted: u32,
    reading: bool,
    read_ended: bool,
    replied: bool,
}

impl Peer {
    pub fn work_pending(&self) -> bool {
        self.io.is_ready() || !self.events.is_empty() || !self.requests.is_empty() || !self.completions.is_empty()
    }

    pub fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }

    pub fn replied(&self) -> bool {
        self.replied
    }

    pub fn received(&self) -> &[u8] {
        &self.tls.received
    }

    pub fn new() -> Peer {
        let limits = io::Limits {
            sockets: 2,
            refusals: 1,
            intake: 19_000,
            receive: 4096,
            output: 19_000,
            sends: 4,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(1),
        };
        let mut requests = Queue::with_capacity(64);
        requests.push(io::Request::Listen { owner: LISTENER, addr: kernel::Addr::from((Ipv4Addr::LOCALHOST, 443)) });
        Peer {
            io: io::Io::new(&limits),
            env: Env { now: Time::ZERO, wall: pki::VALID, limits },
            events: Queue::with_capacity(64),
            requests,
            submissions: Queue::with_capacity(64),
            completions: Queue::with_capacity(64),
            listener: None,
            socket: None,
            tls: Server::new(pki::Server::plain().config()),
            outgoing: VecDeque::new(),
            right: None,
            next_right: 1,
            header: None,
            wanted: 5,
            reading: false,
            read_ended: false,
            replied: false,
        }
    }

    pub fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }

    pub fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    pub fn step(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
        while let Some(complete) = self.completions.pop() {
            io::up(&mut self.io, &self.env, complete, &mut self.events, &mut self.submissions);
        }
        for _ in 0..8 {
            if self.io.is_ready() {
                io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
            }
        }
        while let Some(event) = self.events.pop() {
            self.event(event);
        }
        self.prepare_output();
        if let Some(socket) = self.socket
            && !self.reading
            && !self.read_ended
        {
            self.reading = true;
            self.requests.push(io::Request::Stream {
                stream: socket,
                down: Down::Demand { read: Read::Fill(self.wanted), room: 0 },
            });
        }
        while self.io.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &self.env, request, &mut self.submissions);
        }
        self.io.reclaim();
    }

    fn event(&mut self, event: io::Event) {
        match event {
            io::Event::Listening { listener, .. } => self.listener = Some(listener),
            io::Event::Accepted { socket, .. } => {
                self.socket = Some(socket);
                self.requests.push(io::Request::Bind { socket, owner: SERVER });
                self.requests.push(io::Request::Close { entity: self.listener.expect("listener") });
            }
            io::Event::Stream { owner: SERVER, up: Up::Bytes(bytes) } => self.record(&bytes),
            io::Event::Stream { owner: SERVER, up: Up::End | Up::Failed(_) } => {
                self.read_ended = true;
                self.reading = false;
            }
            io::Event::Output { owner: SERVER, up: OutputUp::Settled { right, outcome } } => {
                assert_eq!(self.right, Some(right), "one server output right");
                match outcome {
                    OutputOutcome::Granted => {
                        let socket = self.socket.expect("live server socket");
                        let bytes: Box<[u8]> = self.outgoing.drain(..).collect();
                        self.requests
                            .push(io::Request::Output { stream: socket, down: OutputDown::Send { right, bytes } });
                        self.right = None;
                    }
                    OutputOutcome::Cancelled | OutputOutcome::Failed(_) => {
                        panic!("fake server output failed: {outcome:?}")
                    }
                }
            }
            io::Event::Closed { owner: SERVER } => {
                self.socket = None;
                self.outgoing.clear();
            }
            io::Event::Closed { .. }
            | io::Event::Connected { .. }
            | io::Event::Connecting { .. }
            | io::Event::Failed { .. }
            | io::Event::Stream { .. }
            | io::Event::Output { .. }
            | io::Event::Spawned { .. }
            | io::Event::Exited { .. }
            | io::Event::Shutdown { .. } => {}
        }
    }

    fn record(&mut self, bytes: &[u8]) {
        self.reading = false;
        match self.header.take() {
            None => {
                assert_eq!(bytes.len(), 5, "TLS record header");
                let length = u16::from_be_bytes([bytes[3], bytes[4]]);
                self.wanted = u32::from(length);
                self.header = Some(Box::from(bytes));
            }
            Some(mut header) => {
                let mut record = header.to_vec();
                record.extend_from_slice(bytes);
                self.tls.receive(&record);
                assert!(self.tls.failed.is_none(), "fake server accepted the TLS record");
                if !self.replied && self.tls.received.windows(4).any(|part| part == b"\r\n\r\n") {
                    self.tls.write(&skein_llm_world::text_response(true));
                    self.replied = true;
                }
                self.outgoing.extend(self.tls.transmit());
                self.wanted = 5;
                header.fill(0);
            }
        }
    }

    fn prepare_output(&mut self) {
        if self.right.is_some() || self.outgoing.is_empty() {
            return;
        }
        let Some(socket) = self.socket else { return };
        let right = Token::new(self.next_right);
        self.next_right += 1;
        let bytes = u32::try_from(self.outgoing.len()).expect("bounded TLS response");
        self.right = Some(right);
        self.requests.push(io::Request::Output { stream: socket, down: OutputDown::Room { right, bytes } });
    }
}
