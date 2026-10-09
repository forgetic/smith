//! Transport stories use the production account entry points and hosted fake issuer.

use std::path::PathBuf;

use skein_fake_oauth as fake;
use skein_io::{self as io, kernel};
use skein_lib::{Duration, Env, Queue, Time};
use skein_oauth as oauth;
use skein_world::Host;
use smith_local_domain as local;
use smith_local_process_world::oauth::{Browser, Peer};
use smith_local_protocol::CredentialRequest;

use crate::local_auth::{Auth, limits};
use crate::local_settings::{Account, OAuth};

struct World {
    sim: skein_sim::Sim,
    pid: skein_sim::Pid,
    peer_pid: skein_sim::Pid,
    browser: Option<(skein_sim::Pid, Browser)>,
    io: io::Io,
    env: Env<io::Limits>,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    auth: Auth,
    peer: Peer,
    saved: Option<oauth::SavedToken>,
    terminal: Option<Result<smith_domain::Grant, local::CredentialFailure>>,
    root: PathBuf,
    lose: bool,
}

fn issuer_limits() -> fake::Limits {
    fake::Limits {
        document: limits().oauth.document,
        uri_bytes: 8192,
        request_bytes: 16_384,
        codes: 4,
        rotations: 4,
        plans: 4,
    }
}

impl World {
    fn new(name: &str, seed: u64, prior: Option<&[u8]>) -> Self {
        let root = std::env::temp_dir().join(format!("smith-auth-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&root).expect("test directory");
        let account = Account {
            number: 7,
            account_id: "acc".into(),
            oauth: Some(OAuth {
                authorization_url: "http://127.0.0.1:443/authorize".into(),
                token_endpoint: "http://127.0.0.1:443/token".into(),
                client_id: "client".into(),
                redirect_uri: "http://127.0.0.1:2345/callback".into(),
                scope: "read".into(),
                address: "127.0.0.1:443".into(),
                server_name: String::new(),
                trust_der: None,
                json: false,
            }),
        };
        let mut issuer = Peer::new(
            (std::net::Ipv4Addr::LOCALHOST, 443).into(),
            skein_fake_peers::Transport::Plaintext,
            smith_agent_process_world::fake::limits(),
            fake::Config {
                authorization_url: b"http://127.0.0.1:443/authorize".as_slice().into(),
                token_endpoint: b"http://127.0.0.1:443/token".as_slice().into(),
                client_id: b"client".as_slice().into(),
                client_secret: None,
                redirect_uri: b"http://127.0.0.1:2345/callback".as_slice().into(),
                refresh_token: b"refresh-old".as_slice().into(),
            },
            issuer_limits(),
            skein_http::server::Limits {
                head: 8192,
                headers: 32,
                body: 16_384,
                read: 1024,
                response: 49_152,
                send: 1024,
            },
        )
        .expect("fake issuer");
        issuer
            .queue(fake::Plan {
                status: 200,
                body: fake::Body::Token(oauth::TokenResponse {
                    access_token: b"access-new".as_slice().into(),
                    refresh_token: Some(b"refresh-new".as_slice().into()),
                    expires_in: 7200,
                }),
                delay: Duration::from_millis(10),
                retry_after: Duration::ZERO,
            })
            .expect("one token plan");
        let mut sim_config = skein_sim::Config::calm();
        sim_config.wall = skein_tls_world::pki::VALID;
        let mut sim = skein_sim::Sim::new(seed, sim_config);
        let pid = sim.spawn_process();
        let peer_pid = sim.spawn_process();
        let io_limits = smith_agent_process_world::limits().io;
        let saved = prior.map(|token| oauth::SavedToken {
            key: 7,
            generation: 1,
            access_token: b"old".as_slice().into(),
            refresh_token: token.into(),
            metadata: None,
            expires_at: sim_config.wall,
        });
        let mut auth = Auth::new(account, 0, [91; 32]).expect("bounded account");
        auth.request();
        auth.begin(saved, Time::ZERO, sim_config.wall);
        Self {
            sim,
            pid,
            peer_pid,
            browser: None,
            io: io::Io::new(&io_limits),
            env: Env { now: Time::ZERO, wall: sim_config.wall, limits: io_limits },
            completions: Queue::with_capacity(256),
            submissions: Queue::with_capacity(256),
            events: Queue::with_capacity(256),
            requests: Queue::with_capacity(256),
            auth,
            peer: issuer,
            saved: None,
            terminal: None,
            root,
            lose: false,
        }
    }

    fn step(&mut self) {
        self.sim.reap(self.peer_pid, self.peer.completions());
        self.peer.iterate(self.sim.now(), self.sim.wall());
        if self.lose && smith_local_process_world::oauth::posts(&self.peer) > 0 {
            self.lose = false;
            self.peer.shutdown();
        }
        self.sim.submit(self.peer_pid, self.peer.submissions());
        if let Some((pid, browser)) = &mut self.browser {
            self.sim.reap(*pid, browser.completions());
            browser.iterate(self.sim.now(), self.sim.wall());
            self.sim.submit(*pid, browser.submissions());
        }
        self.env.now = self.sim.now();
        self.env.wall = self.sim.wall();
        self.sim.reap(self.pid, &mut self.completions);
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
            if self.io.is_due(self.env.now) {
                io::fire(&mut self.io, &self.env, &mut self.events, &mut self.submissions);
            }
        }
        while let Some(event) = self.events.pop() {
            self.auth.event(event, self.env.now, self.env.wall, &mut self.requests);
        }
        self.auth.tick(self.env.now, self.env.wall, &mut self.requests);
        if let Some(url) = self.auth.shown_url() {
            self.browser = Some((self.sim.spawn_process(), Browser::new(&url)));
        }
        while let Some(request) = self.auth.take() {
            match request {
                CredentialRequest::Visit { url } => self.auth.visit(url, &mut self.requests),
                CredentialRequest::Http(request) => self.auth.http(request),
                CredentialRequest::Save { bytes, .. } => {
                    self.saved = Some(oauth::decode_record(&bytes, &limits().oauth.document).expect("saved candidate"));
                    assert!(self.terminal.is_none(), "grant follows durable token acknowledgement");
                    self.auth.stored(self.env.wall);
                }
                CredentialRequest::Ready { event: local::Event::Credential { grant }, value } => {
                    assert_eq!(value.as_ref(), b"access-new");
                    self.terminal = Some(Ok(grant));
                }
                CredentialRequest::Failed { event: local::Event::NoCredential { reason, .. } } => {
                    self.terminal = Some(Err(reason));
                }
                CredentialRequest::Ready { .. } | CredentialRequest::Failed { .. } => {
                    panic!("unexpected OAuth terminal")
                }
            }
        }
        while self.io.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &self.env, request, &mut self.submissions);
        }
        self.io.reclaim();
        self.sim.submit(self.pid, &mut self.submissions);
        if !self.io.is_ready()
            && !self.peer.work_pending(self.env.now)
            && !self.auth.pending()
            && self.sim.ready(self.pid) == 0
            && self.sim.ready(self.peer_pid) == 0
            && self
                .browser
                .as_ref()
                .is_none_or(|(pid, browser)| !browser.work_pending(self.env.now) && self.sim.ready(*pid) == 0)
            && let Some(at) = [
                self.sim.next_due(),
                self.io.next_deadline(),
                self.auth.deadline(),
                self.peer.next_deadline(),
                self.browser.as_ref().and_then(|(_, browser)| browser.next_deadline()),
            ]
            .into_iter()
            .flatten()
            .min()
        {
            self.sim.advance_to(at);
        }
    }
    fn settle(&mut self) {
        for _ in 0..4000 {
            self.step();
            if self.terminal.is_some() {
                self.auth.close(self.env.now, self.env.wall, &mut self.requests);
                self.peer.shutdown();
                for _ in 0..4000 {
                    self.step();
                    if self.io.is_empty()
                        && self.peer.is_empty()
                        && self.browser.as_ref().is_none_or(|(_, browser)| browser.is_empty())
                        && self.requests.is_empty()
                        && self.events.is_empty()
                        && self.completions.is_empty()
                        && self.sim.in_flight(self.pid) == 0
                        && self.sim.in_flight(self.peer_pid) == 0
                        && self.browser.as_ref().is_none_or(|(pid, _)| self.sim.in_flight(*pid) == 0)
                    {
                        assert!(self.browser.as_ref().is_none_or(|(_, browser)| browser.saw_reply()));
                        return;
                    }
                }
                panic!("OAuth transports close all operations");
            }
        }
        panic!("OAuth transport settles within bounded steps")
    }
}

impl Drop for World {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.root));
    }
}

#[test]
fn sign_in_returns_through_the_loopback_listener_and_plaintext_token_endpoint() {
    let mut world = World::new("sign-in", 1, None);
    world.settle();
    let Some(Ok(grant)) = world.terminal else {
        panic!("sign-in grant {:?}, posts {}", world.terminal, smith_local_process_world::oauth::posts(&world.peer))
    };
    assert_eq!(grant.name.generation, 1);
    assert_eq!(smith_local_process_world::oauth::posts(&world.peer), 1);
    assert!(world.saved.is_some(), "candidate is saved before lending");
}

#[test]
fn a_refresh_uses_the_plaintext_endpoint_and_rotates_the_saved_record() {
    let mut world = World::new("refresh", 2, Some(b"refresh-old"));
    world.settle();
    let Some(Ok(grant)) = world.terminal else {
        panic!("refresh grant {:?}, posts {}", world.terminal, smith_local_process_world::oauth::posts(&world.peer))
    };
    assert_eq!(grant.name.generation, 2);
    assert_eq!(smith_local_process_world::oauth::posts(&world.peer), 1);
    assert_eq!(world.saved.as_ref().expect("candidate").refresh_token.as_ref(), b"refresh-new");
}

#[test]
fn a_refused_refresh_returns_no_credential() {
    let mut world = World::new("refused", 3, Some(b"wrong-refresh"));
    world.settle();
    assert_eq!(world.terminal, Some(Err(local::CredentialFailure::Refresh)));
    assert_eq!(smith_local_process_world::oauth::posts(&world.peer), 1);
    assert!(world.saved.is_none());
}

#[test]
fn a_lost_refresh_response_is_uncertain_and_is_never_repeated() {
    let mut world = World::new("lost", 4, Some(b"refresh-old"));
    world.lose = true;
    world.settle();
    assert_eq!(world.terminal, Some(Err(local::CredentialFailure::Refresh)));
    assert_eq!(smith_local_process_world::oauth::posts(&world.peer), 1);
    assert!(world.saved.is_none());
}

#[test]
fn loopback_http_issuer_configuration_needs_no_tls_trust() {
    for address in ["127.0.0.1:8080", "[::1]:8080"] {
        let account = Account {
            number: 0,
            account_id: "local".into(),
            oauth: Some(OAuth {
                authorization_url: format!("http://{address}/authorize"),
                token_endpoint: format!("http://{address}/token"),
                client_id: "client".into(),
                redirect_uri: "http://127.0.0.1:2345/callback".into(),
                scope: String::new(),
                address: address.into(),
                server_name: String::new(),
                trust_der: None,
                json: false,
            }),
        };
        Auth::new(account, 0, [1; 32]).expect("numeric loopback HTTP issuer");
    }
}

#[test]
fn plaintext_issuer_refuses_nonloopback_uris_and_destinations_before_startup() {
    for (uri, destination) in [
        ("http://192.0.2.1:8080/token", "127.0.0.1:8080"),
        ("http://localhost:8080/token", "127.0.0.1:8080"),
        ("http://127.0.0.1:8080/token", "192.0.2.1:8080"),
        ("http://[::1]:0/token", "[::1]:8080"),
        ("http://127.0.0.1:8080@evil.test/token", "127.0.0.1:8080"),
    ] {
        let account = Account {
            number: 0,
            account_id: "local".into(),
            oauth: Some(OAuth {
                authorization_url: uri.into(),
                token_endpoint: uri.into(),
                client_id: "client".into(),
                redirect_uri: "http://127.0.0.1:2345/callback".into(),
                scope: String::new(),
                address: destination.into(),
                server_name: String::new(),
                trust_der: None,
                json: false,
            }),
        };
        assert!(Auth::new(account, 0, [1; 32]).is_err(), "invalid plaintext issuer {uri} -> {destination}");
    }
}
