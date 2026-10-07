//! Host-side opening machine (protocol/channel.md, sections 2 and 5).
use alloc::boxed::Box;
use skein_channel::{Closed, Event, Lower, LowerEvent, Machine, Request, Role, Schema, StreamMode, frame_writer};
use skein_lib::{Duration, Map, Queue, Reader, Time, Token, Writer};
use smith_host_domain::channel;

use crate::answer::decode_answer;
use crate::limits::{Error, Limits};
use crate::translate::{Values, decode_ask, encode_reply, encode_start};

const RULES: u16 = 256;

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
    fn from_wire(name: &smith_channel::CallName) -> CallKey {
        CallKey { activation: name.activation(), completion: name.completion(), position: name.position() }
    }

    fn domain(self) -> channel::CallName {
        channel::CallName { activation: self.activation, completion: self.completion, position: self.position }
    }
}

/// An opening result for the host service.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenEvent {
    /// Framing and terms have been agreed at this version.
    Opened { version: u16 },
    /// The channel ended before or after opening.
    Hangup { why: Closed },
    /// The framed Start entered the channel's bounded output queue.
    Sent { token: Token },
    /// The Start did not enter the channel's output queue.
    Unsent { token: Token, why: skein_channel::Unsent },
    /// The agent's last word in host-domain vocabulary, with its wire record for audit.
    Answer { answer: channel::Answer, record: smith_channel::Answer },
    /// The agent admitted the Start before its later final answer.
    Admitted,
    /// The run is waiting after reading the named message, if any.
    Waiting { read: Option<Token> },
    /// The run announced a long operation with a bounded progress extension.
    Long { span: Duration },
    /// The run ended its previously announced long operation.
    LongDone,
    /// One concrete turn's opaque transcript bytes and checked envelope.
    Turn { turn: channel::Turn },
    /// One content-free, best-effort fact kept in its bounded wire body.
    Fact { body: Box<[u8]> },
    /// A provider rejected this known account and credential generation.
    Rejected { account: u32, generation: u64 },
    /// A provider exhausted this known account for a relative cooldown.
    Exhausted { account: u32, retry_after: Duration },
    /// The independent write stream failed while reads may still settle.
    WriteFailed,
    /// A named operation whose one host terminal remains owed.
    Call { call: Token, name: channel::CallName, deadline: Time, ask: channel::Ask },
    /// The agent withdrew a call; its terminal still remains owed.
    Withdraw { call: Token },
}

/// The initiator's framed channel and checked sending terms.
#[derive(Debug)]
pub struct Component {
    machine: Machine,
    schema: Schema,
    bodies: smith_channel::Limits,
    events: Queue<Event>,
    phase: Phase,
    ended: bool,
    calls_by_name: Map<CallKey, Token>,
    calls_by_token: Map<Token, CallKey>,
    next_call: u64,
    now: Time,
    cancelled: bool,
}

impl Component {
    /// Build one channel with a pipe pair or one socket-like stream.
    pub fn new(limits: &Limits, mode: StreamMode) -> Result<Component, Error> {
        let schema = match smith_channel::schema(&limits.bodies) {
            Ok(schema) => schema,
            Err(error) => return Err(Error::Codec(error)),
        };
        let machine = match Machine::with_mode(schema.clone(), Role::Initiator, limits.channel, mode) {
            Ok(machine) => machine,
            Err(error) => return Err(Error::Channel(error)),
        };
        Ok(Component {
            machine,
            schema,
            bodies: limits.bodies,
            events: Queue::with_capacity(8),
            phase: Phase::Opening,
            ended: false,
            calls_by_name: Map::with_capacity(limits.calls),
            calls_by_token: Map::with_capacity(limits.calls),
            next_call: 1,
            now: Time::ZERO,
            cancelled: false,
        })
    }

    /// Start the pipe opening with its empty credential.
    pub fn open(&mut self, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        self.machine.down(Request::Open { credential: Box::default() }, &mut self.events, below);
        self.fire(to_service, below);
    }

    /// Set the monotonic clock used to turn agent-relative deadlines into host deadlines.
    pub fn set_now(&mut self, now: Time) {
        self.now = now;
    }

    /// Submit the first application record after opening, with host service values.
    pub fn send_start(
        &mut self,
        start: channel::Start,
        window: channel::Window,
        values: Values,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        let frame = encode_start(start, window, values, &self.bodies)?;
        if self.phase != Phase::Opened {
            return Err(Error::MissingValue);
        }
        self.phase = Phase::Started;
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Relay one named sender label and text to the admitted agent.
    pub fn send_message(
        &mut self,
        name: Token,
        label: Box<[u8]>,
        text: Box<[u8]>,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let record =
            smith_channel::Message::new(&self.bodies, smith_channel::MessageParts { name: name.raw(), label, text })?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::MissingValue);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0101, record.measure())?;
        frame.put(&body.finish())?;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Answer one admitted agent call, keeping its durable wire name.
    pub fn send_reply(
        &mut self,
        call: Token,
        reply: channel::Reply,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let key = *self.calls_by_token.get(&call).ok_or(Error::Calls)?;
        let frame = encode_reply(key.domain(), reply, &self.bodies)?;
        self.calls_by_token.remove(&call);
        self.calls_by_name.remove(&key);
        self.machine.down(Request::Send { token, frame }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Acknowledge exact durable commitment of one forwarded turn.
    pub fn send_acknowledge(
        &mut self,
        turn: u32,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let record = smith_channel::Acknowledge::new(&self.bodies, smith_channel::AcknowledgeParts { number: turn })?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::MissingValue);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0103, record.measure())?;
        frame.put(&body.finish())?;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Refresh one named grant with the service-owned credential value.
    pub fn send_grant(
        &mut self,
        grant: channel::Grant,
        credential: Box<[u8]>,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Admitted {
            return Err(Error::Order);
        }
        let value = smith_channel::GrantValue::new(&self.bodies, smith_channel::GrantValueParts { credential })?;
        let grant = smith_channel::Grant::new(
            &self.bodies,
            smith_channel::GrantParts {
                account: grant.account,
                generation: grant.generation,
                valid: grant.valid,
                value,
            },
        )?;
        let record = smith_channel::GrantRefresh::new(&self.bodies, smith_channel::GrantRefreshParts { grant })?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::MissingValue);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0104, record.measure())?;
        frame.put(&body.finish())?;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Ask the agent to stop politely once, preserving answers owed to calls.
    pub fn send_cancel(
        &mut self,
        token: Token,
        to_service: &mut Queue<OpenEvent>,
        below: &mut Queue<Lower>,
    ) -> Result<(), Error> {
        if self.phase != Phase::Started && self.phase != Phase::Admitted || self.cancelled {
            return Err(Error::Order);
        }
        let record = smith_channel::Cancel::new(&self.bodies, smith_channel::CancelParts {})?;
        let Ok(length) = usize::try_from(record.measure()) else {
            return Err(Error::MissingValue);
        };
        let mut body = Writer::new(length);
        record.encode(&mut body)?;
        let mut frame = frame_writer(0x0105, record.measure())?;
        frame.put(&body.finish())?;
        self.cancelled = true;
        self.machine.down(Request::Send { token, frame: frame.finish()? }, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
        Ok(())
    }

    /// Consume one stream event and progress the opening.
    pub fn from_below(&mut self, event: LowerEvent, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        self.machine.up(event, &mut self.events, below);
        self.drain(to_service, below);
        self.fire(to_service, below);
    }

    /// Request the next read or write operation.
    pub fn fire(&mut self, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        self.machine.poll(&mut self.events, below);
        self.drain(to_service, below);
    }

    /// Whether the channel can take application records.
    #[must_use]
    pub fn opened(&self) -> bool {
        self.phase != Phase::Opening
    }

    #[expect(
        clippy::manual_map,
        clippy::too_many_lines,
        reason = "bounded channel event dispatch uses explicit matches"
    )]
    fn drain(&mut self, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        for _ in 0_u8..8_u8 {
            let event = self.events.pop();
            match event {
                Some(Event::Opening { .. }) => {
                    self.machine.down(
                        Request::Refuse { reason: RULES, text: Box::from(*b"unexpected opening") },
                        &mut self.events,
                        below,
                    );
                }
                Some(Event::Ready { version, terms }) => {
                    match smith_channel::peer_terms_gap(&self.schema, Role::Initiator, version, &terms) {
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
                Some(Event::Body { kind, body }) => match kind {
                    0x0107 if self.phase == Phase::Admitted => {
                        match smith_channel::Call::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(record) => {
                                let key = CallKey::from_wire(record.name());
                                if self.calls_by_name.contains_key(&key)
                                    || self.calls_by_name.len() >= self.calls_by_name.capacity()
                                {
                                    self.refuse_rules(below);
                                } else {
                                    match decode_ask(&record) {
                                        Ok(Some(ask)) => {
                                            let call = Token::new(self.next_call);
                                            match self.next_call.checked_add(1) {
                                                Some(next) => {
                                                    self.next_call = next;
                                                    if self.calls_by_name.insert(key, call).is_err()
                                                        || self.calls_by_token.insert(call, key).is_err()
                                                    {
                                                        self.refuse_rules(below);
                                                    } else {
                                                        to_service.push(OpenEvent::Call {
                                                            call,
                                                            name: key.domain(),
                                                            deadline: self.now.saturating_add(record.deadline()),
                                                            ask,
                                                        });
                                                        self.machine.down(Request::Read, &mut self.events, below);
                                                    }
                                                }
                                                None => self.refuse_rules(below),
                                            }
                                        }
                                        Ok(None) | Err(_) => self.refuse_rules(below),
                                    }
                                }
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x0108 if self.phase == Phase::Admitted => {
                        match smith_channel::Withdraw::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(record) => {
                                let key = CallKey::from_wire(record.name());
                                match self.calls_by_name.get(&key) {
                                    Some(call) => {
                                        to_service.push(OpenEvent::Withdraw { call: *call });
                                        self.machine.down(Request::Read, &mut self.events, below);
                                    }
                                    None => self.refuse_rules(below),
                                }
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x010a if self.phase == Phase::Admitted => {
                        match smith_channel::Waiting::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(waiting) => {
                                let read = match waiting.last_read() {
                                    Some(name) => Some(Token::new(*name)),
                                    None => None,
                                };
                                to_service.push(OpenEvent::Waiting { read });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x0109 if self.phase == Phase::Admitted => {
                        match smith_channel::Turn::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(turn) => {
                                to_service.push(OpenEvent::Turn {
                                    turn: channel::Turn {
                                        number: turn.number(),
                                        spent: turn.spent(),
                                        read: match turn.last_read() {
                                            Some(name) => Some(Token::new(*name)),
                                            None => None,
                                        },
                                        body: Box::from(turn.body()),
                                    },
                                });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x010f if self.phase == Phase::Admitted => {
                        match smith_channel::Fact::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(_) => {
                                to_service.push(OpenEvent::Fact { body });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x010d if self.phase == Phase::Admitted => {
                        match smith_channel::Rejected::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(rejected) => {
                                to_service.push(OpenEvent::Rejected {
                                    account: rejected.account(),
                                    generation: rejected.generation(),
                                });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x010e if self.phase == Phase::Admitted => {
                        match smith_channel::Exhausted::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(exhausted) => {
                                to_service.push(OpenEvent::Exhausted {
                                    account: exhausted.account(),
                                    retry_after: exhausted.retry_after(),
                                });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x010b if self.phase == Phase::Admitted => {
                        match smith_channel::Long::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(long) => {
                                to_service.push(OpenEvent::Long { span: long.span() });
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x010c if self.phase == Phase::Admitted => {
                        match smith_channel::LongDone::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(_) => {
                                to_service.push(OpenEvent::LongDone);
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x0106 if self.phase == Phase::Started => {
                        match smith_channel::Admitted::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(_) => {
                                self.phase = Phase::Admitted;
                                to_service.push(OpenEvent::Admitted);
                                self.machine.down(Request::Read, &mut self.events, below);
                            }
                            Err(_) => self.refuse_rules(below),
                        }
                    }
                    0x0110 => match smith_channel::Answer::decode(&self.bodies, &mut Reader::new(&body)) {
                        Ok(record) => {
                            let permitted = match record.result() {
                                smith_channel::RunResult::Refused(_) => self.phase == Phase::Started,
                                smith_channel::RunResult::Accepted(_)
                                | smith_channel::RunResult::Parked
                                | smith_channel::RunResult::Failed(_) => self.phase == Phase::Admitted,
                            };
                            if permitted {
                                match decode_answer(&record) {
                                    Ok(answer) => {
                                        self.phase = Phase::Answered;
                                        to_service.push(OpenEvent::Answer { answer, record });
                                        self.machine.down(Request::Read, &mut self.events, below);
                                    }
                                    Err(_) => self.refuse_rules(below),
                                }
                            } else {
                                self.refuse_rules(below);
                            }
                        }
                        Err(_) => self.refuse_rules(below),
                    },
                    _ => self.refuse_rules(below),
                },
                Some(Event::Closed { why }) => {
                    if !self.ended {
                        self.ended = true;
                        to_service.push(OpenEvent::Hangup { why });
                    }
                }
                Some(Event::Ended) => {
                    if !self.ended {
                        self.ended = true;
                        to_service.push(OpenEvent::Hangup { why: Closed::Stream });
                    }
                }
                Some(Event::Sent { token }) => {
                    to_service.push(OpenEvent::Sent { token });
                }
                Some(Event::Unsent { token, why }) => {
                    to_service.push(OpenEvent::Unsent { token, why });
                }
                Some(Event::Ping | Event::Unsupported { .. } | Event::Refused { .. } | Event::Drained) | None => {}
                Some(Event::OutputFailed) => to_service.push(OpenEvent::WriteFailed),
            }
        }
    }

    fn refuse_rules(&mut self, below: &mut Queue<Lower>) {
        self.machine.down(
            Request::Refuse { reason: RULES, text: Box::from(*b"invalid answer") },
            &mut self.events,
            below,
        );
    }
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
