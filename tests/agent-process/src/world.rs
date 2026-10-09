//! Scripted parent and observation referee beside a hosted agent. The parent
//! knows channel bytes and kernel terminals, never agent state; the referee
//! reads those bytes and independent provider observations, including facts
//! decoded from the host's channel.
//! Scheduling, replay and process heaps belong to Skein (testing.md, 2–5).
use crate::fake;
use skein_io::kernel::{self, Done, Fd, Op, Pipe, Spawn, Way};
use skein_lib::{Duration, Queue, Time, Token, Wall, bytes};
use skein_world::{Host, HostedProgram, Inherited, Memory, Outcome};

#[expect(clippy::struct_excessive_bools, reason = "independent kernel operations each own an in-flight bit")]
struct Parent {
    peer: skein_fake_channel::ScriptedPeer,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    input: Option<Fd>,
    output: Option<Fd>,
    error: Option<Fd>,
    pidfd: Option<Fd>,
    reading: bool,
    error_reading: bool,
    writing: bool,
    pending: Option<(Box<[u8]>, u32)>,
    status: Option<kernel::Exit>,
    signal: bool,
    signalled: bool,
    hang_up: bool,
    closing: u32,
    next: u64,
}

impl Parent {
    fn new(root: Fd, charter: &[u8], hang_up: bool) -> Self {
        let mut peer = crate::host();
        if !hang_up {
            peer.play(crate::start(charter));
        }
        let mut parent = Self {
            peer,
            completions: Queue::with_capacity(16),
            submissions: Queue::with_capacity(16),
            input: None,
            output: None,
            error: None,
            pidfd: None,
            reading: false,
            error_reading: false,
            writing: false,
            pending: None,
            status: None,
            signal: false,
            signalled: false,
            hang_up,
            closing: 0,
            next: 0,
        };
        parent.submit(Op::Spawn {
            spawn: Box::new(Spawn {
                program: b"smith".as_slice().into(),
                args: Box::new([]),
                env: Box::new([]),
                root,
                dir: Box::new([]),
                pipes: Box::new([
                    Pipe { child: 0, way: Way::In, parent: None },
                    Pipe { child: 1, way: Way::Out, parent: None },
                    Pipe { child: 2, way: Way::Out, parent: None },
                ]),
            }),
        });
        parent
    }

    fn submit(&mut self, kind: Op) {
        self.next = self.next.checked_add(1).expect("finite parent tokens");
        self.submissions.push(kernel::Submit { op: Token::new(self.next), kind });
    }

    fn close(&mut self, fd: Fd) {
        self.closing = self.closing.checked_add(1).expect("five parent descriptors");
        self.submit(Op::Close { fd });
    }

    fn answered(&self) -> bool {
        self.peer.observed().iter().any(|frame| frame.kind == 0x0110)
    }
    fn admitted(&self) -> bool {
        self.peer.observed().iter().any(|frame| frame.kind == 0x0106)
    }

    fn completed(&mut self, complete: kernel::Complete) {
        match complete.kind {
            Op::Spawn { spawn } => {
                let Ok(Done::Spawned { pidfd }) = complete.result else { panic!("hosted startup failed") };
                self.close(spawn.root);
                self.pidfd = Some(pidfd);
                self.input = spawn.pipes[0].parent;
                if self.hang_up {
                    let input = self.input.take().expect("child input");
                    self.close(input);
                }
                self.output = spawn.pipes[1].parent;
                self.error = spawn.pipes[2].parent;
                self.submit(Op::Wait { pidfd });
            }
            Op::Wait { .. } => {
                let Ok(Done::Exit(status)) = complete.result else { panic!("agent wait failed") };
                self.status = Some(status);
                let pidfd = self.pidfd.take().expect("live child pidfd");
                self.close(pidfd);
            }
            Op::PipeRead { fd, buf } => {
                let error = self.error == Some(fd);
                if error {
                    self.error_reading = false;
                } else {
                    self.reading = false;
                }
                let Ok(Done::Count(count)) = complete.result else { panic!("agent read failed") };
                if count == 0 {
                    if error {
                        self.error = None;
                    } else {
                        self.output = None;
                    }
                    self.close(fd);
                } else {
                    assert!(!error, "agent wrote unexpected stderr");
                    self.peer.feed(&buf[..usize::try_from(count).expect("read count")]).expect("valid channel");
                }
            }
            Op::PipeWrite { bytes, from, .. } => {
                self.writing = false;
                let Ok(Done::Count(count)) = complete.result else { panic!("agent write failed") };
                let offset = from.checked_add(count).expect("bounded frame offset");
                self.pending =
                    if usize::try_from(offset).expect("offset") == bytes.len() { None } else { Some((bytes, offset)) };
            }
            Op::Close { .. } => {
                assert!(complete.result.is_ok(), "parent close succeeds");
                self.closing = self.closing.checked_sub(1).expect("submitted close");
            }
            Op::Signal { .. } => {
                assert!(complete.result.is_ok(), "live child signal succeeds");
            }
            Op::Socket { .. }
            | Op::Bind { .. }
            | Op::Listen { .. }
            | Op::Accept { .. }
            | Op::Connect { .. }
            | Op::Recv { .. }
            | Op::Send { .. }
            | Op::Shutdown { .. }
            | Op::Open { .. }
            | Op::Read { .. }
            | Op::Write { .. }
            | Op::Sync { .. }
            | Op::Stat { .. }
            | Op::Rename { .. }
            | Op::Remove { .. }
            | Op::MakeDirectory { .. }
            | Op::List { .. }
            | Op::ReadSignal { .. }
            | Op::Cancel { .. } => unreachable!("scripted parent owns only process pipes"),
        }
    }
}

impl Host for Parent {
    fn iterate(&mut self, _now: Time, _wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            self.completed(complete);
        }
        if self.signal && !self.signalled {
            self.signalled = true;
            self.submit(Op::Signal { pidfd: self.pidfd.expect("admitted child"), signal: kernel::Signal::Terminate });
        }
        if !self.reading
            && let Some(fd) = self.output
        {
            self.reading = true;
            self.submit(Op::PipeRead { fd, buf: bytes::zeroed(4096) });
        }
        if !self.error_reading
            && let Some(fd) = self.error
        {
            self.error_reading = true;
            self.submit(Op::PipeRead { fd, buf: bytes::zeroed(256) });
        }
        if self.input.is_some() && self.pending.is_none() && !self.writing {
            self.pending = self.peer.pop_output().map(|bytes| (bytes, 0));
        }
        if !self.writing
            && let Some((bytes, from)) = self.pending.take()
        {
            self.writing = true;
            self.submit(Op::PipeWrite { fd: self.input.expect("child stdin"), bytes, from });
        }
        if self.answered()
            && !self.writing
            && self.pending.is_none()
            && let Some(fd) = self.input.take()
        {
            self.close(fd);
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, _now: Time) -> bool {
        !self.completions.is_empty()
            || !self.submissions.is_empty()
            || (self.signal && !self.signalled)
            || (self.output.is_some() && !self.reading)
            || (self.error.is_some() && !self.error_reading)
            || (self.pending.is_some() && !self.writing)
    }
    fn next_deadline(&self) -> Option<Time> {
        None
    }
    fn is_empty(&self) -> bool {
        self.status.is_some()
            && self.input.is_none()
            && self.output.is_none()
            && self.error.is_none()
            && self.pidfd.is_none()
            && self.closing == 0
            && self.submissions.is_empty()
            && self.completions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        134_217_728
    }
    fn operations(&self) -> u32 {
        16
    }
}

enum Proc {
    Parent(Box<Parent>),
    Agent(Box<smith_agent_shell::Agent>, crate::AgentErrors),
    Peer(Box<skein_fake_peers::llm::Peer>),
}
impl Proc {
    fn host(&self) -> &dyn Host {
        match self {
            Self::Parent(p) => p.as_ref(),
            Self::Agent(p, _) => p.as_ref(),
            Self::Peer(p) => p.as_ref(),
        }
    }
    fn host_mut(&mut self) -> &mut dyn Host {
        match self {
            Self::Parent(p) => p.as_mut(),
            Self::Agent(p, _) => p.as_mut(),
            Self::Peer(p) => p.as_mut(),
        }
    }
}
impl Host for Proc {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.host_mut().iterate(now, wall);
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        self.host_mut().completions()
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        self.host_mut().submissions()
    }
    fn work_pending(&self, now: Time) -> bool {
        self.host().work_pending(now)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.host().next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.host().is_empty()
    }
    fn exit(&self) -> Option<kernel::Exit> {
        self.host().exit()
    }
    fn worst_case(&self) -> u64 {
        self.host()
            .worst_case()
            .checked_add(size_of::<Self>() as u64)
            .and_then(|bound| bound.checked_add(4096))
            .expect("wrapper bound")
    }
    fn operations(&self) -> u32 {
        self.host().operations()
    }
}
fn make(_spawn: &Spawn, inherited: &Inherited) -> Proc {
    let errors = crate::AgentErrors::default();
    Proc::Agent(Box::new(crate::configured(crate::configuration(), 7, inherited, errors.clone())), errors)
}

fn unused_roots(_spawn: &Spawn) -> Vec<skein_world::StartupRoot> {
    [b"unused-a", b"unused-b"]
        .into_iter()
        .map(|name| skein_world::StartupRoot { name: name.as_slice().into(), path: name.as_slice().into() })
        .collect()
}

struct Machine(skein_fake_machine::Machine);
impl skein_world::Machine for Machine {
    fn open_root(&mut self, path: &[u8]) -> Result<skein_sim::Handle, kernel::Error> {
        assert!(matches!(path, b"unused-a" | b"unused-b"), "scenario's unused startup roots");
        Ok(skein_sim::Handle::new(self.0.lay(&[]).raw()))
    }

    fn close_root(&mut self, root: skein_sim::Handle) {
        self.0.close(skein_fake_machine::Opened::new(root.raw()));
    }

    fn step(&mut self, call: skein_sim::Call, answers: &mut Queue<skein_sim::Answer>) {
        skein_fake_machine::step(&mut self.0, call, answers);
    }
}

#[derive(Debug)]
struct Terminal {
    hang_up: bool,
}
impl skein_world::Expectation<Proc> for Terminal {
    fn check(&self, _now: Time, procs: &[Proc]) -> Result<bool, String> {
        let parent = procs.iter().find_map(|p| if let Proc::Parent(p) = p { Some(p) } else { None }).expect("parent");
        let answers = parent.peer.observed().iter().filter(|frame| frame.kind == 0x0110).count();
        if answers > 1 {
            return Err("more than one Answer".into());
        }
        Ok(if self.hang_up {
            answers == 0 && parent.status == Some(kernel::Exit::Code(1))
        } else {
            answers == 1 && parent.status == Some(kernel::Exit::Code(0))
        })
    }
    fn deadline(&self) -> Time {
        Time::from_nanos(Duration::from_secs(120).as_nanos())
    }
}
struct Referee {
    cancel: bool,
    hang_up: bool,
    reviewed: usize,
    admitted_facts: usize,
    answer_facts: usize,
    full_completion: bool,
    expected: skein_world::Expectations<Proc, Terminal>,
}
impl skein_world::Referee<Proc> for Referee {
    fn act(&mut self, _now: Time, procs: &mut [Proc]) {
        let admitted = procs.iter().any(|p| matches!(p, Proc::Parent(parent) if parent.admitted()));
        let exited = procs.iter().any(|p| matches!(p, Proc::Parent(parent) if parent.status.is_some()));
        for proc in procs {
            match proc {
                Proc::Parent(parent) => {
                    if self.cancel && admitted {
                        parent.signal = true;
                    }
                }
                Proc::Peer(peer) => {
                    if exited {
                        peer.shutdown();
                    }
                }
                Proc::Agent(_, _) => {}
            }
        }
    }
    fn observe(&mut self, now: Time, procs: &[Proc]) {
        let parent = procs.iter().find_map(|p| if let Proc::Parent(p) = p { Some(p) } else { None }).expect("parent");
        let answers = parent.peer.observed().iter().filter(|frame| frame.kind == 0x0110).count();
        assert!(answers <= 1, "one Answer for one Start");
        assert!(
            parent.peer.observed().iter().filter(|frame| frame.kind == 0x0106).count() <= 1,
            "one admission for one Start"
        );
        let peer = procs.iter().find_map(|p| if let Proc::Peer(p) = p { Some(p) } else { None }).expect("peer");
        assert!(fake::queries(peer).count() <= 1, "one completion request for the one-turn budget");
        if !self.hang_up && parent.status.is_some() {
            assert!(parent.admitted(), "the host observed admission");
            let peer = procs.iter().find_map(|p| if let Proc::Peer(p) = p { Some(p) } else { None }).expect("peer");
            if !self.cancel {
                assert!(fake::replied(peer), "provider answered before agent exit");
            }
        }
        for frame in parent.peer.observed().get(self.reviewed..).expect("channel observations only grow") {
            if frame.kind == 0x010f {
                let fact =
                    smith_channel::Fact::decode(&smith_channel::CEILINGS, &mut skein_lib::Reader::new(&frame.body))
                        .expect("host's validated Fact");
                match fact.kind() {
                    smith_channel::FactKind::Admitted => self.admitted_facts += 1,
                    smith_channel::FactKind::Ended => self.answer_facts += 1,
                    smith_channel::FactKind::LlmFinished => {
                        if self.full_completion {
                            assert_eq!(
                                fact.count(),
                                u64::from(crate::limits().llm.adapter.client.dialect.parts),
                                "all maximum provider parts decoded"
                            );
                        }
                    }
                    smith_channel::FactKind::LlmStarted
                    | smith_channel::FactKind::LlmRetried
                    | smith_channel::FactKind::ToolStarted
                    | smith_channel::FactKind::ToolFinished
                    | smith_channel::FactKind::CheckStarted
                    | smith_channel::FactKind::CheckFinished
                    | smith_channel::FactKind::TextArrived => {}
                }
            }
        }
        self.reviewed = parent.peer.observed().len();
        assert!(self.admitted_facts <= 1 && self.answer_facts <= 1, "one fact terminal per activation");
        if !self.hang_up && parent.status.is_some() {
            assert_eq!(self.admitted_facts, 1, "the observed admission agrees with the host");
        }
        self.expected.observe(now, procs);
    }
    fn next_deadline(&self) -> Option<Time> {
        self.expected.next_deadline()
    }
    fn overdue(&self, now: Time) -> Option<String> {
        self.expected.overdue(now)
    }
    fn passed(&self) -> bool {
        self.expected.passed()
    }
}

/// Scenario configuration and a settled shared-harness outcome.
pub struct World {
    seed: u64,
    charter: Box<[u8]>,
    cancel: bool,
    hang_up: bool,
    memory: Memory,
    full_completion: bool,
    outcome: Option<Outcome<Proc, Machine>>,
}
impl World {
    /// Prepare one Start; the harness owns startup and settlement.
    #[must_use]
    pub fn new(seed: u64, charter: &[u8]) -> Self {
        Self {
            seed,
            charter: charter.into(),
            cancel: false,
            hang_up: false,
            memory: Memory::Unchecked,
            full_completion: false,
            outcome: None,
        }
    }
    /// Request an actual pidfd termination signal after observed admission.
    pub fn signal(&mut self) {
        self.cancel = true;
    }
    /// Close the channel before Start, without signalling the process.
    pub fn hang_up(&mut self) {
        self.hang_up = true;
    }

    /// Ask the provider for the full configured decoded completion payload.
    pub fn full_completion(&mut self) {
        self.full_completion = true;
    }

    /// Meter every process iteration and drop with the shared allocator.
    pub fn check_memory(&mut self) {
        self.memory = Memory::Checked;
    }
    /// Run until the one Answer and complete descriptor settlement.
    pub fn settle(&mut self) -> bool {
        let mut config = skein_sim::Config::calm();
        config.wall = skein_tls_world::pki::VALID;
        let mut machine = skein_fake_machine::Machine::new();
        let root = machine.lay(&[]);
        let referee = Referee {
            cancel: self.cancel,
            hang_up: self.hang_up,
            reviewed: 0,
            admitted_facts: 0,
            answer_facts: 0,
            full_completion: self.full_completion,
            expected: skein_world::Expectations::new(self.seed, vec![Terminal { hang_up: self.hang_up }]),
        };
        let mut world = skein_world::World::new(self.seed, config, referee, self.memory).with_machine(Machine(machine));
        world.host_roots(
            HostedProgram {
                program: b"smith".as_slice().into(),
                make,
                instances: 1,
                operations: crate::limits().routes.checked_add(3).expect("routes and inherited closes"),
            },
            unused_roots,
        );
        world.spawn_root(skein_sim::Handle::new(root.raw()), |root| {
            Proc::Parent(Box::new(Parent::new(root, &self.charter, self.hang_up)))
        });
        world.spawn(|| {
            Proc::Peer(Box::new(if self.full_completion {
                fake::configured(
                    Box::new([skein_fake_llm_domain::api::Script {
                        cue: Box::new([]),
                        turns: Box::new([skein_fake_llm_domain::api::Turn {
                            lines: (0..crate::limits().llm.adapter.client.dialect.parts)
                                .map(|index| skein_fake_llm_domain::api::Line::Text {
                                    // First response metadata is item_1_<index> and final_answer.
                                    text: vec![
                                        b'x';
                                        usize::try_from(
                                            crate::limits().llm.adapter.client.dialect.answer_bytes
                                                / crate::limits().llm.adapter.client.dialect.parts
                                        )
                                        .expect("bounded text per part")
                                            - format!("item_1_{index}").len()
                                            - b"final_answer".len()
                                            - 9
                                    ]
                                    .into_boxed_slice(),
                                })
                                .collect(),
                            finish: skein_fake_llm_domain::api::Finish::Stop,
                            tokens: 1,
                        }]),
                    }]),
                    skein_llm::Credential {
                        access_token: b"token".as_slice().into(),
                        account_id: b"acc".as_slice().into(),
                    },
                    Duration::ZERO,
                )
            } else {
                fake::peer()
            }))
        });

        self.outcome = Some(world.run());
        self.parent().status == Some(kernel::Exit::Code(u8::from(self.hang_up)))
    }
    fn parent(&self) -> &Parent {
        self.outcome
            .as_ref()
            .expect("settled world")
            .procs
            .iter()
            .find_map(|p| if let Proc::Parent(p) = p { Some(p.as_ref()) } else { None })
            .expect("parent survives")
    }
    fn peer(&self) -> &skein_fake_peers::llm::Peer {
        self.outcome
            .as_ref()
            .expect("settled world")
            .procs
            .iter()
            .find_map(|p| if let Proc::Peer(p) = p { Some(p.as_ref()) } else { None })
            .expect("peer survives")
    }
    /// Validated frames seen by the scripted host.
    #[must_use]
    pub fn observed(&self) -> &[skein_fake_channel::Observed] {
        self.parent().peer.observed()
    }
    /// The host's terminal Answer, independently decoded.
    #[must_use]
    pub fn answer(&self) -> Option<smith_channel::Answer> {
        self.observed().iter().find(|frame| frame.kind == 0x0110).map(|frame| {
            smith_channel::Answer::decode(&smith_channel::CEILINGS, &mut skein_lib::Reader::new(&frame.body))
                .expect("validated Answer")
        })
    }
    /// The independent provider's observations.
    #[must_use]
    pub fn peer_observations(&self) -> &[skein_fake_peers::llm::Observation] {
        self.peer().observations()
    }
    /// Whether the provider emitted a successful terminal.
    #[must_use]
    pub fn peer_replied(&self) -> bool {
        fake::replied(self.peer())
    }
    /// Independently decoded model requests.
    #[must_use]
    pub fn peer_queries(&self) -> Vec<&skein_fake_llm_domain::api::Query> {
        fake::queries(self.peer()).collect()
    }
    /// Decoded provider parts and aggregate text/replay-token bytes in the host Turn.
    #[must_use]
    pub fn provider_answer_shape(&self) -> (usize, usize) {
        let mut count = 0;
        let mut bytes = 0;
        for frame in self.observed().iter().filter(|frame| frame.kind == 0x0109) {
            let envelope =
                smith_channel::Turn::decode(&smith_channel::CEILINGS, &mut skein_lib::Reader::new(&frame.body))
                    .expect("validated turn envelope");
            let turn = smith_transcript::Turn::decode(
                &smith_transcript::CEILINGS,
                &mut skein_lib::Reader::new(envelope.body()),
            )
            .expect("validated transcript");
            for message in turn.messages() {
                if *message.role() == smith_transcript::Role::Assistant {
                    for block in message.blocks() {
                        if let smith_transcript::Block::Text(text) = block {
                            count += 1;
                            bytes += text.text().len();
                            let metadata = skein_llm::Replay::from_bytes(
                                text.replay().as_ref().expect("text replay").bytes(),
                                &crate::limits().llm.adapter.client.dialect,
                            )
                            .expect("provider metadata");
                            for token in metadata.value.as_tokens() {
                                bytes += match token {
                                    skein_json::Token::Key(value)
                                    | skein_json::Token::String(value)
                                    | skein_json::Token::Number(value) => value.len(),
                                    skein_json::Token::ObjectStart
                                    | skein_json::Token::ObjectEnd
                                    | skein_json::Token::ArrayStart
                                    | skein_json::Token::ArrayEnd
                                    | skein_json::Token::True
                                    | skein_json::Token::False
                                    | skein_json::Token::Null => 1,
                                };
                            }
                        }
                    }
                }
            }
        }
        (count, bytes)
    }

    /// Settlement already checked all descriptors and in-flight operations.
    pub fn assert_agent_clean(&self) {
        assert!(self.outcome.as_ref().expect("settled world").killed.is_empty());
    }
    /// The process status actually received by its parent.
    #[must_use]
    pub fn exit(&self) -> Option<kernel::Exit> {
        self.parent().status
    }

    /// Diagnostic bytes observed from the caller-supplied writer.
    #[must_use]
    pub fn errors(&self) -> Vec<u8> {
        self.outcome
            .as_ref()
            .expect("settled world")
            .procs
            .iter()
            .find_map(|proc| if let Proc::Agent(_, errors) = proc { Some(errors.bytes()) } else { None })
            .expect("agent writer")
    }

    /// Whether the scripted host sent a termination signal.
    #[must_use]
    pub fn signalled(&self) -> bool {
        self.parent().signalled
    }

    /// Complete kernel trace for the generic replay assertion.
    #[must_use]
    pub fn trace(&self) -> Vec<String> {
        self.outcome.as_ref().expect("settled world").trace.iter().map(|entry| format!("{entry:?}")).collect()
    }
}
