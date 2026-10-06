//! V2 supervision cells and surviving terminal ledgers (domain/host.md, sections
//! 2–7). Process exit never discards parent calls or turn ACKs.
//! The kit knows channel send order, not private agent stop
//! decisions; policy and durable decisions remain in the parent.
use crate::{
    Ask, Bounce, CallName, Down, End, Event, Fact, Fault, Grant, Invalid, Limits, Reply, Request, RunFailure,
    RunResult, Signal, Start, Up,
};
use alloc::boxed::Box;
use skein_lib::{Deadlines, Env, Id, Map, Queue, Slab, Time, Token};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Spawning,
    Starting,
    Live,
    Cancelled,
    Draining,
    Exiting,
    Terminating,
    Killing,
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Alarm {
    Watch { agent: Id<Agent> },
    Wall { agent: Id<Agent> },
    Grace { agent: Id<Agent> },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Host,
    Delivery,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CallStage {
    Parent,
    Queued,
    Sending,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Call {
    name: CallName,
    deadline: Time,
    kind: Kind,
    stage: CallStage,
    withdrawn: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TurnStage {
    Parent,
    Queued,
    Sending,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct TurnMeta {
    bytes: u64,
    stage: TurnStage,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Account {
    latest: Grant,
    emitted: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Sent {
    Start,
    Message,
    Call(Token),
    Busy { call: Token, name: CallName },
    Ack(u32),
    Grant,
    Cancel,
}

#[derive(Debug)]
#[expect(clippy::struct_excessive_bools, reason = "independent channel and process state spans finite phases")]
pub(crate) struct Agent {
    client: Token,
    logical_run: Token,
    activation: u64,
    phase: Phase,
    start: Option<Start>,
    process: Option<Token>,
    admitted: bool,
    last_word: bool,
    told: bool,
    owed: Option<Fault>,
    host_stopped: bool,
    diagnostic: Option<Fault>,
    reading: bool,
    eof: bool,
    sending: Option<Sent>,
    signals: u32,
    exited: bool,
    reaped: Option<Box<[u8]>>,
    outbox: Queue<Down>,
    busy: Queue<Sent>,
    calls: Map<Token, Call>,
    turns: Map<u32, TurnMeta>,
    accounts: Map<u32, Account>,
    messages: u32,
    unread: Queue<Token>,
    read: Option<Token>,
    number: u32,
    spent: u64,
    turn_bytes: u64,
    cancel_queued: bool,
    cancel_sent: bool,
    progress: Time,
    long: Time,
    // Paused only while the run waits with no message still queued or unread.
    waiting: bool,
    paused: bool,
    wall: Time,
    until: Time,
}

/// Bounded process/channel state, parent terminal rights and content-free facts;
/// no credentials, durable host policy or frame bytes (domain/host.md, sections 2–7).
#[derive(Debug)]
pub struct Domain {
    agents: Slab<Agent>,
    alarms: Deadlines<Alarm>,
    facts: Queue<Fact>,
    lost: u64,
}

impl Domain {
    /// Allocate once from validated bounds; invalid arithmetic/turn/delivery capacity
    /// panics before any process effect (domain/host.md, sections 3, 4 and 6).
    #[must_use]
    pub fn new(limits: &Limits) -> Domain {
        assert!(crate::worst_case(limits).is_some(), "host limits are representable and every sealed delivery fits");
        Domain {
            agents: Slab::<Agent>::with_capacity(limits.agents),
            alarms: Deadlines::with_capacity(limits.agents.checked_mul(3).expect("validated alarms")),
            facts: Queue::with_capacity(limits.facts),
            lost: 0,
        }
    }

    /// Retired slots count until iteration-boundary reclaim (domain/host.md, section 4).
    #[must_use]
    pub fn agents(&self) -> u32 {
        self.agents.len()
    }

    /// Pure earliest armed monotonic deadline (domain/host.md, section 4).
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        self.alarms.next()
    }

    /// Pure scheduling query; expiration itself occurs in fire (domain/host.md, section 4).
    #[must_use]
    pub fn is_due(&self, now: Time) -> bool {
        match self.alarms.next() {
            Some(at) => at <= now,
            None => false,
        }
    }

    /// Reclaim only after parent routed outputs for this iteration (domain/host.md, section 4).
    pub fn reclaim(&mut self) {
        self.agents.reclaim();
    }

    /// Drain one best-effort observation; decisions never depend on it (domain/host.md, section 4).
    pub fn pop_fact(&mut self) -> Option<Fact> {
        self.facts.pop()
    }

    /// Saturating dropped diagnostic count; not a terminal (domain/host.md, section 4).
    #[must_use]
    pub const fn facts_lost(&self) -> u64 {
        self.lost
    }
}

/// Reserve these free output slots for one step/fire: full call withdrawal fanout,
/// lower/process notifications and the next Send/Read; payload bytes priced
/// separately (domain/host.md, sections 2 and 4).
#[must_use]
pub fn max_out(limits: &Limits) -> u32 {
    limits.calls.saturating_add(8)
}

/// Consume one parent notice or actual lower terminal with injected clocks.
/// Caller reserves `max_out` free slots and returns every operation terminal,
/// even after cancellation/EOF (domain/host.md, sections 2–7).
#[expect(clippy::too_many_lines, reason = "one exhaustive typed boundary keeps terminal ownership visible")]
pub fn step(domain: &mut Domain, env: &Env<Limits>, event: Event, out: &mut Queue<Request>) {
    let before = out.len();
    let token = match &event {
        Event::Spawn { .. } => {
            spawn(domain, env, event, out);
            return;
        }
        Event::Message { agent, .. }
        | Event::Answer { agent, .. }
        | Event::Acknowledge { agent, .. }
        | Event::Grant { agent, .. }
        | Event::Stop { agent } => *agent,
        Event::Spawned { owner, .. }
        | Event::Unspawned { owner, .. }
        | Event::Sent { owner }
        | Event::Unsent { owner }
        | Event::Received { owner, .. }
        | Event::Malformed { owner }
        | Event::Hangup { owner }
        | Event::Signalled { owner }
        | Event::Exited { owner }
        | Event::Reaped { owner, .. } => *owner,
    };
    let id = Id::from_token(token);
    let Some(agent) = domain.agents.get_mut(id) else {
        return;
    };
    if agent.phase == Phase::Closed {
        return;
    }
    match event {
        Event::Spawn { .. } => unreachable!("spawn handled before lookup"),
        Event::Spawned { process, .. } => {
            assert!(agent.phase == Phase::Spawning, "one terminal per process spawn");
            agent.process = Some(process);
            agent.phase = Phase::Starting;
            agent.progress = env.now;
            agent.wall = env.now.saturating_add(env.limits.wall_time);
            out.push(Request::Started { client: agent.client, agent: token });
            let start = agent.start.take().expect("spawn retains first message");
            for grant in &start.grants {
                agent.accounts.get_mut(&grant.account).expect("validated start grant").emitted = grant.generation;
            }
            out.push(Request::Send { owner: token, process, message: Down::Start { start } });
            agent.sending = Some(Sent::Start);
            out.push(Request::Wait { owner: token, process });
            out.push(Request::Reap { owner: token, process });
        }
        Event::Unspawned { detail, .. } => {
            assert!(agent.phase == Phase::Spawning, "one terminal per spawn");
            agent.phase = Phase::Closed;
            agent.start = None;
            out.push(Request::Gone {
                client: agent.client,
                end: End::Unspawned,
                detail: tail(&detail, env.limits.detail_bytes),
            });
        }
        Event::Message { name, body, .. } => message(agent, name, body, env, out),
        Event::Answer { call, reply, .. } => answered(agent, call, reply, env),
        Event::Acknowledge { turn, .. } => acknowledge_turn(agent, turn),
        Event::Grant { grant, .. } => grant_name(agent, grant),
        Event::Stop { .. } => {
            match agent.phase {
                Phase::Starting | Phase::Live | Phase::Cancelled | Phase::Draining => agent.host_stopped = true,
                Phase::Spawning | Phase::Exiting | Phase::Terminating | Phase::Killing | Phase::Closed => {}
            }
            if agent.owed == Some(Fault::WallTime) || agent.owed == Some(Fault::Exited) {
                agent.owed = None;
            }
            polite(agent, env, None);
        }
        Event::Sent { .. } | Event::Unsent { .. } => {
            let unsent = match event {
                Event::Unsent { .. } => true,
                Event::Sent { .. } => false,
                Event::Spawn { .. }
                | Event::Spawned { .. }
                | Event::Unspawned { .. }
                | Event::Message { .. }
                | Event::Answer { .. }
                | Event::Acknowledge { .. }
                | Event::Grant { .. }
                | Event::Stop { .. }
                | Event::Received { .. }
                | Event::Malformed { .. }
                | Event::Hangup { .. }
                | Event::Signalled { .. }
                | Event::Exited { .. }
                | Event::Reaped { .. } => unreachable!("send terminal branch"),
            };
            let sent = agent.sending.take().expect("send terminal ends one pending send");
            settle_send(agent, sent);
            if unsent && !agent.last_word {
                draining(agent, env);
            }
        }
        Event::Received { message, .. } => {
            assert!(agent.reading, "record ends one requested read");
            agent.reading = false;
            receive(agent, token, message, env, out);
        }
        Event::Malformed { .. } => {
            assert!(agent.reading, "malformed terminal ends read");
            agent.reading = false;
            agent.eof = true;
            fail(agent, token, Fault::Rules, env, out);
        }
        Event::Hangup { .. } => {
            assert!(agent.reading, "EOF ends pending read");
            agent.reading = false;
            agent.eof = true;
            if !agent.last_word {
                match agent.phase {
                    Phase::Cancelled | Phase::Draining => {
                        if let Some(fault) = agent.owed.take() {
                            report_fault(agent, fault, out);
                        }
                        agent.phase = Phase::Exiting;
                    }
                    Phase::Starting | Phase::Live => fail(agent, token, Fault::Exited, env, out),
                    Phase::Exiting | Phase::Terminating | Phase::Killing => {}
                    Phase::Spawning | Phase::Closed => unreachable!("spawned process reads"),
                }
            }
        }
        Event::Signalled { .. } => {
            agent.signals = agent.signals.checked_sub(1).expect("one terminal per signal");
        }
        Event::Exited { .. } => {
            assert!(!agent.exited, "one terminal per Wait");
            agent.exited = true;
            if !agent.last_word {
                draining(agent, env);
            }
        }
        Event::Reaped { detail, .. } => {
            assert!(agent.exited && agent.reaped.is_none(), "Reap follows Exited once");
            agent.reaped = Some(tail(&detail, env.limits.detail_bytes));
        }
    }
    follow(domain, env, id, out);
    observations(domain, out, before);
}

/// Expire one due timer; repeat while due with `max_out` free slots. Wall never
/// pauses with progress credit (domain/host.md, sections 4 and 6).
pub fn fire(domain: &mut Domain, env: &Env<Limits>, out: &mut Queue<Request>) {
    let before = out.len();
    let Some(alarm) = domain.alarms.expire(env.now) else {
        return;
    };
    let id = match alarm {
        Alarm::Watch { agent } | Alarm::Wall { agent } | Alarm::Grace { agent } => agent,
    };
    let agent = domain.agents.get_mut(id).expect("armed agent lives");
    match alarm {
        Alarm::Watch { .. } => {
            if pause(agent, env) {
                agent.paused = true;
            } else if agent.paused {
                agent.paused = false;
                agent.progress = env.now;
            } else {
                fail(agent, id.token(), Fault::NoProgress, env, out);
            }
        }
        Alarm::Wall { .. } => polite(agent, env, Some(Fault::WallTime)),
        Alarm::Grace { .. } => {
            if let Some(fault) = agent.owed.take() {
                report_fault(agent, fault, out);
            }
            match agent.phase {
                Phase::Cancelled | Phase::Draining | Phase::Exiting => terminate(agent, id.token(), env, out),
                Phase::Terminating => {
                    signal(agent, id.token(), Signal::Kill, out);
                    agent.phase = Phase::Killing;
                }
                Phase::Spawning | Phase::Starting | Phase::Live | Phase::Killing | Phase::Closed => {
                    unreachable!("grace belongs to shutdown")
                }
            }
        }
    }
    follow(domain, env, id, out);
    observations(domain, out, before);
}

#[expect(clippy::manual_map, reason = "explicit admission cases follow programming-model.md, section 10")]
fn spawn(domain: &mut Domain, env: &Env<Limits>, event: Event, out: &mut Queue<Request>) {
    let (client, start) = match event {
        Event::Spawn { client, start } => (client, start),
        Event::Spawned { .. }
        | Event::Unspawned { .. }
        | Event::Message { .. }
        | Event::Answer { .. }
        | Event::Acknowledge { .. }
        | Event::Grant { .. }
        | Event::Stop { .. }
        | Event::Sent { .. }
        | Event::Unsent { .. }
        | Event::Received { .. }
        | Event::Malformed { .. }
        | Event::Hangup { .. }
        | Event::Signalled { .. }
        | Event::Exited { .. }
        | Event::Reaped { .. } => unreachable!("only Spawn enters admission"),
    };
    let refusal = if domain.agents.is_full() {
        Some(End::Busy)
    } else {
        match valid_start(&start, &env.limits) {
            Some(invalid) => Some(End::Invalid(invalid)),
            None => None,
        }
    };
    if let Some(end) = refusal {
        out.push(Request::Gone { client, end, detail: Box::new([]) });
        observation(domain, Fact::Gone { client, end });
        return;
    }
    let mut accounts = Map::with_capacity(env.limits.accounts);
    for grant in &start.grants {
        accounts.insert(grant.account, Account { latest: *grant, emitted: 0 }).expect("validated unique grant names");
    }
    let workspace = start.workspace;
    let logical_run = start.logical_run;
    let activation = start.activation;
    let agent = Agent {
        client,
        logical_run,
        activation,
        phase: Phase::Spawning,
        start: Some(start),
        process: None,
        admitted: false,
        last_word: false,
        told: false,
        owed: None,
        host_stopped: false,
        diagnostic: None,
        reading: false,
        eof: false,
        sending: None,
        signals: 0,
        exited: false,
        reaped: None,
        outbox: Queue::with_capacity(crate::limits::outbox(&env.limits).expect("validated outbox")),
        busy: Queue::with_capacity(2),
        calls: Map::with_capacity(env.limits.calls),
        turns: Map::with_capacity(env.limits.turns),
        accounts,
        messages: 0,
        unread: Queue::with_capacity(env.limits.messages),
        read: None,
        number: 0,
        spent: 0,
        turn_bytes: 0,
        cancel_queued: false,
        cancel_sent: false,
        progress: env.now,
        long: env.now,
        waiting: false,
        paused: false,
        wall: env.now,
        until: env.now,
    };
    let id = domain.agents.insert(agent).expect("slot admitted");
    out.push(Request::Spawn {
        owner: id.token(),
        workspace,
        deadline: env.now.saturating_add(env.limits.spawn_timeout),
    });
}

fn valid_start(start: &Start, limits: &Limits) -> Option<Invalid> {
    if start.activation == 0 {
        return Some(Invalid::Limits);
    }
    if !within(&start.charter, limits.charter_bytes) {
        return Some(Invalid::Charter);
    }
    match &start.transcript {
        Some(transcript) if !within(transcript, limits.transcript_bytes) => return Some(Invalid::Transcript),
        Some(_) | None => {}
    }
    if !within(&start.answered, limits.answered_bytes) {
        return Some(Invalid::Answered);
    }
    if start.directories.len() > usize::try_from(limits.directories).expect("u32 fits usize") {
        return Some(Invalid::Directories);
    }
    match start.workspace {
        Some(_) if start.directories.is_empty() => return Some(Invalid::Directories),
        None if !start.directories.is_empty() => return Some(Invalid::Directories),
        Some(_) | None => {}
    }
    if limits.path_bytes > 4096 {
        return Some(Invalid::Directories);
    }
    for directory in &start.directories {
        if !within(&directory.name, u64::from(limits.name_bytes))
            || directory.conflicts.len() > usize::try_from(limits.conflicts).expect("u32 fits usize")
        {
            return Some(Invalid::Directories);
        }
        for path in &directory.conflicts {
            if !within(path, u64::from(limits.path_bytes)) {
                return Some(Invalid::Directories);
            }
        }
    }
    for (at, directory) in start.directories.iter().enumerate() {
        if !safe_name(&directory.name) || (!directory.git && !directory.conflicts.is_empty()) {
            return Some(Invalid::Directories);
        }
        for earlier in start.directories.get(..at).expect("enumerated directory") {
            if directory.name == earlier.name {
                return Some(Invalid::Directories);
            }
        }
        for (position, path) in directory.conflicts.iter().enumerate() {
            if !relative(path, limits.path_bytes) {
                return Some(Invalid::Directories);
            }
            for earlier in directory.conflicts.get(..position).expect("enumerated conflict") {
                if earlier == path {
                    return Some(Invalid::Directories);
                }
            }
        }
    }
    if start.grants.len() > usize::try_from(limits.accounts).expect("u32 fits usize") {
        return Some(Invalid::Grants);
    }
    for (at, grant) in start.grants.iter().enumerate() {
        if grant.generation == 0 {
            return Some(Invalid::Grants);
        }
        for earlier in start.grants.get(..at).expect("enumerated grant") {
            if grant.account == earlier.account {
                return Some(Invalid::Grants);
            }
        }
    }
    None
}

fn safe_name(name: &[u8]) -> bool {
    if name.is_empty() || name == b"." || name == b".." {
        return false;
    }
    for byte in name {
        if *byte == 0 || *byte == b'/' {
            return false;
        }
    }
    true
}

fn within(bytes: &[u8], limit: u64) -> bool {
    u64::try_from(bytes.len()).expect("length fits u64") <= limit
}

fn relative(path: &[u8], limit: u32) -> bool {
    if path.is_empty() || !within(path, u64::from(limit)) {
        return false;
    }
    let mut from = 0;
    for (at, byte) in path.iter().enumerate() {
        if *byte == 0 {
            return false;
        }
        if *byte == b'/' {
            let part = path.get(from..at).expect("ordered path positions");
            if part.is_empty() || part == b"." || part == b".." {
                return false;
            }
            from = at.checked_add(1).expect("bounded path");
        }
    }
    let part = path.get(from..).expect("bounded path suffix");
    !part.is_empty() && part != b"." && part != b".."
}

fn tail(detail: &[u8], limit: u32) -> Box<[u8]> {
    let keep = detail.len().min(usize::try_from(limit).expect("u32 fits usize"));
    let from = detail.len().checked_sub(keep).expect("bounded tail");
    Box::from(detail.get(from..).expect("bounded suffix"))
}

fn accepts(agent: &Agent) -> bool {
    match agent.phase {
        Phase::Starting | Phase::Live => true,
        Phase::Spawning
        | Phase::Cancelled
        | Phase::Draining
        | Phase::Exiting
        | Phase::Terminating
        | Phase::Killing
        | Phase::Closed => false,
    }
}

fn listens(agent: &Agent) -> bool {
    match agent.phase {
        Phase::Starting | Phase::Live | Phase::Cancelled => !agent.eof && !agent.exited && !agent.last_word,
        Phase::Spawning | Phase::Draining | Phase::Exiting | Phase::Terminating | Phase::Killing | Phase::Closed => {
            false
        }
    }
}

fn named_message(agent: &Agent, name: Token) -> bool {
    if agent.read == Some(name) {
        return true;
    }
    for old in &agent.unread {
        if *old == name {
            return true;
        }
    }
    for queued in &agent.outbox {
        match queued {
            Down::Message { name: old, .. } => {
                if *old == name {
                    return true;
                }
            }
            Down::Start { .. } | Down::Answer { .. } | Down::Acknowledge { .. } | Down::Grant { .. } | Down::Cancel => {
            }
        }
    }
    false
}

fn message(agent: &mut Agent, name: Token, body: Box<[u8]>, env: &Env<Limits>, out: &mut Queue<Request>) {
    let bounce = if !within(&body, env.limits.message_bytes) {
        Some(Bounce::TooLarge)
    } else if !accepts(agent) {
        Some(Bounce::Ending)
    } else if named_message(agent, name) {
        Some(Bounce::ReusedName)
    } else if agent.messages.saturating_add(agent.unread.len()) >= env.limits.messages {
        Some(Bounce::Full)
    } else {
        None
    };
    if let Some(bounce) = bounce {
        out.push(Request::Bounced { client: agent.client, name, bounce });
        return;
    }
    agent.messages = agent.messages.checked_add(1).expect("bounded messages");
    agent.waiting = false;
    agent.outbox.push(Down::Message { name, body });
}

fn known_read(agent: &Agent, read: Option<Token>) -> bool {
    match read {
        None => agent.read.is_none(),
        Some(name) => {
            if agent.read == Some(name) {
                return true;
            }
            for sent in &agent.unread {
                if *sent == name {
                    return true;
                }
            }
            false
        }
    }
}

fn mark_read(agent: &mut Agent, read: Option<Token>) {
    match read {
        Some(name) if agent.read != Some(name) => {
            let count = agent.unread.len();
            for _ in 0..count {
                let sent = agent.unread.pop().expect("known read prefix");
                agent.read = Some(sent);
                if sent == name {
                    break;
                }
            }
        }
        Some(_) | None => {}
    }
}

fn acknowledge_turn(agent: &mut Agent, number: u32) {
    let Some(turn) = agent.turns.get_mut(&number) else {
        assert!(number <= agent.number, "parent ACK names a forwarded turn");
        return;
    };
    match turn.stage {
        TurnStage::Parent => {
            turn.stage = TurnStage::Queued;
            agent.turn_bytes = agent.turn_bytes.checked_sub(turn.bytes).expect("outstanding turn bytes");
            if listens(agent) {
                agent.outbox.push(Down::Acknowledge { turn: number });
            } else {
                agent.turns.remove(&number).expect("exact ACK metadata");
            }
        }
        TurnStage::Queued | TurnStage::Sending => {}
    }
}

fn grant_name(agent: &mut Agent, grant: Grant) {
    if !listens(agent) {
        return;
    }
    let Some(known) = agent.accounts.get_mut(&grant.account) else {
        return;
    };
    if grant.generation <= known.latest.generation {
        return;
    }
    known.latest = grant;
    let count = agent.outbox.len();
    let mut replaced = false;
    for _ in 0..count {
        let old = agent.outbox.pop().expect("stable bounded outbox");
        let next = match old {
            Down::Grant { grant: old } if old.account == grant.account => {
                replaced = true;
                Down::Grant { grant }
            }
            old @ (Down::Grant { .. }
            | Down::Start { .. }
            | Down::Message { .. }
            | Down::Answer { .. }
            | Down::Acknowledge { .. }
            | Down::Cancel) => old,
        };
        agent.outbox.push(next);
    }
    if !replaced {
        agent.outbox.push(Down::Grant { grant });
    }
}

fn answered(agent: &mut Agent, callback: Token, reply: Reply, env: &Env<Limits>) {
    let call = *agent.calls.get(&callback).expect("parent returns one known operation right");
    assert!(call.stage == CallStage::Parent, "one actual parent answer per call");
    let reply = match reply {
        Reply::Delivery(delivery) => {
            assert!(call.kind == Kind::Delivery, "delivery terminal matches delivery call");
            Reply::Delivery(delivery)
        }
        Reply::Host { error, body } => {
            assert!(call.kind == Kind::Host, "tool terminal matches tool call");
            if within(&body, env.limits.answer_bytes) { Reply::Host { error, body } } else { Reply::TooLarge }
        }
        reply @ (Reply::Busy | Reply::Unavailable | Reply::Withdrawn | Reply::TooLarge) => {
            assert!(call.kind == Kind::Host, "submitted delivery owes its full actual terminal");
            reply
        }
    };
    if listens(agent) {
        agent.calls.get_mut(&callback).expect("known call").stage = CallStage::Queued;
        agent.outbox.push(Down::Answer { call: callback, reply });
    } else {
        agent.calls.remove(&callback).expect("actual parent right settled");
    }
}

fn receive(agent: &mut Agent, owner: Token, message: Up, env: &Env<Limits>, out: &mut Queue<Request>) {
    if agent.last_word {
        fail(agent, owner, Fault::Rules, env, out);
        return;
    }
    if agent.told || discarded_work(agent, &message) {
        return;
    }
    if too_large(&message, &env.limits) {
        fail(agent, owner, Fault::TooLarge, env, out);
        return;
    }
    if !valid_record(agent, &message, env) {
        fail(agent, owner, Fault::Rules, env, out);
        return;
    }
    agent.progress = env.now;
    match message {
        Up::Admitted => {
            agent.admitted = true;
            if agent.phase == Phase::Starting {
                agent.phase = Phase::Live;
            }
            out.push(Request::Admitted { client: agent.client });
        }
        Up::Call { call, name, deadline, ask } => {
            let kind = match &ask {
                Ask::Host { .. } => Kind::Host,
                Ask::Deliver { .. } => Kind::Delivery,
            };
            agent.waiting = false;
            if agent.calls.len() >= env.limits.calls {
                agent.busy.push(Sent::Busy { call, name });
            } else {
                agent
                    .calls
                    .insert(call, Call { name, deadline, kind, stage: CallStage::Parent, withdrawn: false })
                    .expect("call admitted within capacity");
                out.push(Request::Called {
                    client: agent.client,
                    logical_run: agent.logical_run,
                    call,
                    name,
                    deadline,
                    ask,
                });
            }
        }
        Up::Withdraw { call } => match agent.calls.get_mut(&call) {
            Some(entry) if entry.stage == CallStage::Parent => {
                entry.withdrawn = true;
                out.push(Request::Withdrawn { client: agent.client, call });
            }
            Some(_) | None => {}
        },
        Up::Turn { turn } => {
            agent.waiting = false;
            mark_read(agent, turn.read);
            agent.number = turn.number;
            agent.spent = turn.spent;
            let bytes = u64::try_from(turn.body.len()).expect("bounded turn length");
            agent.turn_bytes = agent.turn_bytes.checked_add(bytes).expect("validated byte credit");
            agent
                .turns
                .insert(turn.number, TurnMeta { bytes, stage: TurnStage::Parent })
                .expect("maximum turn room reserved before read");
            out.push(Request::Turn { client: agent.client, turn });
        }
        Up::Waiting { read } => {
            mark_read(agent, read);
            agent.waiting = agent.messages == 0 && agent.unread.is_empty();
            out.push(Request::Waiting { client: agent.client, read });
        }
        Up::Long { span } => {
            agent.waiting = false;
            agent.long = agent.long.max(env.now.saturating_add(span));
        }
        Up::LongDone => {
            agent.long = env.now;
        }
        Up::Fact { body } => out.push(Request::Told { client: agent.client, body }),
        Up::Rejected { account, generation } => {
            out.push(Request::Rejected { client: agent.client, account, generation });
        }
        Up::Exhausted { account, retry_after } => {
            out.push(Request::Exhausted { client: agent.client, account, retry_after });
        }
        Up::Answer { answer } => heard_answer(agent, answer, env, out),
    }
}

fn discarded_work(agent: &Agent, message: &Up) -> bool {
    match agent.phase {
        Phase::Terminating | Phase::Killing => match message {
            Up::Answer { .. } => false,
            Up::Admitted
            | Up::Call { .. }
            | Up::Withdraw { .. }
            | Up::Turn { .. }
            | Up::Fact { .. }
            | Up::Long { .. }
            | Up::LongDone
            | Up::Waiting { .. }
            | Up::Rejected { .. }
            | Up::Exhausted { .. } => true,
        },
        Phase::Spawning
        | Phase::Starting
        | Phase::Live
        | Phase::Cancelled
        | Phase::Draining
        | Phase::Exiting
        | Phase::Closed => false,
    }
}

fn heard_answer(agent: &mut Agent, answer: crate::Answer, env: &Env<Limits>, out: &mut Queue<Request>) {
    agent.last_word = true;
    agent.spent = answer.spent;
    let wall_cancel = match &answer.result {
        RunResult::Failed { failure: RunFailure::Cancelled } => agent.owed == Some(Fault::WallTime),
        RunResult::Refused { .. } | RunResult::Accepted { .. } | RunResult::Parked | RunResult::Failed { .. } => false,
    };
    if wall_cancel {
        report_fault(agent, Fault::WallTime, out);
    } else {
        agent.told = true;
        out.push(Request::Answered { client: agent.client, answer });
    }
    agent.owed = None;
    match agent.phase {
        Phase::Starting | Phase::Live => agent.until = env.now.saturating_add(env.limits.grace),
        Phase::Cancelled | Phase::Draining | Phase::Exiting | Phase::Terminating | Phase::Killing => {}
        Phase::Spawning | Phase::Closed => unreachable!("answer after process start"),
    }
    if agent.phase != Phase::Terminating && agent.phase != Phase::Killing {
        agent.phase = Phase::Exiting;
    }
}

fn too_large(message: &Up, limits: &Limits) -> bool {
    match message {
        Up::Call { ask, .. } => match ask {
            Ask::Host { tool, body, .. } => {
                !within(tool, u64::from(limits.name_bytes)) || !within(body, limits.call_bytes)
            }
            Ask::Deliver { fields } => !within(fields, limits.call_bytes),
        },
        Up::Turn { turn } => !within(&turn.body, limits.turn_bytes),
        Up::Fact { body } => !within(body, limits.fact_bytes),
        Up::Answer { answer } => match &answer.result {
            RunResult::Refused { detail } => !within(detail, limits.outcome_bytes),
            RunResult::Accepted { outcome } => !within(outcome, limits.outcome_bytes),
            RunResult::Parked | RunResult::Failed { .. } => false,
        },
        Up::Admitted
        | Up::Withdraw { .. }
        | Up::Long { .. }
        | Up::LongDone
        | Up::Waiting { .. }
        | Up::Rejected { .. }
        | Up::Exhausted { .. } => false,
    }
}

fn valid_record(agent: &Agent, message: &Up, env: &Env<Limits>) -> bool {
    let limits = &env.limits;
    match message {
        Up::Admitted => !agent.admitted && !agent.last_word,
        Up::Answer { answer } => valid_answer(agent, answer, limits),
        Up::Call { call, name, deadline, ask } => {
            if !agent.admitted || name.activation != agent.activation || name.completion == 0 || *deadline < env.now {
                return false;
            }
            let kind = match ask {
                Ask::Host { tool, body, .. } => {
                    if tool.is_empty()
                        || !within(tool, u64::from(limits.name_bytes))
                        || !within(body, limits.call_bytes)
                    {
                        return false;
                    }
                    Kind::Host
                }
                Ask::Deliver { fields } => {
                    if !within(fields, limits.call_bytes) {
                        return false;
                    }
                    Kind::Delivery
                }
            };
            if agent.calls.contains_key(call) {
                return false;
            }
            for (_, old) in &agent.calls {
                if old.name == *name || (old.kind == Kind::Delivery && kind == Kind::Delivery) {
                    return false;
                }
            }
            for busy in &agent.busy {
                match busy {
                    Sent::Busy { call: old, name: old_name } => {
                        if old == call || old_name == name {
                            return false;
                        }
                    }
                    Sent::Start | Sent::Message | Sent::Call(_) | Sent::Ack(_) | Sent::Grant | Sent::Cancel => {
                        unreachable!("busy queue only stores overflow rights")
                    }
                }
            }
            match agent.sending {
                Some(Sent::Busy { call: old, name: old_name }) => {
                    if old == *call || old_name == *name {
                        return false;
                    }
                }
                Some(Sent::Start | Sent::Message | Sent::Call(_) | Sent::Ack(_) | Sent::Grant | Sent::Cancel)
                | None => {}
            }
            true
        }
        Up::Withdraw { call } => {
            if !agent.admitted {
                return false;
            }
            match agent.calls.get(call) {
                Some(entry) => !entry.withdrawn || entry.stage != CallStage::Parent,
                None => true,
            }
        }
        Up::Turn { turn } => {
            agent.admitted
                && agent.number.checked_add(1) == Some(turn.number)
                && valid_spend(agent, turn.spent)
                && known_read(agent, turn.read)
                && within(&turn.body, limits.turn_bytes)
                && agent.turns.len() < limits.turns
                && match agent.turn_bytes.checked_add(u64::try_from(turn.body.len()).expect("bounded payload length")) {
                    Some(bytes) => bytes <= limits.unacknowledged_bytes,
                    None => false,
                }
        }
        Up::Fact { body } => agent.admitted && within(body, limits.fact_bytes),
        Up::Long { span } => agent.admitted && *span <= limits.long_span,
        Up::LongDone => agent.admitted,
        Up::Waiting { read } => agent.admitted && known_read(agent, *read),
        Up::Rejected { account, generation } => match agent.accounts.get(account) {
            Some(account) => agent.admitted && *generation > 0 && *generation <= account.emitted,
            None => false,
        },
        Up::Exhausted { account, .. } => agent.admitted && agent.accounts.contains_key(account),
    }
}

// Agent spend never decreases (domain/host.md, section 6). The kit never reprices.
fn valid_spend(agent: &Agent, spent: u64) -> bool {
    spent >= agent.spent
}

fn valid_answer(agent: &Agent, answer: &crate::Answer, limits: &Limits) -> bool {
    for (_, call) in &agent.calls {
        if call.kind == Kind::Delivery {
            match call.stage {
                CallStage::Parent | CallStage::Queued => return false,
                CallStage::Sending => {}
            }
        }
    }
    if answer.turns != agent.number || answer.completions < answer.turns || !valid_spend(agent, answer.spent) {
        return false;
    }
    match &answer.result {
        RunResult::Refused { detail } => {
            !agent.admitted
                && answer.turns == 0
                && answer.completions == 0
                && answer.input == 0
                && answer.output == 0
                && answer.cache_read == 0
                && answer.cache_write == 0
                && answer.spent == 0
                && within(detail, limits.outcome_bytes)
        }
        RunResult::Accepted { outcome } => agent.admitted && within(outcome, limits.outcome_bytes),
        RunResult::Parked | RunResult::Failed { .. } => agent.admitted,
    }
}

fn settle_send(agent: &mut Agent, sent: Sent) {
    match sent {
        Sent::Call(callback) => {
            let call = agent.calls.remove(&callback).expect("call transmission still reserved");
            assert!(call.stage == CallStage::Sending, "send terminal owns reservation");
        }
        Sent::Ack(number) => {
            let turn = agent.turns.remove(&number).expect("ACK transmission still reserved");
            assert!(turn.stage == TurnStage::Sending, "ACK send terminal owns metadata");
        }
        Sent::Start | Sent::Message | Sent::Busy { .. } | Sent::Grant | Sent::Cancel => {}
    }
}

fn polite(agent: &mut Agent, env: &Env<Limits>, owed: Option<Fault>) {
    match agent.phase {
        Phase::Starting | Phase::Live => {
            if !agent.cancel_queued {
                agent.outbox.push(Down::Cancel);
                agent.cancel_queued = true;
            }
            agent.owed = owed;
            agent.phase = Phase::Cancelled;
            agent.until = env.now.saturating_add(env.limits.grace);
        }
        Phase::Spawning
        | Phase::Cancelled
        | Phase::Draining
        | Phase::Exiting
        | Phase::Terminating
        | Phase::Killing
        | Phase::Closed => {}
    }
}

fn draining(agent: &mut Agent, env: &Env<Limits>) {
    match agent.phase {
        Phase::Starting | Phase::Live => {
            agent.phase = Phase::Draining;
            agent.owed = Some(Fault::Exited);
            agent.until = env.now.saturating_add(env.limits.grace);
        }
        Phase::Cancelled => {
            agent.phase = Phase::Draining;
        }
        Phase::Spawning | Phase::Draining | Phase::Exiting | Phase::Terminating | Phase::Killing | Phase::Closed => {}
    }
}

fn report_fault(agent: &mut Agent, fault: Fault, out: &mut Queue<Request>) {
    if !agent.told {
        agent.told = true;
        out.push(Request::Faulted { client: agent.client, fault });
    }
}

fn fail(agent: &mut Agent, owner: Token, fault: Fault, env: &Env<Limits>, out: &mut Queue<Request>) {
    if !agent.host_stopped && !agent.told {
        report_fault(agent, fault, out);
    } else {
        agent.diagnostic = Some(fault);
    }
    agent.owed = None;
    match agent.phase {
        Phase::Starting | Phase::Live | Phase::Cancelled | Phase::Draining | Phase::Exiting => {
            terminate(agent, owner, env, out);
        }
        Phase::Terminating | Phase::Killing => {}
        Phase::Spawning | Phase::Closed => unreachable!("only spawned processes fail channel/progress"),
    }
}

fn terminate(agent: &mut Agent, owner: Token, env: &Env<Limits>, out: &mut Queue<Request>) {
    signal(agent, owner, Signal::Terminate, out);
    agent.phase = Phase::Terminating;
    agent.until = env.now.saturating_add(env.limits.kill_after);
}

fn signal(agent: &mut Agent, owner: Token, signal: Signal, out: &mut Queue<Request>) {
    agent.signals = agent.signals.checked_add(1).expect("two bounded tree signals at most");
    out.push(Request::Signal { owner, process: agent.process.expect("spawned process"), signal });
}

fn turn_room(agent: &Agent, limits: &Limits) -> bool {
    agent.turns.len() < limits.turns
        && match agent.turn_bytes.checked_add(limits.turn_bytes) {
            Some(bytes) => bytes <= limits.unacknowledged_bytes,
            None => false,
        }
}

fn pause(agent: &Agent, env: &Env<Limits>) -> bool {
    if agent.waiting || !turn_room(agent, &env.limits) || agent.busy.room() == 0 {
        return true;
    }
    for (_, call) in &agent.calls {
        if call.stage == CallStage::Parent && call.deadline > env.now {
            return true;
        }
    }
    false
}

fn send_next(agent: &mut Agent, owner: Token, out: &mut Queue<Request>) {
    if agent.sending.is_some() || !listens(agent) {
        return;
    }
    let (sent, message) = match agent.busy.pop() {
        Some(sent) => {
            let call = match sent {
                Sent::Busy { call, .. } => call,
                Sent::Start | Sent::Message | Sent::Call(_) | Sent::Ack(_) | Sent::Grant | Sent::Cancel => {
                    unreachable!("overflow queue")
                }
            };
            (sent, Down::Answer { call, reply: Reply::Busy })
        }
        None => {
            let Some(message) = agent.outbox.pop() else {
                return;
            };
            let sent = match &message {
                Down::Start { .. } => unreachable!("Start sent directly once"),
                Down::Message { name, .. } => {
                    agent.messages = agent.messages.checked_sub(1).expect("queued message count");
                    agent.unread.push(*name);
                    Sent::Message
                }
                Down::Answer { call, .. } => {
                    let entry = agent.calls.get_mut(call).expect("answer retains callback until Sent");
                    entry.stage = CallStage::Sending;
                    Sent::Call(*call)
                }
                Down::Acknowledge { turn } => {
                    agent.turns.get_mut(turn).expect("exact ACK metadata").stage = TurnStage::Sending;
                    Sent::Ack(*turn)
                }
                Down::Grant { grant } => {
                    agent.accounts.get_mut(&grant.account).expect("known grant").emitted = grant.generation;
                    Sent::Grant
                }
                Down::Cancel => {
                    agent.cancel_sent = true;
                    Sent::Cancel
                }
            };
            (sent, message)
        }
    };
    agent.sending = Some(sent);
    out.push(Request::Send { owner, process: agent.process.expect("spawned process"), message });
}

fn disconnect(agent: &mut Agent, limits: &Limits, out: &mut Queue<Request>) {
    // Queued transmissions cannot happen after EOF/last word, but parent rights survive.
    let count = agent.outbox.len();
    for _ in 0..count {
        let message = agent.outbox.pop().expect("bounded queue snapshot");
        match message {
            Down::Answer { call, .. } => {
                agent.calls.remove(&call).expect("already answered queued call");
            }
            Down::Acknowledge { turn } => {
                agent.turns.remove(&turn).expect("committed queued turn");
            }
            Down::Message { .. } => {
                agent.messages = agent.messages.checked_sub(1).expect("queued message count");
            }
            Down::Start { .. } => unreachable!("start never queued"),
            Down::Grant { .. } | Down::Cancel => {}
        }
    }
    let busy_count = agent.busy.len();
    for _ in 0..busy_count {
        agent.busy.pop().expect("bounded busy snapshot");
    }
    let mut withdrawn = skein_lib::List::with_capacity(limits.calls);
    for (callback, call) in &agent.calls {
        if call.stage == CallStage::Parent && !call.withdrawn {
            withdrawn.push(*callback).expect("one entry per admitted call");
        }
    }
    for callback in withdrawn.into_boxed() {
        agent.calls.get_mut(&callback).expect("retained parent right").withdrawn = true;
        out.push(Request::Withdrawn { client: agent.client, call: callback });
    }
}

fn follow(domain: &mut Domain, env: &Env<Limits>, id: Id<Agent>, out: &mut Queue<Request>) {
    let agent = domain.agents.get_mut(id).expect("entry lives until rights settle");
    let owner = id.token();
    let client = agent.client;
    let diagnostic = agent.diagnostic.take();
    if !listens(agent) && agent.phase != Phase::Spawning {
        disconnect(agent, &env.limits, out);
    }
    send_next(agent, owner, out);
    let may_read = match agent.phase {
        Phase::Starting | Phase::Live | Phase::Cancelled | Phase::Draining => {
            turn_room(agent, &env.limits) && agent.busy.room() > 0
        }
        Phase::Exiting | Phase::Terminating | Phase::Killing => true,
        Phase::Spawning | Phase::Closed => false,
    };
    if may_read && !agent.reading && !agent.eof {
        agent.reading = true;
        out.push(Request::Read { owner, process: agent.process.expect("spawned process") });
    }
    let working = match agent.phase {
        Phase::Starting | Phase::Live => true,
        Phase::Spawning
        | Phase::Cancelled
        | Phase::Draining
        | Phase::Exiting
        | Phase::Terminating
        | Phase::Killing
        | Phase::Closed => false,
    };
    let paused = working && pause(agent, env);
    if agent.paused && !paused {
        agent.progress = env.now;
    }
    agent.paused = paused;
    let watch = if working {
        if paused {
            let mut deadline = None;
            for (_, call) in &agent.calls {
                if call.stage == CallStage::Parent && call.deadline > env.now {
                    deadline = Some(match deadline {
                        Some(at) => call.deadline.min(at),
                        None => call.deadline,
                    });
                }
            }
            deadline
        } else {
            Some(agent.progress.max(agent.long).saturating_add(env.limits.no_progress))
        }
    } else {
        None
    };
    let wall = if working { Some(agent.wall) } else { None };
    let grace = match agent.phase {
        Phase::Cancelled | Phase::Draining | Phase::Exiting | Phase::Terminating => Some(agent.until),
        Phase::Spawning | Phase::Starting | Phase::Live | Phase::Killing | Phase::Closed => None,
    };
    set(&mut domain.alarms, Alarm::Watch { agent: id }, watch);
    set(&mut domain.alarms, Alarm::Wall { agent: id }, wall);
    set(&mut domain.alarms, Alarm::Grace { agent: id }, grace);
    let done = agent.phase == Phase::Closed
        || (agent.exited
            && agent.reaped.is_some()
            && agent.eof
            && !agent.reading
            && agent.sending.is_none()
            && agent.signals == 0
            && agent.calls.is_empty()
            && agent.turns.is_empty());
    if done {
        if agent.phase != Phase::Closed {
            if let Some(fault) = agent.owed.take() {
                report_fault(agent, fault, out);
            }
            out.push(Request::Gone {
                client: agent.client,
                end: End::Stopped,
                detail: agent.reaped.take().expect("empty tree proof"),
            });
        }
        agent.phase = Phase::Closed;
        for alarm in [Alarm::Watch { agent: id }, Alarm::Wall { agent: id }, Alarm::Grace { agent: id }] {
            domain.alarms.cancel(alarm);
        }
        domain.agents.retire(id);
    }
    if let Some(fault) = diagnostic {
        observation(domain, Fact::Faulted { client, fault });
    }
}

fn set(alarms: &mut Deadlines<Alarm>, alarm: Alarm, deadline: Option<Time>) {
    if let Some(at) = deadline {
        alarms.arm(alarm, at).expect("three bounded alarms per agent");
    } else {
        alarms.cancel(alarm);
    }
}

fn observation(domain: &mut Domain, fact: Fact) {
    if domain.facts.try_push(fact).is_err() {
        domain.lost = domain.lost.saturating_add(1);
    }
}

fn observations(domain: &mut Domain, out: &Queue<Request>, before: u32) {
    let mut at = 0;
    for request in out {
        if at >= before {
            let fact = match request {
                Request::Started { client, .. } => Some(Fact::Started { client: *client }),
                Request::Admitted { client } => Some(Fact::Admitted { client: *client }),
                Request::Answered { client, .. } => Some(Fact::Answered { client: *client }),
                Request::Faulted { client, fault } => Some(Fact::Faulted { client: *client, fault: *fault }),
                Request::Gone { client, end, .. } => Some(Fact::Gone { client: *client, end: *end }),
                Request::Called { .. }
                | Request::Withdrawn { .. }
                | Request::Turn { .. }
                | Request::Waiting { .. }
                | Request::Rejected { .. }
                | Request::Exhausted { .. }
                | Request::Told { .. }
                | Request::Bounced { .. }
                | Request::Spawn { .. }
                | Request::Send { .. }
                | Request::Read { .. }
                | Request::Signal { .. }
                | Request::Wait { .. }
                | Request::Reap { .. } => None,
            };
            if let Some(fact) = fact {
                observation(domain, fact);
            }
        }
        at = at.checked_add(1).expect("bounded output queue");
    }
}
