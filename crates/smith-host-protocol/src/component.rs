//! Host-side opening machine (protocol/channel.md, sections 2 and 5).
use alloc::boxed::Box;
use skein_channel::{Closed, Event, Lower, LowerEvent, Machine, Request, Role, Schema, StreamMode, frame_writer};
use skein_lib::{Queue, Reader, Token, Writer};
use smith_host_domain::channel;

use crate::limits::{Error, Limits};
use crate::translate::{Values, encode_start};

const RULES: u16 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Opening,
    Opened,
    Started,
    Admitted,
    Answered,
}

/// An opening result for the host service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenEvent {
    /// Framing and terms have been agreed at this version.
    Opened { version: u16 },
    /// The channel ended before or after opening.
    Hangup { why: Closed },
    /// The framed Start entered the channel's bounded output queue.
    Sent { token: Token },
    /// The Start did not enter the channel's output queue.
    Unsent { token: Token, why: skein_channel::Unsent },
    /// The agent's last word, decoded before the host domain interprets it.
    Answer { answer: smith_channel::Answer },
    /// The agent admitted the Start before its later final answer.
    Admitted,
    /// The run is waiting after reading the named message, if any.
    Waiting { read: Option<Token> },
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
        })
    }

    /// Start the pipe opening with its empty credential.
    pub fn open(&mut self, to_service: &mut Queue<OpenEvent>, below: &mut Queue<Lower>) {
        self.machine.down(Request::Open { credential: Box::default() }, &mut self.events, below);
        self.fire(to_service, below);
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

    #[expect(clippy::manual_map, reason = "production steps do not use closure methods")]
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
                        Ok(answer) => {
                            let permitted = match answer.result() {
                                smith_channel::RunResult::Refused(_) => self.phase == Phase::Started,
                                smith_channel::RunResult::Accepted(_)
                                | smith_channel::RunResult::Parked
                                | smith_channel::RunResult::Failed(_) => self.phase == Phase::Admitted,
                            };
                            if permitted {
                                self.phase = Phase::Answered;
                                to_service.push(OpenEvent::Answer { answer });
                                self.machine.down(Request::Read, &mut self.events, below);
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
                Some(
                    Event::Ping
                    | Event::Unsupported { .. }
                    | Event::Refused { .. }
                    | Event::Drained
                    | Event::OutputFailed,
                )
                | None => {}
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
