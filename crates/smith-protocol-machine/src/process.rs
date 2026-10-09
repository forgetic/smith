//! Plain child execution until skein supplies contained trees
//! (protocol/agent.md, sections 2 and 3; domain/tools.md, section 5).
//! A call is answered only after exit, both pipe closures and child closure.

use alloc::boxed::Box;

use skein_io::{self as io, kernel};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Duration, List, Map, Queue, Time, Token, Writer};
use smith_domain::{run, tools};

use crate::boundary::{Below, Spawn, ToDomain};
use crate::limits::Limits;
use crate::search::Search;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    Shell,
    Search,
    Check,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    None,
    Terminating { kill_at: Time, cancelled: bool },
    Killing { cancelled: bool },
}

/// Bounded first and last bytes of a process's combined output.
#[derive(Debug)]
pub(crate) struct Capture {
    head: List<u8>,
    tail: List<u8>,
    next: u32,
    total: u64,
}

impl Capture {
    pub(crate) fn new(head: u32, tail: u32) -> Capture {
        Capture { head: List::with_capacity(head), tail: List::with_capacity(tail), next: 0, total: 0 }
    }

    pub(crate) fn push(&mut self, byte: u8) {
        self.total = self.total.saturating_add(1);
        if self.head.room() > 0 {
            self.head.push(byte).expect("head has room");
        } else if self.tail.capacity() > 0 {
            if self.tail.room() > 0 {
                self.tail.push(byte).expect("tail has room");
            } else {
                *self.tail.get_mut(self.next).expect("ring slot exists") = byte;
                self.next = self
                    .next
                    .checked_add(1)
                    .expect("ring index fits")
                    .checked_rem(self.tail.capacity())
                    .expect("tail capacity positive");
            }
        }
    }

    pub(crate) fn finish(self) -> (Box<[u8]>, Box<[u8]>, u64) {
        let dropped = self
            .total
            .checked_sub(u64::from(self.head.len()))
            .expect("head is within total")
            .checked_sub(u64::from(self.tail.len()))
            .expect("tail is within total");
        let mut tail = List::with_capacity(self.tail.len());
        for at in 0..self.tail.len() {
            let index = if self.tail.len() == self.tail.capacity() {
                self.next
                    .checked_add(at)
                    .expect("ring index fits")
                    .checked_rem(self.tail.len())
                    .expect("tail length positive")
            } else {
                at
            };
            tail.push(*self.tail.get(index).expect("ring index valid")).expect("tail room");
        }
        (self.head.into_boxed(), tail.into_boxed(), dropped)
    }
}

/// One child and its output pipes, retained until io proves them closed.
#[derive(Debug)]
pub(crate) struct Process {
    pub(crate) purpose: Purpose,
    pub(crate) deadline: Time,
    pub(crate) child: Option<Token>,
    pub(crate) pipes: [Option<Token>; 2],
    pipe_closed: [bool; 2],
    pub(crate) child_closed: bool,
    exit: Option<kernel::Exit>,
    failed: bool,
    stop: Stop,
    output: Capture,
    search: Option<Search>,
}

impl Process {
    pub(crate) fn new(purpose: Purpose, deadline: Time, head: u32, tail: u32, search: Option<Search>) -> Process {
        Process {
            purpose,
            deadline,
            child: None,
            pipes: [None, None],
            pipe_closed: [false, false],
            child_closed: false,
            exit: None,
            failed: false,
            stop: Stop::None,
            output: Capture::new(head, tail),
            search,
        }
    }

    #[expect(clippy::boxed_local, reason = "io transfers the owned pipe list")]
    pub(crate) fn spawned(
        &mut self,
        child: Token,
        pipes: Box<[Token]>,
        routes: &mut Map<Token, Token>,
        owner: Token,
        below: &mut Queue<Below>,
    ) {
        assert!(pipes.len() == 2, "stdout and stderr pipes requested");
        self.child = Some(child);
        for (index, pipe) in pipes.iter().enumerate() {
            *self.pipes.get_mut(index).expect("two pipe slots") = Some(*pipe);
            assert!(routes.insert(*pipe, owner).is_ok(), "reserved two routes per child");
            below.push(Below::Process(io::Request::Stream {
                stream: *pipe,
                down: Down::Demand { read: Read::Fill(1), room: 0 },
            }));
        }
        match self.stop {
            Stop::None => {}
            Stop::Terminating { .. } => {
                below.push(Below::Process(io::Request::Signal {
                    child,
                    to: kernel::Target::Child,
                    signal: kernel::Signal::Terminate,
                }));
            }
            Stop::Killing { .. } => {
                below.push(Below::Process(io::Request::Signal {
                    child,
                    to: kernel::Target::Child,
                    signal: kernel::Signal::Kill,
                }));
            }
        }
    }

    pub(crate) fn byte(&mut self, pipe: Token, byte: u8) {
        let index = usize::from(self.pipes.first().copied().expect("pipe slot") != Some(pipe));
        assert_eq!(self.pipes.get(index).copied().expect("pipe slot"), Some(pipe), "byte belongs to child");
        match self.purpose {
            Purpose::Search if index == 0 => self.search.as_mut().expect("search state").push(byte),
            Purpose::Shell | Purpose::Check | Purpose::Search => self.output.push(byte),
        }
    }

    pub(crate) fn pipe_closed(&mut self, pipe: Token) {
        let index = usize::from(self.pipes.first().copied().expect("pipe slot") != Some(pipe));
        assert_eq!(*self.pipes.get(index).expect("pipe slot"), Some(pipe), "closed pipe belongs to child");
        *self.pipe_closed.get_mut(index).expect("pipe slot") = true;
    }

    pub(crate) fn failed(&mut self) {
        self.failed = true;
    }

    pub(crate) fn exited(&mut self, exit: kernel::Exit) {
        self.exit = Some(exit);
    }

    pub(crate) fn ready(&self) -> bool {
        self.child_closed
            && (self.child.is_none()
                || (*self.pipe_closed.first().expect("pipe slot") && *self.pipe_closed.get(1).expect("pipe slot")))
    }

    pub(crate) fn stop(&mut self, now: Time, grace: Duration, cancelled: bool, below: &mut Queue<Below>) {
        match self.stop {
            Stop::None => {
                if self.exit.is_some() || self.child_closed {
                    return;
                }
                self.stop = Stop::Terminating { kill_at: now.saturating_add(grace), cancelled };
                if let Some(child) = self.child {
                    below.push(Below::Process(io::Request::Signal {
                        child,
                        to: kernel::Target::Child,
                        signal: kernel::Signal::Terminate,
                    }));
                }
            }
            Stop::Terminating { .. } | Stop::Killing { .. } => {}
        }
    }

    pub(crate) fn kill_due(&self, now: Time) -> bool {
        match self.stop {
            Stop::Terminating { kill_at, .. } => now >= kill_at && self.exit.is_none(),
            Stop::None | Stop::Killing { .. } => false,
        }
    }

    pub(crate) fn kill(&mut self, below: &mut Queue<Below>) {
        match self.stop {
            Stop::Terminating { cancelled, .. } => {
                self.stop = Stop::Killing { cancelled };
                if let Some(child) = self.child {
                    below.push(Below::Process(io::Request::Signal {
                        child,
                        to: kernel::Target::Child,
                        signal: kernel::Signal::Kill,
                    }));
                }
            }
            Stop::None | Stop::Killing { .. } => {}
        }
    }

    pub(crate) fn due(&self, now: Time) -> bool {
        self.exit.is_none() && !self.child_closed && now >= self.deadline && self.stop == Stop::None
    }

    pub(crate) fn next_deadline(&self) -> Option<Time> {
        if self.exit.is_some() || self.child_closed {
            return None;
        }
        match self.stop {
            Stop::None => Some(self.deadline),
            Stop::Terminating { kill_at, .. } => Some(kill_at),
            Stop::Killing { .. } => None,
        }
    }

    pub(crate) fn answer(self, owner: Token) -> ToDomain {
        let timed_out = match self.stop {
            Stop::Terminating { cancelled: false, .. } | Stop::Killing { cancelled: false } => {
                matches_signal(self.exit)
            }
            Stop::None | Stop::Terminating { cancelled: true, .. } | Stop::Killing { cancelled: true } => false,
        };
        let cancelled = match self.stop {
            Stop::Terminating { cancelled: true, .. } | Stop::Killing { cancelled: true } => matches_signal(self.exit),
            Stop::None | Stop::Terminating { cancelled: false, .. } | Stop::Killing { cancelled: false } => false,
        };
        let (head, tail, dropped) = self.output.finish();
        match self.purpose {
            Purpose::Shell => {
                let done = if cancelled {
                    tools::Done::Cancelled
                } else if self.failed {
                    tools::Done::Failed { fault: tools::Fault::Other }
                } else {
                    let exit = tool_exit(self.exit, timed_out);
                    tools::Done::Exited { exit, head, tail, dropped }
                };
                ToDomain::Done { owner, done }
            }
            Purpose::Search => {
                let done = if cancelled {
                    tools::Done::Cancelled
                } else if self.failed {
                    tools::Done::Failed { fault: tools::Fault::Other }
                } else {
                    self.search.expect("search state").finish(self.exit, timed_out, head, tail, dropped)
                };
                ToDomain::Done { owner, done }
            }
            Purpose::Check => {
                if cancelled {
                    ToDomain::Aborted { owner }
                } else {
                    let output = tail;
                    let cut = dropped
                        .checked_add(u64::try_from(head.len()).expect("head length fits"))
                        .expect("output count fits");
                    let exit = run_exit(self.exit, timed_out, self.failed);
                    ToDomain::Checked { owner, ran: run::Ran { exit, output, cut } }
                }
            }
        }
    }
}

fn matches_signal(exit: Option<kernel::Exit>) -> bool {
    match exit {
        Some(kernel::Exit::Signal(_)) => true,
        Some(kernel::Exit::Code(_)) | None => false,
    }
}

fn tool_exit(exit: Option<kernel::Exit>, timed_out: bool) -> tools::Exit {
    if timed_out {
        return tools::Exit::TimedOut;
    }
    match exit {
        Some(kernel::Exit::Code(code)) => tools::Exit::Code { code },
        Some(kernel::Exit::Signal(signal)) => tools::Exit::Signal { signal: u8::try_from(signal).unwrap_or(u8::MAX) },
        None => tools::Exit::Signal { signal: 0 },
    }
}

fn run_exit(exit: Option<kernel::Exit>, timed_out: bool, failed: bool) -> run::Exit {
    if timed_out {
        return run::Exit::TimedOut;
    }
    if failed {
        return run::Exit::Unstarted;
    }
    match exit {
        Some(kernel::Exit::Code(code)) => run::Exit::Code { code },
        Some(kernel::Exit::Signal(_)) => run::Exit::Signalled,
        None => run::Exit::Unstarted,
    }
}

fn pipes() -> Box<[kernel::Pipe]> {
    Box::new([
        kernel::Pipe { child: 1, way: kernel::Way::Out, parent: None },
        kernel::Pipe { child: 2, way: kernel::Way::Out, parent: None },
    ])
}

fn environment(vars: &[tools::Var], max_bytes: u32) -> Option<Box<[Box<[u8]>]>> {
    let mut used = 0_usize;
    let mut entries = List::with_capacity(u32::try_from(vars.len()).ok()?);
    for var in vars {
        if var.name.is_empty() || var.name.contains(&b'=') || var.name.contains(&0) || var.value.contains(&0) {
            return None;
        }
        let len = var.name.len().checked_add(1)?.checked_add(var.value.len())?;
        used = used.checked_add(len)?;
        if used > usize::try_from(max_bytes).ok()? {
            return None;
        }
        let mut writer = Writer::new(len);
        writer.put(&var.name).ok()?;
        writer.put(b"=").ok()?;
        writer.put(&var.value).ok()?;
        entries.push(writer.finish()).ok()?;
    }
    Some(entries.into_boxed())
}

#[expect(clippy::too_many_arguments, reason = "owned request fields cross the machine boundary")]
pub(crate) fn shell(
    owner: Token,
    cwd: tools::Place,
    command: Box<[u8]>,
    vars: Box<[tools::Var]>,
    head: u32,
    tail: u32,
    deadline: Time,
    limits: &Limits,
) -> Option<(Process, Below)> {
    if head > limits.output_bytes || tail > limits.output_bytes || command.contains(&0) {
        return None;
    }
    let env = environment(&vars, limits.env_bytes)?;
    let args = Box::new([Box::from(&b"-c"[..]), command]);
    let dir = crate::files::io_path(cwd.path);
    let spawn = Spawn { program: Box::from(&b"/bin/sh"[..]), args, env, dir, pipes: pipes() };
    Some((Process::new(Purpose::Shell, deadline, head, tail, None), Below::Spawn { owner, root: cwd.root, spawn }))
}

#[expect(clippy::too_many_arguments, clippy::single_match, reason = "strict subset uses exhaustive option matches")]
pub(crate) fn search(
    owner: Token,
    at: tools::Place,
    pattern: Box<[u8]>,
    glob: Option<Box<[u8]>>,
    hits: u32,
    bytes: u32,
    deadline: Time,
    limits: &Limits,
) -> Option<(Process, Below)> {
    if hits > limits.search_hits || bytes > limits.search_bytes || pattern.contains(&0) {
        return None;
    }
    match &glob {
        Some(filter) if filter.contains(&0) => return None,
        Some(_) | None => {}
    }
    let count = if glob.is_some() { 6 } else { 5 };
    let mut args = List::with_capacity(count);
    args.push(Box::from(&b"--no-config"[..])).ok()?;
    args.push(Box::from(&b"--json"[..])).ok()?;
    let mut expression = Writer::new(b"--regexp=".len().checked_add(pattern.len())?);
    expression.put(b"--regexp=").ok()?;
    expression.put(&pattern).ok()?;
    args.push(expression.finish()).ok()?;
    match glob {
        Some(glob) => {
            let mut filter = Writer::new(b"--glob=".len().checked_add(glob.len())?);
            filter.put(b"--glob=").ok()?;
            filter.put(&glob).ok()?;
            args.push(filter.finish()).ok()?;
        }
        None => {}
    }
    args.push(Box::from(&b"--"[..])).ok()?;
    args.push(crate::files::io_path(at.path)).ok()?;
    let spawn = Spawn {
        program: Box::from(&b"/usr/bin/rg"[..]),
        args: args.into_boxed(),
        env: Box::new([]),
        dir: crate::files::io_path(Box::new([])),
        pipes: pipes(),
    };
    let parser = Search::new(hits, bytes, limits.search_line_bytes);
    Some((
        Process::new(Purpose::Search, deadline, bytes, bytes, Some(parser)),
        Below::Spawn { owner, root: at.root, spawn },
    ))
}

pub(crate) fn check(
    owner: Token,
    program: run::Place,
    vars: &[tools::Var],
    tail: u32,
    deadline: Time,
    limits: &Limits,
) -> Option<(Process, Below)> {
    if tail > limits.output_bytes || program.path.is_empty() || program.path.contains(&0) {
        return None;
    }
    let env = environment(vars, limits.env_bytes)?;
    let spawn = Spawn {
        program: program.path,
        args: Box::new([]),
        env,
        dir: crate::files::io_path(Box::new([])),
        pipes: pipes(),
    };
    Some((Process::new(Purpose::Check, deadline, 0, tail, None), Below::Spawn { owner, root: program.root, spawn }))
}

pub(crate) fn stream(process: &mut Process, pipe: Token, up: Up, below: &mut Queue<Below>) {
    match up {
        Up::Bytes(bytes) => {
            for byte in bytes {
                process.byte(pipe, byte);
            }
            below.push(Below::Process(io::Request::Stream {
                stream: pipe,
                down: Down::Demand { read: Read::Fill(1), room: 0 },
            }));
        }
        Up::End => below.push(Below::Process(io::Request::Close { entity: pipe })),
        Up::Failed(_) => {
            process.failed();
            below.push(Below::Process(io::Request::Abort { entity: pipe }));
        }
        Up::Room => unreachable!("read-only output pipes grant no write room"),
    }
}
