//! Workspace authority, file and process ownership (protocol/agent.md,
//! sections 2 and 3; domain/tools.md, sections 4–6).
//! The component keeps root tokens and in-flight owners. It never knows
//! descriptors, file content before io reports it, or the domain's state.

use alloc::boxed::Box;

use skein_io as io;
use skein_lib::{Env, List, Map, Queue, Time, Token};
use smith_domain::{run, tools};

use crate::boundary::{Below, BelowEvent, FromDomain, ToDomain};
use crate::guide::{Guide, Step};
use crate::limits::Limits;
use crate::process::Process;

/// One host-admitted root and its immutable write authority.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Root {
    pub(crate) token: Token,
    pub(crate) writable: bool,
}

/// Which terminal the in-flight file request owes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A bounded whole-file load.
    Load,
    /// A bounded name-ordered directory scan.
    Scan,
    /// A conditional synced file replacement.
    Store,
}

/// One file operation kept until io's terminal, even after cancellation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pending {
    pub(crate) kind: Kind,
}

/// Workspace authority and in-flight requests; io keeps the filesystem.
#[derive(Debug)]
pub struct Component {
    roots: List<Root>,
    pending: Map<Token, Pending>,
    guides: Map<Token, Guide>,
    processes: Map<Token, Process>,
    routes: Map<Token, Token>,
    environment: Box<[tools::Var]>,
    environment_bytes: u32,
}

impl Component {
    /// Construct with fixed capacity before any request is admitted.
    #[must_use]
    pub fn new(limits: &Limits) -> Component {
        Component {
            roots: List::with_capacity(limits.roots),
            pending: Map::with_capacity(limits.operations),
            guides: Map::with_capacity(limits.operations),
            processes: Map::with_capacity(limits.processes),
            routes: Map::with_capacity(limits.processes.checked_mul(2).expect("pipe route count fits")),
            environment: Box::new([]),
            environment_bytes: limits.env_bytes,
        }
    }

    /// Install the run's already-opened root tokens and write authority.
    /// A service adopts their descriptors into `FileIo` before calling this.
    pub fn workspace(&mut self, workspace: &run::Workspace) {
        assert!(
            self.pending.is_empty() && self.guides.is_empty() && self.processes.is_empty(),
            "workspace changes only between runs"
        );
        self.roots = List::with_capacity(self.roots.capacity());
        for directory in &workspace.directories {
            self.roots.push(Root { token: directory.root, writable: directory.writable }).expect("admitted root count");
        }
    }

    /// Install explicit shell and check process defaults for this run.
    pub fn configure_environment(&mut self, environment: Box<[tools::Var]>) {
        assert!(self.processes.is_empty(), "configure only between runs");
        let mut bytes = 0_u64;
        for var in &environment {
            bytes = bytes
                .checked_add(u64::try_from(var.name.len()).expect("name length fits"))
                .expect("environment size fits");
            bytes = bytes
                .checked_add(u64::try_from(var.value.len()).expect("value length fits"))
                .expect("environment size fits");
            bytes = bytes.checked_add(1).expect("environment size fits");
        }
        assert!(bytes <= u64::from(self.environment_bytes), "configured environment fits its byte cap");
        self.environment = environment;
    }

    /// Translate one domain request; the caller reserves one cell per output.
    #[expect(clippy::too_many_lines, reason = "one exhaustive routing boundary covers all file and process requests")]
    pub fn from_domain(
        &mut self,
        env: &Env<Limits>,
        request: FromDomain,
        to_domain: &mut Queue<ToDomain>,
        below: &mut Queue<Below>,
    ) {
        match request {
            FromDomain::Op { owner, op, deadline } => {
                self.vacant(owner);
                if env.now >= deadline {
                    to_domain.push(ToDomain::Done { owner, done: tools::Done::TimedOut });
                    return;
                }
                match op {
                    tools::Op::Load { .. } | tools::Op::Scan { .. } | tools::Op::Store { .. } => {
                        match crate::files::request(&self.roots, &env.limits, owner, op) {
                            Ok((kind, request)) => match self.pending.insert(owner, Pending { kind }) {
                                Ok(None) => below.push(Below::File { request, deadline }),
                                Ok(Some(_)) => unreachable!("owner checked"),
                                Err(_) => to_domain.push(ToDomain::Done { owner, done: other() }),
                            },
                            Err(done) => to_domain.push(ToDomain::Done { owner, done }),
                        }
                    }
                    tools::Op::Spawn { cwd, command, env: vars, roots: _, head, tail } => {
                        let admissible = crate::files::check(&self.roots, &env.limits, cwd.root, &cwd.path, false);
                        let prepared = crate::process::shell(
                            owner,
                            cwd,
                            command,
                            vars,
                            &self.environment,
                            head,
                            tail,
                            deadline,
                            &env.limits,
                        );
                        self.start_tool(owner, admissible, prepared, to_domain, below);
                    }
                    tools::Op::Search { at, pattern, glob, hits, bytes } => {
                        let admissible = crate::files::check(&self.roots, &env.limits, at.root, &at.path, false);
                        self.start_tool(
                            owner,
                            admissible,
                            crate::process::search(owner, at, pattern, glob, hits, bytes, deadline, &env.limits),
                            to_domain,
                            below,
                        );
                    }
                }
            }
            FromDomain::Cancel { owner } => {
                if self.pending.contains_key(&owner) {
                    below.push(Below::CancelFile { owner });
                } else if let Some(process) = self.processes.get_mut(&owner) {
                    process.stop(env.now, env.limits.stop_grace, true, below);
                }
            }
            FromDomain::Read { owner, at, max, deadline } => {
                self.vacant(owner);
                if env.now >= deadline
                    || max > env.limits.file_bytes
                    || crate::files::check(&self.roots, &env.limits, at.root, &at.path, false).is_err()
                {
                    to_domain.push(ToDomain::Read { owner, read: run::Read::Failed });
                } else if self.guides.insert(owner, Guide::OpeningRead { max, deadline }).is_ok() {
                    below.push(Below::OpenRead { owner, root: at.root, path: at.path, deadline });
                } else {
                    to_domain.push(ToDomain::Read { owner, read: run::Read::Failed });
                }
            }
            FromDomain::Probe { owner, at, deadline } => {
                self.vacant(owner);
                if env.now >= deadline
                    || crate::files::check(&self.roots, &env.limits, at.root, &at.path, false).is_err()
                {
                    to_domain.push(ToDomain::Probed { owner, executable: false });
                } else if self.guides.insert(owner, Guide::OpeningProbe { deadline }).is_ok() {
                    below.push(Below::OpenRead { owner, root: at.root, path: at.path, deadline });
                } else {
                    to_domain.push(ToDomain::Probed { owner, executable: false });
                }
            }
            FromDomain::Check { owner, program, deadline, tail } => {
                self.vacant(owner);
                if env.now >= deadline {
                    to_domain.push(ToDomain::Checked {
                        owner,
                        ran: run::Ran { exit: run::Exit::TimedOut, output: Box::new([]), cut: 0 },
                    });
                    return;
                }
                let admissible = crate::files::check(&self.roots, &env.limits, program.root, &program.path, false);
                match admissible {
                    Ok(()) => {
                        let prepared =
                            crate::process::check(owner, program, &self.environment, tail, deadline, &env.limits);
                        match prepared {
                            Some((process, request)) => self.start_process(owner, process, request, below, to_domain),
                            None => failed_check(owner, to_domain),
                        }
                    }
                    Err(_) => failed_check(owner, to_domain),
                }
            }
            FromDomain::Abort { owner } => {
                if let Some(process) = self.processes.get_mut(&owner) {
                    process.stop(env.now, env.limits.stop_grace, true, below);
                }
            }
        }
    }

    fn vacant(&self, owner: Token) {
        assert!(
            !self.pending.contains_key(&owner)
                && !self.guides.contains_key(&owner)
                && !self.processes.contains_key(&owner),
            "one in-flight request per owner"
        );
    }

    fn start_tool(
        &mut self,
        owner: Token,
        admissible: Result<(), tools::Done>,
        request: Option<(Process, Below)>,
        to_domain: &mut Queue<ToDomain>,
        below: &mut Queue<Below>,
    ) {
        match admissible {
            Ok(()) => match request {
                Some((process, request)) => self.start_process(owner, process, request, below, to_domain),
                None => to_domain.push(ToDomain::Done { owner, done: other() }),
            },
            Err(done) => to_domain.push(ToDomain::Done { owner, done }),
        }
    }

    fn start_process(
        &mut self,
        owner: Token,
        process: Process,
        request: Below,
        below: &mut Queue<Below>,
        to_domain: &mut Queue<ToDomain>,
    ) {
        let purpose = process.purpose;
        match self.processes.insert(owner, process) {
            Ok(None) => below.push(request),
            Ok(Some(_)) => unreachable!("owner checked"),
            Err(_) => match request {
                Below::Spawn { .. } => match purpose {
                    crate::process::Purpose::Check => failed_check(owner, to_domain),
                    crate::process::Purpose::Shell | crate::process::Purpose::Search => {
                        to_domain.push(ToDomain::Done { owner, done: other() });
                    }
                },
                Below::File { .. } | Below::CancelFile { .. } | Below::OpenRead { .. } | Below::Process(_) => {
                    unreachable!("only spawn start")
                }
            },
        }
    }

    /// Consume one file or process event and release its owner on completion.
    pub fn from_below(
        &mut self,
        _env: &Env<Limits>,
        event: BelowEvent,
        to_domain: &mut Queue<ToDomain>,
        below: &mut Queue<Below>,
    ) {
        match event {
            BelowEvent::File(event) => {
                let owner = event.owner();
                if let Some(guide) = self.guides.remove(&owner) {
                    match guide.event(owner, event, below) {
                        Step::Continue(guide) => {
                            self.guides.insert(owner, guide).expect("guide slot returned");
                        }
                        Step::Done(terminal) => to_domain.push(terminal),
                    }
                } else {
                    let pending = self.pending.remove(&owner).expect("file terminal has admitted owner");
                    to_domain.push(ToDomain::Done { owner, done: crate::files::terminal(pending.kind, event) });
                }
            }
            BelowEvent::Process(event) => self.process_event(event, to_domain, below),
        }
    }

    fn process_event(&mut self, event: io::Event, to_domain: &mut Queue<ToDomain>, below: &mut Queue<Below>) {
        let owner = match &event {
            io::Event::Spawned { owner, .. }
            | io::Event::Exited { owner, .. }
            | io::Event::Stream { owner, .. }
            | io::Event::Failed { owner, .. }
            | io::Event::Closed { owner } => *owner,
            io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Output { .. }
            | io::Event::Shutdown { .. } => unreachable!("no sockets, output grants or signal source requested"),
        };
        let process_owner = match self.routes.get(&owner) {
            Some(route) => *route,
            None => owner,
        };
        let process = self.processes.get_mut(&process_owner).expect("process event has admitted owner");
        match event {
            io::Event::Spawned { child, pipes, .. } => {
                process.spawned(child, pipes, &mut self.routes, process_owner, below);
            }
            io::Event::Exited { exit, .. } => process.exited(exit),
            io::Event::Stream { owner: pipe, up } => crate::process::stream(process, pipe, up, below),
            io::Event::Failed { owner: failed, .. } => {
                process.failed();
                if self.routes.contains_key(&failed) {
                    below.push(Below::Process(io::Request::Abort { entity: failed }));
                }
            }
            io::Event::Closed { owner: closed } => {
                if self.routes.remove(&closed).is_some() {
                    process.pipe_closed(closed);
                } else {
                    process.child_closed = true;
                }
            }
            io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Output { .. }
            | io::Event::Shutdown { .. } => unreachable!("no sockets or signal source requested"),
        }
        if process.ready() {
            let process = self.processes.remove(&process_owner).expect("ready process");
            to_domain.push(process.answer(process_owner));
        }
    }

    /// Advance expired child deadlines, first terminating then killing.
    pub fn fire(&mut self, env: &Env<Limits>, _to_domain: &mut Queue<ToDomain>, below: &mut Queue<Below>) {
        let mut owners = List::with_capacity(self.processes.capacity());
        for (owner, _) in &self.processes {
            owners.push(*owner).expect("one key per process");
        }
        for owner in &owners {
            let process = self.processes.get_mut(owner).expect("retained process");
            if process.due(env.now) {
                process.stop(env.now, env.limits.stop_grace, false, below);
            } else if process.kill_due(env.now) {
                process.kill(below);
            }
        }
    }

    /// First armed process deadline; file deadlines belong to `FileIo`.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        let mut first = None;
        for (_, process) in &self.processes {
            if let Some(deadline) = process.next_deadline() {
                first = Some(match first {
                    Some(before) if before < deadline => before,
                    Some(_) | None => deadline,
                });
            }
        }
        first
    }
}

fn other() -> tools::Done {
    tools::Done::Failed { fault: tools::Fault::Other }
}

fn failed_check(owner: Token, to_domain: &mut Queue<ToDomain>) {
    to_domain
        .push(ToDomain::Checked { owner, ran: run::Ran { exit: run::Exit::Unstarted, output: Box::new([]), cut: 0 } });
}
