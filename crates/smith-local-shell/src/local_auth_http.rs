//! One token POST over Skein's HTTP client, with TLS or loopback plaintext. The connection retains
//! bounded upload and response bytes and never logs credentials. Its owner
//! supplies io events and clock values; failures preserve whether a request
//! may have been sent (protocol/hosts.md, section 5.4; skein oauth.md).

use skein_http::{self as http, client};
use skein_io::{self as io, kernel};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_oauth as oauth;
use skein_tls::client as tls;

/// Startup transport and HTTP destination for one configured account.
pub(crate) struct Destination {
    pub address: kernel::Addr,
    pub transport: skein_llm_connection::Transport,
    pub authority: Box<[u8]>,
    pub target: Box<[u8]>,
}

pub(crate) struct Post {
    id: u64,
    deadline: Time,
    socket: Option<Token>,
    tls: Option<tls::Client>,
    http: client::Client,
    call: Option<client::Call>,
    upload: Option<Box<[u8]>>,
    body: Vec<u8>,
    status: u16,
    remaining: Option<u64>,
    retry_after: Duration,
    sent: bool,
    terminal: Option<oauth::HttpResponse>,
    closing: bool,
    closed: bool,
    tls_up: Queue<tls::Event>,
    cipher_down: Queue<Down>,
    http_up: Queue<client::Event>,
    plain_down: Queue<Down>,
}

fn http_limits() -> client::Limits {
    client::Limits { request: 4096, head: 4096, headers: 32, read: 4096, send: 16_384 }
}

fn tls_limits() -> tls::Limits {
    tls::Limits { read: 4096, send: 16_384, records: tls::MAX_RECORD }
}

/// Retained clients, bounded queues and their maximum payload coexistence.
pub(crate) fn worst_case() -> Option<u64> {
    tls::worst_case(&tls_limits())?
        .checked_add(client::worst_case(&http_limits())?)?
        .checked_add(Queue::<tls::Event>::worst_case(64)?)?
        .checked_add(Queue::<client::Event>::worst_case(64)?)?
        .checked_add(Queue::<Down>::worst_case(256)?.checked_mul(2)?)?
        .checked_add(640_u64.checked_mul(32_768)?)?
        .checked_add(65_536)?
        .checked_add(u64::try_from(size_of::<Post>()).ok()?)
}

pub(crate) fn fits(limits: &io::Limits) -> bool {
    limits.intake >= tls::LARGEST_READ && limits.output >= tls::largest_room(&tls_limits())
}

impl Post {
    pub(crate) fn new(
        destination: &Destination,
        owner: Token,
        request: oauth::HttpRequest,
        out: &mut Queue<io::Request>,
    ) -> Result<Self, String> {
        if request.body.len() > 16_384 {
            return Err("OAuth request exceeds its transport bound".into());
        }
        let mut post = Self {
            id: request.id,
            deadline: request.deadline,
            socket: None,
            tls: match &destination.transport {
                skein_llm_connection::Transport::Tls { server_name, trust } => {
                    Some(tls::Client::new(trust, server_name.clone(), &tls_limits()))
                }
                skein_llm_connection::Transport::Plaintext => None,
            },
            http: client::Client::new(&http_limits()),
            call: Some(client::Call {
                method: http::Method::Post,
                target: destination.target.clone(),
                headers: Box::new([
                    http::Header { name: b"Host".as_slice().into(), value: destination.authority.clone() },
                    http::Header { name: b"Content-Type".as_slice().into(), value: request.content_type.into() },
                ]),
                body: client::Body::Length(u64::try_from(request.body.len()).expect("bounded request")),
                close: true,
            }),
            upload: Some(request.body),
            body: Vec::new(),
            status: 0,
            remaining: None,
            retry_after: Duration::ZERO,
            sent: false,
            terminal: None,
            closing: false,
            closed: false,
            tls_up: Queue::with_capacity(64),
            cipher_down: Queue::with_capacity(256),
            http_up: Queue::with_capacity(64),
            plain_down: Queue::with_capacity(256),
        };
        out.push(io::Request::Connect { owner, addr: destination.address });
        post.route(Time::ZERO, Wall::EPOCH, out);
        Ok(post)
    }

    pub(crate) fn event(&mut self, event: io::Event, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        let env = Env { now, wall, limits: tls_limits() };
        match event {
            io::Event::Connecting { socket, .. } => {
                self.socket = Some(socket);
                if self.closing {
                    out.push(io::Request::Abort { entity: socket });
                }
            }
            io::Event::Connected { .. } if !self.closing => match &mut self.tls {
                Some(tls) => tls::down(tls, &env, tls::Request::Handshake, &mut self.tls_up, &mut self.cipher_down),
                None => self.begin_http(now, wall),
            },
            io::Event::Stream { up, .. } if !self.closing => match &mut self.tls {
                Some(tls) => tls::up(tls, &env, up, &mut self.tls_up, &mut self.cipher_down),
                None => client::up(
                    &mut self.http,
                    &Env { now, wall, limits: http_limits() },
                    up,
                    &mut self.http_up,
                    &mut self.plain_down,
                ),
            },
            io::Event::Failed { .. } => {
                self.fail(now, wall, out);
                if self.socket.is_none() {
                    self.closed = true;
                }
            }
            io::Event::Closed { .. } => self.closed = true,
            io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connected { .. }
            | io::Event::Stream { .. }
            | io::Event::Output { .. }
            | io::Event::Spawned { .. }
            | io::Event::Exited { .. }
            | io::Event::Shutdown { .. } => {}
        }
        self.route(now, wall, out);
    }

    pub(crate) fn tick(&mut self, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        if now >= self.deadline && self.terminal.is_none() && !self.closing {
            self.fail(now, wall, out);
        }
        self.route(now, wall, out);
    }

    pub(crate) fn cancel(&mut self, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        self.fail(now, wall, out);
    }

    pub(crate) fn terminal(&mut self) -> Option<oauth::HttpResponse> {
        self.terminal.take()
    }
    pub(crate) fn closed(&self) -> bool {
        self.closed
    }
    pub(crate) fn deadline(&self) -> Option<Time> {
        (!self.closing).then_some(self.deadline)
    }
    pub(crate) fn pending(&self) -> bool {
        !self.tls_up.is_empty()
            || !self.cipher_down.is_empty()
            || !self.http_up.is_empty()
            || !self.plain_down.is_empty()
    }

    fn route(&mut self, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        let http_env = Env { now, wall, limits: http_limits() };
        let tls_env = Env { now, wall, limits: tls_limits() };
        for _ in 0..128 {
            if self.closing {
                while self.tls_up.pop().is_some() {}
                while self.http_up.pop().is_some() {}
                while self.plain_down.pop().is_some() {}
                while self.cipher_down.pop().is_some() {}
                break;
            }
            if let Some(event) = self.http_up.pop() {
                self.http_event(event, now, wall, out);
            } else if let Some(down) = self.plain_down.pop() {
                if let Down::Send(bytes) = &down
                    && !bytes.is_empty()
                {
                    self.sent = true;
                }
                match &mut self.tls {
                    Some(tls) => {
                        tls::down(tls, &tls_env, tls::Request::Stream(down), &mut self.tls_up, &mut self.cipher_down);
                    }
                    None => {
                        if let Some(socket) = self.socket {
                            out.push(io::Request::Stream { stream: socket, down });
                        }
                    }
                }
            } else if let Some(event) = self.tls_up.pop() {
                match event {
                    tls::Event::Ready(_) => {
                        self.begin_http(now, wall);
                    }
                    tls::Event::Stream(up) => {
                        client::up(&mut self.http, &http_env, up, &mut self.http_up, &mut self.plain_down);
                    }
                    tls::Event::Failed(_) => self.fail(now, wall, out),
                    tls::Event::Closed => {}
                }
            } else if let Some(down) = self.cipher_down.pop() {
                if let Some(socket) = self.socket {
                    out.push(io::Request::Stream { stream: socket, down });
                }
            } else {
                break;
            }
        }
    }

    fn begin_http(&mut self, now: Time, wall: Wall) {
        let env = Env { now, wall, limits: http_limits() };
        client::down(
            &mut self.http,
            &env,
            client::Request::Call(self.call.take().expect("one token POST")),
            &mut self.http_up,
            &mut self.plain_down,
        );
        let length = u32::try_from(self.upload.as_ref().expect("request upload").len()).expect("bounded request");
        client::down(
            &mut self.http,
            &env,
            client::Request::Upload(Down::Demand { read: Read::Nothing, room: length }),
            &mut self.http_up,
            &mut self.plain_down,
        );
    }

    fn http_event(&mut self, event: client::Event, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        let env = Env { now, wall, limits: http_limits() };
        match event {
            client::Event::Upload(Up::Room) => {
                client::down(
                    &mut self.http,
                    &env,
                    client::Request::Upload(Down::Send(self.upload.take().expect("one upload grant"))),
                    &mut self.http_up,
                    &mut self.plain_down,
                );
                client::down(
                    &mut self.http,
                    &env,
                    client::Request::Upload(Down::Finish),
                    &mut self.http_up,
                    &mut self.plain_down,
                );
            }
            client::Event::Response(response) => {
                self.status = response.status;
                self.remaining = match response.framing {
                    client::Framing::Empty => Some(0),
                    client::Framing::Length(length) if length <= 16_384 => Some(length),
                    client::Framing::Length(_) => {
                        self.fail(now, wall, out);
                        return;
                    }
                    client::Framing::Chunked | client::Framing::UntilEnd => None,
                };
                self.retry_after = response
                    .header(b"Retry-After")
                    .and_then(|value| std::str::from_utf8(value).ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map_or(Duration::ZERO, Duration::from_secs);
                self.demand_body(&env);
            }
            client::Event::Body(Up::Bytes(bytes)) => {
                if self.body.len().checked_add(bytes.len()).is_none_or(|length| length > 16_384) {
                    self.fail(now, wall, out);
                    return;
                }
                self.body.extend_from_slice(&bytes);
                if let Some(remaining) = &mut self.remaining {
                    *remaining = remaining
                        .checked_sub(u64::try_from(bytes.len()).expect("bounded body"))
                        .expect("HTTP length enforced");
                }
                self.demand_body(&env);
            }
            client::Event::Done(_) => {
                self.terminal = Some(oauth::HttpResponse {
                    id: self.id,
                    status: self.status,
                    body: std::mem::take(&mut self.body).into(),
                    retry_after: self.retry_after,
                    evidence: oauth::HttpEvidence::Response,
                    now,
                    wall,
                });
                self.close(out);
            }
            client::Event::Failed(_) | client::Event::Body(Up::Failed(_)) | client::Event::Upload(Up::Failed(_)) => {
                self.fail(now, wall, out);
            }
            client::Event::Body(Up::End) | client::Event::Closed => {}
            client::Event::Upload(_) | client::Event::Body(_) => {
                unreachable!("HTTP upload and response streams have separate directions")
            }
        }
    }

    fn demand_body(&mut self, env: &Env<client::Limits>) {
        let count =
            self.remaining.map_or(1, |remaining| u32::try_from(remaining.clamp(1, 4096)).expect("bounded read"));
        if count > 0 {
            client::down(
                &mut self.http,
                env,
                client::Request::Body(Down::Demand { read: Read::Fill(count), room: 0 }),
                &mut self.http_up,
                &mut self.plain_down,
            );
        }
    }

    fn fail(&mut self, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        if !self.closing {
            self.terminal = Some(oauth::HttpResponse {
                id: self.id,
                status: 0,
                body: Box::new([]),
                retry_after: Duration::ZERO,
                evidence: if self.sent { oauth::HttpEvidence::Unknown } else { oauth::HttpEvidence::Unsent },
                now,
                wall,
            });
            self.close(out);
        }
    }

    fn close(&mut self, out: &mut Queue<io::Request>) {
        self.closing = true;
        if let Some(socket) = self.socket {
            out.push(io::Request::Abort { entity: socket });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_plaintext_posts_http_and_preserves_the_response_evidence() {
        let destination = Destination {
            address: kernel::Addr::from((std::net::Ipv4Addr::LOCALHOST, 8080)),
            transport: skein_llm_connection::Transport::Plaintext,
            authority: b"127.0.0.1:8080".as_slice().into(),
            target: b"/token".as_slice().into(),
        };
        let owner = Token::new(1);
        let socket = Token::new(2);
        let mut out = Queue::with_capacity(256);
        let mut post = Post::new(
            &destination,
            owner,
            oauth::HttpRequest {
                id: 7,
                endpoint: b"http://127.0.0.1:8080/token".as_slice().into(),
                content_type: b"application/x-www-form-urlencoded",
                body: b"grant_type=refresh_token&refresh_token=opaque".as_slice().into(),
                deadline: Time::ZERO.checked_add(Duration::from_secs(1)).expect("deadline"),
            },
            &mut out,
        )
        .expect("plaintext POST");
        assert!(post.tls.is_none(), "plaintext creates no TLS state");
        assert!(matches!(out.pop(), Some(io::Request::Connect { .. })));
        post.event(io::Event::Connecting { owner, socket }, Time::ZERO, Wall::EPOCH, &mut out);
        post.event(io::Event::Connected { owner }, Time::ZERO, Wall::EPOCH, &mut out);
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
        let mut received = 0;
        let mut sent = Vec::new();
        for _ in 0..128 {
            let Some(request) = out.pop() else { break };
            match request {
                io::Request::Stream { down: Down::Demand { read, room }, .. } => {
                    let up = if room > 0 {
                        Some(Up::Room)
                    } else {
                        let count = match read {
                            Read::Nothing => 0,
                            Read::Fill(count) => usize::try_from(count).expect("count"),
                            Read::Scan { until, .. } => {
                                let remaining = &response[received..];
                                remaining
                                    .windows(until.as_bytes().len())
                                    .position(|bytes| bytes == until.as_bytes())
                                    .expect("response delimiter")
                                    + until.as_bytes().len()
                            }
                            Read::Line { .. } => panic!("HTTP scans CRLF"),
                        };
                        if count == 0 {
                            None
                        } else {
                            let bytes = response[received..received + count].into();
                            received += count;
                            Some(Up::Bytes(bytes))
                        }
                    };
                    if let Some(up) = up {
                        post.event(io::Event::Stream { owner, up }, Time::ZERO, Wall::EPOCH, &mut out);
                    }
                }
                io::Request::Stream { down: Down::Send(bytes), .. } => sent.extend_from_slice(&bytes),
                io::Request::Abort { .. } => post.event(io::Event::Closed { owner }, Time::ZERO, Wall::EPOCH, &mut out),
                io::Request::Listen { .. }
                | io::Request::Connect { .. }
                | io::Request::Bind { .. }
                | io::Request::Reject { .. }
                | io::Request::Stream { .. }
                | io::Request::Output { .. }
                | io::Request::Spawn { .. }
                | io::Request::Signal { .. }
                | io::Request::Close { .. } => panic!("unexpected POST operation {request:?}"),
            }
        }
        assert!(sent.starts_with(b"POST /token HTTP/1.1\r\n"));
        assert!(sent.ends_with(b"grant_type=refresh_token&refresh_token=opaque"));
        let terminal = post.terminal().expect("one response");
        assert_eq!(terminal.id, 7);
        assert_eq!(terminal.status, 200);
        assert_eq!(terminal.body.as_ref(), b"{}");
        assert_eq!(terminal.evidence, oauth::HttpEvidence::Response);
        assert!(post.terminal().is_none(), "one terminal");
        assert!(post.closed() && out.is_empty(), "plaintext socket settles");
    }
}
