//! One local account's OAuth sign-in and refresh (protocol/hosts.md,
//! section 5.4; domain/host.md, section 7). It retains Skein's OAuth client
//! and one candidate token until the caller has stored that token durably in
//! a user-only file. It never interprets a provider's access token. `begin`,
//! `from_below`, and `next_deadline` are the entry points. A visit must be
//! shown to the person, a redirect comes from a loopback listener, and an
//! HTTP request is answered with transport evidence.
//!
//! | State | Input | Output |
//! |---|---|---|
//! | Idle | begin, valid saved token | lend grant and value |
//! | Idle | begin, missing token | authorization page |
//! | Idle | begin, expiring token | refresh POST |
//! | Exchanging | redirect or HTTP | next OAuth request or token candidate |
//! | Saving | durable save terminal | lend grant and value |
//! | Any active | failure | no credential |

use alloc::boxed::Box;
use core::mem::size_of;
use skein_lib::{Duration, Queue, Time, Wall};
use skein_oauth as oauth;
use smith_domain::{Grant, GrantName};
use smith_local_domain::{self as local, CredentialFailure};

/// Limits for a single configured local account.
#[derive(Clone, Copy, Debug)]
pub struct CredentialLimits {
    /// Skein's bounded exchange and saved-token bounds.
    pub oauth: oauth::ClientLimits,
    /// Refresh when remaining validity is at or below this span.
    pub refresh_before: Duration,
}

/// Checked retained bound for the OAuth client and one token candidate.
#[must_use]
pub fn credential_worst_case(limits: &CredentialLimits) -> Option<u64> {
    oauth::client_worst_case(&limits.oauth)?
        .checked_add(u64::from(limits.oauth.document.record_bytes))?
        .checked_add(u64::try_from(size_of::<Credential>()).ok()?)
}

/// One lower terminal or deadline for the credential component.
#[expect(missing_debug_implementations, reason = "HTTP and redirect events can contain credential values")]
pub enum CredentialEvent {
    /// A redirect received by the loopback listener.
    Redirected { uri: Box<[u8]>, state: Box<[u8]>, code: Option<Box<[u8]>>, error: Option<Box<[u8]>>, now: Time },
    /// A completed token endpoint request with send evidence.
    Http(oauth::HttpResponse),
    /// The user-only token file was synced and renamed durably.
    Stored { wall: Wall },
    /// The file write failed before the token was lent.
    StoreFailed,
    /// The next Skein OAuth deadline fired.
    Tick { now: Time },
    /// The host no longer needs this exchange.
    Cancel,
}

/// One request to the local service or its person and network neighbours.
#[expect(missing_debug_implementations, reason = "token requests and grants contain secret values")]
pub enum CredentialRequest {
    /// Show this authorization page's address to the person.
    Visit { url: Box<[u8]> },
    /// Make one token endpoint request with exact send evidence in its terminal.
    Http(oauth::HttpRequest),
    /// Atomically replace the user's token file with mode 0600, sync file and directory, then answer Stored.
    Save { account: u32, bytes: Box<[u8]> },
    /// Lend a fresh grant to the domain and the access value to the agent's protocol layer.
    Ready { event: local::Event, value: Box<[u8]> },
    /// Tell the domain why the account has no usable credential.
    Failed { event: local::Event },
}

/// One configured account and its saved-token state at startup.
#[expect(missing_debug_implementations, reason = "registration and saved token contain secrets")]
pub struct Begin {
    pub registration: oauth::Registration,
    pub saved: Option<oauth::SavedToken>,
    pub state: Box<[u8]>,
    pub verifier: Box<[u8]>,
    pub now: Time,
    pub wall: Wall,
}

/// One OAuth exchange and candidate awaiting durable storage.
#[expect(missing_debug_implementations, reason = "candidate and client contain credential values")]
pub struct Credential {
    account: u32,
    limits: CredentialLimits,
    client: oauth::Client,
    candidate: Option<oauth::SavedToken>,
    active: bool,
    refreshing: bool,
}

impl Credential {
    /// Construct one bounded account translator.
    pub fn new(account: u32, limits: CredentialLimits) -> Result<Self, oauth::Failure> {
        if credential_worst_case(&limits).is_none() {
            return Err(oauth::Failure::Limit);
        }
        Ok(Self {
            account,
            limits,
            client: oauth::Client::new(limits.oauth)?,
            candidate: None,
            active: false,
            refreshing: false,
        })
    }

    /// Choose a valid saved grant, refresh an expiring token, or start sign-in.
    pub fn begin(&mut self, begin: Begin, out: &mut Queue<CredentialRequest>) {
        assert!(!self.active, "one account exchange at a time");
        assert!(out.room() > 0, "caller reserves one credential output");
        self.active = true;
        match begin.saved {
            Some(record) if record.key != self.account => self.fail(CredentialFailure::Missing, out),
            Some(record) if record.remaining(begin.wall) > self.limits.refresh_before => {
                self.ready(record, begin.wall, out);
            }
            Some(record) => match record.refresh_state() {
                Some(prior) => {
                    self.refreshing = true;
                    let mut oauth_out = Queue::with_capacity(oauth::MAX_OUT);
                    self.client.step(
                        oauth::Event::Refresh { registration: begin.registration, prior, now: begin.now },
                        &mut oauth_out,
                    );
                    self.drain(&mut oauth_out, out);
                }
                None => self.fail(CredentialFailure::Refresh, out),
            },
            None => {
                self.refreshing = false;
                let mut oauth_out = Queue::with_capacity(oauth::MAX_OUT);
                self.client.step(
                    oauth::Event::SignIn {
                        registration: begin.registration,
                        key: self.account,
                        generation: 0,
                        state: begin.state,
                        verifier: Some(begin.verifier),
                        now: begin.now,
                    },
                    &mut oauth_out,
                );
                self.drain(&mut oauth_out, out);
            }
        }
    }

    /// Forward one lower terminal, preserving a saved candidate until file sync succeeds.
    pub fn from_below(&mut self, event: CredentialEvent, out: &mut Queue<CredentialRequest>) {
        assert!(out.room() > 0, "caller reserves one credential output");
        match event {
            CredentialEvent::Stored { wall } => {
                let record = self.candidate.take().expect("stored answers one candidate");
                self.ready(record, wall, out);
            }
            CredentialEvent::StoreFailed => {
                self.candidate = None;
                let reason = if self.refreshing { CredentialFailure::Refresh } else { CredentialFailure::Missing };
                self.fail(reason, out);
            }
            CredentialEvent::Redirected { uri, state, code, error, now } => {
                let mut oauth_out = Queue::with_capacity(oauth::MAX_OUT);
                self.client.step(oauth::Event::Redirected { uri, state, code, error, now }, &mut oauth_out);
                self.drain(&mut oauth_out, out);
            }
            CredentialEvent::Http(response) => {
                let mut oauth_out = Queue::with_capacity(oauth::MAX_OUT);
                self.client.step(oauth::Event::Http(response), &mut oauth_out);
                self.drain(&mut oauth_out, out);
            }
            CredentialEvent::Tick { now } => {
                let mut oauth_out = Queue::with_capacity(oauth::MAX_OUT);
                self.client.step(oauth::Event::Tick { now }, &mut oauth_out);
                self.drain(&mut oauth_out, out);
            }
            CredentialEvent::Cancel => {
                let mut oauth_out = Queue::with_capacity(oauth::MAX_OUT);
                self.client.step(oauth::Event::Cancel, &mut oauth_out);
                self.drain(&mut oauth_out, out);
            }
        }
    }

    /// The next instant by which a redirect, HTTP request, or backoff must progress.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        self.client.next_deadline()
    }

    fn drain(&mut self, from_client: &mut Queue<oauth::Request>, out: &mut Queue<CredentialRequest>) {
        match from_client.pop() {
            Some(oauth::Request::Visit { url }) => out.push(CredentialRequest::Visit { url }),
            Some(oauth::Request::Http(request)) => out.push(CredentialRequest::Http(request)),
            Some(oauth::Request::Tokens { record }) => {
                match oauth::encode_record(&record, &self.limits.oauth.document) {
                    Ok(bytes) => {
                        self.candidate = Some(record);
                        out.push(CredentialRequest::Save { account: self.account, bytes });
                    }
                    Err(_) => self.fail(CredentialFailure::Refresh, out),
                }
            }
            Some(oauth::Request::Failed { failure: _ }) => {
                let reason = if self.refreshing { CredentialFailure::Refresh } else { CredentialFailure::Missing };
                self.fail(reason, out);
            }
            None => {}
        }
        assert!(from_client.is_empty(), "one OAuth step has one output");
    }

    fn ready(&mut self, record: oauth::SavedToken, wall: Wall, out: &mut Queue<CredentialRequest>) {
        if record.remaining(wall) == Duration::ZERO {
            self.fail(CredentialFailure::Refresh, out);
            return;
        }
        self.active = false;
        self.client.step(oauth::Event::Reset, &mut Queue::with_capacity(oauth::MAX_OUT));
        let grant = Grant {
            name: GrantName { account: record.key, generation: record.generation },
            valid: record.remaining(wall),
        };
        out.push(CredentialRequest::Ready { event: local::Event::Credential { grant }, value: record.access_token });
    }

    fn fail(&mut self, reason: CredentialFailure, out: &mut Queue<CredentialRequest>) {
        self.active = false;
        self.client.step(oauth::Event::Reset, &mut Queue::with_capacity(oauth::MAX_OUT));
        out.push(CredentialRequest::Failed { event: local::Event::NoCredential { account: self.account, reason } });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skein_fake_oauth as fake;

    fn bytes(value: &[u8]) -> Box<[u8]> {
        Box::from(value)
    }

    fn now() -> Time {
        Time::from_nanos(1_000_000_000)
    }

    fn wall() -> Wall {
        Wall::from_nanos(1_000_000_000)
    }

    fn limits() -> CredentialLimits {
        CredentialLimits {
            oauth: oauth::ClientLimits {
                document: oauth::Limits {
                    document_bytes: 1024,
                    string_bytes: 512,
                    token_bytes: 256,
                    client_bytes: 64,
                    detail_bytes: 64,
                    record_bytes: 1024,
                    depth: 8,
                    tokens: 64,
                },
                uri_bytes: 256,
                scope_bytes: 64,
                state_bytes: 64,
                code_bytes: 128,
                url_bytes: 1024,
                request_bytes: 1024,
                sign_in_time: Duration::from_secs(120),
                request_time: Duration::from_secs(10),
                backoff_base: Duration::from_secs(2),
                backoff_ceiling: Duration::from_secs(8),
                max_attempts: 3,
            },
            refresh_before: Duration::from_secs(5),
        }
    }

    fn registration() -> oauth::Registration {
        oauth::Registration {
            authorization_url: bytes(b"https://issuer.example/authorize"),
            token_endpoint: bytes(b"https://issuer.example/token"),
            client_id: bytes(b"client"),
            redirect_uri: bytes(b"http://127.0.0.1:2345/callback"),
            scope: bytes(b"read profile"),
            wire: oauth::WireFormat::Form,
            client_secret: None,
            pkce_for_confidential: false,
            metadata_claim: None,
        }
    }

    fn begin(saved: Option<oauth::SavedToken>) -> Begin {
        Begin {
            registration: registration(),
            saved,
            state: bytes(b"0123456789abcdef"),
            verifier: bytes(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            now: now(),
            wall: wall(),
        }
    }

    fn issuer() -> fake::Issuer {
        fake::Issuer::new(
            fake::Config {
                authorization_url: bytes(b"https://issuer.example/authorize"),
                token_endpoint: bytes(b"https://issuer.example/token"),
                client_id: bytes(b"client"),
                client_secret: None,
                redirect_uri: bytes(b"http://127.0.0.1:2345/callback"),
                refresh_token: bytes(b"refresh-old"),
            },
            fake::Limits {
                document: limits().oauth.document,
                uri_bytes: 1024,
                request_bytes: 1024,
                codes: 2,
                rotations: 2,
                plans: 2,
            },
        )
        .expect("fake issuer")
    }

    fn queue_token(issuer: &mut fake::Issuer, status: u16) {
        issuer
            .queue(fake::Plan {
                status,
                body: fake::Body::Token(oauth::TokenResponse {
                    access_token: bytes(b"access-new"),
                    refresh_token: Some(bytes(b"refresh-new")),
                    expires_in: 30,
                }),
                delay: Duration::ZERO,
                retry_after: Duration::ZERO,
            })
            .expect("one response plan");
    }

    fn fake_step(issuer: &mut fake::Issuer, event: fake::Event) -> fake::Request {
        let mut out = Queue::with_capacity(fake::MAX_OUT);
        issuer.step(event, &mut out);
        out.pop().expect("fake answer")
    }

    fn step(credential: &mut Credential, event: CredentialEvent) -> CredentialRequest {
        let mut out = Queue::with_capacity(1);
        credential.from_below(event, &mut out);
        out.pop().expect("credential request")
    }

    #[test]
    fn sign_in_with_fake_issuer_lends_only_after_token_file_is_saved() {
        let mut credential = Credential::new(7, limits()).expect("component");
        let mut issuer = issuer();
        queue_token(&mut issuer, 200);
        let mut out = Queue::with_capacity(1);
        credential.begin(begin(None), &mut out);
        let Some(CredentialRequest::Visit { url }) = out.pop() else { panic!("visit address") };
        let fake::Request::Redirect { uri, state, code } =
            fake_step(&mut issuer, fake::Event::Authorize { url, now: now() })
        else {
            panic!("loopback redirect")
        };
        let CredentialRequest::Http(request) = step(
            &mut credential,
            CredentialEvent::Redirected { uri, state, code: Some(code), error: None, now: now() },
        ) else {
            panic!("token request")
        };
        let fake::Request::Http(response) =
            fake_step(&mut issuer, fake::Event::Post { request, now: now(), wall: wall() })
        else {
            panic!("token response")
        };
        let CredentialRequest::Save { account, bytes: token } = step(&mut credential, CredentialEvent::Http(response))
        else {
            panic!("durable save before grant")
        };
        assert_eq!(account, 7);
        let saved = oauth::decode_record(&token, &limits().oauth.document).expect("saved token bytes");
        assert_eq!(saved.access_token.as_ref(), b"access-new");
        let CredentialRequest::Ready { event: local::Event::Credential { grant }, value } =
            step(&mut credential, CredentialEvent::Stored { wall: wall() })
        else {
            panic!("grant after store")
        };
        assert_eq!(grant.name.account, 7);
        assert_eq!(grant.name.generation, 1);
        assert_eq!(value.as_ref(), b"access-new");
    }

    #[test]
    fn refresh_refused_by_fake_issuer_reports_no_credential() {
        let mut credential = Credential::new(7, limits()).expect("component");
        let mut issuer = issuer();
        let saved = oauth::SavedToken {
            key: 7,
            generation: 1,
            access_token: bytes(b"old"),
            refresh_token: Some(bytes(b"wrong-refresh")),
            metadata: None,
            expires_at: wall(),
        };
        let mut out = Queue::with_capacity(1);
        credential.begin(begin(Some(saved)), &mut out);
        let Some(CredentialRequest::Http(request)) = out.pop() else { panic!("refresh request") };
        let fake::Request::Http(response) =
            fake_step(&mut issuer, fake::Event::Post { request, now: now(), wall: wall() })
        else {
            panic!("refused refresh")
        };
        let CredentialRequest::Failed { event: local::Event::NoCredential { account, reason } } =
            step(&mut credential, CredentialEvent::Http(response))
        else {
            panic!("no credential")
        };
        assert_eq!(account, 7);
        assert_eq!(reason, CredentialFailure::Refresh);
    }

    #[test]
    fn expiring_grant_is_refreshed_before_it_is_lent() {
        let mut credential = Credential::new(7, limits()).expect("component");
        let mut issuer = issuer();
        queue_token(&mut issuer, 200);
        let saved = oauth::SavedToken {
            key: 7,
            generation: 1,
            access_token: bytes(b"old-access"),
            refresh_token: Some(bytes(b"refresh-old")),
            metadata: None,
            expires_at: wall(),
        };
        let mut out = Queue::with_capacity(1);
        credential.begin(begin(Some(saved)), &mut out);
        let Some(CredentialRequest::Http(request)) = out.pop() else { panic!("refresh request") };
        let fake::Request::Http(response) =
            fake_step(&mut issuer, fake::Event::Post { request, now: now(), wall: wall() })
        else {
            panic!("refresh response")
        };
        let CredentialRequest::Save { bytes: token, .. } = step(&mut credential, CredentialEvent::Http(response))
        else {
            panic!("save rotated refresh token")
        };
        let stored = oauth::decode_record(&token, &limits().oauth.document).expect("saved token");
        assert_eq!(stored.generation, 2);
        assert_eq!(stored.refresh_token.as_deref(), Some(b"refresh-new".as_slice()));
        let CredentialRequest::Ready { event: local::Event::Credential { grant }, value } =
            step(&mut credential, CredentialEvent::Stored { wall: wall() })
        else {
            panic!("grant after save")
        };
        assert_eq!(grant.name.generation, 2);
        assert_eq!(value.as_ref(), b"access-new");
    }
    #[test]
    fn an_access_only_token_is_lent_until_expiry_then_fails_without_sign_in() {
        for fresh in [true, false] {
            let mut credential = Credential::new(7, limits()).expect("component");
            let mut saved = oauth::SavedToken {
                key: 7,
                generation: 3,
                access_token: bytes(b"access-only"),
                refresh_token: None,
                metadata: None,
                expires_at: wall(),
            };
            if fresh {
                saved.expires_at = Wall::from_nanos(wall().as_nanos() + Duration::from_secs(3600).as_nanos());
            }
            let mut out = Queue::with_capacity(1);
            credential.begin(begin(Some(saved)), &mut out);
            match out.pop().expect("one terminal") {
                CredentialRequest::Ready { event: local::Event::Credential { grant }, value } if fresh => {
                    assert_eq!(grant.name.generation, 3);
                    assert_eq!(value.as_ref(), b"access-only");
                }
                CredentialRequest::Failed { event: local::Event::NoCredential { reason, .. } } if !fresh => {
                    assert_eq!(reason, CredentialFailure::Refresh);
                }
                _ => panic!("access-only token produces a terminal without sign-in or HTTP"),
            }
            assert!(out.is_empty());
            assert!(credential.next_deadline().is_none());
        }
    }
}
