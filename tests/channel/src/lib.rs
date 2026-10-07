//! Opening the host and agent halves over in-memory pipes and a socket-like
//! stream (protocol/channel.md, sections 2 and 10). Later stories compose the
//! domains after start and application records are translated.

use std::collections::VecDeque;

use skein_channel::{Closed, Control, Lower, LowerEvent, StreamMode, control_frame};
use skein_lib::Queue;
use skein_lib::stream::{self, OutputOutcome};
use smith_host_protocol as host;
use smith_protocol_channel as agent;

/// An opening observation independent of which half emitted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    /// Host completed the opening.
    HostOpened(u16),
    /// Agent completed the opening.
    AgentOpened(u16),
    /// Host's channel ended.
    HostEnded(Closed),
    /// Agent's channel ended.
    AgentEnded(Closed),
}

enum Half {
    Host(host::Component),
    Agent(agent::Component),
}

struct Peer {
    half: Half,
    below: Queue<Lower>,
    host_events: Queue<host::OpenEvent>,
    agent_events: Queue<agent::OpenEvent>,
    incoming: VecDeque<u8>,
    pending: Option<usize>,
}

impl Peer {
    fn host(limits: host::Limits, mode: StreamMode) -> Peer {
        let mut peer = Peer {
            half: Half::Host(host::Component::new(&limits, mode).expect("checked host channel")),
            below: Queue::with_capacity(64),
            host_events: Queue::with_capacity(16),
            agent_events: Queue::with_capacity(16),
            incoming: VecDeque::new(),
            pending: None,
        };
        if let Half::Host(host) = &mut peer.half {
            host.open(&mut peer.host_events, &mut peer.below);
        }
        peer
    }

    fn agent(limits: agent::Limits, mode: StreamMode) -> Peer {
        Peer {
            half: Half::Agent(agent::Component::new(&limits, mode).expect("checked agent channel")),
            below: Queue::with_capacity(64),
            host_events: Queue::with_capacity(16),
            agent_events: Queue::with_capacity(16),
            incoming: VecDeque::new(),
            pending: None,
        }
    }

    fn fire(&mut self) {
        match &mut self.half {
            Half::Host(host) => host.fire(&mut self.host_events, &mut self.below),
            Half::Agent(agent) => agent.fire(&mut self.agent_events, &mut self.below),
        }
    }

    fn receive(&mut self, event: LowerEvent) {
        match &mut self.half {
            Half::Host(host) => host.from_below(event, &mut self.host_events, &mut self.below),
            Half::Agent(agent) => agent.from_below(event, &mut self.agent_events, &mut self.below),
        }
    }

    fn step(&mut self, other: &mut Peer) {
        self.fire();
        while let Some(lower) = self.below.pop() {
            match lower {
                Lower::Read(stream::Down::Demand { read: stream::Read::Fill(count), room: 0 }) => {
                    self.pending = Some(usize::try_from(count).expect("bounded read"));
                }
                Lower::Read(stream::Down::Demand { read: stream::Read::Nothing, room: 0 }) => self.pending = None,
                Lower::Read(stream::Down::Finish)
                | Lower::FinishWrite
                | Lower::Write(stream::OutputDown::Release { .. }) => {}
                Lower::Read(other) => panic!("unexpected read demand: {other:?}"),
                Lower::Write(stream::OutputDown::Room { right, .. }) => {
                    self.receive(LowerEvent::Write(stream::OutputUp::Settled {
                        right,
                        outcome: OutputOutcome::Granted,
                    }));
                }
                Lower::Write(stream::OutputDown::Send { bytes, .. }) => other.incoming.extend(bytes.iter().copied()),
                Lower::Write(stream::OutputDown::Cancel { right }) => {
                    self.receive(LowerEvent::Write(stream::OutputUp::Settled {
                        right,
                        outcome: OutputOutcome::Cancelled,
                    }));
                }
            }
        }
        if let Some(count) = self.pending
            && self.incoming.len() >= count
        {
            let bytes: Box<[u8]> = (0..count).map(|_| self.incoming.pop_front().expect("enough input")).collect();
            self.pending = None;
            self.receive(LowerEvent::Read(stream::Up::Bytes(bytes)));
        }
    }

    fn observations(&mut self, output: &mut Vec<Observation>) {
        while let Some(event) = self.host_events.pop() {
            match event {
                host::OpenEvent::Opened { version } => output.push(Observation::HostOpened(version)),
                host::OpenEvent::Hangup { why } => output.push(Observation::HostEnded(why)),
            }
        }
        while let Some(event) = self.agent_events.pop() {
            match event {
                agent::OpenEvent::Opened { version } => output.push(Observation::AgentOpened(version)),
                agent::OpenEvent::Ended { why } => output.push(Observation::AgentEnded(why)),
            }
        }
    }
}

fn channel_limits(bodies: smith_channel::Limits) -> skein_channel::Limits {
    let schema = smith_channel::schema(&bodies).expect("bounded bodies");
    let version = schema.version(1).expect("v1");
    let largest = version.kinds.iter().map(|kind| kind.largest).max().expect("kinds");
    skein_channel::Limits {
        chunk: 4096,
        credential: 0,
        skip: 4096,
        output_bytes: largest.checked_add(8).expect("frame fits"),
        output_frames: 4,
        kinds: 17,
    }
}

/// A deterministic pair of channels, with independent bytes each way.
pub struct World {
    host: Peer,
    agent: Peer,
    observed: Vec<Observation>,
    channel: skein_channel::Limits,
}

impl World {
    /// Build both halves with independent receiving limits.
    #[must_use]
    pub fn new(host_bodies: smith_channel::Limits, agent_bodies: smith_channel::Limits, mode: StreamMode) -> World {
        let channel = channel_limits(smith_channel::CEILINGS);
        let host = Peer::host(host::Limits { bodies: host_bodies, channel }, mode);
        let agent = Peer::agent(agent::Limits { bodies: agent_bodies, channel }, mode);
        World { host, agent, observed: Vec::new(), channel }
    }

    /// Drive the finite opening until no new bytes remain.
    pub fn settle(&mut self) {
        for _ in 0..1000 {
            self.host.step(&mut self.agent);
            self.agent.step(&mut self.host);
            self.host.observations(&mut self.observed);
            self.agent.observations(&mut self.observed);
        }
    }

    /// Inject an Open from a later version to exercise version refusal.
    pub fn agent_hears_foreign_version(&mut self) {
        let frame = control_frame(
            &Control::Open { magic: *b"smth", lowest: 2, highest: 2, features: 0, credential: Box::default() },
            &self.channel,
        )
        .expect("bounded foreign opening");
        self.agent.incoming.extend(frame.bytes().iter().copied());
    }

    /// Opening results in wire order.
    #[must_use]
    pub fn observations(&self) -> &[Observation] {
        &self.observed
    }
}
