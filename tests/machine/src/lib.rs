//! The machine application world (protocol/agent.md, sections 2 and 8).
//! It drives the component, both skein io faces, simulator and fake machine
//! through their public records; the fake workspace owns the file contents.

use std::collections::BTreeSet;

use skein_fake_machine::{How, Item, Machine, Opened, step as machine_step};
use skein_io::file;
use skein_io::file_layer::{self, FileIo};
use skein_io::kernel::{Complete, Fd, Submit};
use skein_io::{self as io, Io};
use skein_lib::stream::Up;
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_sim::{Answer, Ask, Config, Handle, Pid, Program, Reply, Sim};
use smith_domain::{run, tools};
use smith_protocol_machine::{Below, BelowEvent, Component, FromDomain, Limits, ToDomain};

fn machine_limits() -> Limits {
    Limits {
        operations: 8,
        roots: 1,
        path_bytes: 128,
        file_bytes: 128,
        entries: 16,
        entry_bytes: 2048,
        processes: 2,
        output_bytes: 32,
        search_hits: 8,
        search_bytes: 128,
        search_line_bytes: 512,
        env_bytes: 256,
        stop_grace: Duration::from_millis(1),
    }
}

fn io_limits() -> io::Limits {
    io::Limits {
        sockets: 8,
        refusals: 4,
        intake: 32,
        receive: 16,
        output: 32,
        sends: 2,
        accepts: 1,
        backlog: 2,
        close_timeout: Duration::from_secs(1),
        retry: Duration::from_millis(1),
    }
}

/// One deterministic application loop and its authoritative workspace.
pub struct World {
    pub machine: Machine,
    sim: Sim,
    pid: Pid,
    file_io: FileIo,
    process_io: Io,
    component: Component,
    root: Token,
    root_fd: Fd,
    root_handle: Opened,
    machine_env: Env<Limits>,
    process_env: Env<io::Limits>,
    below: Queue<Below>,
    to_domain: Queue<ToDomain>,
    file_events: Queue<file::Event>,
    process_events: Queue<io::Event>,
    file_subs: Queue<Submit>,
    process_subs: Queue<Submit>,
    completions: Queue<Complete>,
    file_flights: BTreeSet<Token>,
    terminals: Vec<ToDomain>,
}

impl World {
    /// Lay one workspace and make both IO layers with their fixed limits.
    #[must_use]
    pub fn new(seed: u64, items: &[Item]) -> World {
        Self::with_grant(seed, items, true)
    }

    /// Lay a read-only workspace, for the write-authority referee.
    #[must_use]
    pub fn read_only(seed: u64, items: &[Item]) -> World {
        Self::with_grant(seed, items, false)
    }

    fn with_grant(seed: u64, items: &[Item], writable: bool) -> World {
        let mut machine = Machine::new();
        let handle = machine.lay(items);
        let mut sim = Sim::new(seed, Config::calm());
        let pid = sim.spawn_process();
        let root_fd = sim.root(pid, Handle::new(handle.raw()));
        let limits = machine_limits();
        let mut file_io = FileIo::with_whole_limit(16, 128, 32, 128, Duration::from_secs(1));
        file_io.seed_randomness(seed);
        let root = file_io.adopt_root(root_fd).expect("one root slot");
        let mut component = Component::new(&limits);
        component.workspace(&run::Workspace {
            directories: Box::new([run::Directory {
                name: Box::from(&b"repo"[..]),
                root,
                writable,
                git: true,
                conflicts: Box::new([]),
            }]),
        });
        World {
            machine,
            sim,
            pid,
            file_io,
            process_io: Io::new(&io_limits()),
            component,
            root,
            root_fd,
            root_handle: handle,
            machine_env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            process_env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: io_limits() },
            below: Queue::with_capacity(64),
            to_domain: Queue::with_capacity(64),
            file_events: Queue::with_capacity(64),
            process_events: Queue::with_capacity(64),
            file_subs: Queue::with_capacity(64),
            process_subs: Queue::with_capacity(64),
            completions: Queue::with_capacity(64),
            file_flights: BTreeSet::new(),
            terminals: Vec::new(),
        }
    }

    /// The root as the domain names it.
    #[must_use]
    pub fn root(&self) -> Token {
        self.root
    }

    /// The backing descriptor adopted by the file layer.
    #[must_use]
    pub fn root_fd(&self) -> Fd {
        self.root_fd
    }

    /// An external writer changes a file after a session observed its version.
    pub fn change_behind_session(&mut self, path: &[u8], bytes: &[u8]) {
        let opened = self.machine.open(self.root_handle, path, How::Read).expect("external file exists");
        self.machine.write(opened, 0, bytes).expect("external writer can change the file");
        self.machine.close(opened);
    }

    /// Read fake workspace bytes for the tools' write referee.
    #[must_use]
    pub fn content(&mut self, path: &[u8]) -> Vec<u8> {
        let opened = self.machine.open(self.root_handle, path, How::Read).expect("file exists");
        let bytes = self.machine.read(opened, 0, 4096).expect("file readable");
        self.machine.close(opened);
        bytes
    }

    /// Submit one domain request, then drive it until its terminal arrives.
    pub fn request(&mut self, request: FromDomain) -> ToDomain {
        self.component.from_domain(&self.machine_env, request, &mut self.to_domain, &mut self.below);
        self.drive();
        assert_eq!(self.terminals.len(), 1, "one domain terminal");
        self.terminals.pop().expect("the terminal")
    }

    /// Script one flood at the component's child-event boundary. Skein's
    /// simulator has only echo/exit/never programs; this fixture supplies
    /// output bytes while the other stories exercise actual simulated IO.
    pub fn flood(&mut self, owner: Token, bytes: &[u8], head: u32, tail: u32) -> ToDomain {
        let deadline = Time::from_nanos(100_000_000);
        self.component.from_domain(
            &self.machine_env,
            FromDomain::Op {
                owner,
                op: tools::Op::Spawn {
                    cwd: tools::Place { root: self.root, path: Box::new([]) },
                    command: Box::from(&b"flood"[..]),
                    env: Box::new([]),
                    roots: Box::new([]),
                    head,
                    tail,
                },
                deadline,
            },
            &mut self.to_domain,
            &mut self.below,
        );
        match self.below.pop() {
            Some(Below::Spawn { .. }) => {}
            other => panic!("expected a flood spawn: {other:?}"),
        }
        let stdout = Token::new(8001);
        let stderr = Token::new(8002);
        self.component.from_below(
            &self.machine_env,
            BelowEvent::Process(io::Event::Spawned {
                owner,
                child: Token::new(8000),
                pipes: Box::new([stdout, stderr]),
            }),
            &mut self.to_domain,
            &mut self.below,
        );
        assert!(self.below.pop().is_some() && self.below.pop().is_some(), "both pipe demands");
        for byte in bytes {
            self.component.from_below(
                &self.machine_env,
                BelowEvent::Process(io::Event::Stream { owner: stdout, up: Up::Bytes(Box::new([*byte])) }),
                &mut self.to_domain,
                &mut self.below,
            );
            assert!(self.below.pop().is_some(), "demand after each byte");
        }
        self.component.from_below(
            &self.machine_env,
            BelowEvent::Process(io::Event::Exited { owner, exit: io::kernel::Exit::Code(0) }),
            &mut self.to_domain,
            &mut self.below,
        );
        for pipe in [stdout, stderr] {
            self.component.from_below(
                &self.machine_env,
                BelowEvent::Process(io::Event::Closed { owner: pipe }),
                &mut self.to_domain,
                &mut self.below,
            );
        }
        assert!(self.to_domain.is_empty(), "child closure is still owed");
        self.component.from_below(
            &self.machine_env,
            BelowEvent::Process(io::Event::Closed { owner }),
            &mut self.to_domain,
            &mut self.below,
        );
        let terminal = self.to_domain.pop().expect("one flood terminal");
        assert!(self.to_domain.is_empty());
        terminal
    }

    /// Advance one IO pass and all fake-machine calls.
    #[expect(clippy::too_many_lines, reason = "the application loop keeps ordered IO stages in one place")]
    pub fn step(&mut self) -> bool {
        self.machine_env.now = self.sim.now();
        self.process_env.now = self.sim.now();
        let mut progress = false;
        if self.file_io.is_due(self.sim.now()) {
            file_layer::expire(&mut self.file_io, self.sim.now(), &mut self.file_subs);
            progress = true;
        }
        if self.process_io.is_due(self.sim.now()) {
            io::fire(&mut self.process_io, &self.process_env, &mut self.process_events, &mut self.process_subs);
            progress = true;
        }
        if self.component.next_deadline().is_some_and(|at| at <= self.sim.now()) {
            self.component.fire(&self.machine_env, &mut self.to_domain, &mut self.below);
            progress = true;
        }
        while self.process_io.is_ready() {
            io::resume(&mut self.process_io, &self.process_env, &mut self.process_events, &mut self.process_subs);
            progress = true;
        }
        while let Some(request) = self.below.pop() {
            match request {
                Below::File { request, deadline } => {
                    assert!(self.file_io.takes(), "serial file layer");
                    file_layer::down_until(
                        &mut self.file_io,
                        deadline,
                        request,
                        &mut self.file_events,
                        &mut self.file_subs,
                    );
                }
                Below::CancelFile { owner } => file_layer::cancel(&mut self.file_io, owner, &mut self.file_subs),
                Below::OpenRead { owner, root, path, deadline } => {
                    let descriptor = self.file_io.descriptor(root).expect("adopted root");
                    file_layer::down_until(
                        &mut self.file_io,
                        deadline,
                        file::Request::OpenRead { owner, root: descriptor, name: path },
                        &mut self.file_events,
                        &mut self.file_subs,
                    );
                }
                Below::Spawn { owner, root, spawn } => {
                    let descriptor = self.file_io.descriptor(root).expect("adopted root");
                    io::down(
                        &mut self.process_io,
                        &self.process_env,
                        io::Request::Spawn {
                            owner,
                            spawn: io::kernel::Spawn {
                                program: spawn.program,
                                args: spawn.args,
                                env: spawn.env,
                                root: descriptor,
                                dir: spawn.dir,
                                pipes: spawn.pipes,
                            },
                        },
                        &mut self.process_subs,
                    );
                }
                Below::Process(request) => {
                    io::down(&mut self.process_io, &self.process_env, request, &mut self.process_subs);
                }
            }
            progress = true;
        }
        let mut submissions = Queue::with_capacity(128);
        while let Some(submit) = self.file_subs.pop() {
            assert!(self.file_flights.insert(submit.op), "fresh file operation");
            submissions.push(submit);
        }
        while let Some(submit) = self.process_subs.pop() {
            submissions.push(submit);
        }
        if !submissions.is_empty() {
            self.sim.submit(self.pid, &mut submissions);
            self.serve();
            progress = true;
        }
        self.sim.reap(self.pid, &mut self.completions);
        while let Some(complete) = self.completions.pop() {
            if self.file_flights.remove(&complete.op) {
                file_layer::up(&mut self.file_io, complete, &mut self.file_events, &mut self.file_subs);
            } else {
                io::up(
                    &mut self.process_io,
                    &self.process_env,
                    complete,
                    &mut self.process_events,
                    &mut self.process_subs,
                );
            }
            progress = true;
        }
        while let Some(event) = self.file_events.pop() {
            self.component.from_below(&self.machine_env, BelowEvent::File(event), &mut self.to_domain, &mut self.below);
            progress = true;
        }
        while let Some(event) = self.process_events.pop() {
            self.component.from_below(
                &self.machine_env,
                BelowEvent::Process(event),
                &mut self.to_domain,
                &mut self.below,
            );
            progress = true;
        }
        while let Some(event) = self.to_domain.pop() {
            self.terminals.push(event);
            progress = true;
        }
        self.process_io.reclaim();
        progress
    }

    fn serve(&mut self) {
        let mut calls = Queue::with_capacity(64);
        let mut answers = Queue::with_capacity(64);
        for _ in 0..1000 {
            self.sim.calls(&mut calls);
            if calls.is_empty() {
                return;
            }
            while let Some(call) = calls.pop() {
                match &call.ask {
                    Ask::Spawn { program, args, .. } if program.as_ref() == b"/bin/sh" => {
                        let command = args.get(1).expect("shell -c command");
                        let fixture = if command.as_ref() == b"hang" {
                            Program::Never
                        } else if command.as_ref() == b"fail" {
                            Program::Exit(1)
                        } else {
                            Program::Exit(0)
                        };
                        answers.push(Answer { ticket: call.ticket, result: Ok(Reply::Program(fixture)) });
                    }
                    Ask::Spawn { program, .. } if program.as_ref() == b"/usr/bin/rg" => {
                        answers.push(Answer { ticket: call.ticket, result: Ok(Reply::Program(Program::Exit(1))) });
                    }
                    Ask::Spawn { program, .. } if program.as_ref() == b"check-ok" => {
                        answers.push(Answer { ticket: call.ticket, result: Ok(Reply::Program(Program::Exit(0))) });
                    }
                    Ask::Spawn { program, .. } if program.as_ref() == b"check-fail" => {
                        answers.push(Answer { ticket: call.ticket, result: Ok(Reply::Program(Program::Exit(1))) });
                    }
                    Ask::Spawn { program, .. } if program.as_ref() == b"check-hang" => {
                        answers.push(Answer { ticket: call.ticket, result: Ok(Reply::Program(Program::Never)) });
                    }
                    Ask::Spawn { .. }
                    | Ask::Open { .. }
                    | Ask::Read { .. }
                    | Ask::Write { .. }
                    | Ask::Sync { .. }
                    | Ask::Stat { .. }
                    | Ask::Rename { .. }
                    | Ask::Remove { .. }
                    | Ask::MakeDirectory { .. }
                    | Ask::List { .. }
                    | Ask::Close { .. } => {
                        machine_step(&mut self.machine, call, &mut answers);
                    }
                }
            }
            self.sim.answer(&mut answers);
        }
        panic!("fake machine did not settle its calls");
    }
    /// Run bounded passes, advancing simulated time for a pending timer.
    pub fn drive(&mut self) {
        for _ in 0..20_000 {
            if !self.terminals.is_empty() {
                return;
            }
            if self.step() {
                continue;
            }
            let next = [
                self.sim.next_due(),
                self.file_io.next_deadline(),
                self.process_io.next_deadline(),
                self.component.next_deadline(),
            ]
            .into_iter()
            .flatten()
            .min();
            match next {
                Some(at) if at > self.sim.now() => self.sim.advance_to(at),
                Some(_) => panic!("a due timer made no progress"),
                None => panic!("machine world is stuck without a terminal"),
            }
        }
        panic!("machine world exceeded its step bound");
    }

    /// Quiescence and exactly once are checked after each request by `request`.
    pub fn settle(&mut self) {
        for _ in 0..1000 {
            if !self.step() {
                break;
            }
        }
        assert!(self.terminals.is_empty());
        assert!(self.file_flights.is_empty());
        assert_eq!(self.file_io.open_files(), 1, "only the root remains");
        assert!(self.process_io.is_empty());
        self.sim.assert_quiescent(self.pid);
    }
}
