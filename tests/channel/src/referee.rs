//! Independent channel observations and liveness expectations
//! (protocol/channel.md, section 10; testing-strategy.md, section 7).
//!
//! The referee sees submitted and received boundary events, never either
//! channel component's state. Skein owns the judgment and deadline machinery.

use std::collections::{BTreeMap, BTreeSet};

use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Expectations, Judge, Referee};

use crate::Observation;

/// Small boundary facts extracted from a protocol world's ordered observations.
#[derive(Debug)]
pub enum Seen {
    /// Host submitted one Start.
    Started,
    /// Agent submitted one named operation.
    Issued { name: (u64, u32, u32) },
    /// Host received one named operation with its live callback.
    Called { call: Token, name: (u64, u32, u32) },
    /// Host supplied one terminal for a live callback.
    Settled { call: Token },
    /// Agent received one terminal.
    Returned,
    /// Host received one numbered turn and durable body.
    Turn { number: u32, spent: u64, body: Box<[u8]> },
    /// Host received the final answer envelope.
    Answer { turns: u32, spent: u64 },
    /// Host read the channel to EOF.
    Ended,
}

/// Expected pairing, ordering and secrecy of one run's channel boundary.
#[derive(Debug)]
pub struct Meeting {
    started: bool,
    answered: bool,
    ended: bool,
    issued: BTreeSet<(u64, u32, u32)>,
    called: BTreeMap<Token, (u64, u32, u32)>,
    settled: BTreeSet<Token>,
    returned: u32,
    turns: u32,
    spent: u64,
    credential: Box<[u8]>,
}

impl Meeting {
    /// Build a referee that also excludes a known credential from turn bodies.
    #[must_use]
    pub fn new(credential: Box<[u8]>) -> Meeting {
        Meeting {
            started: false,
            answered: false,
            ended: false,
            issued: BTreeSet::new(),
            called: BTreeMap::new(),
            settled: BTreeSet::new(),
            returned: 0,
            turns: 0,
            spent: 0,
            credential,
        }
    }
}

impl Expectations for Meeting {
    type Seen = Seen;
    type Name = &'static str;
    type Stimulus = ();

    fn observe(&mut self, seen: Seen, judge: &mut Judge<Self::Name, Self::Stimulus>) {
        match seen {
            Seen::Started => {
                judge.check(!self.started, "one Start per channel");
                if self.started {
                    return;
                }
                self.started = true;
                judge.expect("answer", Duration::from_secs(3600));
            }
            Seen::Issued { name } => {
                judge.check(self.started && !self.answered, "calls belong to a live Start");
                judge.check(self.issued.insert(name), "each durable call name is issued once while live");
            }
            Seen::Called { call, name } => {
                judge.check(self.issued.contains(&name), "host receives the exact agent-issued call name");
                judge.check(self.called.insert(call, name).is_none(), "one host callback per call");
            }
            Seen::Settled { call } => {
                judge.check(self.called.contains_key(&call), "host terminal belongs to a received call");
                judge.check(self.settled.insert(call), "one host terminal per call");
            }
            Seen::Returned => {
                self.returned = self.returned.saturating_add(1);
                judge.check(
                    self.returned <= u32::try_from(self.settled.len()).unwrap_or(u32::MAX),
                    "agent return follows a host terminal",
                );
            }
            Seen::Turn { number, spent, body } => {
                judge.check(number == self.turns.saturating_add(1), "turns are consecutive within an activation");
                judge.check(spent >= self.spent, "turn spend never falls");
                if !self.credential.is_empty() {
                    judge.check(
                        !body.windows(self.credential.len()).any(|window| window == self.credential.as_ref()),
                        "credential value stays out of turn bytes",
                    );
                }
                self.turns = number;
                self.spent = spent;
            }
            Seen::Answer { turns, spent } => {
                judge.check(self.started && !self.answered, "one answer per started run");
                if self.answered {
                    return;
                }
                judge.check(turns == self.turns && spent >= self.spent, "answer covers all turns and spend");
                judge.check(
                    self.issued.len() == self.called.len() && self.called.len() == self.settled.len(),
                    "every issued call reached the host and received one terminal",
                );
                judge.check(
                    self.returned == u32::try_from(self.settled.len()).unwrap_or(u32::MAX),
                    "every host terminal reached the agent",
                );
                self.answered = true;
                let met = judge.meet(&"answer");
                judge.check(met, "final answer meets the Start obligation");
                judge.expect("end", Duration::from_secs(3600));
            }
            Seen::Ended => {
                judge.check(self.answered && !self.ended, "EOF follows one final answer");
                self.ended = true;
                let met = judge.meet(&"end");
                judge.check(met, "EOF meets the channel-end obligation");
            }
        }
    }
}

/// Feed a world's boundary observations through Skein's shared referee.
#[must_use]
pub fn review(observations: &[Observation], credential: Box<[u8]>) -> Referee<Meeting> {
    let mut referee = Referee::new(Meeting::new(credential));
    let mut stimuli = Vec::new();
    for observation in observations {
        let seen = match observation {
            Observation::HostStarted => Some(Seen::Started),
            Observation::AgentIssuedCall { name } => {
                Some(Seen::Issued { name: (name.activation, name.completion, name.position) })
            }
            Observation::HostCall { call, name, .. } => {
                Some(Seen::Called { call: *call, name: (name.activation, name.completion, name.position) })
            }
            Observation::HostSettledCall { call } => Some(Seen::Settled { call: *call }),
            Observation::AgentHostReturned { .. } | Observation::AgentDelivered { .. } => Some(Seen::Returned),
            Observation::HostTurn { number, spent, body, .. } => {
                Some(Seen::Turn { number: *number, spent: *spent, body: body.clone() })
            }
            Observation::HostAnswer { turns, spent } => Some(Seen::Answer { turns: *turns, spent: *spent }),
            Observation::HostEnded(skein_channel::Closed::Stream) => Some(Seen::Ended),
            Observation::HostOpened(_)
            | Observation::AgentOpened(_)
            | Observation::HostEnded(_)
            | Observation::HostSent(_)
            | Observation::HostUnsent(..)
            | Observation::AgentEnded(_)
            | Observation::AgentStart
            | Observation::AgentContext { .. }
            | Observation::AgentSavedTooLarge
            | Observation::AgentSavedHost { .. }
            | Observation::AgentMessage { .. }
            | Observation::HostWaiting { .. }
            | Observation::HostLong { .. }
            | Observation::HostLongDone
            | Observation::HostFact { .. }
            | Observation::HostRejected { .. }
            | Observation::HostExhausted { .. }
            | Observation::AgentCancel
            | Observation::AgentWriteFailed
            | Observation::HostWriteFailed
            | Observation::AgentAcknowledged { .. }
            | Observation::HostWithdraw { .. }
            | Observation::AgentDeliveryUnsent { .. }
            | Observation::InvalidStart { .. }
            | Observation::HostAdmitted
            | Observation::HostParked { .. }
            | Observation::AgentHistory { .. }
            | Observation::TranscriptFailed(_)
            | Observation::AgentGrant { .. } => None,
        };
        if let Some(seen) = seen {
            referee.observe(Time::ZERO, seen, &mut stimuli);
        }
    }
    referee
}
