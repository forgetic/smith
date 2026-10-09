//! Opening the host and agent halves over in-memory pipes and a socket-like
//! stream (protocol/channel.md, sections 2 and 10). The composed story routes
//! a real host Start and agent answer through both domains and both halves.

use std::collections::VecDeque;

use skein_channel::{Closed, Control, Lower, LowerEvent, StreamMode, control_frame};
use skein_lib::stream::{self, OutputOutcome};
use skein_lib::{Queue, Token};
use smith_host_protocol as host;
use smith_protocol_channel as agent;

pub mod referee;

/// An opening observation independent of which half emitted it.
#[derive(Debug, PartialEq, Eq)]
pub enum Observation {
    /// The scripted host domain submitted this run's one Start.
    HostStarted,
    /// The scripted agent domain issued one durable operation name.
    AgentIssuedCall { name: smith_host_domain::channel::CallName },
    /// The scripted host domain supplied one terminal for a live call.
    HostSettledCall { call: skein_lib::Token },
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
    AgentContext { path: Box<[u8]>, answered: usize, window: u32 },
    /// The agent received a grant name and relative validity without its value.
    AgentGrant { account: u32, generation: u64, valid: skein_lib::Duration },
    /// A settled oversized answer survived Start translation.
    AgentSavedTooLarge,
    /// A saved host decision reached the domain vocabulary with its stable name.
    AgentSavedHost { activation: u64, tool: Box<[u8]>, text: Box<[u8]>, error: bool },
    /// Host's sender label and message text reached the agent's domain face.
    AgentMessage { name: skein_lib::Token, label: Box<[u8]>, text: Box<[u8]> },
    /// The host saw the run wait after reading a named message.
    HostWaiting { read: Option<skein_lib::Token> },
    /// The host received a long operation's bounded span.
    HostLong { span: skein_lib::Duration },
    /// The host received the end of the long operation.
    HostLongDone,
    /// A named message ended refused at the agent ingress.
    HostMessageRefused { name: Token, reason: smith_host_domain::MessageRefusal },
    /// The host received a numbered durable transcript body.
    HostTurn { number: u32, spent: u64, read: Option<skein_lib::Token>, body: Box<[u8]> },
    /// A best-effort content-free fact reached the host.
    HostFact { kind: smith_channel::FactKind, elapsed: skein_lib::Duration, count: u64 },
    /// The host received a credential rejection notice.
    HostRejected { account: u32, generation: u64 },
    /// The host received an account exhaustion notice.
    HostExhausted { account: u32, retry_after: skein_lib::Duration },
    /// The agent received the host's one polite cancellation.
    AgentCancel,
    /// The agent's write stream failed while its read side may continue.
    AgentWriteFailed,
    /// The host's write stream failed while its read side may continue.
    HostWriteFailed,
    /// The agent received exact durable commitment for one turn.
    AgentAcknowledged { turn: u32 },
    /// The host received one named call with its deadline and metadata.
    HostCall {
        call: skein_lib::Token,
        name: smith_host_domain::channel::CallName,
        deadline: skein_lib::Time,
        ask: Box<smith_host_domain::channel::Ask>,
    },
    /// The host received a withdrawal for a live call.
    HostWithdraw { call: skein_lib::Token },
    /// The agent received the host tool's settled terminal under its relay.
    AgentHostReturned { relay: smith_domain::run::RelayName, reply: smith_domain::run::HostReply },
    /// The agent received a checked delivery terminal.
    AgentDelivered { owner: skein_lib::Token, delivery: Box<smith_domain::run::Delivery> },
    /// A delivery frame did not enter the stream.
    AgentDeliveryUnsent { owner: skein_lib::Token },
    /// Host received an invalid Start answer.
    InvalidStart { why: smith_channel::InvalidStart, turns: u32, spent: u64 },
    /// Host received admission before the final answer.
    HostAdmitted,
    /// Host received the agent's parked last word.
    HostParked { turns: u32, spent: u64 },
    /// The host received the agent's one final answer envelope.
    HostAnswer { turns: u32, spent: u64 },
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
    incoming_ended: bool,
    end_reported: bool,
    fail_next_write: bool,
    pending: Option<usize>,
    cut_after: Option<usize>,
    received_bytes: usize,
    host_answer: Option<smith_host_domain::channel::Answer>,
    agent_start: Option<Box<agent::DecodedStart>>,
}

impl Peer {
    fn host(limits: host::Limits, mode: StreamMode) -> Peer {
        let mut peer = Peer {
            half: Half::Host(Box::new(host::Component::new(&limits, mode).expect("checked host channel"))),
            below: Queue::with_capacity(64),
            host_events: Queue::with_capacity(16),
            agent_events: Queue::with_capacity(16),
            incoming: VecDeque::new(),
            incoming_ended: false,
            end_reported: false,
            fail_next_write: false,
            pending: None,
            cut_after: None,
            received_bytes: 0,
            host_answer: None,
            agent_start: None,
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
            incoming_ended: false,
            end_reported: false,
            fail_next_write: false,
            pending: None,
            cut_after: None,
            received_bytes: 0,
            host_answer: None,
            agent_start: None,
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
                Lower::Read(stream::Down::Finish) | Lower::Write(stream::OutputDown::Release { .. }) => {}
                Lower::FinishWrite => other.incoming_ended = true,
                Lower::Read(other) => panic!("unexpected read demand: {other:?}"),
                Lower::Write(stream::OutputDown::Room { right, .. }) => {
                    let outcome = if self.fail_next_write {
                        self.fail_next_write = false;
                        OutputOutcome::Failed(stream::Fault::Other)
                    } else {
                        OutputOutcome::Granted
                    };
                    self.receive(LowerEvent::Write(stream::OutputUp::Settled { right, outcome }));
                }
                Lower::Write(stream::OutputDown::Send { bytes, .. }) => {
                    if !other.incoming_ended {
                        let keep = other.cut_after.map_or(bytes.len(), |remaining| remaining.min(bytes.len()));
                        other.incoming.extend(bytes[..keep].iter().copied());
                        other.received_bytes += keep;
                        if let Some(remaining) = &mut other.cut_after {
                            *remaining -= keep;
                            if *remaining == 0 {
                                other.incoming_ended = true;
                            }
                        }
                    }
                }
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
        if self.pending.is_some() && self.incoming_ended && !self.end_reported {
            self.incoming.clear();
            self.pending = None;
            self.end_reported = true;
            self.receive(LowerEvent::Read(stream::Up::End));
        }
    }

    #[expect(clippy::too_many_lines, reason = "the world collects both channel halves' ordered events")]
    fn observations(&mut self, output: &mut Vec<Observation>) {
        while let Some(event) = self.host_events.pop() {
            match event {
                host::OpenEvent::Opened { version } => output.push(Observation::HostOpened(version)),
                host::OpenEvent::Hangup { why } => output.push(Observation::HostEnded(why)),
                host::OpenEvent::Sent { token } => output.push(Observation::HostSent(token)),
                host::OpenEvent::Unsent { token, why } => output.push(Observation::HostUnsent(token, why)),
                host::OpenEvent::MessageRefused { name, reason } => {
                    output.push(Observation::HostMessageRefused { name, reason });
                }
                host::OpenEvent::Admitted => output.push(Observation::HostAdmitted),
                host::OpenEvent::Waiting { read } => output.push(Observation::HostWaiting { read }),
                host::OpenEvent::Long { span } => output.push(Observation::HostLong { span }),
                host::OpenEvent::LongDone => output.push(Observation::HostLongDone),
                host::OpenEvent::Turn { turn } => output.push(Observation::HostTurn {
                    number: turn.number,
                    spent: turn.spent,
                    read: turn.read,
                    body: turn.body,
                }),
                host::OpenEvent::Fact { body } => {
                    let fact =
                        smith_channel::Fact::decode(&smith_channel::CEILINGS, &mut skein_lib::Reader::new(&body))
                            .expect("validated fact");
                    output.push(Observation::HostFact {
                        kind: fact.kind().clone(),
                        elapsed: fact.elapsed(),
                        count: fact.count(),
                    });
                }
                host::OpenEvent::Rejected { account, generation } => {
                    output.push(Observation::HostRejected { account, generation });
                }
                host::OpenEvent::Exhausted { account, retry_after } => {
                    output.push(Observation::HostExhausted { account, retry_after });
                }
                host::OpenEvent::WriteFailed => output.push(Observation::HostWriteFailed),
                host::OpenEvent::Call { call, name, deadline, ask } => {
                    output.push(Observation::HostCall { call, name, deadline, ask: Box::new(ask) });
                }
                host::OpenEvent::Withdraw { call } => output.push(Observation::HostWithdraw { call }),
                host::OpenEvent::Answer { answer, record } => {
                    assert_eq!((answer.turns, answer.spent), (record.turns(), record.spent()));
                    output.push(Observation::HostAnswer { turns: answer.turns, spent: answer.spent });
                    if let smith_channel::RunResult::Refused(refused) = record.result()
                        && let smith_channel::StartRefusal::Invalid(invalid) = refused.reason()
                    {
                        output.push(Observation::InvalidStart {
                            why: invalid.value().clone(),
                            turns: answer.turns,
                            spent: answer.spent,
                        });
                    }
                    if let smith_channel::RunResult::Parked = record.result() {
                        output.push(Observation::HostParked { turns: answer.turns, spent: answer.spent });
                    }
                    if let smith_channel::RunResult::Failed(failed) = record.result()
                        && let smith_channel::RunFailure::Transcript(reason) = failed.reason()
                    {
                        output.push(Observation::TranscriptFailed(reason.value().clone()));
                    }
                    assert!(self.host_answer.replace(answer).is_none(), "one typed host last word");
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
                    {
                        output.push(Observation::AgentContext {
                            path: mount.path.clone(),
                            answered: start.answered.len(),
                            window: start.window.turns,
                        });
                    }
                    assert!(self.agent_start.replace(start).is_none(), "one decoded Start per channel");
                }
                agent::OpenEvent::Ended { why } => output.push(Observation::AgentEnded(why)),
                agent::OpenEvent::Message { name, label, text } => {
                    output.push(Observation::AgentMessage { name, label, text });
                }
                agent::OpenEvent::HostReturned { relay, reply } => {
                    output.push(Observation::AgentHostReturned { relay, reply });
                }
                agent::OpenEvent::Delivered { owner, delivery } => {
                    output.push(Observation::AgentDelivered { owner, delivery });
                }
                agent::OpenEvent::DeliveryUnsent { owner } => output.push(Observation::AgentDeliveryUnsent { owner }),
                agent::OpenEvent::Acknowledged { turn } => output.push(Observation::AgentAcknowledged { turn }),
                agent::OpenEvent::Grant { grant } => output.push(Observation::AgentGrant {
                    account: grant.name.account,
                    generation: grant.name.generation,
                    valid: grant.valid,
                }),
                agent::OpenEvent::Cancel => output.push(Observation::AgentCancel),
                agent::OpenEvent::WriteFailed => output.push(Observation::AgentWriteFailed),
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
    let version = schema.version(2).expect("v2");
    let largest = version.kinds.iter().map(|kind| kind.largest).max().expect("kinds");
    skein_channel::Limits {
        chunk: 4096,
        credential: 0,
        skip: 4096,
        output_bytes: largest.checked_add(8).expect("frame fits"),
        output_frames: 4,
        kinds: 18,
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
        let host = Peer::host(host::Limits { bodies: host_bodies, channel, calls: 8 }, mode);
        let agent = Peer::agent(
            &agent::Limits {
                bodies: agent_bodies,
                charter: smith_charter::CEILINGS,
                transcript: smith_transcript::CEILINGS,
                channel,
                endpoints: 1,
                calls: 8,
                turns: 8,
                fact_reserve_frames: 1,
                fact_reserve_bytes: 128,
                grants: 8,
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

    /// End one input stream after exactly this many more wire bytes arrive.
    pub fn cut_agent_input_after(&mut self, bytes: usize) {
        self.agent.cut_after = Some(bytes);
        if bytes == 0 {
            self.agent.incoming_ended = true;
        }
    }

    /// End the host input stream after exactly this many more wire bytes arrive.
    pub fn cut_host_input_after(&mut self, bytes: usize) {
        self.host.cut_after = Some(bytes);
        if bytes == 0 {
            self.host.incoming_ended = true;
        }
    }

    /// Actual bytes each side received from the wire, including opening bytes.
    #[must_use]
    pub fn received_bytes(&self) -> (usize, usize) {
        (self.host.received_bytes, self.agent.received_bytes)
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
        self.send_domain_start(
            start,
            smith_host_domain::channel::Window { turns: 1, bytes: 1_000_000_000 },
            smith_host_protocol::Values { paths: Box::default(), credentials: Box::default() },
        );
    }

    /// Route the host domain's actual first Send through the host protocol half.
    pub fn send_domain_start(
        &mut self,
        start: smith_host_domain::channel::Start,
        window: smith_host_domain::channel::Window,
        values: smith_host_protocol::Values,
    ) {
        self.observed.push(Observation::HostStarted);
        if let Half::Host(host) = &mut self.host.half {
            host.send_start(
                start,
                window,
                values,
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
        self.observed.push(Observation::HostStarted);
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
                    None,
                    skein_lib::Token::new(4),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded answer");
        }
    }

    /// Have the admitted scripted domain park after its effects settle.
    pub fn agent_parks(&mut self) {
        self.agent_answers(smith_domain::run::Answer::Parked { spent: smith_domain::run::Spend::ZERO, turns: 0 });
    }

    /// Send one actual typed domain answer through the agent half.
    pub fn agent_answers(&mut self, answer: smith_domain::run::Answer) {
        self.agent_answers_with_fence(answer, None);
    }

    /// Send the final fence with its one actual domain terminal.
    pub fn agent_answers_with_fence(&mut self, answer: smith_domain::run::Answer, read: Option<Token>) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_answer(
                    answer,
                    read,
                    skein_lib::Token::new(33),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded parked answer");
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

    /// Tell one actual run refusal through the agent half.
    pub fn agent_refuses_message(&mut self, name: Token, reason: smith_domain::run::MessageRefusal) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_message_refused(name, reason, Token::new(34), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded message refusal");
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

    /// Tell the host that checks begin with this progress extension.
    pub fn agent_starts_long(&mut self, span: skein_lib::Duration) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_long(span, skein_lib::Token::new(26), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded long span");
        }
    }

    /// Tell the host that the checks ended.
    pub fn agent_ends_long(&mut self) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_long_done(skein_lib::Token::new(27), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded long end");
        }
    }

    /// Offer one content-free domain fact through the agent's bounded output.
    pub fn agent_sends_fact(&mut self, fact: smith_domain::Fact, elapsed: skein_lib::Duration) -> bool {
        if let Half::Agent(agent) = &mut self.agent.half {
            return agent
                .send_fact(
                    fact,
                    elapsed,
                    skein_lib::Token::new(30),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded fact");
        }
        false
    }

    /// Facts dropped by the agent half because output room was reserved.
    #[must_use]
    pub fn lost_facts(&self) -> u64 {
        if let Half::Agent(agent) = &self.agent.half {
            return agent.lost_facts();
        }
        0
    }

    /// Have the scripted domain tell one concrete turn to the host.
    pub fn agent_tells_turn(
        &mut self,
        number: u32,
        read: Option<skein_lib::Token>,
        turn: &smith_domain_session::record::Turn,
    ) {
        self.try_agent_tells_turn(number, read, turn).expect("bounded turn");
    }

    /// Attempt a scripted turn while the host may still hold all its credit.
    ///
    /// # Errors
    /// Returns the channel's flow-control or encoding error.
    pub fn try_agent_tells_turn(
        &mut self,
        number: u32,
        read: Option<skein_lib::Token>,
        turn: &smith_domain_session::record::Turn,
    ) -> Result<(), agent::Error> {
        if let Half::Agent(agent) = &mut self.agent.half {
            let mut spent = smith_domain::run::Spend::ZERO;
            spent.units = turn.spent;
            agent.send_turn(
                number,
                read,
                spent,
                turn,
                skein_lib::Token::new(28),
                &mut self.agent.agent_events,
                &mut self.agent.below,
            )?;
        }
        Ok(())
    }

    /// Have the scripted host acknowledge its exact durable turn.
    pub fn host_acknowledges(&mut self, turn: u32) {
        if let Half::Host(host) = &mut self.host.half {
            host.send_acknowledge(turn, skein_lib::Token::new(29), &mut self.host.host_events, &mut self.host.below)
                .expect("bounded acknowledgement");
        }
    }

    /// Refresh one service-owned credential under a new generation.
    pub fn host_refreshes_grant(&mut self, account: u32, generation: u64, credential: Box<[u8]>) {
        if let Half::Host(host) = &mut self.host.half {
            host.send_grant(
                smith_host_domain::channel::Grant { account, generation, valid: skein_lib::Duration::from_nanos(50) },
                credential,
                skein_lib::Token::new(32),
                &mut self.host.host_events,
                &mut self.host.below,
            )
            .expect("bounded grant");
        }
    }

    /// Read the agent half's credential table as a service would during LLM preparation.
    #[must_use]
    pub fn agent_grant_value(&self, account: u32, generation: u64) -> Option<Box<[u8]>> {
        if let Half::Agent(agent) = &self.agent.half {
            let name = smith_domain::GrantName { account, generation };
            return agent.grant_value(name).map(Box::from);
        }
        None
    }

    /// Forward one provider rejection from the scripted agent domain.
    pub fn agent_rejects_grant(&mut self, account: u32, generation: u64) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_rejected(
                    smith_domain::GrantName { account, generation },
                    skein_lib::Token::new(34),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded rejected notice");
        }
    }

    /// Forward one account cooldown from the scripted agent domain.
    pub fn agent_exhausts_account(&mut self, account: u32, retry_after: skein_lib::Duration) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_exhausted(
                    account,
                    retry_after,
                    skein_lib::Token::new(35),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded exhaustion notice");
        }
    }

    /// Ask the admitted agent to cancel once.
    pub fn host_cancels(&mut self) {
        if let Half::Host(host) = &mut self.host.half {
            host.send_cancel(skein_lib::Token::new(36), &mut self.host.host_events, &mut self.host.below)
                .expect("one cancel");
        }
    }

    /// Make the agent's next output-room operation fail without ending input.
    pub fn agent_next_write_fails(&mut self) {
        self.agent.fail_next_write = true;
    }

    /// Have the scripted domain relay a declared host tool under a durable name.
    pub fn agent_calls_host(&mut self, name: smith_domain::run::CallName, relay: smith_domain::run::RelayName) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_host_call(
                    skein_lib::Time::ZERO,
                    name,
                    relay,
                    Box::from(*b"check"),
                    smith_domain::run::HostEffect::Read,
                    smith_domain::run::HostInput::attested(Box::from(*b"{}")).expect("attested object"),
                    skein_lib::Time::from_nanos(100),
                    skein_lib::Token::new(20),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded host call");
            self.observed.push(Observation::AgentIssuedCall {
                name: smith_host_domain::channel::CallName {
                    activation: name.activation,
                    completion: name.completion,
                    position: name.position,
                },
            });
        }
    }

    /// Have the scripted domain submit checked change fields for delivery.
    pub fn agent_delivers(&mut self, name: smith_domain::run::CallName, owner: skein_lib::Token) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_delivery(
                    skein_lib::Time::ZERO,
                    name,
                    owner,
                    smith_domain::run::outcome::Change {
                        fields: Box::from([smith_domain::run::outcome::Field {
                            name: Box::from(*b"title"),
                            value: Box::from(*b"Fix"),
                        }]),
                    },
                    skein_lib::Time::from_nanos(100),
                    skein_lib::Token::new(23),
                    &mut self.agent.agent_events,
                    &mut self.agent.below,
                )
                .expect("bounded delivery call");
            self.observed.push(Observation::AgentIssuedCall {
                name: smith_host_domain::channel::CallName {
                    activation: name.activation,
                    completion: name.completion,
                    position: name.position,
                },
            });
        }
    }

    /// Withdraw the scripted live relay while its terminal remains owed.
    pub fn agent_withdraws(&mut self, relay: smith_domain::run::RelayName) {
        if let Half::Agent(agent) = &mut self.agent.half {
            agent
                .send_withdraw(relay, skein_lib::Token::new(21), &mut self.agent.agent_events, &mut self.agent.below)
                .expect("bounded withdraw");
        }
    }

    /// Have the scripted host settle one live call through its half.
    pub fn host_answers(&mut self, call: skein_lib::Token, reply: smith_host_domain::channel::Reply) {
        if let Half::Host(host) = &mut self.host.half {
            host.send_reply(call, reply, skein_lib::Token::new(22), &mut self.host.host_events, &mut self.host.below)
                .expect("bounded host answer");
            self.observed.push(Observation::HostSettledCall { call });
        }
    }

    /// Inject a version-one Open to exercise version refusal.
    pub fn agent_hears_foreign_version(&mut self) {
        let frame = control_frame(
            &Control::Open { magic: *b"smth", lowest: 1, highest: 1, features: 0, credential: Box::default() },
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

    /// Take the typed last word for a real host domain or a boundary assertion.
    pub fn take_host_answer(&mut self) -> Option<smith_host_domain::channel::Answer> {
        self.host.host_answer.take()
    }

    /// Take the agent protocol half's translated Start for the real agent domain.
    pub fn take_agent_start(&mut self) -> Option<Box<agent::DecodedStart>> {
        self.agent.agent_start.take()
    }
}
