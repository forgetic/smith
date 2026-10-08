//! Reusable local-host processes and hosted factories (testing.md, sections
//! 2.1, 2.2 and 5). Each process owns its IO and outside observations. Launch
//! arguments carry immutable scenario settings; no process knows a simulator,
//! fake checkout, another process's heap or the driving loop.
use crate::{
    llm::Peer,
    oauth::{Browser, Peer as Issuer},
    terminal::Terminal,
};
use serde::{Deserialize, Serialize};
use skein_fake_oauth as fake_oauth;
use skein_io::kernel;
use skein_lib::{Duration, Queue, Time, Token, Wall};
use skein_world::{Host, Inherited, StartupRoot};
use smith::local_host::{Local, Resources, token_limits};
use smith::local_settings;
use smith_agent_service as agent;
use smith_local_domain as local;
use smith_local_service as service;
use std::path::PathBuf;

/// Immutable configuration sent by the terminal and passed to a spawned agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Launch {
    pub seed: u64,
    pub tls: bool,
    pub trust_der: Option<PathBuf>,
    pub change: bool,
    pub in_process: bool,
    pub keep_facts: bool,
    pub authenticated: bool,
    pub state_directory: PathBuf,
    pub token_directory: PathBuf,
    pub root_path: PathBuf,
}
impl Launch {
    /// Exact arguments used unchanged by both kernel backends.
    #[must_use]
    pub fn arguments(&self) -> Box<[Box<[u8]>]> {
        Box::new([serde_json::to_vec(self).expect("scenario arguments").into_boxed_slice()])
    }
    fn read(spawn: &kernel::Spawn) -> Self {
        assert_eq!(spawn.args.len(), 1, "one bounded scenario document");
        serde_json::from_slice(&spawn.args[0]).expect("immutable scenario settings")
    }
}

/// The bounded agent settings shared by both local placements.
pub fn lower_configuration() -> agent::Config {
    let mut lower = smith_agent_process_world::configuration();
    lower.limits.llm.head = Some(Duration::from_secs(10));
    lower.limits.machine.file_bytes = 4096;
    lower.limits.machine.entries = 32;
    lower.limits.file_bytes = 8192;
    lower.limits.file_entries = 32;
    lower.limits.file_slots = 8;
    lower
}

/// Plaintext local configuration for a replayable simulated invocation.
pub fn configuration(root: kernel::Fd, change: bool) -> (service::Config, agent::Config) {
    configuration_with(root, change, false)
}

/// Same bounded local policy with transport selected by immutable scenario settings.
#[must_use]
pub fn configuration_with(root: kernel::Fd, change: bool, tls: bool) -> (service::Config, agent::Config) {
    let lower = lower_configuration_with(tls);
    let mut source = serde_json::json!({
        "agent": {}, "chat": "main", "instructions": "@local-shell Assist",
        "models": [{"endpoint": "", "name": "fake", "max_tokens": 1024,
            "input_price": 0, "cached_price": 0, "output_price": 0, "price_unit": 1}],
        "budget": {"turns": 8, "spend": 1, "seconds": 60}, "waiting_seconds": 30,
        "contract": {"form": "report", "max": 128}
    });
    if change {
        source["directories"] = serde_json::json!([{ "name": "repo", "path": "repo", "writable": true, "git": true }]);
        source["contract"] = serde_json::json!({ "form": "change", "checks_must_pass": false, "fields": [{"name": "title", "max": 256}, {"name": "body", "max": 1024}] });
    }
    let settings: local_settings::Settings = serde_json::from_value(source).expect("host settings");
    let endpoints = lower.channel_endpoints.clone();
    let policy = local_settings::policy(&settings, &endpoints, lower.limits.domain).expect("host charter policy");
    let charter = local_settings::charter(&policy.config, &endpoints).expect("wire charter");
    let mut host = smith_host_world::limits();
    host.accounts = 1;
    host.charter_bytes = 65_536;
    host.transcript_bytes = 1_048_576;
    host.answered_bytes = 65_536;
    host.message_bytes = 4096;
    host.messages = 8;
    host.call_bytes = 32_768;
    host.turns = 8;
    host.turn_bytes = 65_536;
    host.unacknowledged_bytes = 1_000_000_000;
    host.fact_bytes = 512;
    host.outcome_bytes = 4096;
    let queue = local::max_out(&policy.limits).max(smith_host_domain::max_out(&host)).max(256);
    let process = service::ProcessLimits {
        io: lower.limits.io,
        channel: smith_host_protocol::Limits {
            bodies: lower.limits.channel.bodies,
            channel: lower.limits.channel.channel,
            calls: host.calls,
        },
        detail_bytes: host.detail_bytes,
        queue,
    };
    (
        service::Config {
            local: policy.config,
            limits: service::Limits { local: policy.limits, host, process, queue },
            charter,
            endpoints,
            paths: policy.paths,
            launch: service::Launch {
                program: b"smith-agent".as_slice().into(),
                arguments: Box::new([]),
                environment: Box::new([]),
                root,
                directory: b".".as_slice().into(),
            },
        },
        lower,
    )
}

/// The same agent policy, with loopback plaintext or trust in the test root.
#[must_use]
pub fn lower_configuration_with(tls: bool) -> agent::Config {
    let mut lower = lower_configuration();
    if tls {
        lower.llm_endpoints = smith_protocol_llm::Endpoints::new(
            Box::new([smith_protocol_llm::ConfiguredEndpoint {
                name: smith_domain::llm::Endpoint(0),
                destination: skein_llm_connection::Endpoint {
                    address: kernel::Addr::from((std::net::Ipv4Addr::LOCALHOST, 443)),
                    transport: skein_llm_connection::Transport::Tls {
                        server_name: skein_tls_world::pki::name(),
                        trust: skein_tls_world::pki::client(&[]),
                    },
                    llm: skein_llm_world::call(7).endpoint,
                },
                account: 0,
                reasoning_effort: None,
                cache_key: None,
                identity: smith_protocol_llm::IdentityProfile::Plain,
            }]),
            1,
            1,
        )
        .expect("one TLS endpoint");
    }
    lower
}

/// Account configuration shared by simulated and real hosted startup.
#[must_use]
pub fn accounts(authenticated: bool) -> Box<[local_settings::Account]> {
    accounts_with(authenticated, false, None)
}

/// Accounts configured for the same issuer face in either tier.
#[must_use]
pub fn accounts_with(
    authenticated: bool,
    tls: bool,
    trust_der: Option<&std::path::Path>,
) -> Box<[local_settings::Account]> {
    if authenticated {
        let mut accounts = Box::new([local_settings::Account {
            number: 0,
            account_id: "acc".into(),
            oauth: Some(local_settings::OAuth {
                authorization_url: "http://127.0.0.1:444/authorize".into(),
                token_endpoint: "http://127.0.0.1:444/token".into(),
                client_id: "client".into(),
                redirect_uri: "http://127.0.0.1:2345/callback".into(),
                scope: "read".into(),
                address: "127.0.0.1:444".into(),
                server_name: String::new(),
                trust_der: None,
                json: false,
            }),
        }]);
        if tls {
            let oauth = accounts[0].oauth.as_mut().expect("OAuth account");
            oauth.authorization_url = "https://127.0.0.1:444/authorize".into();
            oauth.token_endpoint = "https://127.0.0.1:444/token".into();
            oauth.server_name = "skein.test".into();
            oauth.trust_der = Some(trust_der.expect("test trust file").to_str().expect("UTF-8 trust path").into());
        }
        accounts
    } else {
        Box::new([local_settings::Account { number: 0, account_id: "acc".into(), oauth: None }])
    }
}

/// The scripted issuer and its one successful token rotation.
#[must_use]
pub fn issuer() -> Issuer {
    issuer_with(skein_fake_peers::Transport::Plaintext)
}

/// The identical issuer domain, with TLS selected in the real-loop tier.
#[must_use]
pub fn issuer_with(transport: skein_fake_peers::Transport) -> Issuer {
    let scheme = if transport == skein_fake_peers::Transport::Tls { "https" } else { "http" };
    let limits = fake_oauth::Limits {
        document: token_limits(),
        uri_bytes: 8192,
        request_bytes: 16_384,
        codes: 4,
        rotations: 4,
        plans: 4,
    };
    let mut issuer = Issuer::new(
        (std::net::Ipv4Addr::LOCALHOST, 444).into(),
        transport,
        smith_agent_process_world::fake::limits(),
        fake_oauth::Config {
            authorization_url: format!("{scheme}://127.0.0.1:444/authorize").into_bytes().into(),
            token_endpoint: format!("{scheme}://127.0.0.1:444/token").into_bytes().into(),
            client_id: b"client".as_slice().into(),
            client_secret: None,
            redirect_uri: b"http://127.0.0.1:2345/callback".as_slice().into(),
            refresh_token: b"refresh-old".as_slice().into(),
        },
        limits,
        skein_http::server::Limits { head: 8192, headers: 32, body: 16_384, read: 1024, response: 49_152, send: 1024 },
    )
    .expect("issuer fixture");
    issuer
        .queue(fake_oauth::Plan {
            status: 200,
            body: fake_oauth::Body::Token(skein_oauth::TokenResponse {
                access_token: b"access-new".as_slice().into(),
                refresh_token: Some(b"refresh-new".as_slice().into()),
                expires_in: 7200,
            }),
            delay: Duration::ZERO,
            retry_after: Duration::ZERO,
        })
        .expect("token plan");
    issuer
}

/// Local shell, retained emitted facts and unused-stderr settlement.
pub struct LocalProcess {
    local: Local,
    facts: Vec<local::Fact>,
    keep_facts: bool,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    stderr_closed: bool,
}
impl LocalProcess {
    /// Wrap a configured shared shell inside its own metered factory call.
    #[must_use]
    pub fn new(local: Local, stderr: kernel::Fd, keep_facts: bool) -> Self {
        let mut submissions = Queue::with_capacity(2048);
        submissions.push(kernel::Submit { op: Token::new(u64::MAX), kind: kernel::Op::Close { fd: stderr } });
        Self {
            local,
            facts: Vec::with_capacity(256),
            keep_facts,
            completions: Queue::with_capacity(2048),
            submissions,
            stderr_closed: false,
        }
    }

    /// Facts emitted at the public boundary, independent of internal state.
    #[must_use]
    pub fn facts(&self) -> &[local::Fact] {
        &self.facts
    }
    /// The shared shell's public terminal outcome.
    #[must_use]
    pub fn result(&self) -> Option<Result<local::ExitStatus, &str>> {
        self.local.result()
    }
}
impl Host for LocalProcess {
    fn iterate(&mut self, now: Time, wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            if complete.op == Token::new(u64::MAX) {
                assert!(complete.result.is_ok(), "unused stderr closes");
                self.stderr_closed = true;
            } else {
                self.local.completions().push(complete);
            }
        }
        self.local.iterate(now, wall);
        while let Some(fact) = self.local.pop_fact() {
            if self.keep_facts {
                assert!(self.facts.len() < 256);
                self.facts.push(fact);
            }
        }
        while let Some(submit) = self.local.submissions().pop() {
            self.submissions.push(submit);
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        self.local.work_pending(now) || !self.completions.is_empty() || !self.submissions.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        self.local.next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.local.is_empty() && self.stderr_closed && self.completions.is_empty() && self.submissions.is_empty()
    }
    fn exit(&self) -> Option<kernel::Exit> {
        self.is_empty()
            .then(|| kernel::Exit::Code(u8::from(!matches!(self.local.result(), Some(Ok(local::ExitStatus::Success))))))
    }
    fn worst_case(&self) -> u64 {
        self.local.worst_case() + 1_048_576 + (size_of::<local::Fact>() * 256 + size_of::<Self>()) as u64
    }
    fn operations(&self) -> u32 {
        self.local.operations() + 1
    }
}

/// Browser admitted before sign-in, receiving a bounded URL flag between turns.
pub struct BrowserProcess {
    browser: Option<Browser>,
    url: Box<[u8]>,
    length: usize,
    stopped: bool,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
}
impl Default for BrowserProcess {
    fn default() -> Self {
        Self::new()
    }
}
impl BrowserProcess {
    /// Prepare a browser without allocating outside its process later.
    #[must_use]
    pub fn new() -> Self {
        Self {
            browser: None,
            url: vec![0; 8192].into_boxed_slice(),
            length: 0,
            stopped: false,
            completions: Queue::with_capacity(64),
            submissions: Queue::with_capacity(64),
        }
    }
    /// The referee copies only observed URL bytes into already owned storage.
    pub fn visit(&mut self, url: &[u8]) {
        assert!(self.length == 0 && self.browser.is_none() && url.len() <= self.url.len());
        self.url[..url.len()].copy_from_slice(url);
        self.length = url.len();
    }
    /// Stop an unused browser once the terminal exits.
    pub fn shutdown(&mut self) {
        self.stopped = true;
    }
    /// Whether one authorization page was actually configured.
    #[must_use]
    pub fn pages(&self) -> u32 {
        u32::from(self.length > 0)
    }
    /// The browser's actual loopback callback response.
    #[must_use]
    pub fn replied(&self) -> bool {
        self.browser.as_ref().is_some_and(Browser::saw_reply)
    }
}
impl Host for BrowserProcess {
    fn iterate(&mut self, now: Time, wall: Wall) {
        if self.browser.is_none() && self.length > 0 {
            self.browser = Some(Browser::new(&self.url[..self.length]));
        }
        if let Some(browser) = &mut self.browser {
            while let Some(complete) = self.completions.pop() {
                browser.completions().push(complete);
            }
            browser.iterate(now, wall);
            while let Some(submit) = browser.submissions().pop() {
                self.submissions.push(submit);
            }
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        (self.browser.is_none() && self.length > 0)
            || self.browser.as_ref().is_some_and(|b| b.work_pending(now))
            || !self.completions.is_empty()
            || !self.submissions.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        self.browser.as_ref().and_then(Host::next_deadline)
    }
    fn is_empty(&self) -> bool {
        self.browser.as_ref().map_or(self.stopped, Host::is_empty)
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        crate::oauth::browser_worst_case() + 131_072
    }
    fn operations(&self) -> u32 {
        128
    }
}

/// Issuer transport and the wall time of its first observed token POST.
pub struct IssuerProcess {
    peer: Issuer,
    first_post: Option<Wall>,
}
impl IssuerProcess {
    /// Build the scenario's independent issuer inside its metered process.
    #[must_use]
    pub fn new() -> Self {
        Self::configured(skein_fake_peers::Transport::Plaintext)
    }
    /// Construct the same issuer process with explicit tier transport.
    #[must_use]
    pub fn configured(transport: skein_fake_peers::Transport) -> Self {
        Self { peer: issuer_with(transport), first_post: None }
    }
    /// Peer observations only.
    #[must_use]
    pub fn peer(&self) -> &Issuer {
        &self.peer
    }
    /// Observed token endpoint arrival time.
    #[must_use]
    pub fn first_post(&self) -> Option<Wall> {
        self.first_post
    }
    /// Flag normal shutdown after terminal settlement.
    pub fn shutdown(&mut self) {
        self.peer.shutdown();
    }
}
impl Default for IssuerProcess {
    fn default() -> Self {
        Self::new()
    }
}
impl Host for IssuerProcess {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.peer.iterate(now, wall);
        if self.first_post.is_none() && crate::oauth::posts(&self.peer) > 0 {
            self.first_post = Some(wall);
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        self.peer.completions()
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        self.peer.submissions()
    }
    fn work_pending(&self, now: Time) -> bool {
        self.peer.work_pending(now)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.peer.next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.peer.is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.peer.worst_case() + size_of::<Self>() as u64
    }
    fn operations(&self) -> u32 {
        self.peer.operations()
    }
}

/// The scenario's processes, driven unchanged by either shared kernel loop.
pub enum Proc {
    /// The scripted terminal, starting one local invocation and draining its exit.
    Terminal(Box<Terminal>),
    /// The actual shared local shell, ending after its public result settles.
    Local(Box<LocalProcess>),
    /// The spawned agent service, ending after its channel and descriptors settle.
    Agent(Box<smith_agent_process_world::process::Agent>),
    /// The independent fake provider, stopped after the terminal exits.
    Peer(Box<Peer>),
    /// The independent fake issuer and observed POST timing.
    Issuer(Box<IssuerProcess>),
    /// The scripted browser, following one authorization redirect to callback.
    Browser(Box<BrowserProcess>),
    /// The simulated fake-checkout child; real worlds execute git externally.
    Git(Box<crate::git::Prepared>),
}
impl Proc {
    fn host(&self) -> &dyn Host {
        match self {
            Self::Terminal(p) => p.as_ref(),
            Self::Local(p) => p.as_ref(),
            Self::Agent(p) => p.as_ref(),
            Self::Peer(p) => p.as_ref(),
            Self::Issuer(p) => p.as_ref(),
            Self::Browser(p) => p.as_ref(),
            Self::Git(p) => p.as_ref(),
        }
    }
    fn host_mut(&mut self) -> &mut dyn Host {
        match self {
            Self::Terminal(p) => p.as_mut(),
            Self::Local(p) => p.as_mut(),
            Self::Agent(p) => p.as_mut(),
            Self::Peer(p) => p.as_mut(),
            Self::Issuer(p) => p.as_mut(),
            Self::Browser(p) => p.as_mut(),
            Self::Git(p) => p.as_mut(),
        }
    }
}
impl Host for Proc {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.host_mut().iterate(now, wall);
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        self.host_mut().completions()
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        self.host_mut().submissions()
    }
    fn work_pending(&self, now: Time) -> bool {
        self.host().work_pending(now)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.host().next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.host().is_empty()
    }
    fn exit(&self) -> Option<kernel::Exit> {
        self.host().exit()
    }
    fn worst_case(&self) -> u64 {
        self.host().worst_case() + size_of::<Self>() as u64
    }
    fn operations(&self) -> u32 {
        self.host().operations()
    }
}
fn root(inherited: &Inherited, name: &[u8]) -> kernel::Fd {
    inherited.roots.iter().find(|(n, _)| n.as_ref() == name).expect("named child root").1
}

/// Fresh launch, delivery and colocated effect roots for the local shell.
#[must_use]
pub fn local_roots(spawn: &kernel::Spawn) -> Vec<StartupRoot> {
    let launch = Launch::read(spawn);
    let path = launch.root_path.as_os_str().as_encoded_bytes();
    let mut roots = vec![StartupRoot { name: b"launch".as_slice().into(), path: path.into() }];
    if launch.change {
        let path = launch.root_path.join("repo");
        roots.push(StartupRoot {
            name: b"delivery".as_slice().into(),
            path: path.as_os_str().as_encoded_bytes().into(),
        });
        if launch.in_process {
            roots.push(StartupRoot {
                name: b"effect".as_slice().into(),
                path: path.as_os_str().as_encoded_bytes().into(),
            });
        }
    }
    roots
}
/// Adopt local startup resources without any backend-specific operations.
#[must_use]
pub fn make_local(spawn: &kernel::Spawn, inherited: &Inherited) -> Proc {
    let launch = Launch::read(spawn);
    let (mut config, lower) = configuration_with(root(inherited, b"launch"), launch.change, launch.tls);
    config.launch.arguments = launch.arguments();
    let local = Local::new(
        config,
        launch.in_process.then_some(lower),
        Resources {
            state_directory: launch.state_directory,
            token_directory: launch.token_directory,
            input: inherited.pipes.iter().find(|(child, _)| *child == 0).expect("stdin").1,
            output: inherited.pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1,
            signals: inherited.signal,
            delivery_roots: if launch.change { Box::new([root(inherited, b"delivery")]) } else { Box::new([]) },
            effect_roots: if launch.change && launch.in_process {
                Box::new([root(inherited, b"effect")])
            } else {
                Box::new([])
            },
            delivery_environment: Box::new([]),
            accounts: accounts_with(launch.authenticated, launch.tls, launch.trust_der.as_deref()),
            seed: launch.seed,
            oauth_entropy: [31; 32],
        },
    )
    .expect("actual shared local shell");
    Proc::Local(Box::new(LocalProcess::new(
        local,
        inherited.pipes.iter().find(|(child, _)| *child == 2).expect("stderr").1,
        launch.keep_facts,
    )))
}
/// The agent's declared workspace directories, each owned by this child.
#[must_use]
pub fn agent_roots(spawn: &kernel::Spawn) -> Vec<StartupRoot> {
    let launch = Launch::read(spawn);
    if launch.change {
        let path = launch.root_path.join("repo");
        vec![StartupRoot { name: b"repo".as_slice().into(), path: path.as_os_str().as_encoded_bytes().into() }]
    } else {
        vec![]
    }
}
/// Construct the actual agent service with the local scenario's settings.
#[must_use]
pub fn make_agent(spawn: &kernel::Spawn, inherited: &Inherited) -> Proc {
    let launch = Launch::read(spawn);
    Proc::Agent(Box::new(smith_agent_process_world::process::configured(
        lower_configuration_with(launch.tls),
        launch.seed,
        inherited,
    )))
}
