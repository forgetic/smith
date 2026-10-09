//! One spawned agent's IO, pipe channel and host-domain terminals
//! (protocol/hosts.md, sections 3 and 5.6). The adapter keeps IO tokens,
//! process resources, channel buffers, and bounded plain-tree snapshots.
//! File demands settle serially before markers or a snapshot become a terminal.
//! It never knows local delivery
//! policy, a credential's meaning, or the agent's private domain state.
//! `up` converts kernel completions to host observations; `down` translates
//! host effects and submits IO. Gone remains the host domain's decision after
//! the process adapter has reported child exit, pipe EOF and reap.

use alloc::boxed::Box;
use skein_io::{self as io, file, file_layer, kernel};
use skein_lib::{Env, List, Map, Queue, Time, Token, Wall, stream};
use smith_host_domain as host;
use smith_host_protocol as protocol;
use smith_local_domain as local;
use smith_local_protocol as local_protocol;

use crate::StartValues;

/// Reusable child executable and credential-free environment.
#[derive(Clone, Debug)]
pub struct Launch {
    pub program: Box<[u8]>,
    pub arguments: Box<[Box<[u8]>]>,
    pub environment: Box<[Box<[u8]>]>,
    pub root: kernel::Fd,
    pub directory: Box<[u8]>,
}

impl Launch {
    fn for_spawn(&self) -> protocol::Launch {
        protocol::Launch {
            program: self.program.clone(),
            arguments: self.arguments.clone(),
            environment: self.environment.clone(),
            root: self.root,
            directory: self.directory.clone(),
        }
    }
}

/// Fixed lower capacities for one agent child and its channel.
#[derive(Clone, Copy, Debug)]
pub struct ProcessLimits {
    pub io: io::Limits,
    pub channel: protocol::Limits,
    pub detail_bytes: u32,
    pub queue: u32,
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Io(Token),
    File(Token),
}

#[derive(Debug)]
enum PlainPurpose {
    Capture { directory: u32 },
    Compare { owner: Token, directory: u32 },
}

#[derive(Debug)]
struct PlainScan {
    purpose: PlainPurpose,
    scan: local_protocol::Plain,
}

#[derive(Debug)]
pub(crate) struct ProcessAdapter {
    limits: ProcessLimits,
    launch: Launch,
    io: io::Io,
    process: Option<protocol::Process>,
    git: Option<local_protocol::GitChild>,
    markers: Option<(Token, local_protocol::Markers)>,
    plain: Option<PlainScan>,
    snapshots: Map<u32, local_protocol::Snapshot>,
    capture_directories: List<u32>,
    capture_next: u32,
    capture_deadline: Time,
    capture_done: Option<bool>,
    files: file_layer::FileIo,
    file_roots: Box<[Token]>,
    file_requests: Queue<(file::Request, Time)>,
    file_events: Queue<file::Event>,
    io_submissions: Queue<kernel::Submit>,
    file_submissions: Queue<kernel::Submit>,
    operations: Map<Token, Operation>,
    next_operation: u64,
    delivery_roots: Box<[kernel::Fd]>,
    git_environment: Box<[Box<[u8]>]>,
    io_events: Queue<io::Event>,
    io_requests: Queue<io::Request>,
    process_events: Queue<protocol::ProcessEvent>,
    submissions: Queue<kernel::Submit>,
    completions: Queue<kernel::Complete>,
    send: u64,
    failed: bool,
    force_pending: bool,
    terminal_input: Option<Token>,
    signals: Option<Token>,
    closing: Option<u32>,
    close_root_pending: bool,
}

impl ProcessAdapter {
    pub(crate) fn new(limits: ProcessLimits, launch: Launch) -> Option<Self> {
        if !limits.io.is_usable() || limits.queue < io::MAX_OUT_UP.events.max(io::MAX_OUT_UP.submissions) {
            return None;
        }
        Some(Self {
            io: io::Io::new(&limits.io),
            limits,
            launch,
            process: None,
            git: None,
            markers: None,
            plain: None,
            snapshots: Map::with_capacity(64),
            capture_directories: List::with_capacity(64),
            capture_next: 0,
            capture_deadline: Time::ZERO,
            capture_done: None,
            files: file_layer::FileIo::with_whole_limit(65, 4096, 256, 65_536, skein_lib::Duration::from_secs(30)),
            file_roots: Box::new([]),
            file_requests: Queue::with_capacity(limits.queue),
            file_events: Queue::with_capacity(limits.queue),
            io_submissions: Queue::with_capacity(limits.queue),
            file_submissions: Queue::with_capacity(limits.queue),
            operations: Map::with_capacity(limits.queue),
            next_operation: 1,
            delivery_roots: Box::new([]),
            git_environment: Box::new([]),
            io_events: Queue::with_capacity(limits.queue),
            io_requests: Queue::with_capacity(limits.queue),
            process_events: Queue::with_capacity(limits.queue),
            submissions: Queue::with_capacity(limits.queue),
            completions: Queue::with_capacity(limits.queue),
            send: 1,
            failed: false,
            force_pending: false,
            terminal_input: None,
            signals: None,
            closing: None,
            close_root_pending: false,
        })
    }

    pub(crate) fn worst_case(limits: &ProcessLimits) -> Option<u64> {
        io::worst_case(&limits.io)?
            .checked_add(file_layer::FileIo::worst_case(65, 4096, 256, 65_536)?)?
            .checked_add(local_protocol::plain_worst_case(&plain_limits())?.checked_mul(65)?)?
            .checked_add(Map::<u32, local_protocol::Snapshot>::worst_case(64)?)?
            .checked_add(List::<u32>::worst_case(64)?)?
            .checked_add(local_protocol::markers_worst_case(&git_limits(limits))?)?
            .checked_add(Queue::<(file::Request, Time)>::worst_case(limits.queue)?)?
            .checked_add(Queue::<file::Event>::worst_case(limits.queue)?)?
            .checked_add(Queue::<kernel::Submit>::worst_case(limits.queue)?.checked_mul(2)?)?
            .checked_add(Map::<Token, Operation>::worst_case(limits.queue)?)?
            .checked_add(64_u64.checked_mul(u64::try_from(size_of::<Token>()).ok()?)?)?
            .checked_add(local_protocol::git_child_worst_case(&git_limits(limits))?)?
            .checked_add(4096)?
            .checked_add(List::<Box<[u8]>>::worst_case(64)?)?
            .checked_add(protocol::process_worst_case(&limits.channel, limits.detail_bytes)?)?
            .checked_add(Queue::<io::Event>::worst_case(limits.queue)?)?
            .checked_add(Queue::<io::Request>::worst_case(limits.queue)?)?
            .checked_add(Queue::<protocol::ProcessEvent>::worst_case(limits.queue)?)?
            .checked_add(Queue::<kernel::Submit>::worst_case(limits.queue)?)?
            .checked_add(Queue::<kernel::Complete>::worst_case(limits.queue)?)
    }

    pub(crate) fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }

    pub(crate) fn adopt_terminal(&mut self, input: kernel::Fd, signals: kernel::Fd) -> Result<(), kernel::Fd> {
        let stream = self.io.adopt_read_pipe(input)?;
        self.terminal_input = Some(stream);
        let signal = self.io.adopt_signals(signals)?;
        self.signals = Some(signal);
        self.io_requests
            .push(io::Request::Stream { stream, down: stream::Down::Demand { read: stream::Read::Fill(1), room: 0 } });
        Ok(())
    }

    pub(crate) fn adopt_delivery_roots(&mut self, roots: Box<[kernel::Fd]>, environment: Box<[Box<[u8]>]>) {
        let mut file_roots = List::with_capacity(u32::try_from(roots.len()).expect("admitted root count"));
        for root in &roots {
            file_roots
                .push(self.files.adopt_root(*root).expect("bounded workspace roots"))
                .expect("one token per root");
        }
        self.file_roots = file_roots.into_boxed();
        self.delivery_roots = roots;
        self.git_environment = environment;
    }

    pub(crate) fn close(&mut self) {
        if self.closing.is_some() {
            return;
        }
        self.closing = Some(u32::try_from(self.file_roots.len()).expect("admitted root count"));
        if let Some(entity) = self.terminal_input {
            self.io_requests.push(io::Request::Close { entity });
        }
        if let Some(entity) = self.signals {
            self.io_requests.push(io::Request::Close { entity });
        }
    }

    pub(crate) fn closed(&self) -> bool {
        self.closing.is_some()
            && self.io.is_empty()
            && self.files.takes()
            && self.files.open_files() == 0
            && self.operations.is_empty()
            && self.file_requests.is_empty()
            && self.file_events.is_empty()
            && self.io_requests.is_empty()
            && self.io_events.is_empty()
            && self.io_submissions.is_empty()
            && self.file_submissions.is_empty()
            && self.submissions.is_empty()
            && self.completions.is_empty()
    }

    pub(crate) fn capture_plain(&mut self, directories: List<u32>, deadline: Time) {
        assert!(self.plain.is_none() && self.markers.is_none(), "file inspection is serial");
        self.snapshots = Map::with_capacity(64);
        self.capture_directories = directories;
        self.capture_next = 0;
        self.capture_deadline = deadline;
        self.capture_done = None;
        self.next_capture();
    }

    pub(crate) fn take_capture_done(&mut self) -> Option<bool> {
        self.capture_done.take()
    }

    fn next_capture(&mut self) {
        match self.capture_directories.get(self.capture_next) {
            Some(directory) => {
                let directory = *directory;
                let root = match self.file_roots.get(usize::try_from(directory).expect("bounded directory")) {
                    Some(root) => *root,
                    None => {
                        self.capture_done = Some(false);
                        return;
                    }
                };
                let scan = local_protocol::Plain::new(Token::new(0), root, plain_limits(), self.capture_deadline)
                    .expect("checked plain limits");
                let action = scan.start();
                self.plain = Some(PlainScan { purpose: PlainPurpose::Capture { directory }, scan });
                match action {
                    local_protocol::PlainAction::File { request, deadline } => {
                        self.file_requests.push((request, deadline));
                    }
                    local_protocol::PlainAction::Done | local_protocol::PlainAction::Failed => {
                        unreachable!("root is first scan")
                    }
                }
            }
            None => self.capture_done = Some(true),
        }
    }

    pub(crate) fn plain_status(
        &mut self,
        owner: Token,
        directory: u32,
        deadline: Time,
        events: &mut Queue<local::Event>,
    ) {
        assert!(self.plain.is_none() && self.markers.is_none(), "file inspection is serial");
        let root = match self.file_roots.get(usize::try_from(directory).expect("bounded directory")) {
            Some(root) if self.snapshots.get(&directory).is_some() => *root,
            Some(_) | None => {
                events.push(local::Event::StoreFailed { reason: local::StoreFailure::Read });
                return;
            }
        };
        let scan = local_protocol::Plain::new(owner, root, plain_limits(), deadline).expect("checked plain limits");
        let action = scan.start();
        self.plain = Some(PlainScan { purpose: PlainPurpose::Compare { owner, directory }, scan });
        self.plain_action(action, events);
    }

    fn plain_action(&mut self, action: local_protocol::PlainAction, events: &mut Queue<local::Event>) {
        match action {
            local_protocol::PlainAction::File { request, deadline } => self.file_requests.push((request, deadline)),
            local_protocol::PlainAction::Done => {
                let active = self.plain.take().expect("active plain scan");
                let snapshot = active.scan.finish();
                match active.purpose {
                    PlainPurpose::Capture { directory } => {
                        self.snapshots.insert(directory, snapshot).expect("bounded snapshots");
                        self.capture_next = self.capture_next.checked_add(1).expect("bounded directory position");
                        self.next_capture();
                    }
                    PlainPurpose::Compare { owner, directory } => {
                        let before = self.snapshots.get(&directory).expect("capture precedes compare");
                        events.push(local::Event::PlainStatus { owner, changed: *before != snapshot });
                    }
                }
            }
            local_protocol::PlainAction::Failed => {
                let active = self.plain.take().expect("active plain scan");
                match active.purpose {
                    PlainPurpose::Capture { .. } => self.capture_done = Some(false),
                    PlainPurpose::Compare { .. } => {
                        events.push(local::Event::StoreFailed { reason: local::StoreFailure::Read });
                    }
                }
            }
        }
    }

    pub(crate) fn start_markers(
        &mut self,
        owner: Token,
        directory: u32,
        paths: Box<[Box<[u8]>]>,
        deadline: Time,
        events: &mut Queue<local::Event>,
    ) {
        assert!(self.markers.is_none() && self.plain.is_none(), "one local file inspection at a time");
        let root = self.file_roots.get(usize::try_from(directory).expect("admitted directory"));
        let markers = match root {
            Some(root) => local_protocol::Markers::new(owner, *root, paths, deadline, git_limits(&self.limits)),
            None => None,
        };
        match markers {
            Some(markers) => {
                let action = markers.start();
                self.markers = Some((owner, markers));
                self.marker_action(owner, action, events);
            }
            None => events.push(local::Event::Git {
                owner,
                result: local::GitResult::Failed {
                    reason: smith_domain::run::DeliveryReason::TooLarge,
                    diagnostic: Box::new(smith_domain::run::Diagnostic::empty()),
                },
            }),
        }
    }

    fn marker_action(&mut self, owner: Token, action: local_protocol::MarkerAction, events: &mut Queue<local::Event>) {
        match action {
            local_protocol::MarkerAction::File { request, deadline } => self.file_requests.push((request, deadline)),
            local_protocol::MarkerAction::Done(result) => {
                self.markers = None;
                events.push(local::Event::Git { owner, result });
            }
        }
    }

    pub(crate) fn start_git(
        &mut self,
        owner: Token,
        directory: u32,
        op: smith_local_domain::GitOp,
        deadline: Time,
        events: &mut Queue<smith_local_domain::Event>,
    ) {
        assert!(self.git.is_none(), "the local domain serializes git operations");
        let root = match usize::try_from(directory) {
            Ok(index) => self.delivery_roots.get(index).copied(),
            Err(_) => None,
        };
        self.git = match root {
            Some(root) => local_protocol::GitChild::new(
                owner,
                Token::new(1 << 62),
                root,
                self.git_environment.clone(),
                op,
                deadline,
                git_limits(&self.limits),
                &mut self.io_requests,
            ),
            None => None,
        };
        if self.git.is_none() {
            events.push(smith_local_domain::Event::Git {
                owner,
                result: smith_local_domain::GitResult::Failed {
                    reason: smith_domain::run::DeliveryReason::Broken,
                    diagnostic: Box::new(smith_domain::run::Diagnostic::empty()),
                },
            });
        }
    }

    pub(crate) fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    pub(crate) fn next_deadline(&self) -> Option<Time> {
        let git_deadline = match &self.git {
            Some(git) => git.next_deadline(),
            None => None,
        };
        let process_deadline = match &self.process {
            Some(process) => process.next_deadline(),
            None => None,
        };
        let mut next: Option<Time> = None;
        for deadline in
            [self.io.next_deadline(), process_deadline, git_deadline, self.files.next_deadline()].into_iter().flatten()
        {
            next = Some(match next {
                Some(current) => current.min(deadline),
                None => deadline,
            });
        }
        next
    }

    pub(crate) fn work_pending(&self, now: Time) -> bool {
        self.force_pending
            || self.io.is_ready()
            || self.io.is_due(now)
            || !self.io_events.is_empty()
            || !self.io_requests.is_empty()
            || !self.process_events.is_empty()
            || !self.completions.is_empty()
            || !self.file_events.is_empty()
            || !self.file_requests.is_empty()
            || !self.io_submissions.is_empty()
            || !self.file_submissions.is_empty()
            || self.files.is_due(now)
            || (self.closing.unwrap_or(0) > 0 && self.files.takes() && !self.close_root_pending)
    }

    pub(crate) const fn failed(&self) -> bool {
        self.failed
    }

    pub(crate) fn output_room(&self) -> bool {
        self.io_requests.room() >= 64 && self.process_events.room() >= 64
    }

    pub(crate) fn force_stop(&mut self) {
        self.force_pending = true;
        if self.output_room() {
            self.flush_force();
        }
    }

    fn flush_force(&mut self) {
        if !self.force_pending {
            return;
        }
        if let Some(process) = &self.process
            && process.kill_if_spawned(&mut self.io_requests)
        {
            self.force_pending = false;
        }
    }

    fn reap(&mut self, env: &Env<io::Limits>) {
        for _ in 0..self.completions.capacity() {
            if self.io_events.room() < io::MAX_OUT_UP.events
                || self.io_submissions.room() < io::MAX_OUT_UP.submissions
                || self.file_events.room() == 0
                || self.file_submissions.room() < 2
            {
                break;
            }
            let Some(complete) = self.completions.pop() else { break };
            let route = self.operations.remove(&complete.op).expect("one route per submitted kernel operation");
            match route {
                Operation::Io(op) => io::up(
                    &mut self.io,
                    env,
                    kernel::Complete { op, kind: complete.kind, result: complete.result },
                    &mut self.io_events,
                    &mut self.io_submissions,
                ),
                Operation::File(op) => file_layer::up(
                    &mut self.files,
                    kernel::Complete { op, kind: complete.kind, result: complete.result },
                    &mut self.file_events,
                    &mut self.file_submissions,
                ),
            }
        }
    }

    fn markers_up(&mut self, now: Time, local_events: &mut Queue<local::Event>) {
        if self.files.is_due(now) && self.file_submissions.room() >= 2 {
            file_layer::expire(&mut self.files, now, &mut self.file_submissions);
        }
        for _ in 0..self.file_events.capacity() {
            if local_events.room() == 0 || self.file_requests.room() == 0 {
                break;
            }
            let Some(event) = self.file_events.pop() else { break };
            if self.close_root_pending && event.owner() == Token::new(u64::MAX) {
                self.close_root_pending = false;
                continue;
            }
            match &mut self.plain {
                Some(active) => {
                    let action = active.scan.from_file(event);
                    self.plain_action(action, local_events);
                }
                None => {
                    let (owner, markers) = self.markers.as_mut().expect("file terminal belongs to marker inspection");
                    let owner = *owner;
                    let action = markers.from_file(event);
                    self.marker_action(owner, action, local_events);
                }
            }
        }
    }

    pub(crate) fn up(
        &mut self,
        now: Time,
        wall: Wall,
        host_events: &mut Queue<host::Event>,
        terminal: &mut local_protocol::Terminal,
        terminal_events: &mut Queue<local_protocol::TerminalEvent>,
        local_events: &mut Queue<smith_local_domain::Event>,
    ) {
        let env = Env { now, wall, limits: self.limits.io };
        self.reap(&env);
        for _ in 0..self.limits.io.sockets {
            if self.io_events.room() < io::MAX_OUT_UP.events || self.io_submissions.room() < io::MAX_OUT_UP.submissions
            {
                break;
            }
            if self.io.is_ready() {
                io::resume(&mut self.io, &env, &mut self.io_events, &mut self.io_submissions);
            }
            if self.io.is_due(now)
                && self.io_events.room() >= io::MAX_OUT_UP.events
                && self.io_submissions.room() >= io::MAX_OUT_UP.submissions
            {
                io::fire(&mut self.io, &env, &mut self.io_events, &mut self.io_submissions);
            }
        }
        self.markers_up(now, local_events);
        for _ in 0..self.io_events.capacity() {
            if !self.output_room()
                || terminal_events.room() < local_protocol::terminal_max_out()
                || local_events.room() == 0
            {
                break;
            }
            let Some(event) = self.io_events.pop() else { break };
            let git_owned = match &self.git {
                Some(git) => match io_owner(&event) {
                    Some(owner) => git.owns(owner),
                    None => false,
                },
                None => false,
            };
            if git_owned {
                self.git.as_mut().expect("git route exists").from_io(event, local_events, &mut self.io_requests);
                continue;
            }
            match event {
                io::Event::Shutdown { .. } => terminal.interrupt(terminal_events),
                io::Event::Stream { owner, up } if Some(owner) == self.terminal_input => match up {
                    stream::Up::Bytes(bytes) => {
                        for byte in bytes {
                            terminal.feed(byte, terminal_events);
                        }
                        if self.closing.is_none() {
                            self.io_requests.push(io::Request::Stream {
                                stream: owner,
                                down: stream::Down::Demand { read: stream::Read::Fill(1), room: 0 },
                            });
                        }
                    }
                    stream::Up::End | stream::Up::Failed(_) => terminal.closed(terminal_events),
                    stream::Up::Room => unreachable!("read-only terminal asks for no output room"),
                },
                io::Event::Closed { owner } if Some(owner) == self.terminal_input || Some(owner) == self.signals => {}
                other @ (io::Event::Listening { .. }
                | io::Event::Accepted { .. }
                | io::Event::Connecting { .. }
                | io::Event::Connected { .. }
                | io::Event::Stream { .. }
                | io::Event::Output { .. }
                | io::Event::Spawned { .. }
                | io::Event::Exited { .. }
                | io::Event::Failed { .. }
                | io::Event::Closed { .. }) => self.process.as_mut().expect("spawn precedes process IO").from_io(
                    now,
                    other,
                    &mut self.process_events,
                    &mut self.io_requests,
                ),
            }
        }
        if self.output_room()
            && let Some(git) = &mut self.git
        {
            git.fire(now, &mut self.io_requests);
        }
        let git_done = match &self.git {
            Some(git) => git.done(),
            None => false,
        };
        if git_done {
            self.git = None;
        }
        if self.output_room() {
            self.flush_force();
            if let Some(process) = &mut self.process {
                process.fire(now, &mut self.process_events, &mut self.io_requests);
            }
        }
        for _ in 0..self.process_events.capacity() {
            if host_events.room() == 0 {
                break;
            }
            let Some(event) = self.process_events.pop() else { break };
            host_events.push(to_host_event(event));
        }
    }

    pub(crate) fn down(
        &mut self,
        now: Time,
        wall: Wall,
        requests: &mut Queue<host::Request>,
        start_values: &mut Option<StartValues>,
        grant_values: &Map<u32, Box<[u8]>>,
        host_events: &mut Queue<host::Event>,
    ) {
        for _ in 0..requests.capacity() {
            if !self.output_room() || host_events.room() == 0 {
                break;
            }
            let Some(request) = requests.pop() else { break };
            self.request(request, start_values, grant_values, host_events);
        }
        if let Some(process) = &mut self.process {
            for _ in 0..self.limits.queue {
                if self.io_requests.room() < 64 {
                    break;
                }
                let Some(request) = process.next_channel_down() else { break };
                process.channel_down(request, &mut self.io_requests);
            }
        }
        let env = Env { now, wall, limits: self.limits.io };
        for _ in 0..self.io_requests.capacity() {
            if self.io_submissions.room() < io::MAX_OUT_DOWN.submissions || !self.io.takes() {
                break;
            }
            let Some(request) = self.io_requests.pop() else { break };
            io::down(&mut self.io, &env, request, &mut self.io_submissions);
        }
        if self.files.takes()
            && self.file_submissions.room() >= 2
            && self.file_events.room() > 0
            && let Some((request, deadline)) = self.file_requests.pop()
        {
            file_layer::down_until(
                &mut self.files,
                deadline,
                request,
                &mut self.file_events,
                &mut self.file_submissions,
            );
        }
        if self.closing.unwrap_or(0) > 0
            && !self.close_root_pending
            && self.files.takes()
            && self.file_submissions.room() >= 2
            && self.file_events.room() > 0
        {
            let position = self.closing.expect("closing roots").checked_sub(1).expect("positive roots to close");
            self.closing = Some(position);
            let root =
                *self.file_roots.get(usize::try_from(position).expect("root position fits")).expect("adopted root");
            self.close_root_pending = true;
            file_layer::down(
                &mut self.files,
                now,
                file::Request::Close { owner: Token::new(u64::MAX), file: root },
                &mut self.file_events,
                &mut self.file_submissions,
            );
        }
        self.flush_submissions();
        self.io.reclaim();
    }

    fn flush_submissions(&mut self) {
        for _ in 0..self.io_submissions.capacity() {
            if self.submissions.room() == 0 || self.operations.len() == self.operations.capacity() {
                break;
            }
            let Some(submit) = self.io_submissions.pop() else { break };
            self.route_submission(submit, false);
        }
        for _ in 0..self.file_submissions.capacity() {
            if self.submissions.room() == 0 || self.operations.len() == self.operations.capacity() {
                break;
            }
            let Some(submit) = self.file_submissions.pop() else { break };
            self.route_submission(submit, true);
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "kernel operations are external; only Cancel references another operation"
    )]
    fn route_submission(&mut self, submit: kernel::Submit, file: bool) {
        let global = Token::new(self.next_operation);
        self.next_operation = self.next_operation.checked_add(1).expect("kernel operation tokens never wrap");
        let kind = match submit.kind {
            kernel::Op::Cancel { target } => {
                let mut found = None;
                for (global, route) in &self.operations {
                    match route {
                        Operation::File(original) if file && *original == target => found = Some(*global),
                        Operation::Io(original) if !file && *original == target => found = Some(*global),
                        Operation::File(_) | Operation::Io(_) => {}
                    }
                }
                kernel::Op::Cancel { target: found.unwrap_or(Token::new(u64::MAX)) }
            }
            other => other,
        };
        let route = if file { Operation::File(submit.op) } else { Operation::Io(submit.op) };
        self.operations.insert(global, route).expect("reserved kernel route");
        self.submissions.push(kernel::Submit { op: global, kind });
    }

    fn request(
        &mut self,
        request: host::Request,
        start_values: &mut Option<StartValues>,
        grant_values: &Map<u32, Box<[u8]>>,
        host_events: &mut Queue<host::Event>,
    ) {
        match request {
            host::Request::Spawn { owner, deadline, .. } => {
                let Ok(mut process) = protocol::Process::new(owner, &self.limits.channel, self.limits.detail_bytes)
                else {
                    self.failed = true;
                    host_events.push(host::Event::Unspawned { owner, detail: Box::from(&b"invalid channel"[..]) });
                    return;
                };
                process.spawn(self.launch.for_spawn(), deadline, &mut self.io_requests);
                self.process = Some(process);
            }
            host::Request::Send { owner, message, .. } => {
                let send_token = Token::new(self.send);
                self.send = self.send.checked_add(1).expect("bounded send tokens");
                let process = self.process.as_mut().expect("Spawn precedes Send");
                let sent = match message {
                    host::Down::Start { start, window } => {
                        let values = start_values.take().expect("Start values paired before Spawn");
                        process.send_start(
                            start,
                            window,
                            protocol::Values { paths: values.paths, credentials: values.credentials },
                            send_token,
                            &mut self.process_events,
                            &mut self.io_requests,
                        )
                    }
                    host::Down::Message { name, label, text } => process.send_message(
                        name,
                        label,
                        text,
                        send_token,
                        &mut self.process_events,
                        &mut self.io_requests,
                    ),
                    host::Down::Answer { call, reply } => {
                        process.send_reply(call, reply, send_token, &mut self.process_events, &mut self.io_requests)
                    }
                    host::Down::Acknowledge { turn } => {
                        process.send_acknowledge(turn, send_token, &mut self.process_events, &mut self.io_requests)
                    }
                    host::Down::Grant { grant } => {
                        let value = grant_values.get(&grant.account).expect("grant value supplied before send");
                        process.send_grant(
                            grant,
                            value.clone(),
                            send_token,
                            &mut self.process_events,
                            &mut self.io_requests,
                        )
                    }
                    host::Down::Cancel => {
                        process.send_cancel(send_token, &mut self.process_events, &mut self.io_requests)
                    }
                };
                if sent.is_err() {
                    self.failed = true;
                    host_events.push(host::Event::Unsent { owner });
                }
            }
            host::Request::Signal { owner, signal, .. } => {
                let signal = match signal {
                    host::Signal::Terminate => kernel::Signal::Terminate,
                    host::Signal::Kill => kernel::Signal::Kill,
                };
                self.process.as_ref().expect("spawned child").signal(signal, &mut self.io_requests);
                host_events.push(host::Event::Signalled { owner });
            }
            host::Request::Read { .. } | host::Request::Wait { .. } | host::Request::Reap { .. } => {}
            host::Request::Started { .. }
            | host::Request::Admitted { .. }
            | host::Request::Called { .. }
            | host::Request::Withdrawn { .. }
            | host::Request::Turn { .. }
            | host::Request::Waiting { .. }
            | host::Request::Rejected { .. }
            | host::Request::Exhausted { .. }
            | host::Request::Told { .. }
            | host::Request::Answered { .. }
            | host::Request::Faulted { .. }
            | host::Request::MessageRefused { .. }
            | host::Request::Gone { .. } => unreachable!("parent notice cannot reach lower process adapter"),
        }
    }
}

fn to_host_event(event: protocol::ProcessEvent) -> host::Event {
    match event {
        protocol::ProcessEvent::Spawned { agent, process } => host::Event::Spawned { owner: agent, process },
        protocol::ProcessEvent::Unspawned { agent, detail } => host::Event::Unspawned { owner: agent, detail },
        protocol::ProcessEvent::Exited { agent } => host::Event::Exited { owner: agent },
        protocol::ProcessEvent::Reaped { agent, detail } => host::Event::Reaped { owner: agent, detail },
        protocol::ProcessEvent::Channel { agent, event } => channel_event(agent, event),
    }
}

fn channel_event(agent: Token, event: protocol::OpenEvent) -> host::Event {
    use protocol::OpenEvent;
    match event {
        OpenEvent::Opened { .. } => unreachable!("opened is emitted as Spawned"),
        OpenEvent::Hangup { .. } => host::Event::Hangup { owner: agent },
        OpenEvent::Sent { .. } => host::Event::Sent { owner: agent },
        OpenEvent::Unsent { .. } => host::Event::Unsent { owner: agent },
        OpenEvent::Answer { answer, .. } => {
            host::Event::Received { owner: agent, message: host::Up::Answer { answer } }
        }
        OpenEvent::MessageRefused { name, reason } => {
            host::Event::Received { owner: agent, message: host::Up::MessageRefused { name, reason } }
        }
        OpenEvent::Admitted => host::Event::Received { owner: agent, message: host::Up::Admitted },
        OpenEvent::Waiting { read } => host::Event::Received { owner: agent, message: host::Up::Waiting { read } },
        OpenEvent::Long { span } => host::Event::Received { owner: agent, message: host::Up::Long { span } },
        OpenEvent::LongDone => host::Event::Received { owner: agent, message: host::Up::LongDone },
        OpenEvent::Turn { turn } => host::Event::Received { owner: agent, message: host::Up::Turn { turn } },
        OpenEvent::Fact { body } => host::Event::Received { owner: agent, message: host::Up::Fact { body } },
        OpenEvent::Rejected { account, generation } => {
            host::Event::Received { owner: agent, message: host::Up::Rejected { account, generation } }
        }
        OpenEvent::Exhausted { account, retry_after } => {
            host::Event::Received { owner: agent, message: host::Up::Exhausted { account, retry_after } }
        }
        OpenEvent::WriteFailed => host::Event::Malformed { owner: agent },
        OpenEvent::Call { call, name, deadline, ask } => {
            host::Event::Received { owner: agent, message: host::Up::Call { call, name, deadline, ask } }
        }
        OpenEvent::Withdraw { call } => host::Event::Received { owner: agent, message: host::Up::Withdraw { call } },
    }
}

fn git_limits(limits: &ProcessLimits) -> local_protocol::GitLimits {
    local_protocol::GitLimits {
        argument_bytes: 262_144,
        output_bytes: 65_536,
        conflicts: 64,
        path_bytes: 4096,
        detail_bytes: limits.detail_bytes,
    }
}

fn io_owner(event: &io::Event) -> Option<Token> {
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
        | io::Event::Closed { owner } => Some(*owner),
        io::Event::Shutdown { .. } => None,
    }
}

fn plain_limits() -> local_protocol::PlainLimits {
    local_protocol::PlainLimits { entries: 256, path_bytes: 4096, file_bytes: 65_536 }
}
