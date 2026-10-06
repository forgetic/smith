//! Every retained V2 inventory and terminal path is metered by skein's allocator
//! (programming-model.md, section 6.3; domain/host.md, sections 4, 6 and 10).
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_world::domain::heap::{Counting, Meter};
use smith_host_domain::{
    self as host, Answer, Ask, CallName, Delivered, Delivery, Directory, Down, Effect, Event, Grant, Limits, Receipt,
    Reply, Request, RunResult, Start, Turn, Up,
};

#[global_allocator]
static HEAP: Counting = Counting;

fn bytes(length: u64) -> Box<[u8]> {
    vec![b'x'; usize::try_from(length).expect("small test length")].into_boxed_slice()
}
fn maximum_start(limits: Limits) -> Start {
    Start {
        logical_run: Token::new(7),
        activation: 1,
        workspace: (limits.directories > 0).then_some(Token::new(8)),
        charter: bytes(limits.charter_bytes),
        transcript: Some(bytes(limits.transcript_bytes)),
        answered: bytes(limits.answered_bytes),
        directories: (0..limits.directories)
            .map(|at| Directory {
                name: vec![
                    b'a' + u8::try_from(at).expect("few dirs");
                    usize::try_from(limits.name_bytes).expect("small")
                ]
                .into_boxed_slice(),
                writable: true,
                git: true,
                conflicts: (0..limits.conflicts)
                    .map(|index| {
                        let mut path = bytes(u64::from(limits.path_bytes));
                        *path.last_mut().expect("positive exact path cap") =
                            b'a' + u8::try_from(index).expect("finite conflict count");
                        path
                    })
                    .collect(),
            })
            .collect(),
        grants: (0..limits.accounts)
            .map(|account| Grant { account, generation: 1, valid: Duration::from_secs(100) })
            .collect(),
    }
}
fn maximum_receipts() -> Delivered {
    Delivered::new(
        (0..host::MAX_DIRECTORIES)
            .map(|directory| Receipt::new(directory, bytes(512)).expect("maximum receipt"))
            .collect(),
    )
    .expect("maximum delivered")
}

struct Measured {
    domain: host::Domain,
    env: Env<Limits>,
    out: Queue<Request>,
    meter: Meter,
    bound: u64,
    held_start: Option<Start>,
    held_messages: Vec<Box<[u8]>>,
    retain_message: bool,
}
impl Measured {
    fn new(limits: Limits, caller_reserve: u64) -> Self {
        let out = Queue::with_capacity(host::max_out(&limits));
        let held_messages = Vec::with_capacity(usize::try_from(limits.agents).expect("finite agent count"));
        let meter = Meter::new();
        let domain = host::Domain::new(&limits);
        Self {
            domain,
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            out,
            meter,
            bound: host::worst_case(&limits).expect("valid").checked_add(caller_reserve).expect("small"),
            held_start: None,
            held_messages,
            retain_message: false,
        }
    }
    fn step(&mut self, event: Event, retain_start: bool) -> Option<Token> {
        self.meter.start();
        host::step(&mut self.domain, &self.env, event, &mut self.out);
        self.drain(retain_start)
    }
    fn fire(&mut self, seconds: u64) {
        self.env.now = Time::ZERO.saturating_add(Duration::from_secs(seconds));
        while self.domain.is_due(self.env.now) {
            self.meter.start();
            host::fire(&mut self.domain, &self.env, &mut self.out);
            self.drain(false);
        }
    }
    fn drain(&mut self, retain_start: bool) -> Option<Token> {
        let measurement = self.meter.end();
        let mut owner = None;
        while let Some(request) = self.out.pop() {
            match request {
                Request::Spawn { owner: token, .. } => owner = Some(token),
                Request::Send { message, .. } => match message {
                    Down::Start { start } if retain_start => {
                        assert!(self.held_start.replace(start).is_none());
                    }
                    Down::Message { body, .. } if self.retain_message => {
                        assert!(self.held_messages.len() < self.held_messages.capacity());
                        self.held_messages.push(body);
                    }
                    Down::Start { .. }
                    | Down::Message { .. }
                    | Down::Answer { .. }
                    | Down::Acknowledge { .. }
                    | Down::Grant { .. }
                    | Down::Cancel => {}
                },
                Request::Started { .. }
                | Request::Admitted { .. }
                | Request::Called { .. }
                | Request::Withdrawn { .. }
                | Request::Turn { .. }
                | Request::Waiting { .. }
                | Request::Rejected { .. }
                | Request::Exhausted { .. }
                | Request::Told { .. }
                | Request::Answered { .. }
                | Request::Faulted { .. }
                | Request::MessageBounced { .. }
                | Request::Bounced { .. }
                | Request::Gone { .. }
                | Request::Read { .. }
                | Request::Signal { .. }
                | Request::Wait { .. }
                | Request::Reap { .. } => {}
            }
        }
        self.meter.check(measurement, self.bound, &self.env.limits);
        self.domain.reclaim();
        owner
    }
    fn spawn(&mut self, client: u64) -> Token {
        let start = maximum_start(self.env.limits);
        self.step(Event::Spawn { client: Token::new(client), start }, false).expect("valid Spawn")
    }
    fn up(&mut self, owner: Token, message: Up) {
        self.step(Event::Received { owner, message }, false);
    }
    fn start(&mut self, owner: Token) {
        self.step(Event::Spawned { owner, process: owner }, false);
        self.up(owner, Up::Admitted);
    }
    fn cleanup(&mut self, owner: Token, signals: u32, pending_send: bool) {
        if pending_send {
            self.step(Event::Sent { owner }, false);
        }
        self.step(Event::Exited { owner }, false);
        self.step(Event::Reaped { owner, detail: bytes(u64::from(self.env.limits.detail_bytes)) }, false);
        self.step(Event::Hangup { owner }, false);
        for _ in 0..signals {
            self.step(Event::Signalled { owner }, false);
        }
    }
}

#[test]
fn maximum_v2_starts_and_full_queued_replies_fit_every_slot() {
    for agents in [1, 8, 32] {
        let mut limits = smith_host_world::limits();
        limits.agents = agents;
        limits.charter_bytes = 4096;
        limits.transcript_bytes = 2048;
        limits.answered_bytes = 1024;
        let mut measured = Measured::new(limits, 0);
        let owners: Vec<Token> = (1..=u64::from(agents)).map(|client| measured.spawn(client)).collect();
        let retained = u64::from(agents) * (limits.charter_bytes + limits.transcript_bytes + limits.answered_bytes);
        assert!(measured.meter.held() >= retained, "maximum starts actually retained");
        for owner in owners {
            measured.start(owner);
            for position in 0..limits.calls {
                measured.up(
                    owner,
                    Up::Call {
                        call: Token::new(20 + u64::from(position)),
                        name: CallName { activation: 1, completion: 1, position },
                        deadline: Time::ZERO.saturating_add(Duration::from_secs(50)),
                        ask: Ask::Host {
                            tool: Box::from(&b"tool"[..]),
                            effect: Effect::Read,
                            body: bytes(limits.call_bytes),
                        },
                    },
                );
                measured.step(
                    Event::Answer {
                        agent: owner,
                        call: Token::new(20 + u64::from(position)),
                        reply: Reply::Host { error: false, body: bytes(limits.answer_bytes) },
                    },
                    false,
                );
            }
            for name in 0..limits.messages {
                measured.step(
                    Event::Message {
                        agent: owner,
                        name: Token::new(100 + u64::from(name)),
                        body: bytes(limits.message_bytes),
                    },
                    false,
                );
            }
            for number in 1..=limits.turns {
                measured.up(
                    owner,
                    Up::Turn {
                        turn: Turn {
                            number,
                            spent: u64::from(number),
                            spend_overflow: false,
                            usage_overflow: false,
                            read: None,
                            body: bytes(limits.turn_bytes),
                        },
                    },
                );
                measured.step(Event::Acknowledge { agent: owner, turn: number }, false);
            }
            for _ in 0..(1 + limits.calls + limits.messages + limits.turns) {
                measured.step(Event::Sent { owner }, false);
            }
            measured.up(
                owner,
                Up::Answer {
                    answer: Answer {
                        turns: limits.turns,
                        completions: u32::MAX,
                        input: u64::MAX,
                        output: u64::MAX,
                        cache_read: u64::MAX,
                        cache_write: u64::MAX,
                        spent: u64::from(limits.turns),
                        spend_overflow: false,
                        usage_overflow: false,
                        result: RunResult::Parked,
                    },
                },
            );
            measured.cleanup(owner, 0, false);
        }
        assert_eq!(measured.domain.agents(), 0);
        assert_eq!(measured.domain.next_deadline(), None);
    }
}

#[test]
fn maximum_start_io_ownership_coexists_with_full_pre_read_message_queue() {
    let mut limits = smith_host_world::limits();
    limits.calls = 0;
    limits.directories = 0;
    limits.accounts = 0;
    limits.charter_bytes = 65_536;
    limits.transcript_bytes = 32_768;
    limits.answered_bytes = 16_384;
    limits.message_bytes = 16_384;
    let caller = limits.charter_bytes + limits.transcript_bytes + limits.answered_bytes;
    let mut measured = Measured::new(limits, caller);
    let owner = measured.spawn(1);
    measured.step(Event::Message { agent: owner, name: Token::new(100), body: bytes(limits.message_bytes) }, false);
    measured.step(Event::Spawned { owner, process: owner }, true);
    for name in 0..limits.messages {
        measured.step(
            Event::Message { agent: owner, name: Token::new(100 + u64::from(name)), body: bytes(limits.message_bytes) },
            false,
        );
    }
    assert!(measured.held_start.is_some(), "actual IO still owns the maximum first Send");
    assert!(measured.meter.held() >= caller + u64::from(limits.messages) * limits.message_bytes);
    // The domain max and caller ownership sum are checked at every step;
    // dropping the caller's Start is a real send terminal ownership boundary.
    measured.held_start = None;
    measured.up(owner, Up::Admitted);
    measured.up(
        owner,
        Up::Answer {
            answer: Answer {
                turns: 0,
                completions: 0,
                input: 0,
                output: 0,
                cache_read: 0,
                cache_write: 0,
                spent: 0,
                spend_overflow: false,
                usage_overflow: false,
                result: RunResult::Parked,
            },
        },
    );
    measured.cleanup(owner, 0, true);
}

#[test]
fn delivery_replies_and_shutdown_rights_stay_priced() {
    let limits = smith_host_world::limits();
    let mut measured = Measured::new(limits, 0);
    let owner = measured.spawn(1);
    measured.start(owner);
    measured.step(Event::Sent { owner }, false);
    for completion in 1..=2 {
        measured.up(
            owner,
            Up::Call {
                call: Token::new(20),
                name: CallName { activation: 1, completion, position: 0 },
                deadline: Time::ZERO.saturating_add(Duration::from_secs(5)),
                ask: Ask::Deliver { fields: bytes(limits.call_bytes) },
            },
        );
        measured.step(
            Event::Answer {
                agent: owner,
                call: Token::new(20),
                reply: Reply::Delivery(Delivery::Delivered(maximum_receipts())),
            },
            false,
        );
        measured.step(Event::Sent { owner }, false);
    }
    // Keep a real parent delivery right through both tree signals and EOF;
    // disconnect's bounded withdrawal snapshot is measured too.
    measured.up(
        owner,
        Up::Call {
            call: Token::new(21),
            name: CallName { activation: 1, completion: 3, position: 0 },
            deadline: Time::ZERO.saturating_add(Duration::from_secs(5)),
            ask: Ask::Deliver { fields: bytes(limits.call_bytes) },
        },
    );
    measured.fire(5);
    measured.fire(15);
    measured.fire(17);
    measured.cleanup(owner, 2, false);
    assert_eq!(measured.domain.agents(), 1);
    measured.step(
        Event::Answer {
            agent: owner,
            call: Token::new(21),
            reply: Reply::Delivery(Delivery::Delivered(maximum_receipts())),
        },
        false,
    );
    assert_eq!(measured.domain.agents(), 0);
}

#[test]
fn checked_capacity_arithmetic_refuses_unrepresentable_or_incompatible_caps() {
    let mut limits = smith_host_world::limits();
    assert!(host::worst_case(&limits).is_some());
    limits.answer_bytes = Delivered::worst_case() - 1;
    assert_eq!(host::worst_case(&limits), None);
    limits = smith_host_world::limits();
    limits.messages = u32::MAX;
    assert_eq!(host::worst_case(&limits), None);
    limits = smith_host_world::limits();
    limits.charter_bytes = u64::MAX;
    assert_eq!(host::worst_case(&limits), None);
    limits = smith_host_world::limits();
    limits.turns = 0;
    assert_eq!(host::worst_case(&limits), None);
}

#[test]
fn a_refused_in_flight_message_body_coexists_with_the_full_new_queued_payload_budget() {
    for agents in [1, 8, 32] {
        let limits = Limits { agents, message_bytes: 16_384, ..smith_host_world::limits() };
        let caller = u64::from(agents).checked_mul(limits.message_bytes).expect("exact old bodies at caller");
        let mut measured = Measured::new(limits, caller);
        let owners: Vec<Token> = (1..=u64::from(agents)).map(|client| measured.spawn(client)).collect();
        for owner in &owners {
            measured.start(*owner);
            measured.step(Event::Sent { owner: *owner }, false);
            measured.retain_message = true;
            measured.step(
                Event::Message { agent: *owner, name: Token::new(100), body: bytes(limits.message_bytes) },
                false,
            );
            measured.retain_message = false;
            measured.up(*owner, Up::MessageBounced { name: Token::new(100), reason: host::MessageRefusal::Busy });
            for name in 0..limits.messages {
                measured.step(
                    Event::Message {
                        agent: *owner,
                        name: Token::new(u64::from(name)),
                        body: bytes(limits.message_bytes),
                    },
                    false,
                );
            }
        }
        assert_eq!(measured.held_messages.len(), usize::try_from(agents).expect("finite actual callers"));
        let full = caller
            .checked_mul(u64::from(limits.messages).checked_add(1).expect("queued plus actual old body"))
            .expect("finite payload sum");
        assert!(measured.meter.held() >= full, "actual old in-flight bodies and all new maximum queued bodies coexist");
        for owner in &owners {
            measured.step(Event::Sent { owner: *owner }, false);
        }
        // Only those actual terminals release the caller's old payload ownership.
        measured.held_messages.clear();
        for owner in owners {
            for _ in 0..limits.messages {
                measured.step(Event::Sent { owner }, false);
            }
            measured.step(Event::Stop { agent: owner }, false);
            measured.cleanup(owner, 0, true);
        }
        assert_eq!(measured.domain.agents(), 0);
    }
}
