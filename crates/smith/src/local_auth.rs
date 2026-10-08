//! One local account's shell transport (protocol/hosts.md, section 5.4).
//! It keeps the bounded OAuth translator, one HTTP/TLS POST and a loopback
//! listener. Clock values and unpredictable startup entropy are injected.
//! It never opens a browser or interprets access tokens. File requests go
//! through the shared token store before a grant is lent.

use std::net::{SocketAddr, ToSocketAddrs};

use skein_io::{self as io, kernel};
use skein_lib::stream::{Delimiter, Down, OutputDown, OutputOutcome, OutputUp, Read, Up};
use skein_lib::{Duration, Queue, Time, Token, Wall};
use skein_oauth as oauth;
use smith_local_protocol::{Begin, Credential, CredentialEvent, CredentialLimits, CredentialRequest};

use crate::local_auth_http::{Destination, Post};
use crate::local_host::token_limits;
use crate::local_settings::{Account, OAuth};

pub(crate) struct Auth {
    settings: Account,
    credential: Credential,
    destination: Option<Destination>,
    loopback: Option<SocketAddr>,
    redirect_path: Box<[u8]>,
    post: Option<Post>,
    pending_post: Option<oauth::HttpRequest>,
    listener: Option<Token>,
    callback: Option<Token>,
    callback_right: bool,
    pending_url: Option<Box<[u8]>>,
    wanted: bool,
    refresh: Option<Time>,
    next: u64,
    entropy: [u8; 32],
    http_owner: Token,
    listener_owner: Token,
    callback_owner: Token,
    out: Queue<CredentialRequest>,
    stopped: bool,
}

pub(crate) fn limits() -> CredentialLimits {
    CredentialLimits {
        oauth: oauth::ClientLimits {
            document: token_limits(),
            uri_bytes: 4096,
            scope_bytes: 1024,
            state_bytes: 64,
            code_bytes: 4096,
            url_bytes: 8192,
            request_bytes: 16_384,
            sign_in_time: Duration::from_secs(120),
            request_time: Duration::from_secs(20),
            backoff_base: Duration::from_secs(1),
            backoff_ceiling: Duration::from_secs(8),
            max_attempts: 3,
        },
        refresh_before: Duration::from_secs(60),
    }
}

/// Account state, one POST, transient tokens and bounded emitted records.
pub(crate) fn worst_case() -> Option<u64> {
    smith_local_protocol::credential_worst_case(&limits())?
        .checked_add(crate::local_auth_http::worst_case()?)?
        .checked_add(Queue::<CredentialRequest>::worst_case(8)?)?
        .checked_add(8_u64.checked_mul(32_768)?)?
        .checked_add(65_536)?
        .checked_add(crate::config::TRUST_BYTES)?
        .checked_add(u64::try_from(std::mem::size_of::<Auth>()).ok()?)
}

impl Auth {
    pub(crate) fn new(settings: Account, position: usize, entropy: [u8; 32]) -> Result<Self, String> {
        let (destination, loopback, redirect_path) = match &settings.oauth {
            Some(config) => {
                for value in [&config.authorization_url, &config.token_endpoint, &config.redirect_uri] {
                    if value.len() > 4096 {
                        return Err("OAuth endpoint URI is too long".into());
                    }
                }
                if config.client_id.len() > usize::try_from(token_limits().client_bytes).expect("client bound")
                    || config.scope.len() > 1024
                    || config.address.len() > 4096
                    || config.server_name.len() > 253
                    || config.trust_der.as_ref().is_some_and(|path| path.len() > 4096)
                {
                    return Err("OAuth settings exceed their bounds".into());
                }
                let authority_and_path =
                    config.token_endpoint.strip_prefix("https://").ok_or("token endpoint must use https")?;
                let (authority, target) = authority_and_path.split_once('/').ok_or("token endpoint requires a path")?;
                if authority.is_empty() || authority.contains('@') || authority.contains('#') || target.contains('#') {
                    return Err("invalid token endpoint".into());
                }
                let address = config
                    .address
                    .to_socket_addrs()
                    .map_err(|error| format!("OAuth address: {error}"))?
                    .next()
                    .ok_or("OAuth address resolved to no destination")?;
                let destination = Destination {
                    address: kernel::Addr::from(address),
                    server_name: skein_tls::Name::new(&config.server_name).ok_or("invalid OAuth server name")?,
                    trust: crate::config::trust(config.trust_der.as_deref())?,
                    authority: authority.as_bytes().into(),
                    target: format!("/{target}").into_bytes().into(),
                };
                let redirect = config.redirect_uri.strip_prefix("http://").ok_or("redirect must use loopback http")?;
                let (address, path) = redirect.split_once('/').ok_or("redirect requires a path")?;
                let address: SocketAddr = address.parse().map_err(|_| "invalid redirect address")?;
                if !address.ip().is_loopback() || address.port() == 0 || path.contains('?') || path.contains('#') {
                    return Err("redirect must use a fixed loopback port and path".into());
                }
                (Some(destination), Some(address), format!("/{path}").into_bytes().into())
            }
            None => (None, None, Box::new([]) as Box<[u8]>),
        };
        let position = u64::try_from(position).map_err(|_| "account position overflow")?;
        let owner =
            position.checked_mul(3).and_then(|value| value.checked_add(1000)).ok_or("account owner overflow")?;
        Ok(Self {
            credential: Credential::new(settings.number, limits())
                .map_err(|error| format!("OAuth bounds: {error:?}"))?,
            settings,
            destination,
            loopback,
            redirect_path,
            post: None,
            pending_post: None,
            listener: None,
            callback: None,
            callback_right: false,
            pending_url: None,
            wanted: false,
            refresh: None,
            next: 1,
            entropy,
            http_owner: Token::new(owner),
            listener_owner: Token::new(owner.checked_add(1).ok_or("account owner overflow")?),
            callback_owner: Token::new(owner.checked_add(2).ok_or("account owner overflow")?),
            out: Queue::with_capacity(8),
            stopped: false,
        })
    }

    pub(crate) fn account(&self) -> u32 {
        self.settings.number
    }
    pub(crate) fn account_id(&self) -> &[u8] {
        self.settings.account_id.as_bytes()
    }
    pub(crate) fn load_failed(&mut self) {
        self.wanted = false;
        self.refresh = None;
    }
    pub(crate) fn request(&mut self) {
        self.wanted = true;
    }
    pub(crate) fn take(&mut self) -> Option<CredentialRequest> {
        self.out.pop()
    }

    pub(crate) fn begin(&mut self, saved: Option<oauth::SavedToken>, now: Time, wall: Wall) {
        if !self.wanted || self.credential.next_deadline().is_some() || self.stopped {
            return;
        }
        self.wanted = false;
        self.refresh = None;
        let registration = self.settings.oauth.as_ref().map_or_else(empty_registration, registration);
        let state = self.nonce(b"state");
        let verifier = self.nonce(b"verifier");
        self.next = self.next.checked_add(1).expect("OAuth exchange names remain representable");
        self.credential.begin(Begin { registration, saved, state, verifier, now, wall }, &mut self.out);
    }

    fn nonce(&self, purpose: &[u8]) -> Box<[u8]> {
        let mut bytes = self.entropy.to_vec();
        bytes.extend_from_slice(&self.settings.number.to_be_bytes());
        bytes.extend_from_slice(&self.next.to_be_bytes());
        bytes.extend_from_slice(purpose);
        let hash = io::digest::digest(&bytes);
        let mut hex = String::with_capacity(64);
        use std::fmt::Write;
        for part in hash.0 {
            write!(&mut hex, "{part:016x}").expect("String write");
        }
        hex.into_bytes().into()
    }

    pub(crate) fn visit(&mut self, url: Box<[u8]>, out: &mut Queue<io::Request>) {
        if let Some(address) = self.loopback {
            self.pending_url = Some(url);
            out.push(io::Request::Listen { owner: self.listener_owner, addr: kernel::Addr::from(address) });
        } else {
            self.credential.from_below(CredentialEvent::Cancel, &mut self.out);
        }
    }

    pub(crate) fn shown_url(&mut self) -> Option<Box<[u8]>> {
        if self.listener.is_some() { self.pending_url.take() } else { None }
    }

    pub(crate) fn http(&mut self, request: oauth::HttpRequest) {
        assert!(self.pending_post.is_none(), "one token POST at a time");
        self.pending_post = Some(request);
    }

    pub(crate) fn stored(&mut self, wall: Wall) {
        self.credential.from_below(CredentialEvent::Stored { wall }, &mut self.out);
    }
    pub(crate) fn store_failed(&mut self) {
        self.credential.from_below(CredentialEvent::StoreFailed, &mut self.out);
    }
    pub(crate) fn lent(&mut self, valid: Duration, now: Time) {
        self.wanted = false;
        self.refresh = if self.settings.oauth.is_some() {
            Some(
                now.saturating_add(
                    Duration::from_nanos(valid.as_nanos().saturating_sub(limits().refresh_before.as_nanos()))
                        .max(Duration::from_secs(1)),
                ),
            )
        } else {
            None
        };
    }
    pub(crate) fn due(&mut self, now: Time) {
        if self.refresh.is_some_and(|at| now >= at) {
            self.wanted = true;
            self.refresh = None;
        }
    }
    pub(crate) fn wants(&self) -> bool {
        self.wanted && self.credential.next_deadline().is_none() && !self.stopped
    }

    pub(crate) fn owns(&self, owner: Token) -> bool {
        owner == self.http_owner || owner == self.listener_owner || owner == self.callback_owner
    }

    pub(crate) fn event(&mut self, event: io::Event, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        let owner = event_owner(&event);
        if owner == self.http_owner {
            if let Some(post) = &mut self.post {
                post.event(event, now, wall, out);
            }
            return;
        }
        match event {
            io::Event::Listening { listener, .. } => {
                self.listener = Some(listener);
                if self.stopped {
                    self.listener = None;
                    out.push(io::Request::Close { entity: listener });
                }
            }
            io::Event::Accepted { socket, .. } => {
                if self.stopped || self.callback.is_some() {
                    out.push(io::Request::Abort { entity: socket });
                    return;
                }
                self.callback = Some(socket);
                out.push(io::Request::Bind { socket, owner: self.callback_owner });
                out.push(io::Request::Stream {
                    stream: socket,
                    down: Down::Demand {
                        read: Read::Scan { until: Delimiter::new(b"\n").expect("LF delimiter"), max: 8192 },
                        room: 0,
                    },
                });
            }
            io::Event::Stream { up: Up::Bytes(bytes), .. } => {
                if let Some(Redirect { state, code, error }) = redirect(&bytes, &self.redirect_path) {
                    let uri =
                        self.settings.oauth.as_ref().expect("loopback registration").redirect_uri.as_bytes().into();
                    self.credential
                        .from_below(CredentialEvent::Redirected { uri, state, code, error, now }, &mut self.out);
                } else {
                    self.credential.from_below(CredentialEvent::Cancel, &mut self.out);
                }
                let socket = self.callback.expect("callback socket");
                self.callback_right = true;
                out.push(io::Request::Output {
                    stream: socket,
                    down: OutputDown::Room { right: Token::new(1), bytes: 105 },
                });
                if let Some(listener) = self.listener.take() {
                    out.push(io::Request::Close { entity: listener });
                }
            }
            io::Event::Output { up: OutputUp::Settled { right, outcome: OutputOutcome::Granted }, .. } => {
                self.callback_right = false;
                let socket = self.callback.expect("callback socket");
                out.push(io::Request::Output {
                    stream: socket,
                    down: OutputDown::Send {
                        right,
                        bytes: b"HTTP/1.1 200 OK\r\nContent-Length: 18\r\nConnection: close\r\n\r\nRedirect received\n"
                            .as_slice()
                            .into(),
                    },
                });
                out.push(io::Request::Close { entity: socket });
            }
            io::Event::Closed { owner } if owner == self.callback_owner => self.callback = None,
            io::Event::Closed { .. } => {}
            io::Event::Failed { .. }
            | io::Event::Stream { up: Up::End | Up::Failed(_), .. }
            | io::Event::Output { up: OutputUp::Settled { .. }, .. } => {
                if self.credential.next_deadline().is_some() {
                    self.credential.from_below(CredentialEvent::Cancel, &mut self.out);
                }
                if matches!(event, io::Event::Output { .. }) {
                    self.callback_right = false;
                }
                self.close_loopback(out);
            }
            _ => {}
        }
    }

    pub(crate) fn tick(&mut self, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        if self.post.as_ref().is_some_and(Post::closed) {
            self.post = None;
        }
        if self.post.is_none()
            && let Some(request) = self.pending_post.take()
        {
            if let Some(destination) = &self.destination {
                match Post::new(destination, self.http_owner, request, out) {
                    Ok(post) => self.post = Some(post),
                    Err(_) => self.credential.from_below(CredentialEvent::Cancel, &mut self.out),
                }
            } else {
                self.credential.from_below(CredentialEvent::Cancel, &mut self.out);
            }
        }
        if let Some(post) = &mut self.post {
            post.tick(now, wall, out);
            if let Some(response) = post.terminal() {
                self.credential.from_below(CredentialEvent::Http(response), &mut self.out);
            }
        }
        if self.credential.next_deadline().is_some_and(|at| now >= at) {
            self.credential.from_below(CredentialEvent::Tick { now }, &mut self.out);
            if self.credential.next_deadline().is_none() {
                self.close_loopback(out);
            }
        }
    }

    pub(crate) fn close(&mut self, now: Time, wall: Wall, out: &mut Queue<io::Request>) {
        self.stopped = true;
        self.refresh = None;
        self.wanted = false;
        self.pending_post = None;
        self.pending_url = None;
        if self.credential.next_deadline().is_some() {
            self.credential.from_below(CredentialEvent::Cancel, &mut self.out);
        }
        if let Some(post) = &mut self.post {
            post.cancel(now, wall, out);
        }
        self.close_loopback(out);
    }

    fn close_loopback(&mut self, out: &mut Queue<io::Request>) {
        if let Some(listener) = self.listener.take() {
            out.push(io::Request::Close { entity: listener });
        }
        if let Some(socket) = self.callback
            && !self.callback_right
        {
            out.push(io::Request::Close { entity: socket });
        }
    }
    pub(crate) fn pending(&self) -> bool {
        !self.out.is_empty()
            || self.wants()
            || self.pending_post.is_some()
            || self.post.as_ref().is_some_and(Post::pending)
    }
    pub(crate) fn deadline(&self) -> Option<Time> {
        [self.refresh, self.credential.next_deadline(), self.post.as_ref().and_then(Post::deadline)]
            .into_iter()
            .flatten()
            .min()
    }
}

fn registration(config: &OAuth) -> oauth::Registration {
    oauth::Registration {
        authorization_url: config.authorization_url.as_bytes().into(),
        token_endpoint: config.token_endpoint.as_bytes().into(),
        client_id: config.client_id.as_bytes().into(),
        redirect_uri: config.redirect_uri.as_bytes().into(),
        scope: config.scope.as_bytes().into(),
        wire: if config.json { oauth::WireFormat::Json } else { oauth::WireFormat::Form },
        client_secret: None,
        pkce_for_confidential: false,
        metadata_claim: None,
    }
}

fn empty_registration() -> oauth::Registration {
    oauth::Registration {
        authorization_url: Box::new([]),
        token_endpoint: Box::new([]),
        client_id: Box::new([]),
        redirect_uri: Box::new([]),
        scope: Box::new([]),
        wire: oauth::WireFormat::Form,
        client_secret: None,
        pkce_for_confidential: false,
        metadata_claim: None,
    }
}

pub(crate) fn event_owner(event: &io::Event) -> Token {
    match event {
        io::Event::Listening { owner, .. }
        | io::Event::Accepted { owner, .. }
        | io::Event::Connecting { owner, .. }
        | io::Event::Connected { owner }
        | io::Event::Stream { owner, .. }
        | io::Event::Output { owner, .. }
        | io::Event::Spawned { owner, .. }
        | io::Event::Exited { owner, .. }
        | io::Event::Failed { owner, .. }
        | io::Event::Closed { owner } => *owner,
        io::Event::Shutdown { .. } => Token::new(u64::MAX),
    }
}

struct Redirect {
    state: Box<[u8]>,
    code: Option<Box<[u8]>>,
    error: Option<Box<[u8]>>,
}

fn redirect(bytes: &[u8], path: &[u8]) -> Option<Redirect> {
    let text = std::str::from_utf8(bytes).ok()?.strip_prefix("GET ")?.split_once(' ')?.0;
    let (target, query) = text.split_once('?')?;
    if target.as_bytes() != path {
        return None;
    }
    let mut state = None;
    let mut code = None;
    let mut error = None;
    for field in query.split('&') {
        let (key, value) = field.split_once('=')?;
        let value = percent(value.as_bytes())?;
        match key {
            "state" if state.is_none() => state = Some(value),
            "code" if code.is_none() => code = Some(value),
            "error" if error.is_none() => error = Some(value),
            "state" | "code" | "error" => return None,
            _ => {}
        }
    }
    Some(Redirect { state: state?, code, error })
}

fn percent(input: &[u8]) -> Option<Box<[u8]>> {
    let mut decoded = Vec::with_capacity(input.len());
    let mut position = 0;
    while position < input.len() {
        match input[position] {
            b'%' => {
                let high = char::from(*input.get(position + 1)?).to_digit(16)?;
                let low = char::from(*input.get(position + 2)?).to_digit(16)?;
                decoded.push(u8::try_from(high * 16 + low).ok()?);
                position += 3;
            }
            b'+' => {
                decoded.push(b' ');
                position += 1;
            }
            value => {
                decoded.push(value);
                position += 1;
            }
        }
    }
    Some(decoded.into())
}
