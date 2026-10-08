//! One spawned agent's IO, pipe channel and host-domain terminals
//! (protocol/hosts.md, sections 3 and 5.6). The adapter keeps IO tokens,
//! process resources and channel buffers. It never knows local delivery
//! policy, a credential's meaning, or the agent's private domain state.
//! `up` converts kernel completions to host observations; `down` translates
//! host effects and submits IO. Gone remains the host domain's decision after
//! the process adapter has reported child exit, pipe EOF and reap.

use alloc::boxed::Box;
use skein_io::{self as io, kernel};
use skein_lib::{Env, Map, Queue, Time, Token, Wall, stream};
use smith_host_domain as host;
use smith_host_protocol as protocol;
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

#[derive(Debug)]
pub(crate) struct ProcessAdapter {
    limits: ProcessLimits,
    launch: Launch,
    io: io::Io,
    process: Option<protocol::Process>,
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
        })
    }

    pub(crate) fn worst_case(limits: &ProcessLimits) -> Option<u64> {
        io::worst_case(&limits.io)?
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

    pub(crate) fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    pub(crate) fn next_deadline(&self) -> Option<Time> {
        let process_deadline = match &self.process {
            Some(process) => process.next_deadline(),
            None => None,
        };
        match (self.io.next_deadline(), process_deadline) {
            (Some(io), Some(process)) => Some(io.min(process)),
            (Some(io), None) => Some(io),
            (None, Some(process)) => Some(process),
            (None, None) => None,
        }
    }

    pub(crate) fn work_pending(&self, now: Time) -> bool {
        self.io.is_ready()
            || self.io.is_due(now)
            || !self.io_events.is_empty()
            || !self.io_requests.is_empty()
            || !self.process_events.is_empty()
            || !self.completions.is_empty()
    }

    pub(crate) const fn failed(&self) -> bool {
        self.failed
    }

    pub(crate) fn force_stop(&mut self) {
        self.force_pending = true;
        self.flush_force();
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

    pub(crate) fn up(
        &mut self,
        now: Time,
        wall: Wall,
        host_events: &mut Queue<host::Event>,
        terminal: &mut local_protocol::Terminal,
        terminal_events: &mut Queue<local_protocol::TerminalEvent>,
    ) {
        let env = Env { now, wall, limits: self.limits.io };
        for _ in 0..self.completions.capacity() {
            let Some(complete) = self.completions.pop() else { break };
            io::up(&mut self.io, &env, complete, &mut self.io_events, &mut self.submissions);
        }
        for _ in 0..self.limits.io.sockets {
            if self.io.is_ready() {
                io::resume(&mut self.io, &env, &mut self.io_events, &mut self.submissions);
            }
            if self.io.is_due(now) {
                io::fire(&mut self.io, &env, &mut self.io_events, &mut self.submissions);
            }
        }
        for _ in 0..self.io_events.capacity() {
            let Some(event) = self.io_events.pop() else { break };
            match event {
                io::Event::Shutdown { .. } => terminal.interrupt(terminal_events),
                io::Event::Stream { owner, up } if Some(owner) == self.terminal_input => match up {
                    stream::Up::Bytes(bytes) => {
                        for byte in bytes {
                            terminal.feed(byte, terminal_events);
                        }
                        self.io_requests.push(io::Request::Stream {
                            stream: owner,
                            down: stream::Down::Demand { read: stream::Read::Fill(1), room: 0 },
                        });
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
        self.flush_force();
        if let Some(process) = &mut self.process {
            process.fire(now, &mut self.process_events, &mut self.io_requests);
        }
        for _ in 0..self.process_events.capacity() {
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
            let Some(request) = requests.pop() else { break };
            self.request(request, start_values, grant_values, host_events);
        }
        if let Some(process) = &mut self.process {
            for _ in 0..self.limits.queue {
                let Some(request) = process.next_channel_down() else { break };
                process.channel_down(request, &mut self.io_requests);
            }
        }
        let env = Env { now, wall, limits: self.limits.io };
        for _ in 0..self.io_requests.capacity() {
            let Some(request) = self.io_requests.pop() else { break };
            io::down(&mut self.io, &env, request, &mut self.submissions);
        }
        self.io.reclaim();
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
            | host::Request::Bounced { .. }
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
