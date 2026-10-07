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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observation {
    /// Host completed the opening.
    HostOpened(u16),
    /// Agent completed the opening.
    AgentOpened(u16),
    /// Host's channel ended.
    HostEnded(Closed),
    /// Host's Start was admitted to the output queue.
    HostSent(skein_lib::Token),
    /// Host's Start could not enter the output queue.
    HostUnsent(skein_lib::Token, skein_channel::Unsent),
    /// Agent's channel ended.
    AgentEnded(Closed),
    /// Agent received a valid Start body.
    AgentStart,
    /// Agent Start kept mount paths, post-transcript answers and credential values below the domain.
    AgentContext { path: Box<[u8]>, answered: usize, credential: Box<[u8]>, window: u32 },
    /// A settled oversized answer survived Start translation.
    AgentSavedTooLarge,
    /// A saved host decision reached the domain vocabulary with its stable name.
    AgentSavedHost { activation: u64, tool: Box<[u8]>, text: Box<[u8]>, error: bool },
    /// Host's sender label and message text reached the agent's domain face.
    AgentMessage { name: skein_lib::Token, text: Box<[u8]> },
    /// The host saw the run wait after reading a named message.
    HostWaiting { read: Option<skein_lib::Token> },
    /// Host received an invalid Start answer.
    InvalidStart { why: smith_channel::InvalidStart, turns: u32, spent: u64 },
    /// Host received admission before the final answer.
    HostAdmitted,
    /// Host received the agent's parked last word.
    HostParked { turns: u32, spent: u64 },
    /// Agent translated a saved turn into concrete session history.
    AgentHistory { turns: usize, place: u32 },
    /// A saved turn could not enter the domain as concrete history.
    TranscriptFailed(smith_channel::TranscriptRefusal),
}

enum Half {
    Host(Box<host::Component>),
    Agent(Box<agent::Component>),
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
            half: Half::Host(Box::new(host::Component::new(&limits, mode).expect("checked host channel"))),
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

    fn agent(limits: &agent::Limits, mode: StreamMode) -> Peer {
        Peer {
            half: Half::Agent(Box::new(
                agent::Component::new(limits, mode, test_endpoints()).expect("checked agent channel"),
            )),
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
                host::OpenEvent::Sent { token } => output.push(Observation::HostSent(token)),
                host::OpenEvent::Unsent { token, why } => output.push(Observation::HostUnsent(token, why)),
                host::OpenEvent::Admitted => output.push(Observation::HostAdmitted),
                host::OpenEvent::Waiting { read } => output.push(Observation::HostWaiting { read }),
                host::OpenEvent::Answer { answer } => {
                    if let smith_channel::RunResult::Refused(refused) = answer.result()
                        && let smith_channel::StartRefusal::Invalid(invalid) = refused.reason()
                    {
                        output.push(Observation::InvalidStart {
                            why: invalid.value().clone(),
                            turns: answer.turns(),
                            spent: answer.spent(),
                        });
                    }
                    if let smith_channel::RunResult::Parked = answer.result() {
                        output.push(Observation::HostParked { turns: answer.turns(), spent: answer.spent() });
                    }
                    if let smith_channel::RunResult::Failed(failed) = answer.result()
                        && let smith_channel::RunFailure::Transcript(reason) = failed.reason()
                    {
                        output.push(Observation::TranscriptFailed(reason.value().clone()));
                    }
                }
            }
        }
        while let Some(event) = self.agent_events.pop() {
            match event {
                agent::OpenEvent::Opened { version } => output.push(Observation::AgentOpened(version)),
                agent::OpenEvent::Start { start } => {
                    output.push(Observation::AgentStart);
                    if let Some(transcript) = &start.transcript
                        && let Some(turn) = transcript.turns.first()
                    {
                        output.push(Observation::AgentHistory { turns: transcript.turns.len(), place: turn.sequence });
                    }
                    if start.answered.iter().any(|call| matches!(call.answer, smith_domain::Answered::TooLarge)) {
                        output.push(Observation::AgentSavedTooLarge);
                    }
                    for call in &start.answered {
                        if let smith_domain::Answered::Host(answer) = &call.answer {
                            output.push(Observation::AgentSavedHost {
                                activation: call.name.activation,
                                tool: call.tool.clone(),
                                text: Box::from(answer.text()),
                                error: answer.error(),
                            });
                        }
                    }
                    if let Some(mounts) = &start.mounts
                        && let Some(mount) = mounts.first()
                        && let Some(grant) = start.grants.first()
                    {
                        output.push(Observation::AgentContext {
                            path: mount.path.clone(),
                            answered: start.answered.len(),
                            credential: grant.credential.clone(),
                            window: start.window.turns,
                        });
                    }
                }
                agent::OpenEvent::Ended { why } => output.push(Observation::AgentEnded(why)),
                agent::OpenEvent::Message { name, text } => output.push(Observation::AgentMessage { name, text }),
            }
        }
    }
}

fn test_endpoints() -> agent::Endpoints {
    let mut entries = skein_lib::List::with_capacity(1);
    entries.push(agent::Endpoint { name: Box::default(), number: 0, dialect: 0, account: 0 }).expect("one endpoint");
    agent::Endpoints::new(entries)
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
        let agent = Peer::agent(
            &agent::Limits {
                bodies: agent_bodies,
                charter: smith_charter::CEILINGS,
                transcript: smith_transcript::CEILINGS,
                channel,
                endpoints: 1,
            },
            mode,
        );
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

    /// Send one Start with the supplied opaque charter bytes.
    pub fn send_start(&mut self, charter: Box<[u8]>) {
        self.send_start_with_turns(charter, None);
    }

    /// Send a Start carrying saved turn bytes, if present.
    pub fn send_start_with_turns(&mut self, charter: Box<[u8]>, transcript: Option<Box<[Box<[u8]>]>>) {
        let start = smith_host_domain::channel::Start {
            logical_run: skein_lib::Token::new(1),
            activation: 1,
            workspace: None,
            charter,
            transcript,
            answered: Box::default(),
            directories: Box::default(),
            grants: Box::default(),
        };
        if let Half::Host(host) = &mut self.host.half {
            host.send_start(
                start,
                smith_host_domain::channel::Window { turns: 1, bytes: 1_000_000_000 },
                smith_host_protocol::Values { paths: Box::default(), credentials: Box::default() },
                skein_lib::Token::new(2),
                &mut self.host.host_events,
                &mut self.host.below,
            )
            .expect("bounded Start");
        }
    }

    /// Send one Start with a mount, a saved host decision and a grant value.
    pub fn send_start_with_context(&mut self, charter: Box<[u8]>, reply: smith_host_domain::SavedReply) {
        self.send_start_with_context_and_turns(charter, reply, None);
    }

    /// Send one resumed Start with saved turns and a later host decision.
    pub fn send_start_with_context_and_turns(
        &mut self,
        charter: Box<[u8]>,
        reply: smith_host_domain::SavedReply,
        transcript: Option<Box<[Box<[u8]>]>>,
    ) {
        let start = smith_host_domain::channel::Start {
            logical_run: skein_lib::Token::new(1),
            activation: 7,
            workspace: Some(skein_lib::Token::new(2)),
            charter,
            transcript,
            answered: Box::from([smith_host_domain::channel::AnsweredCall {
                name: smith_host_domain::channel::CallName { activation: 6, completion: 1, position: 0 },
                tool: Box::from(*b"check"),
                reply,
            }]),
            directories: Box::from([smith_host_domain::channel::Directory {
                name: Box::from(*b"src"),
                writable: true,
                git: true,
                conflicts: Box::default(),
            }]),
            grants: Box::from([smith_host_domain::channel::Grant {
                account: 3,
                generation: 4,
                valid: skein_lib::Duration::from_nanos(5),
            }]),
        };
        if let Half::Host(host) = &mut self.host.half {
            host.send_start(
                start,
                smith_host_domain::channel::Window { turns: 2, bytes: 1_000_000_000 },
                smith_host_protocol::Values {
                    paths: Box::from([Box::from(*b"/tmp/src")]),
                    credentials: Box::from([Box::from(*b"secret")]),
                },
                skein_lib::Token::new(2),
                &mut self.host.host_events,
                &mut self.host.below,
            )
            .expect("bounded Start");
        }
    }

    /// Supply a truncated Start body inside a valid channel frame.
    pub fn agent_hears_malformed_start(&mut self) {
        let mut frame = skein_channel::frame_writer(0x0100, 2).expect("bounded Start frame");
        frame.put(&[0, 1]).expect("measured body");
        let frame = frame.finish().expect("complete frame");
        self.agent.incoming.extend(frame.bytes().iter().copied());
    }

    /// Have the scripted domain admit the Start and park without a turn.
    pub fn agent_admits_and_parks(&mut self) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_admitted(skein_lib::Token::new(3), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded admitted");
            agent
                .send_answer(
                    smith_domain::run::Answer::Parked { spent: smith_domain::run::Spend::ZERO, turns: 0 },
                    skein_lib::Token::new(4),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded answer");
        }
    }

    /// Have the scripted domain admit without ending the run.
    pub fn agent_admits(&mut self) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_admitted(skein_lib::Token::new(3), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded admitted");
        }
    }

    /// Relay one named message through the host's half.
    pub fn send_message(&mut self, name: skein_lib::Token, label: Box<[u8]>, text: Box<[u8]>) {
        if let Half::Host(host) = &mut self.host.half {
            host.send_message(
                name,
                label,
                text,
                skein_lib::Token::new(5),
                &mut self.host.host_events,
                &mut self.host.below,
            )
            .expect("bounded message");
        }
    }

    /// Tell the host that the run waits after its latest read message.
    pub fn agent_waits(&mut self, read: Option<skein_lib::Token>) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_waiting(read, skein_lib::Token::new(6), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded waiting");
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
