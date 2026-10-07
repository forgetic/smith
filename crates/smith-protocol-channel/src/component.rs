//! Agent-side opening machine (protocol/channel.md, sections 2 and 5).
use alloc::boxed::Box;
use skein_channel::{
    Closed, Event, Frame, Lower, LowerEvent, Machine, Request, Role, Schema, StreamMode, frame_writer,
};
use skein_lib::{Duration, Map, Queue, Reader, Time, Token, Writer};

use crate::calls;
use crate::facts;
use crate::limits::{Error, Limits};
use crate::transcript::decode_transcript;
use crate::translate::{
    DecodedStart, Endpoints, answer_record, decode_charter, invalid_start, saved_delivery, start_context,
};
use crate::turn::encode_turn;
use smith_domain::run;
use smith_domain_session::record;

const RULES: u16 = 256;
const UNAUTHORIZED: u16 = 258;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Opening,
    Opened,
    Started,
    Admitted,
    Answered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CallKey {
    activation: u64,
    completion: u32,
    position: u32,
}

impl CallKey {
    fn from_domain(name: run::CallName) -> CallKey {
        CallKey { activation: name.activation, completion: name.completion, position: name.position }
    }

    fn from_wire(name: &smith_channel::CallName) -> CallKey {
        CallKey { activation: name.activation(), completion: name.completion(), position: name.position() }
    }

    fn domain(self) -> run::CallName {
        run::CallName { activation: self.activation, completion: self.completion, position: self.position }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CallRoute {
    Host { relay: run::RelayName, send: Token },
    Delivery { owner: Token, send: Token },
}

/// One channel event for the agent service.
#[derive(Debug)]
pub enum OpenEvent {
    /// Framing and terms have been agreed at this version.
    Opened { version: u16 },
    /// A structurally valid Start with its charter translated before domain admission.
    Start { start: Box<DecodedStart> },
    /// A host-labelled message for the admitted run.
    Message { name: Token, text: Box<[u8]> },
    /// A declared host tool's one settled terminal.
    HostReturned { relay: run::RelayName, reply: run::HostReply },
    /// A delivery's one settled terminal.
    Delivered { owner: Token, delivery: Box<run::Delivery> },
    /// A delivery frame never entered the stream; service settles its IO operation.
    DeliveryUnsent { owner: Token },
    /// The host durably kept this numbered turn and its preceding prefix.
    Acknowledged { turn: u32 },
    /// The channel ended before or after opening.
    Ended { why: Closed },
}

/// The responder's framed channel and checked sending terms.
#[derive(Debug)]
pub struct Component {
    machine: Machine,
    schema: Schema,
    events: Queue<Event>,
    phase: Phase,
    ended: bool,
    bodies: smith_channel::Limits,
    charter: smith_charter::v1::Limits,
    transcript: smith_transcript::v2::Limits,
    endpoints: Endpoints,
    calls: Map<CallKey, CallRoute>,
    window: Option<smith_domain::Window>,
    turns: Map<u32, u64>,
    last_sent_turn: u32,
    held_turn_bytes: u64,
    fact_reserve_frames: u32,
    fact_reserve_bytes: u32,
    lost_facts: u64,
}

impl Component {
    /// Build one channel with configured endpoint names and either stream shape.
    pub fn new(limits: &Limits, mode: StreamMode, endpoints: Endpoints) -> Result<Component, Error> {
        if !endpoints.fits(limits.endpoints) {
            return Err(Error::Endpoints);
        }
        let schema = match smith_channel::schema(&limits.bodies) {
            Ok(schema) => schema,
            Err(error) => return Err(Error::Codec(error)),
        };
        let machine = match Machine::with_mode(schema.clone(), Role::Responder, limits.channel, mode) {
            Ok(machine) => machine,
            Err(error) => return Err(Error::Channel(error)),
        };
        Ok(Component {
            machine,
            schema,
            events: Queue::with_capacity(8),
            phase: Phase::Opening,
            ended: false,
            bodies: limits.bodies,
            charter: limits.charter,
            transcript: limits.transcript,
            endpoints,
            calls: Map::with_capacity(limits.calls),
            window: None,
            turns: Map::with_capacity(limits.turns),
            last_sent_turn: 0,
            held_turn_bytes: 0,
            fact_reserve_frames: limits.fact_reserve_frames,
            fact_reserve_bytes: limits.fact_reserve_bytes,
            lost_facts: 0,
        })
    }

    /// Consume one stream event and progress the opening.
    pub fn from_below(&mut self, event: LowerEvent, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        self.machine.up(event, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
    }

    /// Request the next read or write operation, including the initial Open read.
    pub fn fire(&mut self, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        self.machine.poll(&mut self.events, below);
        self.drain(to_service, below);
    }

    /// Send the domain's admission notice once the Start has entered it.
    pub fn send_admitted(
        &mut self,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Started {
            return Err(Error::Order);
        }
        let record = smith_channel::Admitted::new(&self.bodies, smith_channel::AdmittedParts {})?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0106, record.measure())?;
        frame.put(&body.finish())?;
        self.phase = Phase::Admitted;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Send the domain's final answer and finish the write stream after it.
    pub fn send_answer(
        &mut self,
        answer: run::Answer,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        let permitted = match &answer {
            run::Answer::Refused(_) => self.phase == Phase::Started,
            run::Answer::Parked { .. } | run::Answer::Accepted { .. } | run::Answer::Failed { .. } => {
                self.phase == Phase::Admitted
            }
        };
        if !permitted {
            return Err(Error::Order);
        }
        let record = answer_record(answer, &self.bodies, &self.charter)?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0110, record.measure())?;
        frame.put(&body.finish())?;
        self.phase = Phase::Answered;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.machine.down(Request::Finish, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Tell the host that the run waits after the last message it read.
    #[expect(clippy::manual_map, reason = "production steps do not use closure methods")]
    pub fn send_waiting(
        &mut self,
        read: Option<Token>,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let last_read = match read {
            Some(name) => Some(name.raw()),
            None => None,
        };
        let record = smith_channel::Waiting::new(&self.bodies, smith_channel::WaitingParts { last_read })?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x010a, record.measure())?;
        frame.put(&body.finish())?;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Announce the bounded span of a check or other long operation.
    pub fn send_long(
        &mut self,
        span: Duration,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let record = smith_channel::Long::new(&self.bodies, smith_channel::LongParts { span })?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x010b, record.measure())?;
        frame.put(&body.finish())?;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// End the announced long operation when its checks finish.
    pub fn send_long_done(
        &mut self,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let record = smith_channel::LongDone::new(&self.bodies, smith_channel::LongDoneParts {})?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x010c, record.measure())?;
        frame.put(&body.finish())?;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Tell the host one concrete transcript turn with activation-wide spend.
    #[expect(clippy::manual_map, reason = "production steps do not use closure methods")]
    #[expect(clippy::too_many_arguments, reason = "turn and its host envelope have independent names")]
    pub fn send_turn(
        &mut self,
        number: u32,
        read: Option<Token>,
        spent: run::Spend,
        turn: &record::Turn,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted || number == 0 {
            return Err(Error::Order);
        }
        let body = encode_turn(turn, &self.transcript, &self.endpoints)?;
        let window = self.window.ok_or(Error::Order)?;
        let Ok(body_bytes) = u64::try_from(body.len()) else {
            return Err(Error::Window);
        };
        let next = self.last_sent_turn.checked_add(1).ok_or(Error::Window)?;
        let held_bytes = self.held_turn_bytes.checked_add(body_bytes).ok_or(Error::Window)?;
        if number != next || self.turns.len() >= window.turns || held_bytes > window.bytes {
            return Err(Error::Window);
        }
        let last_read = match read {
            Some(name) => Some(name.raw()),
            None => None,
        };
        let record = smith_channel::Turn::new(
            &self.bodies,
            smith_channel::TurnParts { number, spent: spent.units, last_read, body },
        )?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0109, record.measure())?;
        frame.put(&body.finish())?;
        let frame = frame.finish()?;
        if self.turns.insert(number, body_bytes).is_err() {
            return Err(Error::Window);
        }
        self.last_sent_turn = number;
        self.held_turn_bytes = held_bytes;
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Offer a content-free domain observation without spending reserved output room.
    pub fn send_fact(
        &mut self,
        fact: smith_domain::Fact,
        elapsed: Duration,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<bool, Error> {
        match facts::project(fact) {
            Some((kind, count)) => self.offer_fact(kind, elapsed, count, token, to_service, below),
            None => Ok(false),
        }
    }

    /// Count text observed from the LLM without putting its bytes on the host channel.
    pub fn send_text_arrived(
        &mut self,
        elapsed: Duration,
        count: u64,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<bool, Error> {
        self.offer_fact(smith_channel::FactKind::TextArrived, elapsed, count, token, to_service, below)
    }

    /// Number of projected facts the output reserve has dropped.
    #[must_use]
    pub fn lost_facts(&self) -> u64 {
        self.lost_facts
    }

    fn offer_fact(
        &mut self,
        kind: smith_channel::FactKind,
        elapsed: Duration,
        count: u64,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<bool, Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let record = smith_channel::Fact::new(&self.bodies, smith_channel::FactParts { kind, elapsed, count })?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::ResultCapacity);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x010f, record.measure())?;
        frame.put(&body.finish())?;
        let frame = frame.finish()?;
        let room = self.machine.room();
        let reserve = self.fact_reserve_bytes.checked_add(frame.wire_len());
        let bytes_fit = match reserve {
            Some(needed) => room.bytes >= needed,
            None => false,
        };
        if room.frames <= self.fact_reserve_frames || !bytes_fit {
            self.lost_facts = self.lost_facts.saturating_add(1);
            return Ok(false);
        }
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(true)
    }

    /// Relay one declared host tool under its durable name and live attempt.
    #[expect(clippy::too_many_arguments, reason = "the typed channel boundary carries each domain value")]
    pub fn send_host_call(
        &mut self,
        now: Time,
        name: run::CallName,
        relay: run::RelayName,
        tool: Box<[u8]>,
        effect: run::HostEffect,
        input: run::HostInput,
        deadline: Time,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let key = CallKey::from_domain(name);
        if self.calls.contains_key(&key) || self.calls.len() >= self.calls.capacity() {
            return Err(Error::Calls);
        }
        let frame = calls::host_call(name, tool, effect, input, deadline.saturating_since(now), &self.bodies)?;
        if self.calls.insert(key, CallRoute::Host { relay, send: token }).is_err() {
            return Err(Error::Calls);
        }
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Relay one checked delivery after its run and checkout effects settled.
    #[expect(clippy::too_many_arguments, reason = "the typed channel boundary carries each domain value")]
    pub fn send_delivery(
        &mut self,
        now: Time,
        name: run::CallName,
        owner: Token,
        change: run::outcome::Change,
        deadline: Time,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let key = CallKey::from_domain(name);
        if self.calls.contains_key(&key) || self.calls.len() >= self.calls.capacity() {
            return Err(Error::Calls);
        }
        let frame = calls::delivery_call(name, change, deadline.saturating_since(now), &self.bodies)?;
        if self.calls.insert(key, CallRoute::Delivery { owner, send: token }).is_err() {
            return Err(Error::Calls);
        }
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Ask the host to settle a live tool relay; its eventual answer remains owed.
    pub fn send_withdraw(
        &mut self,
        relay: run::RelayName,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let mut name = None;
        for (key, route) in &self.calls {
            match route {
                CallRoute::Host { relay: held, send: _ } if *held == relay => name = Some(key.domain()),
                CallRoute::Host { .. } | CallRoute::Delivery { .. } => {}
            }
        }
        let name = name.ok_or(Error::Calls)?;
        let frame = calls::withdraw(name, &self.bodies)?;
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Whether the channel can take application records.
    #[must_use]
    pub fn opened(&self) -> bool {
        self.phase != Phase::Opening
    }

    #[expect(clippy::too_many_lines, reason = "bounded event dispatch keeps channel order in one place")]
    fn drain(&mut self, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        for _ in 0_u8..8_u8 {
            let event = self.events.pop();
            match event {
                Some(Event::Opening { credential, lowest: _, highest }) => {
                    if credential.is_empty() {
                        self.machine.down(Request::Accept { version: highest.min(1) }, &mut self.events, below);
                    } else {
                        self.machine.down(
                            Request::Refuse { reason: UNAUTHORIZED, text: Box::from(*b"credential on pipe") },
                            &mut self.events,
                            below,
                        );
                    }
                }
                Some(Event::Ready { version, terms }) => {
                    match smith_channel::peer_terms_gap(&self.schema, Role::Responder, version, &terms) {
                        Some(kind) => {
                            self.machine.down(
                                Request::Refuse { reason: 2, text: kind_text(kind) },
                                &mut self.events,
                                below,
                            );
                        }
                        None => {
                            self.phase = Phase::Opened;
                            to_service.push(OpenEvent::Opened { version });
                            self.machine.down(Request::Read, &mut self.events, below);
                        }
                    }
                }
                Some(Event::Body { kind: 0x0101, body })
                    if self.phase == Phase::Started || self.phase == Phase::Admitted =>
                {
                    match smith_channel::Message::decode(&self.bodies, &mut Reader::new(&body)) {
                        Ok(message) => match labelled_message(&message) {
                            Some(text) => {
                                to_service.push(OpenEvent::Message { name: Token::new(message.name()), text });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            None => self.refuse_rules(below),
                        },
                        Err(_) => self.refuse_rules(below),
                    }
                }
                Some(Event::Body { kind: 0x0102, body }) if self.phase == Phase::Admitted => {
                    match smith_channel::HostAnswer::decode(&self.bodies, &mut Reader::new(&body)) {
                        Ok(answer) => {
                            let key = CallKey::from_wire(answer.name());
                            match self.calls.remove(&key) {
                                Some(CallRoute::Host { relay, send: _ }) => match calls::host_reply(answer.reply()) {
                                    Some(reply) => {
                                        to_service.push(OpenEvent::HostReturned { relay, reply });
                                        self.machine.down(Request::Read, &mut self.events, below);
                                    }
                                    None => self.refuse_rules(below),
                                },
                                Some(CallRoute::Delivery { owner, send: _ }) => match answer.reply() {
                                    smith_channel::Reply::Delivery(delivery) => {
                                        match saved_delivery(delivery.value()) {
                                            Ok(delivery) => {
                                                to_service
                                                    .push(OpenEvent::Delivered { owner, delivery: Box::new(delivery) });
                                                self.machine.down(Request::Read, &mut self.events, below);
                                            }
                                            Err(_) => self.refuse_rules(below),
                                        }
                                    }
                                    smith_channel::Reply::Host(_)
                                    | smith_channel::Reply::Busy
                                    | smith_channel::Reply::Unavailable
                                    | smith_channel::Reply::Withdrawn
                                    | smith_channel::Reply::TooLarge => self.refuse_rules(below),
                                },
                                None => self.refuse_rules(below),
                            }
                        }
                        Err(_) => self.refuse_rules(below),
                    }
                }
                Some(Event::Body { kind: 0x0103, body }) if self.phase == Phase::Admitted => {
                    match smith_channel::Acknowledge::decode(&self.bodies, &mut Reader::new(&body)) {
                        Ok(acknowledge) => {
                            if acknowledge.number() > self.last_sent_turn || acknowledge.number() == 0 {
                                self.refuse_rules(below);
                                continue;
                            }
                            let mut released = 0_u64;
                            let mut old = skein_lib::List::with_capacity(self.turns.len());
                            for (number, bytes) in &self.turns {
                                if *number <= acknowledge.number() {
                                    released = match released.checked_add(*bytes) {
                                        Some(total) => total,
                                        None => {
                                            self.refuse_rules(below);
                                            return;
                                        }
                                    };
                                    if old.push(*number).is_err() {
                                        self.refuse_rules(below);
                                        return;
                                    }
                                }
                            }
                            for number in &old {
                                self.turns.remove(number);
                            }
                            self.held_turn_bytes = match self.held_turn_bytes.checked_sub(released) {
                                Some(held) => held,
                                None => {
                                    self.refuse_rules(below);
                                    return;
                                }
                            };
                            to_service.push(OpenEvent::Acknowledged { turn: acknowledge.number() });
                            self.machine.down(Request::Read, &mut self.events, below);
                        }
                        Err(_) => self.refuse_rules(below),
                    }
                }
                Some(Event::Body { kind, body }) => {
                    if self.phase != Phase::Opened || kind != 0x0100 {
                        self.refuse_rules(below);
                    } else {
                        match smith_channel::Start::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(start) => {
                                self.phase = Phase::Started;
                                match decode_charter(start.charter(), &self.charter, &self.endpoints) {
                                    Ok(charter) => {
                                        match decode_transcript(
                                            start.transcript().as_slice(),
                                            &self.transcript,
                                            &self.endpoints,
                                        ) {
                                            Ok(transcript) => match start_context(start, charter, transcript) {
                                                Ok(mut start) => {
                                                    start.window.turns = start.window.turns.min(self.turns.capacity());
                                                    self.window = Some(start.window);
                                                    to_service.push(OpenEvent::Start { start: Box::new(start) });
                                                    self.machine.down(Request::Read, &mut self.events, below);
                                                }
                                                Err(_) => self.refuse_rules(below),
                                            },
                                            Err(reason) => self.transcript_refused(reason, to_service, below),
                                        }
                                    }
                                    Err(invalid) => self.invalid_start(invalid_start(invalid), below),
                                }
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                }
                Some(Event::Closed { why }) => {
                    if !self.ended {
                        self.ended = true;
                        to_service.push(OpenEvent::Ended { why });
                    }
                }
                Some(Event::Ended) => {
                    if !self.ended {
                        self.ended = true;
                        to_service.push(OpenEvent::Ended { why: Closed::Stream });
                    }
                }
                Some(Event::Unsent { token, why: _ }) => {
                    let mut found = None;
                    for (key, route) in &self.calls {
                        let send = match route {
                            CallRoute::Host { send, .. } | CallRoute::Delivery { send, .. } => *send,
                        };
                        if send == token {
                            found = Some(*key);
                        }
                    }
                    if let Some(key) = found {
                        match self.calls.remove(&key) {
                            Some(CallRoute::Host { relay, send: _ }) => {
                                to_service.push(OpenEvent::HostReturned {
                                    relay,
                                    reply: run::HostReply::Unanswered(run::Unanswered::Lost),
                                });
                            }
                            Some(CallRoute::Delivery { owner, send: _ }) => {
                                to_service.push(OpenEvent::DeliveryUnsent { owner });
                            }
                            None => {}
                        }
                    }
                }
                Some(
                    Event::Ping
                    | Event::Unsupported { .. }
                    | Event::Refused { .. }
                    | Event::Sent { .. }
                    | Event::Drained
                    | Event::OutputFailed,
                )
                | None => {}
            }
        }
    }

    fn refuse_rules(&mut self, below: &mut Queue<Lower>) {
        self.machine.down(
            Request::Refuse { reason: RULES, text: Box::from(*b"invalid Start") },
            &mut self.events,
            below,
        );
    }

    fn invalid_start(&mut self, invalid: smith_channel::InvalidStart, below: &mut Queue<Lower>) {
        match invalid_answer(&self.bodies, invalid) {
            Some(frame) => {
                self.phase = Phase::Answered;
                self.machine.down(Request::Send { token: Token::new(0), frame }, &mut self.events, below);
                self.machine.down(Request::Finish, &mut self.events, below);
            }
            None => self.refuse_rules(below),
        }
    }

    fn transcript_refused(
        &mut self,
        reason: record::Refusal,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) {
        let reason = match reason {
            record::Refusal::Version => run::TranscriptRefusal::Version,
            record::Refusal::Endpoint => run::TranscriptRefusal::Endpoint,
            record::Refusal::Dialect => run::TranscriptRefusal::Dialect,
            record::Refusal::Malformed => run::TranscriptRefusal::Malformed,
            record::Refusal::Unresolved => run::TranscriptRefusal::Unresolved,
            record::Refusal::TooLarge => run::TranscriptRefusal::TooLarge,
        };
        let admitted = self.send_admitted(Token::new(0), to_service, below);
        let answered = match admitted {
            Ok(()) => self.send_answer(
                run::Answer::Failed { failure: run::Failure::Transcript(reason), spent: run::Spend::ZERO, turns: 0 },
                Token::new(0),
                to_service,
                below,
            ),
            Err(error) => Err(error),
        };
        if answered.is_err() {
            self.refuse_rules(below);
        }
    }
}

fn labelled_message(message: &smith_channel::Message) -> Option<Box<[u8]>> {
    let capacity = message.label().len().checked_add(2)?.checked_add(message.text().len())?;
    let mut writer = Writer::new(capacity);
    writer.put(message.label()).ok()?;
    writer.put(b": ").ok()?;
    writer.put(message.text()).ok()?;
    Some(writer.finish())
}

fn invalid_answer(limits: &smith_channel::Limits, invalid: smith_channel::InvalidStart) -> Option<Frame> {
    let reason =
        smith_channel::InvalidStartValue::new(limits, smith_channel::InvalidStartValueParts { value: invalid }).ok()?;
    let refused = smith_channel::Refused::new(
        limits,
        smith_channel::RefusedParts { reason: smith_channel::StartRefusal::Invalid(reason) },
    )
    .ok()?;
    let record = smith_channel::Answer::new(
        limits,
        smith_channel::AnswerParts { turns: 0, spent: 0, result: smith_channel::RunResult::Refused(refused) },
    )
    .ok()?;
    let body_len = usize::try_from(record.measure()).ok()?;
    let mut body = Writer::new(body_len);
    record.encode(&mut body).ok()?;
    let mut frame = frame_writer(0x0110, record.measure()).ok()?;
    frame.put(&body.finish()).ok()?;
    frame.finish().ok()
}

fn kind_text(kind: u16) -> Box<[u8]> {
    let bytes = kind.to_be_bytes();
    Box::from([
        b'k',
        b'i',
        b'n',
        b'd',
        b' ',
        b'0',
        b'x',
        hex(bytes[0] >> 4),
        hex(bytes[0] & 15),
        hex(bytes[1] >> 4),
        hex(bytes[1] & 15),
    ])
}

fn hex(nibble: u8) -> u8 {
    if nibble < 10 {
        b'0'.checked_add(nibble).expect("hex digit fits")
    } else {
        b'a'.checked_add(nibble.checked_sub(10).expect("letter digit")).expect("hex digit fits")
    }
}
