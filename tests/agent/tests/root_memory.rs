//! Attained V2 root ownership at public Start, Complete and concrete Turn seams.
//! Records come from actual shared Client terminals, including the real Wait
//! result; fixture ownership is counted independently from production pricing.
//! Root entrances and caller-copy transients are measured separately. Native
//! Client/peer allocation transients are outside these root/caller spans.
//! Contract: domain/run.md, sections 3, 6, 13 and 14; domain/session.md, section 3;
//! programming-model.md, section 6.3; testing-strategy.md, sections 2.3 and 6.

use skein_fake_llm_domain::api::{Finish, Line, Script, Turn as ScriptTurn};
use skein_lib::{Duration, Env, Queue, ReplyTo, Time, Token, Wall};
use skein_world::domain::heap::{Counting, Meter};
use smith_agent_world::{
    LIMITS,
    wire::{self, Configuration, Observed, Wire},
};
use smith_domain::{self as root, Domain, Event, Grant, GrantName, Limits, Request, llm, run, session};
use smith_protocol_llm::{self as adapter, Receiving};

#[global_allocator]
static HEAP: Counting = Counting;

const WORKER: Token = Token::new(91);
const PARENT: Token = Token::new(92);
const LARGE: usize = 8192;
const ID: &[u8] = b"call_0000000000000001";
const SAID: &[u8] = b"tiny actual text";
const RESTORED: &[u8] = b"actual restored text";

fn size<T>() -> u64 {
    u64::try_from(size_of::<T>()).expect("fixture cell size fits")
}

fn len(value: &[u8]) -> u64 {
    u64::try_from(value.len()).expect("bounded fixture payload")
}

fn bounds(messages: u32) -> Limits {
    Limits {
        accounts: 1,
        decoded_call_bytes: 16_384,
        run: run::Limits {
            runs: 1,
            conversations: 1,
            run_conversations: 1,
            calls: 1,
            run_bytes: 4096,
            repositories: 1,
            host_tools: 0,
            host_input_bytes: 64,
            host_reply_bytes: 64,
            answer_bytes: 64,
            guide_bytes: 64,
            outcome_bytes: 256,
            check_tail: 64,
            facts: 0,
            messages: 1,
            message_bytes: u32::try_from(LARGE).expect("bounded caller message"),
            ..LIMITS.run
        },
        session: session::Limits {
            sessions: 1,
            messages,
            // Source generation has independent room for maximum provider and
            // delegate receiving credit plus context. Restore replaces this
            // allowance with the separately observed exact payload/credit cap.
            session_bytes: 1_048_576,
            completion_bytes: 131_072,
            completion_blocks: 32,
            failure_bytes: 256,
            delegated_result_bytes: 262_144,
            parallel_tools: 1,
            facts: 0,
            tools: root::tools::Limits {
                kits: 1,
                calls: 1,
                repos: 1,
                path_bytes: 64,
                known_files: 1,
                file_bytes: 64,
                read_bytes: 64,
                list_entries: 1,
                list_bytes: 4096,
                match_lines: 1,
                env_bytes: 0,
                shell_head: 0,
                shell_tail: 0,
                search_hits: 1,
                search_bytes: 64,
                facts: 0,
                ..LIMITS.session.tools
            },
            ..LIMITS.session
        },
        ..LIMITS
    }
}

fn charter(restoring: bool) -> run::Charter {
    run::Charter {
        brief: b"@root-memory".as_slice().into(),
        checkout: run::charter::Checkout {
            repositories: Box::new([run::charter::Repository {
                name: b"work".as_slice().into(),
                root: Token::new(7),
                writable: false,
            }]),
        },
        grants: run::charter::Grants {
            deliver: None,
            tools: run::charter::Tools { inspect: false, modify: false, shell: false },
            agents: false,
            host_tools: Box::new([]),
        },
        outcome: run::outcome::OutcomeSpec {
            change: None,
            verdicts: Box::new([]),
            report: Some(run::outcome::TextSpec { min: 0, max: 64, fields: Box::new([]) }),
            failure: None,
        },
        budget: LIMITS.run.budget,
        llm: run::charter::Llm {
            account: 0,
            endpoint: run::charter::Endpoint(0),
            model: b"fixture-model".as_slice().into(),
            max_tokens: 128,
            dialect: 2,
        },
        models: Box::new([]),
        resume: restoring,
        waiting: Duration::from_secs(1),
    }
}

fn scripts(cycles: u32) -> Box<[Script]> {
    let mut turns = Vec::new();
    for _ in 0..cycles {
        turns.push(ScriptTurn {
            lines: Box::new([Line::Call { name: b"wait".as_slice().into(), arguments: b"{}".as_slice().into() }]),
            finish: Finish::ToolCalls,
            tokens: 1,
        });
        turns.push(ScriptTurn { lines: Box::new([Line::Text { text: SAID.into() }]), finish: Finish::Stop, tokens: 1 });
    }
    turns.push(ScriptTurn { lines: Box::new([Line::Text { text: RESTORED.into() }]), finish: Finish::Stop, tokens: 1 });
    Box::new([Script { cue: b"@root-memory".as_slice().into(), turns: turns.into() }])
}

fn wire_limits() -> adapter::Limits {
    let mut client = skein_llm_world::limits();
    client.http.request = 32_768;
    client.dialect.parts = 32;
    client.dialect.request_bytes = 32_768;
    client.dialect.document_bytes = 32_768;
    client.dialect.string_bytes = 16_384;
    client.dialect.tokens = 4096;
    client.dialect.answer_bytes = 256;
    adapter::Limits { client, tool_bytes: 16_384, result_bytes: 16_384 }
}

struct Complete {
    owner: Token,
    prompt: llm::Prompt,
    receiving: Receiving,
}

fn actual(complete: Complete, configuration: &Configuration, cycles: u32) -> Event {
    let owner = complete.owner;
    let mut wire =
        Wire::prepare(owner, complete.prompt, complete.receiving, configuration, &wire_limits(), scripts(cycles))
            .expect("actual root receiving contract admits the Client");
    assert!(wire.start().is_empty());
    let mut terminal = None;
    for _ in 0..100_000 {
        let (progress, returned) = wire.tick();
        for event in returned {
            assert!(terminal.replace(event).is_none(), "one genuine native terminal");
        }
        if !progress {
            break;
        }
    }
    let terminal = terminal.expect("bounded actual byte peer completed");
    assert!(matches!(&terminal, Event::Completed { owner: got, .. } if *got == owner));
    assert_eq!(wire.observed, [Observed::Completed(owner), Observed::Reusable]);
    assert!(wire.close().is_empty());
    assert!(wire.settle().is_empty());
    assert!(wire.settle().is_empty());
    assert_eq!(wire.observed, [Observed::Completed(owner), Observed::Reusable, Observed::Close, Observed::Closed]);
    drop(wire);
    terminal
}

// The fixture ledger counts actual public cells and owning byte fields. It
// neither calls owned_bytes/content_cost nor recreates admission or rendering.
fn record_blocks(blocks: &[session::llm::Block]) -> u64 {
    let mut bytes = u64::try_from(blocks.len()).expect("bounded blocks") * size::<session::llm::Block>();
    for block in blocks {
        bytes += match block {
            session::llm::Block::Text { text, replay } => {
                len(text) + replay.as_ref().map_or(0, |value| len(&value.bytes))
            }
            session::llm::Block::ToolCall { id, name, input, call: session::llm::Decoded::Historical, replay } => {
                len(id) + len(name) + len(input) + replay.as_ref().map_or(0, |value| len(&value.bytes))
            }
            session::llm::Block::ToolResult { id, result: session::llm::Returned::Text { text, replay, .. } } => {
                len(id) + len(text) + replay.as_ref().map_or(0, |value| len(&value.bytes))
            }
            session::llm::Block::Refusal { .. }
            | session::llm::Block::Opaque { .. }
            | session::llm::Block::ToolCall { .. }
            | session::llm::Block::ToolResult { .. } => {
                panic!("generated fixture has only literal text, historical Wait and concrete feedback")
            }
        };
    }
    bytes
}

fn messages_bytes(messages: &[session::llm::Message]) -> u64 {
    u64::try_from(messages.len()).expect("bounded messages") * size::<session::llm::Message>()
        + messages.iter().map(|message| record_blocks(&message.content)).sum::<u64>()
}

fn turn_bytes(turn: &session::record::Turn) -> u64 {
    messages_bytes(&turn.messages)
}

fn transcript_bytes(history: &root::Transcript) -> u64 {
    u64::try_from(history.turns.len()).expect("bounded turns") * size::<session::record::Turn>()
        + history.turns.iter().map(turn_bytes).sum::<u64>()
        + messages_bytes(&history.after)
}

fn prompt_bytes(prompt: &llm::Prompt) -> u64 {
    let mut bytes = len(&prompt.model)
        + len(&prompt.system)
        + u64::try_from(prompt.served.len()).expect("bounded descriptors") * size::<llm::Served>()
        + u64::try_from(prompt.messages.len()).expect("bounded messages") * size::<llm::Message>();
    for served in &prompt.served {
        assert!(matches!(served, llm::Served::Finish | llm::Served::Wait));
    }
    for message in &prompt.messages {
        bytes += u64::try_from(message.content.len()).expect("bounded blocks") * size::<llm::Block>();
        for block in &message.content {
            bytes += match block {
                llm::Block::Text { text, replay } => len(text) + replay.as_ref().map_or(0, |value| len(&value.bytes)),
                llm::Block::ToolCall { id, name, input, replay } => {
                    len(id) + len(name) + len(input) + replay.as_ref().map_or(0, |value| len(&value.bytes))
                }
                llm::Block::ToolResult { id, result: llm::Returned::Text { text, replay, .. } } => {
                    len(id) + len(text) + replay.as_ref().map_or(0, |value| len(&value.bytes))
                }
                llm::Block::Refusal { .. } | llm::Block::Opaque { .. } | llm::Block::ToolResult { .. } => {
                    panic!("literal supported root fixture fields")
                }
            };
        }
    }
    bytes
}

// Prompt deliberately has no Clone; the outside caller copies only the whole
// observed fixture values. This does not translate records or classify calls.
fn copy_prompt(prompt: &llm::Prompt) -> llm::Prompt {
    let expected = prompt_bytes(prompt);
    let copy_meter = Meter::new();
    copy_meter.start();
    let messages = prompt
        .messages
        .iter()
        .map(|message| llm::Message {
            role: message.role,
            content: message
                .content
                .iter()
                .map(|block| match block {
                    llm::Block::Text { text, replay } => {
                        llm::Block::Text { text: text.clone(), replay: replay.clone() }
                    }
                    llm::Block::ToolCall { id, name, input, replay } => llm::Block::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                        replay: replay.clone(),
                    },
                    llm::Block::ToolResult { id, result: llm::Returned::Text { text, error, replay } } => {
                        llm::Block::ToolResult {
                            id: id.clone(),
                            result: llm::Returned::Text { text: text.clone(), error: *error, replay: replay.clone() },
                        }
                    }
                    llm::Block::Refusal { .. } | llm::Block::Opaque { .. } | llm::Block::ToolResult { .. } => {
                        panic!("outside copies only the observed supported fixture fields")
                    }
                })
                .collect(),
        })
        .collect();
    let copied = llm::Prompt {
        endpoint: prompt.endpoint,
        model: prompt.model.clone(),
        system: prompt.system.clone(),
        tools: prompt.tools,
        served: prompt.served.clone(),
        messages,
        max_tokens: prompt.max_tokens,
    };
    let measured = copy_meter.end();
    assert_eq!(measured.held(), expected, "whole prompt arrays and payload/replay fields are owned independently");
    assert!(measured.peak() <= expected, "caller prompt copy has no unpriced construction transient");
    copied
}

fn copy_transcript(history: &root::Transcript) -> root::Transcript {
    let expected = transcript_bytes(history);
    let copy_meter = Meter::new();
    copy_meter.start();
    let copied = history.clone();
    let measured = copy_meter.end();
    assert_eq!(measured.held(), expected, "copied Turn/Message/Block arrays and payloads are independently owned");
    assert!(measured.peak() <= expected, "caller history copy has no unpriced construction transient");
    copied
}

fn copy_turn(turn: &session::record::Turn) -> session::record::Turn {
    let expected = turn_bytes(turn);
    let copy_meter = Meter::new();
    copy_meter.start();
    let copied = turn.clone();
    let measured = copy_meter.end();
    assert_eq!(measured.held(), expected, "copied concrete Turn fields are independently owned");
    assert!(measured.peak() <= expected, "caller Turn copy has no unpriced construction transient");
    copied
}

fn assert_rewritten(history: &root::Transcript, prompt: &llm::Prompt) {
    let expected = history.turns.iter().flat_map(|turn| &turn.messages);
    for (record, message) in expected.zip(&prompt.messages) {
        assert_eq!(message.role, record.role);
        assert_eq!(message.content.len(), record.content.len());
        for (original, block) in record.content.iter().zip(&message.content) {
            match (original, block) {
                (session::llm::Block::Text { text: original, replay: envelope }, llm::Block::Text { text, replay }) => {
                    assert_eq!(text, original);
                    assert_eq!(replay, envelope);
                }
                (
                    session::llm::Block::ToolCall {
                        id: original_id,
                        name: original_name,
                        input: original_input,
                        replay: envelope,
                        ..
                    },
                    llm::Block::ToolCall { id, name, input, replay },
                ) => {
                    assert_eq!(id, original_id);
                    assert_eq!(name, original_name);
                    assert_eq!(input, original_input);
                    assert_eq!(replay, envelope);
                }
                (
                    session::llm::Block::ToolResult {
                        id: original_id,
                        result: session::llm::Returned::Text { text: original, error: original_error, replay: envelope },
                    },
                    llm::Block::ToolResult { id, result: llm::Returned::Text { text, error, replay } },
                ) => {
                    assert_eq!(id, original_id);
                    assert_eq!(text, original);
                    assert_eq!(error, original_error);
                    assert_eq!(replay, envelope);
                }
                _ => panic!("actual root rewrite preserves the independent observed fields"),
            }
        }
    }
    let waking = prompt.messages.last().expect("actual waking User");
    assert_eq!(waking.role, llm::Role::User);
    let [llm::Block::Text { text, replay: None }] = waking.content.as_ref() else { panic!("real waking text") };
    assert_eq!(text.as_ref(), b"Begin the work your brief describes.");
}

struct Counted {
    meter: Meter,
    domain: Option<Domain>,
    env: Env<Limits>,
    out: Queue<Request>,
    queue_bytes: u64,
    records: Vec<session::record::Turn>,
    prefix: Option<root::Transcript>,
    saved: Option<root::Transcript>,
    initial: Option<llm::Prompt>,
    held_prompt: Option<llm::Prompt>,
    prompt_copy: Option<llm::Prompt>,
    turn_copy: Option<session::record::Turn>,
    complete: Option<Complete>,
    admitted: Option<Token>,
    read: Option<Token>,
    probe: Option<Token>,
    waiting: bool,
    answer: Option<run::Answer>,
    calls: u32,
}

impl Counted {
    fn new(limits: &Limits) -> Self {
        let meter = Meter::new();
        meter.start();
        let out = Queue::with_capacity(root::max_out(limits));
        let domain = Domain::new(limits, 41);
        let measured = meter.end();
        let queue_bytes = Queue::<Request>::worst_case(root::max_out(limits)).expect("output container fits");
        meter.check(measured, root::worst_case(limits).expect("compatible root limits") + queue_bytes, limits);
        Self {
            meter,
            domain: Some(domain),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: *limits },
            out,
            queue_bytes,
            records: Vec::new(),
            prefix: None,
            saved: None,
            initial: None,
            held_prompt: None,
            prompt_copy: None,
            turn_copy: None,
            complete: None,
            admitted: None,
            read: None,
            probe: None,
            waiting: false,
            answer: None,
            calls: 0,
        }
    }

    fn outside(&self) -> u64 {
        let records = u64::try_from(self.records.capacity()).expect("bounded outside vector")
            * size::<session::record::Turn>()
            + self.records.iter().map(turn_bytes).sum::<u64>();
        self.queue_bytes
            + records
            + self.prefix.as_ref().map_or(0, transcript_bytes)
            + self.saved.as_ref().map_or(0, transcript_bytes)
            + self.initial.as_ref().map_or(0, prompt_bytes)
            + self.held_prompt.as_ref().map_or(0, prompt_bytes)
            + self.prompt_copy.as_ref().map_or(0, prompt_bytes)
            + self.turn_copy.as_ref().map_or(0, turn_bytes)
            + self.complete.as_ref().map_or(0, |complete| prompt_bytes(&complete.prompt))
    }

    fn drain(&mut self, measured: skein_world::domain::heap::Measured) {
        while let Some(request) = self.out.pop() {
            match request {
                Request::Admitted { worker, run } => {
                    assert_eq!(worker, WORKER);
                    assert!(self.admitted.replace(run).is_none());
                }
                Request::Read { owner, .. } => {
                    assert!(self.read.replace(owner).is_none());
                }
                Request::Probe { owner, .. } => {
                    assert!(self.probe.replace(owner).is_none());
                }
                Request::Complete {
                    owner,
                    prompt,
                    max_completion_bytes,
                    max_completion_blocks,
                    decoded_call_bytes,
                    max_failure_bytes,
                    ..
                } => {
                    self.calls += 1;
                    assert!(
                        self.complete
                            .replace(Complete {
                                owner,
                                prompt,
                                receiving: Receiving {
                                    max_completion_bytes,
                                    max_completion_blocks,
                                    decoded_call_bytes,
                                    max_failure_bytes,
                                }
                            })
                            .is_none()
                    );
                }
                Request::Turn { worker, number, turn, .. } => {
                    assert_eq!(worker, WORKER);
                    assert_eq!(number, u32::try_from(self.records.len()).expect("bounded records") + 1);
                    self.records.push(turn);
                }
                Request::Waiting { worker, .. } => {
                    assert_eq!(worker, WORKER);
                    self.waiting = true;
                }
                Request::Answer { to, answer } => {
                    assert_eq!(to.into_token(), PARENT);
                    assert!(self.answer.replace(answer).is_none(), "one original Start terminal");
                }
                Request::HostCall { .. }
                | Request::WithdrawHost { .. }
                | Request::Checking { .. }
                | Request::Deliver { .. }
                | Request::Rejected { .. }
                | Request::Exhausted { .. }
                | Request::Cancel { .. }
                | Request::Io { .. }
                | Request::CancelIo { .. }
                | Request::Check { .. }
                | Request::Abort { .. }
                | Request::MessageBounced { .. } => {
                    panic!("this positive Wait/text fixture owes only discovery, provider and parent observations")
                }
            }
        }
        let bound = root::worst_case(&self.env.limits).expect("compatible root limits") + self.outside();
        self.meter.check(measured, bound, &self.env.limits);
    }

    fn step(&mut self, event: Event) {
        self.meter.start();
        let domain = self.domain.as_mut().expect("live root");
        domain.reclaim();
        root::step(domain, &self.env, event, &mut self.out);
        while domain.pop_fact().is_some() {}
        while domain.pop_content().is_some() {}
        let measured = self.meter.end();
        self.drain(measured);
    }

    fn resume(&mut self) {
        self.meter.start();
        let domain = self.domain.as_mut().expect("live root");
        domain.reclaim();
        root::resume(domain, &self.env, &mut self.out);
        while domain.pop_fact().is_some() {}
        while domain.pop_content().is_some() {}
        let measured = self.meter.end();
        self.drain(measured);
    }

    fn start(&mut self, history: Option<root::Transcript>, restoring: bool) {
        self.step(Event::Start {
            reply_to: ReplyTo::new(PARENT),
            worker: WORKER,
            charter: charter(restoring),
            transcript: history,
            grants: Box::new([Grant {
                name: GrantName { account: 0, generation: 1 },
                valid: Duration::from_secs(3600),
            }]),
        });
        assert!(self.admitted.is_some() && self.read.is_some());
        assert_eq!(self.calls, 0, "selected Start ownership survives real outstanding discovery");
        assert!(self.complete.is_none());
    }

    fn discover(&mut self) {
        let owner = self.read.take().expect("actual root discovery request");
        self.step(Event::Read { owner, read: run::Read::Missing });
        if let Some(owner) = self.probe.take() {
            self.step(Event::Probed { owner, executable: false });
        }
    }

    fn cycle(&mut self, configuration: &Configuration, cycles: u32) {
        for _ in 0..100 {
            if let Some(complete) = self.complete.take() {
                if self.initial.is_none() {
                    self.initial = Some(copy_prompt(&complete.prompt));
                }
                let event = actual(complete, configuration, cycles);
                self.step(event);
            } else {
                self.resume();
            }
            if self.complete.is_none() && (self.waiting || self.answer.is_some()) {
                break;
            }
        }
        assert!(self.complete.is_none());
        assert!(!self.domain.as_ref().expect("live root").is_ready());
        assert!(self.waiting || self.answer.is_some(), "bounded root loop reached an outside wait or actual terminal");
    }

    fn replace_root(&mut self, limits: &Limits) {
        assert!(self.answer.is_some(), "previous original Start settled");
        let previous_bound = root::worst_case(&self.env.limits).expect("previous root bound") + self.outside();
        self.meter.start();
        drop(self.domain.take());
        let measured = self.meter.end();
        self.meter.check(measured, previous_bound, &self.env.limits);
        self.env.limits = *limits;
        self.answer = None;
        self.admitted = None;
        self.read = None;
        self.probe = None;
        self.waiting = false;
        self.calls = 0;
        self.records.clear();
        self.meter.start();
        self.domain = Some(Domain::new(limits, 42));
        let measured = self.meter.end();
        self.drain(measured);
    }
}

fn assert_generated(history: &root::Transcript, messages: u32) {
    assert_eq!(history.turns.len(), usize::try_from((messages - 4) / 2).expect("reachable Turns"));
    assert!(history.after.is_empty());
    let mut previous = None;
    let mut count = 0_u32;
    let mut large = 0;
    for (index, turn) in history.turns.iter().enumerate() {
        assert_eq!(turn.sequence, u32::try_from(index).expect("bounded sequence") + 1);
        assert_eq!(turn.version, session::record::VERSION);
        assert_eq!(turn.endpoint, session::llm::Endpoint(0));
        assert_eq!(turn.dialect, 2);
        assert_eq!(turn.messages.len(), if index % 2 == 0 { 3 } else { 1 });
        assert_eq!(turn.messages.iter().filter(|message| message.role == session::llm::Role::Assistant).count(), 1);
        for message in &turn.messages {
            if message.role == session::llm::Role::Assistant {
                assert_eq!(previous, Some(session::llm::Role::User), "whole-history predecessor, not per-Turn minimum");
            } else if previous.is_none() {
                assert_eq!(message.role, session::llm::Role::User);
            }
            previous = Some(message.role);
            count += 1;
            for block in &message.content {
                match block {
                    session::llm::Block::Text { text, .. } => {
                        if text.len() == LARGE {
                            assert!(text.iter().all(|byte| *byte == b'x'));
                            large += 1;
                        }
                    }
                    session::llm::Block::ToolCall { id, name, input, call, .. } => {
                        assert_eq!(id.as_ref(), ID);
                        assert_eq!(name.as_ref(), b"wait");
                        assert_eq!(input.as_ref(), b"{}");
                        assert_eq!(call, &session::llm::Decoded::Historical);
                    }
                    session::llm::Block::ToolResult {
                        id,
                        result: session::llm::Returned::Text { text, error, replay },
                    } => {
                        assert_eq!(id.as_ref(), ID);
                        assert_eq!(text.as_ref(), b"waiting");
                        assert!(!*error);
                        assert!(replay.is_none());
                    }
                    session::llm::Block::Refusal { .. }
                    | session::llm::Block::Opaque { .. }
                    | session::llm::Block::ToolResult { .. } => panic!("real supported fixture records"),
                }
            }
        }
    }
    assert_eq!(count, messages - 4, "maximum reachable history leaves waking and provider slots");
    assert_eq!(large, 1);
}

fn cap(source: &Counted) -> u64 {
    let first = source.initial.as_ref().expect("observed fresh root prompt");
    let [message] = first.messages.as_ref() else { panic!("fresh actual waking message") };
    let [llm::Block::Text { text, replay: None }] = message.content.as_ref() else { panic!("plain charter brief") };
    assert_eq!(text.as_ref(), b"Begin the work your brief describes.");
    assert_eq!(first.served.len(), 2);
    // Independent public opening ownership plus actual concrete record fields.
    // Message/Turn envelopes are deliberately separate from this payload cap.
    let opening = len(&first.model)
        + len(&first.system)
        + 2 * size::<session::llm::Descriptor>()
        + size::<session::llm::Block>()
        + len(text);
    let prefix = source
        .prefix
        .as_ref()
        .expect("actual prefix")
        .turns
        .iter()
        .flat_map(|turn| &turn.messages)
        .map(|message| record_blocks(&message.content))
        .sum::<u64>();
    opening
        + prefix
        + session::completion_reserve(&source.env.limits.session).expect("public required receiving credit")
}

fn generate_prefix(counted: &mut Counted, configuration: &Configuration, messages: u32, cycles: u32) -> Limits {
    counted.start(None, false);
    counted.discover();
    counted.cycle(configuration, cycles);
    assert!(counted.waiting);
    for at in 1..cycles {
        counted.waiting = false;
        let run = counted.admitted.expect("actual admitted handle");
        let text = if at == 1 { vec![b'x'; LARGE].into_boxed_slice() } else { b"x".as_slice().into() };
        counted.step(Event::Message { run, name: Token::new(u64::from(at)), text });
        counted.cycle(configuration, cycles);
        assert!(counted.waiting);
    }
    let copy_meter = Meter::new();
    copy_meter.start();
    let prefix = root::Transcript {
        version: session::record::VERSION,
        endpoint: session::llm::Endpoint(0),
        dialect: 2,
        turns: counted.records.clone().into(),
        after: Box::new([]),
    };
    let copied = copy_meter.end();
    let expected = transcript_bytes(&prefix);
    assert_eq!(copied.held(), expected, "concrete source-record handoff owns all public envelopes and payloads");
    assert!(copied.peak() <= expected, "record handoff has no unpriced construction transient");
    counted.prefix = Some(prefix);
    assert_generated(counted.prefix.as_ref().expect("actual selected history"), messages);
    counted.saved = counted.prefix.as_ref().map(copy_transcript);
    let capped = cap(counted);
    let tight = Limits {
        session: session::Limits { session_bytes: capped, ..counted.env.limits.session },
        ..counted.env.limits
    };
    let run = counted.admitted.expect("actual source run");
    counted.waiting = false;
    counted.step(Event::Cancel { run });
    counted.cycle(configuration, cycles);
    assert!(
        matches!(counted.answer, Some(run::Answer::Failed { failure: run::Failure::Cancelled, turns, .. }) if turns == 2 * cycles)
    );

    tight
}

fn restore_exact(counted: &mut Counted, configuration: &Configuration, messages: u32, cycles: u32, tight: &Limits) {
    counted.replace_root(tight);
    counted.start(counted.prefix.as_ref().map(copy_transcript), true);
    counted.discover();
    assert_eq!(counted.calls, 1, "exact reservation-inclusive payload is admitted before native work");
    let complete = counted.complete.as_ref().expect("actual rewritten Complete");
    assert_eq!(complete.prompt.messages.len(), usize::try_from(messages - 3).expect("restored plus waking"));
    assert_eq!(complete.prompt.model, counted.initial.as_ref().expect("original observation").model);
    assert_eq!(complete.prompt.system, counted.initial.as_ref().expect("original observation").system);
    assert_rewritten(counted.prefix.as_ref().expect("original generated prefix"), &complete.prompt);
    counted.held_prompt = Some(copy_prompt(&complete.prompt));
    counted.prompt_copy = counted.held_prompt.as_ref().map(copy_prompt);
    counted.cycle(configuration, cycles);
    let [turn] = counted.records.as_slice() else { panic!("one actual concrete restored Turn") };
    assert_eq!(turn.sequence, 2 * cycles + 1);
    assert_eq!(turn.messages.len(), 2);
    let [_, assistant] = turn.messages.as_ref() else { panic!("actual wake/assistant Turn") };
    let [session::llm::Block::Text { text, .. }] = assistant.content.as_ref() else {
        panic!("actual restored native text")
    };
    assert_eq!(text.as_ref(), RESTORED);
    counted.turn_copy = Some(copy_turn(turn));
    assert!(matches!(
        counted.answer,
        Some(run::Answer::Failed { failure: run::Failure::Model(run::Fault::ContextFull), turns: 1, .. })
    ));
    counted.resume();
}

fn refuse_one_over(counted: &mut Counted, configuration: &Configuration, cycles: u32, tight: &Limits) {
    for extra_message in [true, false] {
        let mut negative = *tight;
        if extra_message {
            negative.session.session_bytes += 16_384;
            assert!(size::<session::llm::Block>() + len(b"x") < 16_384, "count control has independent byte headroom");
        }
        counted.replace_root(&negative);
        let mut history = copy_transcript(counted.prefix.as_ref().expect("saved actual records"));
        if extra_message {
            history.after = Box::new([session::llm::Message {
                role: session::llm::Role::User,
                content: Box::new([session::llm::Block::Text { text: b"x".as_slice().into(), replay: None }]),
            }]);
        } else {
            let text = history
                .turns
                .iter_mut()
                .flat_map(|turn| &mut turn.messages)
                .flat_map(|message| &mut message.content)
                .find_map(|block| match block {
                    session::llm::Block::Text { text, .. } if text.len() == LARGE => Some(text),
                    session::llm::Block::Text { .. }
                    | session::llm::Block::Refusal { .. }
                    | session::llm::Block::Opaque { .. }
                    | session::llm::Block::ToolCall { .. }
                    | session::llm::Block::ToolResult { .. } => None,
                })
                .expect("literal generated large message");
            *text = vec![b'x'; LARGE + 1].into_boxed_slice();
        }
        counted.start(Some(history), true);
        counted.discover();
        counted.cycle(configuration, cycles);
        assert_eq!(counted.calls, 0, "one-over valid history starts no provider or owned tool");
        assert!(counted.records.is_empty());
        assert!(matches!(
            counted.answer,
            Some(run::Answer::Failed {
                failure: run::Failure::Transcript(run::TranscriptRefusal::TooLarge),
                turns: 0,
                ..
            })
        ));
    }
}

fn reclaim_every_owner(mut counted: Counted, tight: &Limits) {
    let final_bound = root::worst_case(tight).expect("compatible final caps") + counted.outside();
    counted.meter.start();
    drop(counted.domain.take());
    drop(counted.prefix.take());
    drop(counted.saved.take());
    drop(counted.initial.take());
    drop(counted.held_prompt.take());
    drop(counted.prompt_copy.take());
    drop(counted.turn_copy.take());
    counted.records.clear();
    counted.records.shrink_to_fit();
    let measured = counted.meter.end();
    counted.meter.check(measured, final_bound, tight);
    let Counted { meter, out, .. } = counted;
    drop(out);
    assert_eq!(meter.held(), 0, "all root allocations and retained outside copies reclaimed");
}

#[test]
fn restored_root_arrays_payload_rewrites_and_turn_copies_stay_within_the_attained_bound() {
    let configuration = wire::configurations().into_vec().pop().expect("actual native Anthropic fixture");
    for messages in [16, 32] {
        let cycles = (messages - 4) / 4;
        let mut counted = Counted::new(&bounds(messages));
        let tight = generate_prefix(&mut counted, &configuration, messages, cycles);
        restore_exact(&mut counted, &configuration, messages, cycles, &tight);
        refuse_one_over(&mut counted, &configuration, cycles, &tight);
        reclaim_every_owner(counted, &tight);
    }
}
