//! Shared local shell pass (protocol/hosts.md, sections 5.3–5.6). It keeps
//! chat and token file owners, the terminal writer, and kernel routing. It
//! knows no simulator or checkout state. `Local::new` takes all directories
//! and terminal descriptors; `Host::iterate` performs the service and shell
//! passes. Every store acknowledgement follows file and directory sync.

use std::path::PathBuf;

use skein_io::{self as io, kernel};
use skein_lib::stream::{OutputDown, OutputOutcome, OutputUp};
use skein_lib::{Env, Map, Queue, Time, Token, Wall};
use skein_world::Host;
use smith_agent_service as agent;
use smith_local_domain as domain;
use smith_local_service as service;

use crate::{
    local_auth::{self, Auth},
    local_settings::Account,
    local_store::Store,
    local_tokens::Tokens,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Service(Token),
    Writer(Token),
    Launch,
}

/// Paths and inherited descriptors owned by one invocation's shared shell.
pub struct Resources {
    pub state_directory: PathBuf,
    pub token_directory: PathBuf,
    pub input: kernel::Fd,
    pub output: kernel::Fd,
    pub signals: kernel::Fd,
    pub delivery_roots: Box<[kernel::Fd]>,
    pub effect_roots: Box<[kernel::Fd]>,
    pub delivery_environment: Box<[Box<[u8]>]>,
    pub accounts: Box<[Account]>,
    pub seed: u64,
    /// Unpredictable bytes from the binary shell; worlds supply deterministic bytes.
    pub oauth_entropy: [u8; 32],
}

/// The local service and its shell effects, independent of the driving loop.
pub struct Local {
    service: service::Service,
    store: Store,
    tokens: Tokens,
    accounts: Box<[Auth]>,
    writer: io::Io,
    env: Env<io::Limits>,
    output: Token,
    launch_root: Option<kernel::Fd>,
    launch_closing: bool,
    routes: Map<Token, Route>,
    operation: u64,
    pending: Option<(Token, Box<[u8]>)>,
    right: u64,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    local_completions: Queue<kernel::Complete>,
    local_submissions: Queue<kernel::Submit>,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    exit: Option<domain::ExitStatus>,
    closing: bool,
    error: Option<String>,
    worst: u64,
    operations: u32,
}

impl Local {
    /// Compose either placement with the same file and terminal shell pass.
    pub fn new(config: service::Config, lower: Option<agent::Config>, resources: Resources) -> Result<Self, String> {
        let mut worst = match &lower {
            Some(lower) => service::in_process_worst_case(&config.limits, &lower.limits),
            None => service::worst_case(&config.limits),
        }
        .ok_or("local memory calculation overflowed")?;
        if resources.accounts.len() > usize::try_from(config.limits.local.agent.accounts).expect("account bound fits")
            || resources.accounts.iter().any(|account| account.account_id.len() > 1024)
        {
            return Err("local accounts exceed their configured bounds".into());
        }
        for (position, account) in resources.accounts.iter().enumerate() {
            if resources.accounts[..position].iter().any(|prior| prior.number == account.number) {
                return Err("local account numbers must be unique".into());
            }
        }
        let endpoints = config.endpoints.clone();
        let launch_root = config.launch.root;
        let queue = config.limits.queue;
        let outer_queue = queue.checked_mul(2).ok_or("shell queue count overflowed")?;
        let show_bytes = config.limits.local.show_bytes;
        let io_limits = config.limits.process.io;
        if !resources.accounts.is_empty() && !crate::local_auth_http::fits(&io_limits) {
            return Err("local IO limits cannot carry OAuth TLS records".into());
        }
        worst = worst
            .checked_add(
                local_auth::worst_case()
                    .and_then(|bound| bound.checked_mul(u64::try_from(resources.accounts.len()).ok()?))
                    .ok_or("account memory calculation overflowed")?,
            )
            .ok_or("account memory calculation overflowed")?;
        let operations = io::operations(&io_limits)
            .and_then(|count| count.checked_mul(2))
            .and_then(|count| count.checked_add(64))
            .and_then(|count| count.checked_add(lower.as_ref().map_or(0, |lower| lower.limits.routes)))
            .ok_or("local IO operation count overflowed")?;
        let service = match lower {
            Some(lower) => service::Service::new_in_process(config, lower, resources.effect_roots, resources.seed),
            None if resources.effect_roots.is_empty() => service::Service::new(config, resources.seed),
            None => return Err("spawned mode cannot adopt colocated roots".into()),
        }
        .map_err(|error| format!("local service: {error:?}"))?;
        let store = Store::new(resources.state_directory, endpoints, 1 << 20)
            .map_err(|error| format!("chat store: {error}"))?;
        let tokens = Tokens::new(&resources.token_directory, token_limits())?;
        let mut writer = io::Io::new(&io_limits);
        let output = writer.adopt_write_pipe(resources.output).map_err(|_| "terminal output cannot be adopted")?;
        let requests = Queue::with_capacity(queue);
        worst = worst
            .checked_add(io::worst_case(&io_limits).ok_or("terminal memory overflowed")?)
            .and_then(|count| count.checked_add(u64::from(queue).checked_mul(u64::from(show_bytes.max(8192)))?))
            .and_then(|count| count.checked_add(Queue::<kernel::Complete>::worst_case(queue)?.checked_mul(3)?))
            .and_then(|count| count.checked_add(Queue::<kernel::Submit>::worst_case(queue)?.checked_mul(3)?))
            .and_then(|count| count.checked_add(Queue::<io::Event>::worst_case(queue)?))
            .and_then(|count| count.checked_add(Queue::<io::Request>::worst_case(queue)?))
            .and_then(|count| count.checked_add(Map::<Token, Route>::worst_case(operations)?))
            .and_then(|count| count.checked_add(u64::try_from(std::mem::size_of::<Self>()).ok()?))
            .ok_or("shell memory calculation overflowed")?;
        let accounts = resources
            .accounts
            .into_vec()
            .into_iter()
            .enumerate()
            .map(|(position, account)| Auth::new(account, position, resources.oauth_entropy))
            .collect::<Result<Vec<_>, _>>()?;
        let mut local = Self {
            service,
            store,
            tokens,
            accounts: accounts.into(),
            writer,
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: io_limits },
            output,
            launch_root: Some(launch_root),
            launch_closing: false,
            routes: Map::with_capacity(operations),
            operation: 1,
            pending: None,
            right: 1,
            events: Queue::with_capacity(queue),
            requests,
            local_completions: Queue::with_capacity(queue),
            local_submissions: Queue::with_capacity(queue),
            completions: Queue::with_capacity(outer_queue),
            submissions: Queue::with_capacity(outer_queue),
            exit: None,
            closing: false,
            error: None,
            worst,
            operations,
        };
        local
            .service
            .adopt_terminal(resources.input, resources.signals)
            .map_err(|fd| format!("local terminal {} cannot be adopted", fd.raw()))?;
        local
            .service
            .adopt_delivery_roots(resources.delivery_roots, resources.delivery_environment)
            .map_err(|error| format!("delivery directories: {error:?}"))?;
        Ok(local)
    }

    /// Content-free service facts available to the caller's independent referee.
    pub fn pop_fact(&mut self) -> Option<domain::Fact> {
        self.service.pop_fact()
    }

    /// The completed invocation's status after every lower close settles.
    #[must_use]
    pub fn result(&self) -> Option<Result<domain::ExitStatus, &str>> {
        if !self.is_empty() {
            return None;
        }
        self.exit.map(|status| match &self.error {
            Some(error) => Err(error.as_str()),
            None => Ok(status),
        })
    }

    /// The configured chat store, for shell diagnostics and crash-cut controls.
    pub fn store(&mut self) -> &mut Store {
        &mut self.store
    }

    fn files(&mut self) {
        for _ in 0..self.service.shell_requests().capacity() {
            let Some(request) = self.service.shell_requests().pop() else { break };
            let event = match request {
                domain::Request::Load => self.store.load(),
                domain::Request::SaveState { state, fresh } => self.store.save_state(state, fresh),
                domain::Request::SaveTurn { number, read, turn } => self.store.save_turn(number, read, turn),
                domain::Request::SaveDelivery { record } => self.store.save_delivery(&record),
                domain::Request::Credential { account } => {
                    self.credential(account);
                    continue;
                }
                domain::Request::Exit { status } => {
                    self.exit = Some(status);
                    continue;
                }
                domain::Request::Show { .. }
                | domain::Request::Git { .. }
                | domain::Request::PlainStatus { .. }
                | domain::Request::Agent(_)
                | domain::Request::External(_) => unreachable!("service owns lower effects"),
            };
            self.service.local_event(match event {
                Ok(event) => event,
                Err(reason) => domain::Event::StoreFailed { reason },
            });
        }
    }

    fn credential(&mut self, account: u32) {
        if let Some(auth) = self.accounts.iter_mut().find(|auth| auth.account() == account) {
            auth.request();
        } else {
            self.service
                .local_event(domain::Event::NoCredential { account, reason: domain::CredentialFailure::Missing });
        }
    }

    fn authentication(&mut self) {
        for auth in &mut self.accounts {
            if self.exit.is_some() {
                auth.close(self.env.now, self.env.wall, &mut self.requests);
            }
            auth.due(self.env.now);
            if auth.wants() {
                match self.tokens.load(auth.account()) {
                    Ok(saved) => auth.begin(saved, self.env.now, self.env.wall),
                    Err(_) => {
                        auth.load_failed();
                        self.service.local_event(domain::Event::NoCredential {
                            account: auth.account(),
                            reason: domain::CredentialFailure::Missing,
                        });
                    }
                }
            }
            auth.tick(self.env.now, self.env.wall, &mut self.requests);
            if let Some(url) = auth.shown_url() {
                let mut text = b"Sign in: ".to_vec();
                text.extend_from_slice(&url);
                text.push(b'\n');
                self.service.output().push(text.into());
            }
            for _ in 0..8 {
                let Some(request) = auth.take() else { break };
                match request {
                    smith_local_protocol::CredentialRequest::Visit { url } => auth.visit(url, &mut self.requests),
                    smith_local_protocol::CredentialRequest::Http(request) => auth.http(request),
                    smith_local_protocol::CredentialRequest::Save { account, bytes } => {
                        match self.tokens.save(account, &bytes) {
                            Ok(()) => auth.stored(self.env.wall),
                            Err(_) => auth.store_failed(),
                        }
                    }
                    smith_local_protocol::CredentialRequest::Ready {
                        event: domain::Event::Credential { grant },
                        value,
                    } => {
                        let length = u16::try_from(auth.account_id().len()).expect("bounded account identifier");
                        let mut envelope = Vec::with_capacity(2 + auth.account_id().len() + value.len());
                        envelope.extend_from_slice(&length.to_be_bytes());
                        envelope.extend_from_slice(auth.account_id());
                        envelope.extend_from_slice(&value);
                        auth.lent(grant.valid, self.env.now);
                        self.service.credential(grant, envelope.into());
                    }
                    smith_local_protocol::CredentialRequest::Failed { event } => self.service.local_event(event),
                    smith_local_protocol::CredentialRequest::Ready { .. } => {
                        unreachable!("credential translator lends only a grant")
                    }
                }
            }
        }
    }

    fn writer_up(&mut self) {
        while self.writer.is_ready() {
            io::resume(&mut self.writer, &self.env, &mut self.events, &mut self.local_submissions);
        }
        for _ in 0..self.local_completions.capacity() {
            let Some(complete) = self.local_completions.pop() else { break };
            io::up(&mut self.writer, &self.env, complete, &mut self.events, &mut self.local_submissions);
        }
        for _ in 0..self.env.limits.sockets {
            if self.writer.is_ready() {
                io::resume(&mut self.writer, &self.env, &mut self.events, &mut self.local_submissions);
            }
            if self.writer.is_due(self.env.now) {
                io::fire(&mut self.writer, &self.env, &mut self.events, &mut self.local_submissions);
            }
        }
        while let Some(event) = self.events.pop() {
            let owner = local_auth::event_owner(&event);
            if let Some(auth) = self.accounts.iter_mut().find(|auth| auth.owns(owner)) {
                auth.event(event, self.env.now, self.env.wall, &mut self.requests);
                continue;
            }
            match event {
                io::Event::Output { owner, up: OutputUp::Settled { right, outcome } } if owner == self.output => {
                    let (wanted, text) = self.pending.take().expect("terminal has one output right");
                    assert_eq!(right, wanted);
                    match outcome {
                        OutputOutcome::Granted => self.requests.push(io::Request::Output {
                            stream: self.output,
                            down: OutputDown::Send { right, bytes: text },
                        }),
                        OutputOutcome::Cancelled | OutputOutcome::Failed(_) => self.write_failed(),
                    }
                }
                io::Event::Failed { .. } | io::Event::Stream { up: skein_lib::stream::Up::Failed(_), .. } => {
                    self.write_failed()
                }
                io::Event::Closed { .. } => {}
                other => panic!("unexpected terminal writer event: {other:?}"),
            }
        }
    }

    fn write_failed(&mut self) {
        self.error = Some("terminal output failed".into());
        self.service.local_event(domain::Event::StoreFailed { reason: domain::StoreFailure::Write });
    }

    fn writer_down(&mut self) {
        if self.pending.is_none()
            && !self.closing
            && let Some(text) = self.service.output().pop()
        {
            let right = Token::new(self.right);
            self.right = self.right.checked_add(1).expect("terminal output names remain representable");
            let bytes = u32::try_from(text.len()).expect("bounded show text");
            if bytes > 0 {
                self.pending = Some((right, text));
                self.requests
                    .push(io::Request::Output { stream: self.output, down: OutputDown::Room { right, bytes } });
            }
        }
        if self.exit.is_some() && !self.closing && self.pending.is_none() && self.service.output().is_empty() {
            self.closing = true;
            self.requests.push(io::Request::Close { entity: self.output });
            let fd = self.launch_root.take().expect("one launch root");
            self.launch_closing = true;
            self.submit(kernel::Submit { op: Token::new(0), kind: kernel::Op::Close { fd } }, Route::Launch);
        }
        while self.writer.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.writer, &self.env, request, &mut self.local_submissions);
        }
        self.writer.reclaim();
    }
    fn submit(&mut self, submit: kernel::Submit, route: Route) {
        let kind = match submit.kind {
            kernel::Op::Cancel { target } => {
                let wanted = match route {
                    Route::Service(_) => Route::Service(target),
                    Route::Writer(_) => Route::Writer(target),
                    Route::Launch => unreachable!("launch directory only closes"),
                };
                let target = self
                    .routes
                    .iter()
                    .find_map(|(global, active)| (*active == wanted).then_some(*global))
                    .unwrap_or(Token::new(u64::MAX));
                kernel::Op::Cancel { target }
            }
            other => other,
        };
        let op = Token::new(self.operation);
        self.operation = self.operation.checked_add(1).expect("shell operation names remain representable");
        self.routes.insert(op, route).expect("configured operation routes");
        self.submissions.push(kernel::Submit { op, kind });
    }
}

impl Host for Local {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
        while let Some(complete) = self.completions.pop() {
            match self.routes.remove(&complete.op).expect("each operation has one lower owner") {
                Route::Service(op) => self.service.completions().push(kernel::Complete {
                    op,
                    kind: complete.kind,
                    result: complete.result,
                }),
                Route::Writer(op) => {
                    self.local_completions.push(kernel::Complete { op, kind: complete.kind, result: complete.result })
                }
                Route::Launch => {
                    self.launch_closing = false;
                    if complete.result.is_err() {
                        self.error = Some("launch directory close failed".into());
                    }
                }
            }
        }
        self.writer_up();
        service::iterate(&mut self.service, now, wall);
        self.files();
        self.authentication();
        self.writer_down();
        while let Some(submit) = self.service.submissions().pop() {
            let op = submit.op;
            self.submit(submit, Route::Service(op));
        }
        while let Some(submit) = self.local_submissions.pop() {
            let op = submit.op;
            self.submit(submit, Route::Writer(op));
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        self.service.work_pending(now)
            || self.accounts.iter().any(Auth::pending)
            || self.writer.is_ready()
            || self.writer.is_due(now)
            || !self.completions.is_empty()
            || !self.local_completions.is_empty()
            || !self.local_submissions.is_empty()
            || !self.events.is_empty()
            || !self.requests.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        [self.service.next_deadline(), self.writer.next_deadline()]
            .into_iter()
            .flatten()
            .chain(self.accounts.iter().filter_map(Auth::deadline))
            .min()
    }
    fn is_empty(&self) -> bool {
        self.exit.is_some()
            && self.closing
            && self.writer.is_empty()
            && self.pending.is_none()
            && self.routes.is_empty()
            && !self.launch_closing
            && self.launch_root.is_none()
            && self.completions.is_empty()
            && self.submissions.is_empty()
            && self.local_completions.is_empty()
            && self.local_submissions.is_empty()
            && self.events.is_empty()
            && self.requests.is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.worst
    }
    fn operations(&self) -> u32 {
        self.operations
    }
}

/// Bounded saved-token documents shared by both placements and the shell store.
#[must_use]
pub fn token_limits() -> skein_oauth::Limits {
    skein_oauth::Limits {
        document_bytes: 16_384,
        string_bytes: 8192,
        token_bytes: 8192,
        client_bytes: 1024,
        detail_bytes: 1024,
        record_bytes: 32_768,
        depth: 16,
        tokens: 1024,
    }
}
