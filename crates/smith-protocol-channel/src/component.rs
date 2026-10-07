//! Agent-side opening machine (protocol/channel.md, sections 2 and 5).
use alloc::boxed::Box;
use skein_channel::{
    Closed, Event, Frame, Lower, LowerEvent, Machine, Request, Role, Schema, StreamMode, frame_writer,
};
use skein_lib::{Queue, Reader, Token, Writer};

use crate::limits::{Error, Limits};

const RULES: u16 = 256;
const UNAUTHORIZED: u16 = 258;

/// An opening result for the agent service; the domain is not entered yet.
#[derive(Debug)]
pub enum OpenEvent {
    /// Framing and terms have been agreed at this version.
    Opened { version: u16 },
    /// A structurally valid Start, before charter and transcript translation.
    Start { start: smith_channel::Start },
    /// The channel ended before or after opening.
    Ended { why: Closed },
}

/// The responder's framed channel and checked sending terms.
#[derive(Debug)]
pub struct Component {
    machine: Machine,
    schema: Schema,
    events: Queue<Event>,
    opened: bool,
    ended: bool,
    started: bool,
    bodies: smith_channel::Limits,
}

impl Component {
    /// Build one channel with a pipe pair or one socket-like stream.
    pub fn new(limits: &Limits, mode: StreamMode) -> Result<Component, Error> {
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
            opened: false,
            ended: false,
            started: false,
            bodies: limits.bodies,
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

    /// Whether the channel can take application records.
    #[must_use]
    pub const fn opened(&self) -> bool {
        self.opened
    }

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
                            self.opened = true;
                            to_service.push(OpenEvent::Opened { version });
                            self.machine.down(Request::Read, &mut self.events, below);
                        }
                    }
                }
                Some(Event::Body { kind, body }) => {
                    if !self.opened || self.started || kind != 0x0100 {
                        self.refuse_rules(below);
                    } else {
                        match smith_channel::Start::decode(&self.bodies, &mut Reader::new(&body)) {
                            Ok(start) => {
                                self.started = true;
                                match charter_problem(start.charter()) {
                                    Some(invalid) => self.invalid_start(invalid, below),
                                    None => to_service.push(OpenEvent::Start { start }),
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
                Some(
                    Event::Ping
                    | Event::Unsupported { .. }
                    | Event::Refused { .. }
                    | Event::Sent { .. }
                    | Event::Unsent { .. }
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
                self.machine.down(Request::Send { token: Token::new(0), frame }, &mut self.events, below);
                self.machine.down(Request::Finish, &mut self.events, below);
            }
            None => self.refuse_rules(below),
        }
    }
}

fn charter_problem(bytes: &[u8]) -> Option<smith_channel::InvalidStart> {
    if bytes.len() < 2 {
        return Some(smith_channel::InvalidStart::MalformedCharter);
    }
    if bytes.get(..2) != Some(&[0, 1][..]) {
        return Some(smith_channel::InvalidStart::CharterVersion);
    }
    match smith_charter::Charter::decode(&smith_charter::CEILINGS, &mut Reader::new(bytes)) {
        Ok(_) => None,
        Err(_) => Some(smith_channel::InvalidStart::MalformedCharter),
    }
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
