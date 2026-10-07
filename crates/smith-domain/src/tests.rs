//! Feed the domain events, inspect the requests that come out: the paths that
//! cross from the run to the sessions and back, the tickets the top level
//! keeps for them, and the hand-offs it defers to the ready list. What each
//! child domain does on its own is its own tests' business.

use alloc::boxed::Box;

use skein_lib::{Duration, Env, List, Queue, ReplyTo, Time, Token, Wall};
use smith_domain_run::charter::{Endpoint, Families, Grants, Llm, Tools};
use smith_domain_run::outcome::{Change, ChangeSpec, Declared, OutcomeSpec, Verdict, VerdictRule};
use smith_domain_run::{self as run, Ask, Charter};
use smith_domain_run::{Directory, Workspace};
use smith_domain_session as session;

use crate::limits;
use crate::llm::{Block, Completion, Decoded, Message, Problem, Prompt, Returned, Role, Said, Served, Stop, Usage};
use crate::tools::{self, Call, Name, Op, Part, Path, Place};
use crate::{Domain, Event, Fact, Limits, Request, fire, max_out, resume, step, worst_case};

const BUDGET: run::Budget = run::Budget { turns: 10, spend: 1, time: Duration::from_secs(600) };

const CEILING: session::Budget = session::Budget {
    turns: 100,
    input: 1_000_000,
    output: 100_000,
    cache_read: 1_000_000,
    cache_write: 1_000_000,
    time: Duration::from_secs(3600),
};

const LIMITS: Limits = Limits {
    accounts: 4,
    endpoints: 3,
    decoded_call_bytes: 4096,
    skew: Duration::ZERO,
    run: run::Limits {
        runs: 2,
        conversations: 4,
        run_bytes: 4096,
        brief_sections: 4,
        directories: 2,
        directory_name_bytes: 64,
        conflicts: 64,
        conflict_path_bytes: 4096,
        host_tools: 2,
        host_input_bytes: 65_536,
        host_reply_bytes: 65_536,
        host_timeout: Duration::from_secs(60),
        host_backoff: Duration::from_millis(50),
        verdicts: 2,
        calls: 4,
        budget: run::Budget { turns: CEILING.turns, spend: 1, time: CEILING.time },
        max_tokens: 1024,
        models: 1,
        run_conversations: 3,
        answer_bytes: 64,
        nudges: 1,
        guide_bytes: 64,
        io_timeout: Duration::from_secs(10),
        outcome_bytes: 1024,
        delivery_timeout: Duration::from_secs(300),
        check_timeout: Duration::from_secs(300),
        check_tail: 256,
        facts: 64,
        messages: 8,
        message_bytes: 4096,
        waiting: Duration::from_secs(300),
    },
    session: session::Limits {
        sessions: 4,
        spend: 1,
        messages: 16,
        session_bytes: 2_097_152,
        completion_bytes: 4096,
        completion_blocks: 16,
        failure_bytes: 512,
        delegated_result_bytes: 262_144,
        budget: CEILING,
        max_tokens: 1024,
        retries: 1,
        backoff_base: Duration::from_millis(100),
        backoff_max: Duration::from_secs(1),
        call_timeout: Duration::from_secs(30),
        tool_timeout: Duration::from_secs(20),
        facts: 64,
        parallel_tools: 2,
        tools: tools::Limits {
            kits: 4,
            calls: 4,
            repos: 2,
            path_bytes: 64,
            known_files: 8,
            file_bytes: 65_536,
            read_bytes: 4096,
            list_entries: 16,
            list_bytes: 4096,
            match_lines: 4,
            file_timeout: Duration::from_secs(60),
            env_bytes: 64,
            shell_timeout: Duration::from_secs(60),
            shell_timeout_max: Duration::from_secs(600),
            shell_head: 64,
            shell_tail: 64,
            search_hits: 8,
            search_bytes: 256,
            search_timeout: Duration::from_secs(30),
            facts: 64,
        },
    },
};

const USAGE: Usage = Usage { input_tokens: 100, output_tokens: 10, cache_read_tokens: 0, cache_write_tokens: 0 };

#[test]
fn decoded_batch_reserves_refusal_cells_before_any_payload() {
    let cell = u64::try_from(size_of::<Decoded>()).expect("cell size fits");
    let offered = crate::translate::Offered {
        host_tools: Box::new([]),
        finish: false,
        deliver: false,
        agents: false,
        wait: true,
    };
    let mut peer = crate::peer::Peer::new(Token::new(17), 1, 0, offered, &LIMITS.session);
    let completion = Completion {
        content: Box::new([
            Said::ToolCall {
                id: skein_lib::bytes::copy_of(b"oversized"),
                name: skein_lib::bytes::copy_of(b"invalid"),
                input: skein_lib::bytes::copy_of(b"{}"),
                call: Decoded::Invalid { problem: Problem::Missing { field: Box::new([b'x'; 32]) } },
                replay: None,
            },
            Said::ToolCall {
                id: skein_lib::bytes::copy_of(b"fits"),
                name: skein_lib::bytes::copy_of(b"wait"),
                input: skein_lib::bytes::copy_of(b"{}"),
                call: Decoded::Served { ask: Ask::Wait },
                replay: None,
            },
        ]),
        stop: Stop::ToolUse,
        usage: USAGE,
    };
    let completion = peer.completion(completion, cell.checked_mul(2).expect("two cells fit"));
    let mut calls = List::with_capacity(2);
    for block in completion.content {
        match block {
            session::llm::Block::ToolCall { call, .. } => calls.push(call).expect("two calls"),
            session::llm::Block::Text { .. }
            | session::llm::Block::Refusal { .. }
            | session::llm::Block::Opaque { .. }
            | session::llm::Block::ToolResult { .. } => unreachable!("provider supplied only calls"),
        }
    }
    match calls.get(0).expect("first call") {
        session::llm::Decoded::Invalid { problem } => assert_eq!(problem, &Problem::TooLarge),
        session::llm::Decoded::Owned { .. }
        | session::llm::Decoded::Delegated { .. }
        | session::llm::Decoded::Historical => unreachable!("oversized payload is refused"),
    }
    match calls.get(1).expect("second call") {
        session::llm::Decoded::Delegated { .. } => assert_eq!(peer.tickets(), 1),
        session::llm::Decoded::Owned { .. }
        | session::llm::Decoded::Invalid { .. }
        | session::llm::Decoded::Historical => unreachable!("later payload-free wait fits"),
    }
}

#[test]
fn decoded_receiving_cells_are_required_before_provider_work() {
    let mut limits = LIMITS;
    limits.decoded_call_bytes = u64::from(limits.session.completion_blocks)
        .checked_mul(u64::try_from(size_of::<Decoded>()).expect("cell size fits"))
        .expect("configured cells fit");
    assert!(worst_case(&limits).is_some());
    limits.decoded_call_bytes = limits.decoded_call_bytes.checked_sub(1).expect("nonempty receiving cells");
    assert!(worst_case(&limits).is_none());
}

/// The domain, its environment, and room for one entry point's output.
struct Harness {
    domain: Domain,
    env: Env<Limits>,
    out: Queue<Request>,
    turns: List<crate::Turn>,
}

impl Harness {
    fn new() -> Harness {
        Harness::with(&LIMITS)
    }

    fn with(limits: &Limits) -> Harness {
        Harness {
            domain: Domain::new(limits, crate::Config { endpoints: Box::new([Endpoint(1), Endpoint(2)]) }, 1),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: *limits },
            out: Queue::with_capacity(max_out(limits)),
            turns: List::with_capacity(CEILING.turns),
        }
    }

    /// Steps `event`, returning what it emitted, oldest first.
    fn step(&mut self, event: Event) -> Box<[Request]> {
        step(&mut self.domain, &self.env, event, &mut self.out);
        self.drain()
    }

    fn fire(&mut self) -> Box<[Request]> {
        assert!(self.domain.is_due(self.env.now), "an alarm is due");
        fire(&mut self.domain, &self.env, &mut self.out);
        self.drain()
    }

    /// The loop's next iteration, as far as the domain goes: the reclaim point
    /// of this one, then whatever is ready, at the start of the domain's stage.
    fn next(&mut self) -> Box<[Request]> {
        self.domain.reclaim();
        let mut all = List::with_capacity(max_out(&LIMITS));
        for _ in 0..max_out(&LIMITS) {
            if !self.domain.is_ready() {
                break;
            }
            resume(&mut self.domain, &self.env, &mut self.out);
            for request in self.drain() {
                all.push(request).expect("a test's iteration emits little");
            }
        }
        assert!(!self.domain.is_ready(), "what is made ready while the list is drained waits for the next iteration");
        all.into_boxed()
    }

    fn drain(&mut self) -> Box<[Request]> {
        let mut requests = List::with_capacity(max_out(&LIMITS));
        for _ in 0..max_out(&LIMITS) {
            let Some(request) = self.out.pop() else { break };
            match request {
                Request::Turn { turn, number, .. } => {
                    assert_eq!(
                        number,
                        self.turns.len().checked_add(1).expect("next actual turn number"),
                        "actual main output numbering"
                    );
                    self.turns.push(turn).expect("bounded activation fixture turns");
                }
                Request::Answer { to, answer } => {
                    match &answer {
                        run::Answer::Parked { turns, .. }
                        | run::Answer::Accepted { turns, .. }
                        | run::Answer::Failed { turns, .. } => {
                            assert_eq!(*turns, self.turns.len(), "terminal follows every actual Turn");
                        }
                        run::Answer::Refused(_) => assert!(self.turns.is_empty()),
                    }
                    requests.push(Request::Answer { to, answer }).expect("room for max_out");
                }
                request @ (Request::HostCall { .. }
                | Request::WithdrawHost { .. }
                | Request::Admitted { .. }
                | Request::Checking { .. }
                | Request::Deliver { .. }
                | Request::Complete { .. }
                | Request::Rejected { .. }
                | Request::Exhausted { .. }
                | Request::Cancel { .. }
                | Request::Io { .. }
                | Request::CancelIo { .. }
                | Request::Read { .. }
                | Request::Probe { .. }
                | Request::Check { .. }
                | Request::Abort { .. }
                | Request::Waiting { .. }) => {
                    requests.push(request).expect("room for max_out");
                }
            }
        }
        assert!(self.out.is_empty(), "an entry point emits at most max_out");
        requests.into_boxed()
    }

    /// Starts a run of `charter` for call `call`, and has it find no guide:
    /// the run's token, and main's first call to its LLM.
    fn admit(&mut self, call: u64, charter: Charter) -> (Token, Token, Prompt) {
        let emitted = self.step(Event::Start {
            answered: Box::default(),
            workspace: Some(workspace()),
            grants: Box::new([crate::Grant {
                name: crate::GrantName { account: 0, generation: 0 },
                valid: Duration::from_secs(100_000),
            }]),
            reply_to: ReplyTo::new(Token::new(call)),
            host_run: Token::new(call),
            activation: 1,
            charter,
            transcript: None,
        });
        let [Request::Admitted { host_run: _, run }, Request::Read { owner, .. }] = &*emitted else {
            panic!("expected an admitted run, got {emitted:?}");
        };
        assert_eq!(owner, run);
        let run = *run;
        let emitted = self.step(Event::Read { owner: run, read: run::Read::Missing });
        let prepared = match emitted.as_ref() {
            [Request::Probe { owner, .. }] => self.step(Event::Probed { owner: *owner, executable: false }),
            [Request::Complete { .. }] => emitted,
            _ => panic!("expected convention probe or prepared main, got {emitted:?}"),
        };
        let (main, prompt) = completing(prepared);
        (run, main, prompt)
    }

    /// The session `owner`'s LLM answers with `content`, stopping for tools.
    fn answer(&mut self, owner: Token, content: Box<[Said]>) -> Box<[Request]> {
        let completion = Completion { content, stop: Stop::ToolUse, usage: USAGE };
        self.step(Event::Completed { owner, completion })
    }

    /// The session `owner`'s LLM ends its turn saying `text`.
    fn says(&mut self, owner: Token, text: &[u8]) -> Box<[Request]> {
        let completion = Completion {
            content: Box::new([Said::Text { text: bytes(text), replay: None }]),
            stop: Stop::EndTurn,
            usage: USAGE,
        };
        self.step(Event::Completed { owner, completion })
    }
}

fn bytes(text: &[u8]) -> Box<[u8]> {
    Box::from(text)
}

fn workspace() -> Workspace {
    Workspace {
        directories: Box::new([Directory {
            name: bytes(b"temper"),
            root: Token::new(900),
            writable: true,
            git: true,
            conflicts: Box::new([]),
        }]),
    }
}

fn charter() -> Charter {
    Charter {
        instructions: Box::new([]),
        brief: smith_domain_run::Brief {
            sections: Box::new([smith_domain_run::Section {
                title: b"Task".as_slice().into(),
                text: bytes(b"Review the change."),
            }]),
        },

        grants: Grants { wait: true, deliver: None, tools: TOOLS, agents: true, host_tools: Box::new([]) },
        outcome: OutcomeSpec { change: None, verdicts: Box::new([rule(b"approve")]), report: None, failure: None },
        budget: BUDGET,
        llm: Llm {
            prices: run::Prices { input: 0, cached: 0, output: 0, unit: 1 },
            account: 0,
            endpoint: Endpoint(1),
            model: bytes(b"model-a"),
            max_tokens: 512,
            dialect: 1,
        },
        models: Box::new([]),
        conventions: Some(smith_domain_run::Conventions {
            guide: b"AGENTS.md".as_slice().into(),
            checks: b".temper/pre-pr".as_slice().into(),
        }),
        resume: false,
        waiting: Duration::from_secs(30),
    }
}

#[test]
fn root_refuses_invalid_grants_before_admission() {
    let mut h = Harness::new();
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        workspace: Some(workspace()),
        grants: Box::new(
            [crate::Grant { name: crate::GrantName { account: 0, generation: 0 }, valid: Duration::from_secs(60) }; 5],
        ),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: charter(),
        transcript: None,
    });
    let [Request::Answer { answer, .. }] = &*emitted else { panic!("expected admission refusal: {emitted:?}") };
    assert_eq!(answer, &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Grants)));
}

#[test]
fn root_refuses_unconfigured_main_and_sub_agent_endpoints_before_admission() {
    for (main, child) in [(Endpoint(3), Endpoint(2)), (Endpoint(1), Endpoint(3))] {
        let mut h = Harness::new();
        let mut requested = charter();
        requested.llm.endpoint = main;
        requested.models = Box::new([Llm { endpoint: child, model: bytes(b"model-b"), ..requested.llm.clone() }]);
        let emitted = h.step(Event::Start {
            answered: Box::default(),
            workspace: None,
            grants: Box::new([]),
            reply_to: ReplyTo::new(Token::new(7)),
            host_run: Token::new(7),
            activation: 1,
            charter: requested,
            transcript: None,
        });
        assert_eq!(
            &*emitted,
            &[Request::Answer {
                to: ReplyTo::new(Token::new(7)),
                answer: run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Endpoint)),
            }]
        );
        assert_eq!(h.domain.peers(), 0);
    }
    let mut h = Harness::new();
    let mut requested = charter();
    requested.models = Box::new([Llm { endpoint: Endpoint(2), model: bytes(b"model-b"), ..requested.llm.clone() }]);
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        workspace: None,
        grants: Box::new([]),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: requested,
        transcript: None,
    });
    let [Request::Admitted { .. }] = &*emitted else {
        panic!("configured endpoints admit the start: {emitted:?}");
    };
}

#[test]
fn root_refuses_incompatible_conversation_limits_before_admission() {
    let limits = Limits { run: run::Limits { directories: 3, ..LIMITS.run }, ..LIMITS };
    let mut h = Harness::with(&limits);
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        workspace: Some(workspace()),
        grants: Box::new([]),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: charter(),
        transcript: None,
    });
    let [Request::Answer { answer, .. }] = &*emitted else { panic!("expected admission refusal: {emitted:?}") };
    assert_eq!(answer, &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Conversation)));
}

#[test]
fn oversized_resume_history_is_refused_when_the_conversation_opens() {
    let mut h = Harness::new();
    let mut turns = List::with_capacity(17);
    for sequence in 1..=17_u32 {
        turns
            .push(session::record::Turn {
                version: session::record::VERSION,
                endpoint: session::llm::Endpoint(1),
                dialect: 1,
                sequence,
                usage: Usage::ZERO,
                spent: 0,
                messages: Box::new([session::llm::Message { role: Role::User, content: Box::new([]) }]),
            })
            .expect("room for each oversized turn");
    }
    let transcript = session::record::Transcript {
        version: session::record::VERSION,
        endpoint: session::llm::Endpoint(1),
        dialect: 1,
        turns: turns.into_boxed(),
    };
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        workspace: Some(workspace()),
        grants: Box::new([]),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: Charter { resume: true, ..charter() },
        transcript: Some(transcript),
    });
    let [Request::Admitted { run, .. }, Request::Read { .. }] = &*emitted else {
        panic!("expected an admitted run: {emitted:?}")
    };
    let emitted = h.step(Event::Read { owner: *run, read: run::Read::Missing });
    let emitted = match emitted.as_ref() {
        [Request::Probe { owner, .. }] => h.step(Event::Probed { owner: *owner, executable: false }),
        _ => emitted,
    };
    let [Request::Answer { answer, .. }] = &*emitted else { panic!("expected history refusal: {emitted:?}") };
    match answer {
        run::Answer::Failed { failure, spent, turns } => {
            assert_eq!(
                (*failure, *spent, *turns),
                (run::Failure::Transcript(run::TranscriptRefusal::TooLarge), run::Spend::ZERO, 0)
            );
        }
        run::Answer::Refused(_) | run::Answer::Parked { .. } | run::Answer::Accepted { .. } => {
            panic!("expected transcript refusal: {answer:?}")
        }
    }
}

const TOOLS: Tools = Tools { inspect: true, modify: true, shell: false };

fn rule(name: &[u8]) -> VerdictRule {
    VerdictRule {
        name: bytes(name),
        text_max: 1024,
        fields: Box::new([]),
        items: smith_domain_run::outcome::ItemSpec { min: 0, max: 0, kinds: Box::new([]) },
    }
}

fn verdict(name: &[u8]) -> Ask {
    Ask::Finish {
        outcome: Declared::Verdict(Verdict {
            name: bytes(name),
            text: bytes(b"ok"),
            items: Box::new([]),
            fields: Box::new([]),
        }),
    }
}

/// A sub-agent that may only read, and itself ask for sub-agents.
fn sub_agent(brief: &[u8]) -> Ask {
    let families = Families { tools: Tools { inspect: true, modify: false, shell: false }, agents: true };
    Ask::SubAgent { brief: bytes(brief), families, llm: None, share: None }
}

fn served(id: &[u8], ask: Ask) -> Said {
    Said::ToolCall {
        id: bytes(id),
        name: bytes(b"served"),
        input: bytes(b"{}"),
        call: Decoded::Served { ask },
        replay: None,
    }
}

fn owned(id: &[u8], call: Call) -> Said {
    Said::ToolCall {
        id: bytes(id),
        name: bytes(b"read"),
        input: bytes(b"{}"),
        call: Decoded::Owned { call },
        replay: None,
    }
}

fn path(absolute: bool, names: &[&[u8]]) -> Path {
    let mut parts = List::with_capacity(4);
    for name in names {
        parts.push(Part::Name { name: Name::new(bytes(name)).expect("a name") }).expect("short paths");
    }
    Path { absolute, parts: parts.into_boxed() }
}

/// The one call to an LLM `emitted` holds: its owner and its prompt.
fn completing(emitted: Box<[Request]>) -> (Token, Prompt) {
    let Ok(one) = Box::<[Request; 1]>::try_from(emitted) else {
        panic!("expected one request");
    };
    let [Request::Complete { owner, prompt, timeout, grant: _, .. }] = *one else {
        panic!("expected a call to an LLM");
    };
    assert_eq!(timeout, LIMITS.session.call_timeout);
    (owner, prompt)
}

/// The blocks of `prompt`'s last message.
fn last(prompt: &Prompt) -> &[Block] {
    &prompt.messages.last().expect("a prompt has messages").content
}

#[test]
fn opaque_reasoning_returns_in_its_position_without_becoming_text() {
    let mut h = Harness::new();
    let (_, owner, _) = h.admit(7, charter());
    let content = Box::new([
        Said::Opaque { bytes: bytes(b"provider reasoning") },
        owned(b"r", Call::Read { path: path(false, &[b"src"]), skip: 0, lines: Some(1) }),
    ]);
    let emitted = h.answer(owner, content);
    let [Request::Io { owner: tool, .. }] = &*emitted else { panic!("expected read") };
    let done = tools::Done::Loaded { content: bytes(b"line"), version: tools::Version::new([1; 4]) };
    let (_, prompt) = completing(h.step(Event::Done { owner: *tool, done }));
    let assistant = prompt.messages.get(1).expect("assistant message kept");
    let Some(Block::Opaque { bytes: value }) = assistant.content.first() else { panic!("opaque block preserved") };
    assert_eq!(value.as_ref(), b"provider reasoning");
    let Some(crate::Content::Call { input, .. }) = h.domain.pop_content() else { panic!("call content is available") };
    assert_eq!(input.as_ref(), b"{}", "call arguments remain content");
    let Some(crate::Content::Usage { usage, .. }) = h.domain.pop_content() else { panic!("usage fact") };
    assert_eq!(usage, USAGE);
    let Some(crate::Content::Tool { done: tools::Done::Loaded { content, .. }, .. }) = h.domain.pop_content() else {
        panic!("tool output content")
    };
    assert_eq!(content.as_ref(), b"line");
}

#[test]
fn content_overflow_drops_and_counts_without_changing_decisions() {
    let mut keeping = Harness::new();
    let limits = Limits { session: session::Limits { facts: 0, ..LIMITS.session }, ..LIMITS };
    let mut dropping = Harness::with(&limits);
    let (_, kept, _) = keeping.admit(7, charter());
    let (_, dropped, _) = dropping.admit(7, charter());
    assert_eq!(keeping.says(kept, b"working"), dropping.says(dropped, b"working"));
    assert!(keeping.domain.pop_content().is_some());
    assert!(dropping.domain.pop_content().is_none());
    assert!(dropping.domain.facts_lost() > keeping.domain.facts_lost(), "content loss is counted");
}

#[test]
fn grant_updates_do_not_change_a_call_in_flight_and_rejections_are_once_per_generation() {
    let limits = Limits { session: session::Limits { retries: 3, ..LIMITS.session }, ..LIMITS };
    let mut h = Harness::with(&limits);
    let (_, owner, _) = h.admit(7, charter());
    let next = crate::Grant { name: crate::GrantName { account: 0, generation: 1 }, valid: Duration::from_secs(60) };
    assert!(h.step(Event::Grant { grant: next }).is_empty());
    assert_eq!(
        &*h.step(Event::Failed {
            owner,
            failure: crate::llm::Failure::Unauthorized,
            evidence: crate::llm::Evidence::Unknown,
            detail: Box::default()
        }),
        &[Request::Rejected { grant: crate::GrantName { account: 0, generation: 0 } }]
    );
    h.env.now = h.domain.next_deadline().expect("unauthorized retry backoff");
    let emitted = h.fire();
    let [Request::Complete { grant, .. }] = &*emitted else { panic!("expected retry") };
    assert_eq!(*grant, next.name);
    assert_eq!(
        &*h.step(Event::Failed {
            owner,
            failure: crate::llm::Failure::Unauthorized,
            evidence: crate::llm::Evidence::Unknown,
            detail: Box::default()
        }),
        &[Request::Rejected { grant: next.name }]
    );
    h.env.now = h.domain.next_deadline().expect("second retry");
    drop(h.fire());
    assert!(
        h.step(Event::Failed {
            owner,
            failure: crate::llm::Failure::Unauthorized,
            evidence: crate::llm::Evidence::Unknown,
            detail: Box::default()
        })
        .is_empty()
    );
}

#[test]
fn no_grant_fails_locally_and_exhaustion_reports_the_account() {
    let mut h = Harness::new();
    let mut ungranted = charter();
    ungranted.llm.account = 9;
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        workspace: Some(workspace()),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: ungranted,
        grants: Box::new([]),
        transcript: None,
    });
    let [Request::Admitted { run, .. }, Request::Read { .. }] = &*emitted else { panic!("expected admission") };
    let emitted = h.step(Event::Read { owner: *run, read: run::Read::Missing });
    let [Request::Probe { .. }] = &*emitted else { panic!("writable directory probes checks: {emitted:?}") };
    assert!(h.step(Event::Probed { owner: *run, executable: false }).is_empty(), "no provider request without a grant");
    assert!(h.domain.next_deadline().is_some(), "local unauthorized waits for a grant");
    let mut h = Harness::new();
    let (_, owner, _) = h.admit(7, charter());
    let retry_after = Duration::from_secs(30);
    let emitted = h.step(Event::Failed {
        owner,
        failure: crate::llm::Failure::Exhausted { retry_after },
        evidence: crate::llm::Evidence::Unknown,
        detail: Box::default(),
    });
    let Some(Request::Exhausted { account: 0, retry_after: span }) = emitted.first() else {
        panic!("exhaustion notice")
    };
    assert_eq!(*span, retry_after);
    let mut failed = false;
    for request in &emitted {
        if let Request::Answer {
            answer:
                run::Answer::Failed {
                    failure:
                        run::Failure::Model(run::Fault::Completion {
                            failure: run::CompletionFailure::Exhausted { .. },
                            evidence: run::CompletionEvidence::Unknown,
                        }),
                    ..
                },
            ..
        } = request
        {
            failed = true;
        }
    }
    assert!(failed, "the exhausted account ends the run");
}

#[test]
fn the_limits_fit_and_a_session_must_take_what_the_run_asks() {
    assert!(worst_case(&LIMITS).is_some());
    let fewer = Limits { session: session::Limits { sessions: 3, spend: 1, ..LIMITS.session }, ..LIMITS };
    assert_eq!(worst_case(&fewer), None, "a session for every conversation the run may have");
    let budget = session::Budget { turns: 50, ..CEILING };
    let smaller = Limits { session: session::Limits { budget, ..LIMITS.session }, ..LIMITS };
    assert_eq!(worst_case(&smaller), None, "a session's budget as large as the run's");
}

#[test]
fn what_an_entry_point_may_emit_grows_with_the_tools_cancels_only_once() {
    assert_eq!(max_out(&LIMITS), 573);
    // A kit's close cancels as many operations as the tools run, which go out
    // to io and lead nowhere else: what the run is sent does not grow with
    // them.
    let tools = tools::Limits { calls: 256, ..LIMITS.session.tools };
    let wide = Limits { session: session::Limits { parallel_tools: 8, tools, ..LIMITS.session }, ..LIMITS };
    // Four session records plus eight delegated calls may reach the run;
    // each can open/say at most MAX_OUT sessions. The kit's io cancels do not.
    assert_eq!(limits::session_steps(&wide), 50);
    assert_eq!(max_out(&wide), 14601);
    let wider =
        Limits { session: session::Limits { tools: tools::Limits { calls: 512, ..tools }, ..wide.session }, ..wide };
    assert_eq!(limits::run_out(&wider), limits::run_out(&wide), "io cancels never multiply run hand-offs");
    assert_eq!(max_out(&wider) - max_out(&wide), 50 * 256, "one additional kit cancel per session step");
}

#[test]
fn a_run_opens_main_as_a_session_through_to_its_first_call_to_the_llm() {
    let mut h = Harness::new();
    let (_, main, prompt) = h.admit(7, charter());
    assert_eq!((prompt.endpoint, &*prompt.model, prompt.max_tokens), (session::llm::Endpoint(1), &b"model-a"[..], 512));
    assert_eq!(prompt.tools, tools::Grants { inspect: true, modify: true, shell: false });
    assert_eq!(
        &*prompt.served,
        &[Served::Wait, Served::Finish, Served::SubAgent],
        "main may finish, and ask for sub-agents"
    );
    let [Message { role: Role::User, content }] = &*prompt.messages else {
        panic!("expected the first message only");
    };
    assert!(matches_text(content), "the run's first message");
    assert_eq!((h.domain.peers(), h.domain.tickets(), h.domain.flights()), (1, 0, 0));

    // Relative paths start in the first repository, mounted under its name.
    let relative = Call::Read { path: path(false, &[b"src", b"lib.rs"]), skip: 0, lines: None };
    let absolute = Call::Read { path: path(true, &[b"temper", b"README"]), skip: 0, lines: None };
    let emitted = h.answer(main, Box::new([owned(b"c1", relative), owned(b"c2", absolute)]));
    let [Request::Io { op: Op::Load { at: first, .. }, .. }, Request::Io { op: Op::Load { at: second, .. }, .. }] =
        &*emitted
    else {
        panic!("expected two loads, got {emitted:?}");
    };
    assert_eq!(first, &Place { root: Token::new(900), path: bytes(b"src/lib.rs") });
    assert_eq!(second, &Place { root: Token::new(900), path: bytes(b"README") });
}

fn matches_text(content: &[Block]) -> bool {
    match content {
        [Block::Text { text, replay: None }] => !text.is_empty(),
        _ => false,
    }
}

#[test]
fn an_unsafe_workspace_mount_refuses_start_before_discovery() {
    let mut h = Harness::new();
    let checkout = Workspace {
        directories: Box::new([Directory {
            name: bytes(b"ai/temper"),
            root: Token::new(900),
            writable: true,
            git: true,
            conflicts: Box::new([]),
        }]),
    };
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        grants: Box::new([crate::Grant {
            name: crate::GrantName { account: 0, generation: 0 },
            valid: Duration::from_secs(100_000),
        }]),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: charter(),
        workspace: Some(checkout),
        transcript: None,
    });
    let requests = Box::<[Request; 1]>::try_from(emitted).expect("one refusal terminal, no admission or discovery");
    let [Request::Answer { to, answer }] = *requests else {
        panic!("unsafe mount refuses before admission or discovery");
    };
    assert_eq!(to.into_token(), Token::new(7));
    assert_eq!(answer, run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Workspace)));
    assert_eq!(h.domain.peers(), 0);
}

#[test]
fn an_opening_larger_than_a_session_holds_refuses_main_as_invalid() {
    // Limits a session honours whatever the charter, but not every opening:
    // that is the charter's, and is refused at the sessions' entrance.
    let run = run::Limits { run_bytes: 16_384, ..LIMITS.run };
    let limits = Limits {
        run,
        session: session::Limits {
            session_bytes: 8192,
            completion_bytes: 128,
            completion_blocks: 1,
            delegated_result_bytes: crate::feedback_worst_case(&run).expect("complete compatible feedback"),
            ..LIMITS.session
        },
        ..LIMITS
    };
    assert!(worst_case(&limits).is_some());
    let mut h = Harness::with(&limits);
    let brief = filler(10_240);
    let emitted = h.step(Event::Start {
        answered: Box::default(),
        workspace: Some(workspace()),
        grants: Box::new([crate::Grant {
            name: crate::GrantName { account: 0, generation: 0 },
            valid: Duration::from_secs(100_000),
        }]),
        reply_to: ReplyTo::new(Token::new(7)),
        host_run: Token::new(7),
        activation: 1,
        charter: Charter { instructions: brief, ..charter() },
        transcript: None,
    });
    let [Request::Admitted { run, .. }, Request::Read { .. }] = &*emitted else {
        panic!("expected an admitted run, got {emitted:?}");
    };
    let emitted = h.step(Event::Read { owner: *run, read: run::Read::Missing });
    let [Request::Probe { .. }] = &*emitted else {
        panic!("writable directory probes checks before opening: {emitted:?}");
    };
    let emitted = h.step(Event::Probed { owner: *run, executable: false });
    let [Request::Answer { to: _, answer }] = &*emitted else {
        panic!("expected the run's answer, got {emitted:?}");
    };
    assert_eq!(answer, &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Conversation)));
    h.domain.reclaim();
    assert_eq!((h.domain.peers(), h.domain.session().sessions()), (0, 0));
}

#[test]
fn a_finish_the_run_rejects_at_once_comes_back_from_the_ready_list_as_the_sessions_answer() {
    let mut h = Harness::new();
    let (_, main, _) = h.admit(7, charter());
    // The run judges the finish within the step, and rejects it; the answer
    // waits for the next iteration.
    assert!(h.answer(main, Box::new([served(b"f1", verdict(b"merge"))])).is_empty());
    assert_eq!((h.domain.tickets(), h.domain.flights()), (0, 1), "concrete rejection waits on its actual call right");
    assert!(!h.domain.is_ready(), "not before the reclaim point");

    let (owner, prompt) = completing(h.next());
    assert_eq!(owner, main);
    let [Block::ToolResult { id, result: Returned::Text { text, error: true, replay: None } }] = last(&prompt) else {
        panic!("expected the rejection, got {:?}", last(&prompt));
    };
    assert_eq!(&**id, b"f1");
    assert!(text.starts_with(b"rejected"), "canonical rejection retains its problem details: {text:?}");
    let [_, Message { role: Role::Assistant, content }, _] = &*prompt.messages else {
        panic!("expected the call and its result");
    };
    assert_eq!(
        &**content,
        &[Block::ToolCall { id: bytes(b"f1"), name: bytes(b"served"), input: bytes(b"{}"), replay: None }]
    );
    assert_eq!(
        (h.domain.tickets(), h.domain.flights()),
        (0, 0),
        "concrete answer stays in the session transcript without a local ticket"
    );
}

#[test]
fn an_accepted_finish_closes_main_and_the_run_answers_the_worker() {
    let mut h = Harness::new();
    let (_, main, _) = h.admit(7, charter());
    assert!(h.answer(main, Box::new([served(b"f1", verdict(b"approve"))])).is_empty());
    // The answer and the close come in the next iteration: the close first,
    // which withdraws the call, whose answer has won.
    let emitted = h.next();
    let [Request::Answer { to, answer: run::Answer::Accepted { outcome: _, spent, .. } }] = &*emitted else {
        panic!("expected the run accepted, got {emitted:?}");
    };
    assert_eq!(to, &ReplyTo::new(Token::new(7)));
    assert_eq!((spent.turns, spent.input), (1, USAGE.input_tokens));
    h.domain.reclaim();
    assert_eq!((h.domain.peers(), h.domain.tickets(), h.domain.flights()), (0, 0, 0), "main's tickets went with it");
}

#[test]
fn a_change_lands_through_the_worker_and_the_run_answers_with_it() {
    let mut h = Harness::new();
    let outcome = OutcomeSpec {
        change: Some(ChangeSpec {
            checks_must_pass: true,
            fields: Box::new([
                smith_domain_run::outcome::FieldRule { name: b"title".as_slice().into(), max: 1024 },
                smith_domain_run::outcome::FieldRule { name: b"body".as_slice().into(), max: 1024 },
            ]),
        }),
        verdicts: Box::new([]),
        report: None,
        failure: None,
    };
    let (_, main, _) = h.admit(7, Charter { outcome, ..charter() });
    let change = Change {
        fields: Box::new([
            smith_domain_run::outcome::Field { name: b"title".as_slice().into(), value: bytes(b"Fix the bug") },
            smith_domain_run::outcome::Field { name: b"body".as_slice().into(), value: bytes(b"It was in main.rs.") },
        ]),
    };
    let finish = Ask::Finish { outcome: Declared::Change(change.clone()) };
    let emitted = h.answer(main, Box::new([served(b"f1", finish)]));
    let [Request::Deliver { host_run, owner, change: pushed, name: _, deadline: _ }] = &*emitted else {
        panic!("expected the change pushed, got {emitted:?}");
    };
    assert_eq!((host_run, pushed), (&Token::new(7), &change));
    assert!(
        h.step(Event::Delivered { owner: *owner, delivery: delivered() }).is_empty(),
        "the answer and the close wait"
    );
    let emitted = h.next();
    let [Request::Answer { to: _, answer: run::Answer::Accepted { outcome, spent: _, .. } }] = &*emitted else {
        panic!("expected the run accepted, got {emitted:?}");
    };
    assert_eq!(outcome, &Declared::Change(change));
    h.domain.reclaim();
    assert_eq!((h.domain.peers(), h.domain.flights(), h.domain.tickets()), (0, 0, 0));
}

#[test]
fn a_sub_agent_opens_a_child_session_whose_last_message_answers_the_call() {
    let mut h = Harness::new();
    let (_, main, _) = h.admit(7, charter());
    // The run opens the child within the step: its session calls its LLM.
    let (child, prompt) = completing(h.answer(main, Box::new([served(b"a1", sub_agent(b"Find the bug."))])));
    assert_ne!(child, main);
    assert_eq!(prompt.tools, tools::Grants { inspect: true, modify: false, shell: false }, "the families asked for");
    assert!(prompt.served.is_empty(), "a child receives workspace tools only");
    assert_eq!((h.domain.peers(), h.domain.flights()), (2, 1));

    // The child calls finish, which it was not offered: no call. Its next
    // provider request waits until the next iteration's reclaim point.
    assert!(h.answer(child, Box::new([served(b"f1", verdict(b"approve"))])).is_empty());
    let (again, prompt) = completing(h.next());
    assert_eq!(again, child);
    let [Block::ToolResult { id: _, result: Returned::Invalid { problem: Problem::UnknownTool } }] = last(&prompt)
    else {
        panic!("expected the call refused, got {:?}", last(&prompt));
    };

    // The child yields: the run closes it (from the ready list), and its end
    // answers main's call (from the ready list again).
    assert!(h.says(child, b"It is in main.rs.").is_empty());
    assert!(h.next().is_empty(), "the child closes, and ends");
    let (owner, prompt) = completing(h.next());
    assert_eq!(owner, main);
    let [Block::ToolResult { id, result: Returned::Text { text, error: false, replay: None } }] = last(&prompt) else {
        panic!("expected the child's answer, got {:?}", last(&prompt));
    };
    assert_eq!(&**id, b"a1");
    assert_eq!(&**text, b"It is in main.rs.");
    h.domain.reclaim();
    assert_eq!((h.domain.peers(), h.domain.flights(), h.domain.tickets()), (1, 0, 0));
}

/// Main, admitted, whose LLM says `text` bytes and asks for two read-only
/// sub-agents side by side: what that emitted.
fn side_by_side_with(limits: &Limits, text: u32) -> (Harness, Token, Box<[Request]>) {
    let mut h = Harness::with(limits);
    let (run, main, _) = h.admit(7, charter());
    let content = Box::new([
        Said::Text { text: filler(text), replay: None },
        served(b"a1", sub_agent(b"Look here.")),
        served(b"a2", sub_agent(b"Look there.")),
    ]);
    let emitted = h.answer(main, content);
    (h, run, emitted)
}

/// The sub-agents `emitted` opened, whose sessions call their LLMs.
fn children(emitted: &[Request]) -> Option<[Token; 2]> {
    let [Request::Complete { owner: first, .. }, Request::Complete { owner: second, .. }] = emitted else {
        return None;
    };
    Some([*first, *second])
}

fn filler(len: u32) -> Box<[u8]> {
    let mut text = List::with_capacity(len);
    for _ in 0..len {
        text.push(b'x').expect("room for the text");
    }
    text.into_boxed()
}

fn assert_no_provider_completion(emitted: &[Request]) {
    for request in emitted {
        match request {
            Request::Complete { .. } => {
                panic!("no room for another provider receiving reserve after the exact result edge")
            }
            Request::Waiting { .. }
            | Request::Turn { .. }
            | Request::HostCall { .. }
            | Request::WithdrawHost { .. }
            | Request::Admitted { .. }
            | Request::Answer { .. }
            | Request::Checking { .. }
            | Request::Deliver { .. }
            | Request::Rejected { .. }
            | Request::Exhausted { .. }
            | Request::Cancel { .. }
            | Request::Io { .. }
            | Request::CancelIo { .. }
            | Request::Read { .. }
            | Request::Probe { .. }
            | Request::Check { .. }
            | Request::Abort { .. } => {}
        }
    }
}

#[test]
fn full_history_reserves_every_child_answer_before_effect_and_keeps_late_bytes() {
    // Find the exact receiving edge from admission/dispatch outputs.
    // C bounds provider content; D bounds each complete canonical child result.
    // A one-byte tighter transcript must refuse the entire adjacent read batch.
    let mut lower = LIMITS.session.delegated_result_bytes.checked_mul(2).expect("two result caps");
    let mut upper = LIMITS.session.session_bytes;
    assert!(
        children(
            &side_by_side_with(
                &Limits { session: session::Limits { session_bytes: lower, ..LIMITS.session }, ..LIMITS },
                256
            )
            .2
        )
        .is_none()
    );
    assert!(children(&side_by_side_with(&LIMITS, 256).2).is_some());
    for _ in 0_u32..32 {
        if upper.checked_sub(lower).expect("ordered receiving edge") <= 1 {
            break;
        }
        let distance = upper.checked_sub(lower).expect("ordered receiving edge");
        let middle = lower.checked_add(distance.checked_div(2).expect("nonzero divisor")).expect("midpoint");
        let limits = Limits { session: session::Limits { session_bytes: middle, ..LIMITS.session }, ..LIMITS };
        if children(&side_by_side_with(&limits, 256).2).is_some() {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    assert_eq!(upper, lower.checked_add(1).expect("adjacent receiving edge"));
    let tight = Limits { session: session::Limits { session_bytes: lower, ..LIMITS.session }, ..LIMITS };
    let (tight_world, _, refused) = side_by_side_with(&tight, 256);
    assert!(children(&refused).is_none(), "all result credit precedes the first child effect");
    let refused_turn = tight_world.turns.last().expect("actual assistant and unstarted result tail survive");
    assert_eq!(
        refused_turn.messages.last().expect("results").content.as_ref(),
        &[
            session::llm::Block::ToolResult { id: bytes(b"a1"), result: session::llm::Returned::NotRun },
            session::llm::Block::ToolResult { id: bytes(b"a2"), result: session::llm::Returned::NotRun },
        ]
    );

    let limits = Limits { session: session::Limits { session_bytes: upper, ..LIMITS.session }, ..LIMITS };
    let (mut world, run, emitted) = side_by_side_with(&limits, 256);
    let [first, second] = children(&emitted).expect("the complete batch was reserved");
    let answer = filler(limits.run.answer_bytes);
    assert!(world.says(first, &answer).is_empty());
    assert!(world.says(second, &answer).is_empty());
    assert!(world.next().is_empty(), "actual child endings queue both owning results");
    assert_eq!(world.domain.flights(), 2);
    assert_eq!(world.domain.run().runs(), 1, "only main remains after both child terminals");
    assert!(world.step(Event::Cancel { run }).is_empty());
    // Cancel emission cannot replace either result which already won its terminal.
    let emitted = world.next();
    assert_no_provider_completion(&emitted);
    let turn = world.turns.last().expect("actual main Turn precedes its failed final answer");
    for (index, id) in [b"a1".as_slice(), b"a2".as_slice()].into_iter().enumerate() {
        assert_eq!(
            *turn
                .messages
                .last()
                .expect("actual result message")
                .content
                .get(index)
                .expect("one result per admitted child"),
            session::llm::Block::ToolResult {
                id: bytes(id),
                result: session::llm::Returned::Text { text: answer.clone(), error: false, replay: None }
            }
        );
    }
    assert_eq!(world.domain.tickets(), 0, "concrete records carry no local replay tickets");
    assert!(
        u64::from(limits.run.answer_bytes).checked_mul(2).expect("two exact child answers")
            <= limits::uncharged(&limits).expect("priced queued payload")
    );
}

#[test]
fn the_asks_of_an_answer_that_yields_are_forgotten() {
    let mut h = Harness::new();
    let (_, main, _) = h.admit(7, charter());
    let completion =
        Completion { content: Box::new([served(b"f1", verdict(b"approve"))]), stop: Stop::MaxTokens, usage: USAGE };
    // The run nudges at once: the session is continued within the step.
    let (owner, prompt) = completing(h.step(Event::Completed { owner: main, completion }));
    assert_eq!(owner, main);
    assert_eq!(h.domain.tickets(), 0, "the call that did not run left no ticket");
    let [_, _, Message { role: Role::User, content }] = &*prompt.messages else {
        panic!("expected the nudge after the call that did not run");
    };
    let [Block::ToolResult { id: _, result: Returned::NotRun }, Block::Text { .. }] = &**content else {
        panic!("expected the call not run, then the nudge, got {content:?}");
    };
}

#[test]
fn an_ask_larger_than_the_session_may_hold_is_too_large() {
    let mut h = Harness::new();
    let (_, main, _) = h.admit(7, charter());
    let mut brief = List::with_capacity(70_000);
    for _ in 0..70_000_u32 {
        brief.push(b'x').expect("room for the brief");
    }
    let brief = brief.into_boxed();
    let ask = Ask::SubAgent { brief, families: Families { tools: TOOLS, agents: false }, llm: None, share: None };
    assert!(h.answer(main, Box::new([served(b"a1", ask)])).is_empty());
    let (_, prompt) = completing(h.next());
    let [Block::ToolResult { id: _, result: Returned::Invalid { problem: Problem::TooLarge } }] = last(&prompt) else {
        panic!("expected the call too large, got {:?}", last(&prompt));
    };
    assert_eq!((h.domain.tickets(), h.domain.peers()), (0, 1));
}

#[test]
fn the_runs_deadline_fires_before_the_sessions_expiry_at_the_same_instant() {
    let mut h = Harness::new();
    let (_, main, _) = h.admit(7, charter());
    h.env.now = Time::ZERO.saturating_add(BUDGET.time);
    // The run winds down: its close of main waits for the next iteration, and
    // main's own expiry fires meanwhile.
    assert!(h.fire().is_empty());
    assert_eq!(&*h.fire(), &[Request::Cancel { owner: main }]);
    assert!(!h.domain.is_due(h.env.now));
    assert!(h.next().is_empty(), "the close finds main closing already");
    let emitted = h.step(Event::Cancelled { owner: main });
    let [Request::Answer { to: _, answer: run::Answer::Failed { failure, spent: _, .. } }] = &*emitted else {
        panic!("expected the run failed, got {emitted:?}");
    };
    assert_eq!(failure, &run::Failure::Budget(run::Exhausted::Time));
}

#[test]
fn the_facts_of_both_child_domains_are_gathered() {
    let mut h = Harness::new();
    let (_run, _main, _prompt) = h.admit(7, charter());
    let mut run = 0_u32;
    let mut sessions = 0_u32;
    for _ in 0..64_u32 {
        match h.domain.pop_fact() {
            Some(Fact::Run { .. }) => run += 1,
            Some(Fact::Session { .. }) => sessions += 1,
            None => break,
        }
    }
    assert!(run > 0 && sessions > 0, "{run} of the run's, {sessions} of the sessions'");
    assert_eq!(h.domain.facts_lost(), 0);
}

fn delivered() -> run::Delivery {
    run::Delivery::Delivered(
        run::Delivered::new(Box::new([
            run::Receipt::new(0, b"host receipt".as_slice().into()).expect("bounded receipt")
        ]))
        .expect("one writable mount"),
    )
}

fn opaque_tool(name: &[u8], effect: run::HostEffect) -> run::HostTool {
    run::HostTool {
        name: bytes(name),
        description: bytes(b"Uninterpreted host operation"),
        schema: bytes(br#"{"type":"object"}"#),
        effect,
        timeout: Duration::from_secs(2),
    }
}

fn opaque_call(id: &[u8], tool: &[u8], effect: run::HostEffect) -> Said {
    let input = bytes(br#" {"whole":[true,{"external":"policy"}]} "#);
    Said::ToolCall {
        id: bytes(id),
        name: bytes(tool),
        input: input.clone(),
        call: Decoded::Served {
            ask: Ask::Host {
                tool: bytes(tool),
                effect,
                input: run::HostInput::attested(input).expect("typed protocol attestation"),
            },
        },
        replay: None,
    }
}

#[test]
fn declared_host_reads_run_together_write_waits_and_mismatched_effect_never_relays() {
    let mut h = Harness::new();
    let mut charter = charter();
    let read = opaque_tool(b"outside_read", run::HostEffect::Read);
    let write = opaque_tool(b"outside_write", run::HostEffect::Write);
    charter.grants.host_tools = Box::new([read.clone(), write.clone()]);
    let (_, owner, prompt) = h.admit(71, charter);
    assert!(prompt.served.contains(&Served::Host(read)) && prompt.served.contains(&Served::Host(write)));
    let emitted = h.answer(
        owner,
        Box::new([
            opaque_call(b"one", b"outside_read", run::HostEffect::Read),
            opaque_call(b"two", b"outside_read", run::HostEffect::Read),
            opaque_call(b"three", b"outside_write", run::HostEffect::Write),
        ]),
    );
    let [
        Request::HostCall { relay: first, effect: run::HostEffect::Read, name: first_name, .. },
        Request::HostCall { relay: second, effect: run::HostEffect::Read, name: second_name, .. },
    ] = &*emitted
    else {
        panic!("adjacent host reads are concurrent, got {emitted:?}");
    };
    assert_ne!(first_name, second_name);
    let first = *first;
    let second = *second;
    let answer = run::HostAnswer::new(bytes(b"exact external error"), true).expect("bounded text");
    assert!(h.step(Event::HostReturned { relay: first, reply: run::HostReply::Answered(answer.clone()) }).is_empty());
    assert!(h.next().is_empty(), "one pending read prevents exclusive write");
    assert!(h.step(Event::HostReturned { relay: second, reply: run::HostReply::Answered(answer.clone()) }).is_empty());
    let emitted = h.next();
    let [Request::HostCall { relay, effect: run::HostEffect::Write, .. }] = &*emitted else {
        panic!("write follows settled reads, got {emitted:?}");
    };
    assert!(h.step(Event::HostReturned { relay: *relay, reply: run::HostReply::Answered(answer.clone()) }).is_empty());
    let (owner, prompt) = completing(h.next());
    let [Block::ToolResult { result: Returned::Text { text: actual, error: true, replay: None }, .. }, ..] =
        last(&prompt)
    else {
        panic!("exact error reaches next completion");
    };
    assert_eq!(actual.as_ref(), answer.text());
    assert!(h.answer(owner, Box::new([opaque_call(b"bad", b"outside_write", run::HostEffect::Read)])).is_empty());
    let (_, prompt) = completing(h.next());
    let [Block::ToolResult { result: Returned::Invalid { problem: Problem::UnknownTool }, .. }] = last(&prompt) else {
        panic!("effect mismatch is rejected before session scheduling");
    };
}

fn convention_workspace() -> Workspace {
    Workspace {
        directories: Box::new([
            Directory {
                name: bytes(b"work"),
                root: Token::new(900),
                writable: true,
                git: true,
                conflicts: Box::new([]),
            },
            Directory {
                name: bytes(b"reference"),
                root: Token::new(901),
                writable: false,
                git: true,
                conflicts: Box::new([]),
            },
        ]),
    }
}

/// Actual composed root entrance; decoy paths belong to the fake filesystem,
/// not to Smith's selection. Read-only mounts supply guides but no check probe.
fn convention_main(harness: &mut Harness, selected: Option<run::Conventions>) -> (Token, Token, Prompt) {
    let (guide_path, check_path) = match &selected {
        Some(conventions) => (conventions.guide.as_ref(), conventions.checks.as_ref()),
        None => (b"AGENTS.md".as_slice(), b".smith/check".as_slice()),
    };
    let guide_path = bytes(guide_path);
    let check_path = bytes(check_path);
    let mounted = Some(convention_workspace());
    let charter = Charter {
        conventions: selected,

        outcome: OutcomeSpec {
            change: Some(ChangeSpec { checks_must_pass: true, fields: Box::new([]) }),
            verdicts: Box::new([]),
            report: None,
            failure: None,
        },
        ..charter()
    };
    let emitted = harness.step(Event::Start {
        answered: Box::default(),
        reply_to: ReplyTo::new(Token::new(77)),
        host_run: Token::new(77),
        activation: 1,
        charter,
        workspace: mounted,
        transcript: None,
        grants: Box::new([crate::Grant {
            name: crate::GrantName { account: 0, generation: 1 },
            valid: Duration::from_secs(3600),
        }]),
    });
    let [Request::Admitted { run, .. }, Request::Read { owner, at, max, deadline }] = emitted.as_ref() else {
        panic!("actual admitted discovery: {emitted:?}");
    };
    let run = *run;
    assert_eq!(*owner, run);
    assert_eq!(at.root, Token::new(900));
    assert_eq!((max, deadline), (&LIMITS.run.guide_bytes, &Time::ZERO.saturating_add(LIMITS.run.io_timeout)));
    // All three fake paths exist. Only the caller-selected guide is meaningful;
    // the others return decoy text and would fail the public prompt assertion.
    let files = [
        (guide_path.as_ref(), b"Caller-selected writable guide".as_slice()),
        (b"AGENTS.md".as_slice(), b"DECOY legacy guide".as_slice()),
        (b"legacy/GUIDE".as_slice(), b"DECOY other guide".as_slice()),
    ];
    let mut found = None;
    for (path, content) in files {
        if path == at.path.as_ref() {
            found = Some(content);
            break;
        }
    }
    let text = found.expect("fake guide path exists");
    assert_eq!(at.path, guide_path);
    let emitted = harness.step(Event::Read { owner: run, read: run::Read::Text { text: bytes(text), whole: false } });
    let [Request::Probe { owner, at, deadline }] = emitted.as_ref() else {
        panic!("only writable directory has a check probe: {emitted:?}");
    };
    assert_eq!(*owner, run);
    assert_eq!(at.root, Token::new(900));
    assert_eq!(at.path, check_path);
    assert_eq!(*deadline, Time::ZERO.saturating_add(LIMITS.run.io_timeout));
    let emitted = harness.step(Event::Probed { owner: run, executable: true });
    let [Request::Read { owner, at, .. }] = emitted.as_ref() else {
        panic!("readonly mount still supplies a guide: {emitted:?}");
    };
    assert_eq!(*owner, run);
    assert_eq!(at.root, Token::new(901));
    assert_eq!(at.path, guide_path);
    let (main, prompt) = completing(harness.step(Event::Read {
        owner: run,
        read: run::Read::Text { text: bytes(b"Caller-selected readonly guide"), whole: true },
    }));
    let system = prompt.system.as_ref();
    for fragment in [
        b"Caller-selected writable guide".as_slice(),
        b"Caller-selected readonly guide",
        b"The file goes on: read the rest with your tools.",
        guide_path.as_ref(),
        check_path.as_ref(),
    ] {
        assert!(skein_lib::bytes::find(system, fragment).is_some(), "selected policy is rendered verbatim");
    }
    assert!(skein_lib::bytes::find(system, b"DECOY").is_none());
    if guide_path.as_ref() != b"AGENTS.md" {
        assert!(skein_lib::bytes::find(system, b"AGENTS.md").is_none());
    }
    if check_path.as_ref() != b".temper/pre-pr" {
        assert!(skein_lib::bytes::find(system, b".temper/pre-pr").is_none());
    }
    (run, main, prompt)
}

#[test]
fn caller_conventions_select_default_custom_and_explicit_legacy_check_delivery_paths() {
    for (selected, expected_path) in [
        (None, b".smith/check".as_slice()),
        (
            Some(run::Conventions { guide: bytes(b"docs/WORKFLOW"), checks: bytes(b".ci/check-suite") }),
            b".ci/check-suite".as_slice(),
        ),
        (
            Some(run::Conventions { guide: bytes(b"AGENTS.md"), checks: bytes(b".temper/pre-pr") }),
            b".temper/pre-pr".as_slice(),
        ),
    ] {
        let mut harness = Harness::new();
        let (_, main, _) = convention_main(&mut harness, selected);
        let change = Change { fields: Box::new([]) };
        let emitted = harness.answer(
            main,
            Box::new([served(b"finish-convention", Ask::Finish { outcome: Declared::Change(change.clone()) })]),
        );
        let [Request::Check { owner, program, deadline, tail }, Request::Checking { host_run, deadline: said }] =
            emitted.as_ref()
        else {
            panic!("actual exclusive Check precedes host delivery: {emitted:?}");
        };
        assert_eq!(program.root, Token::new(900), "readonly mount is never checked");
        assert_eq!(program.path.as_ref(), expected_path);
        assert_eq!(*host_run, Token::new(77));
        assert_eq!(deadline, said);
        assert_eq!(*tail, LIMITS.run.check_tail);
        assert_eq!(*deadline, Time::ZERO.saturating_add(LIMITS.run.check_timeout));
        let owner = *owner;
        let emitted = harness.step(Event::Checked {
            owner,
            ran: run::Ran { exit: run::Exit::Code { code: 0 }, output: bytes(b"check passed"), cut: 0 },
        });
        let [Request::Deliver { owner: delivered_owner, change: delivered_change, .. }] = emitted.as_ref() else {
            panic!("only actual successful writable Check admits host delivery: {emitted:?}");
        };
        assert_eq!(*delivered_owner, owner);
        assert_eq!(delivered_change, &change);
        assert!(harness.step(Event::Delivered { owner, delivery: delivered() }).is_empty());
        let emitted = harness.next();
        let [Request::Answer { answer: run::Answer::Accepted { outcome, .. }, .. }] = emitted.as_ref() else {
            panic!("one actual accepted terminal after settled Check and delivery: {emitted:?}");
        };
        assert_eq!(outcome, &Declared::Change(change));
        harness.domain.reclaim();
        assert_eq!((harness.domain.peers(), harness.domain.flights(), harness.domain.tickets()), (0, 0, 0));
    }
}

#[test]
fn custom_check_cancellation_waits_for_the_abort_terminal() {
    let mut harness = Harness::new();
    let (run, main, _) = convention_main(
        &mut harness,
        Some(run::Conventions { guide: bytes(b"docs/WORKFLOW"), checks: bytes(b".ci/check-suite") }),
    );
    let emitted = harness.answer(
        main,
        Box::new([served(
            b"finish-convention",
            Ask::Finish { outcome: Declared::Change(Change { fields: Box::new([]) }) },
        )]),
    );
    let [Request::Check { owner, .. }, Request::Checking { .. }] = emitted.as_ref() else {
        panic!("actual custom Check: {emitted:?}");
    };
    let owner = *owner;
    assert!(harness.step(Event::Cancel { run }).is_empty());
    let emitted = harness.next();
    let [Request::Abort { owner: aborted }] = emitted.as_ref() else {
        panic!("one actual check abort request: {emitted:?}");
    };
    assert_eq!(*aborted, owner);
    assert!(harness.next().is_empty(), "cancel requests no settlement or delivery");
    assert!(harness.step(Event::Aborted { owner }).is_empty());
    let emitted = harness.next();
    let [Request::Answer { answer: run::Answer::Failed { failure: run::Failure::Cancelled, .. }, .. }] =
        emitted.as_ref()
    else {
        panic!("only the actual abort terminal permits the original Start cancellation answer: {emitted:?}");
    };
    harness.domain.reclaim();
    assert_eq!((harness.domain.peers(), harness.domain.flights(), harness.domain.tickets()), (0, 0, 0));
}

#[test]
fn invalid_conventions_are_refused_at_original_root_start_before_any_effect() {
    let oversized = Box::new([b'x'; run::Conventions::PATH_CAPACITY + 1]);
    for path in [
        b"".as_slice(),
        b"/abs",
        b"a//b",
        b"a/../b",
        b"a/./b",
        b"a/",
        b".",
        b"..",
        b"zero\0byte",
        b"a\\b",
        b"line\nbreak",
        oversized.as_ref(),
    ] {
        for bad_guide in [true, false] {
            let mut harness = Harness::new();
            let conventions = if bad_guide {
                run::Conventions { guide: bytes(path), checks: bytes(b".ci/check") }
            } else {
                run::Conventions { guide: bytes(b"docs/GUIDE"), checks: bytes(path) }
            };
            let emitted = harness.step(Event::Start {
                answered: Box::default(),
                workspace: Some(workspace()),
                reply_to: ReplyTo::new(Token::new(77)),
                host_run: Token::new(77),
                activation: 1,
                charter: Charter { conventions: Some(conventions), ..charter() },
                transcript: None,
                grants: Box::new([]),
            });
            let [Request::Answer { answer, .. }] = emitted.as_ref() else {
                panic!("invalid selection has only its original Start terminal: {emitted:?}");
            };
            assert_eq!(answer, &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Conventions)));
            assert!(harness.next().is_empty());
            harness.domain.reclaim();
            assert_eq!((harness.domain.peers(), harness.domain.flights(), harness.domain.tickets()), (0, 0, 0));
        }
    }
}

/// Settle the positive default-path companion using its provider terminal.
fn settle_default_convention_control(harness: &mut Harness, run: Token, main: Token, receiving: &Limits) {
    let mut cancelled = List::with_capacity(max_out(receiving));
    for request in harness.step(Event::Cancel { run }) {
        cancelled.push(request).unwrap();
    }
    for request in harness.next() {
        cancelled.push(request).unwrap();
    }
    let [Request::Cancel { owner }] = cancelled.as_slice() else {
        panic!("positive default control cancels its actual provider request: {cancelled:?}");
    };
    assert_eq!(*owner, main);
    let mut ended = List::with_capacity(max_out(receiving));
    for request in harness.step(Event::Cancelled { owner: main }) {
        ended.push(request).unwrap();
    }
    for request in harness.next() {
        ended.push(request).unwrap();
    }
    let [Request::Answer { answer: run::Answer::Failed { failure: run::Failure::Cancelled, .. }, .. }] =
        ended.as_slice()
    else {
        panic!("positive default control settles its original Start: {ended:?}");
    };
}

#[test]
fn maximum_custom_guide_headings_obey_the_session_receiving_limit_after_discovery() {
    for maximum_paths in [false, true] {
        let run = run::Limits { run_bytes: 16_384, ..LIMITS.run };
        let receiving = Limits {
            run,
            session: session::Limits {
                session_bytes: 8192,
                completion_bytes: 128,
                completion_blocks: 1,
                delegated_result_bytes: crate::feedback_worst_case(&run).unwrap(),
                ..LIMITS.session
            },
            ..LIMITS
        };
        assert!(worst_case(&receiving).is_some());
        let mut harness = Harness::with(&receiving);
        let selected = if maximum_paths {
            Some(run::Conventions {
                guide: Box::new([b'g'; run::Conventions::PATH_CAPACITY]),
                checks: Box::new([b'c'; run::Conventions::PATH_CAPACITY]),
            })
        } else {
            None
        };
        let emitted = harness.step(Event::Start {
            answered: Box::default(),
            reply_to: ReplyTo::new(Token::new(7)),
            host_run: Token::new(7),
            activation: 1,
            workspace: Some(Workspace {
                directories: Box::new([
                    Directory {
                        name: bytes(b"work"),
                        root: Token::new(900),
                        writable: false,
                        git: true,
                        conflicts: Box::new([]),
                    },
                    Directory {
                        name: bytes(b"reference"),
                        root: Token::new(901),
                        writable: false,
                        git: true,
                        conflicts: Box::new([]),
                    },
                ]),
            }),
            charter: Charter { conventions: selected, ..charter() },
            transcript: None,
            grants: Box::new([crate::Grant {
                name: crate::GrantName { account: 0, generation: 1 },
                valid: Duration::from_secs(3600),
            }]),
        });
        let [Request::Admitted { run, .. }, Request::Read { .. }] = emitted.as_ref() else {
            panic!("valid maximum paths admit before actual opening size is known: {emitted:?}");
        };
        let run = *run;
        let emitted = harness.step(Event::Read {
            owner: run,
            read: run::Read::Text { text: bytes(b"Small actual guide"), whole: true },
        });
        let [Request::Read { .. }] = emitted.as_ref() else {
            panic!("both admitted guide requests occur before actual opening refusal: {emitted:?}");
        };
        let emitted = harness.step(Event::Read {
            owner: run,
            read: run::Read::Text { text: bytes(b"Small actual guide"), whole: true },
        });
        if maximum_paths {
            let [
                Request::Answer {
                    answer: run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Conversation)), ..
                },
            ] = emitted.as_ref()
            else {
                panic!(
                    "repeated maximum guide headings exceed receiving ownership before any provider or tool effect: {emitted:?}"
                );
            };
        } else {
            let (main, _) = completing(emitted);
            settle_default_convention_control(&mut harness, run, main, &receiving);
        }
        harness.domain.reclaim();
        assert_eq!(
            (
                harness.domain.peers(),
                harness.domain.flights(),
                harness.domain.tickets(),
                harness.domain.session().sessions()
            ),
            (0, 0, 0, 0)
        );
    }
}

fn scalar_limits() -> Limits {
    Limits {
        run: run::Limits { budget: run::Budget { spend: 1000, ..LIMITS.run.budget }, ..LIMITS.run },
        session: session::Limits { spend: 1000, ..LIMITS.session },
        ..LIMITS
    }
}

fn scalar_charter() -> Charter {
    let selected = charter();
    Charter {
        budget: run::Budget { spend: 100, ..selected.budget },
        llm: Llm { prices: run::Prices { input: 1, cached: 2, output: 3, unit: 1 }, ..selected.llm },
        ..selected
    }
}

#[test]
fn scalar_crossing_yields_a_turn_and_ends_without_consulting_credentials() {
    let mut harness = Harness::with(&scalar_limits());
    let (_, main, _) = harness.admit(1, scalar_charter());
    // Exact-cap completion owns its Turn. Closing needs no credential
    // lookup or subsequent provider request.
    assert!(harness.domain.grants.remove(&0).is_some());
    let usage = Usage { input_tokens: 100, ..Usage::ZERO };
    let completion = Completion {
        content: Box::new([Said::Text { text: bytes(b"unfinished"), replay: None }]),
        stop: Stop::EndTurn,
        usage,
    };
    assert!(harness.step(Event::Completed { owner: main, completion }).is_empty());
    let emitted = harness.next();
    let [Request::Answer { answer: run::Answer::Failed { failure, spent, turns }, .. }] = emitted.as_ref() else {
        panic!("scalar exhaustion must settle locally: {emitted:?}")
    };
    assert_eq!(*failure, run::Failure::Budget(run::Exhausted::Spend));
    assert_eq!((spent.units, spent.turns, *turns), (100, 1, 1));
    assert_eq!(harness.turns.len(), 1);
    harness.domain.reclaim();
    assert_eq!((harness.domain.completions.len(), harness.domain.peers(), harness.domain.flights()), (0, 0, 0));
}

#[test]
fn a_priced_crossing_finish_settles_without_publishing_another_completion() {
    let mut harness = Harness::with(&scalar_limits());
    let (_, main, _) = harness.admit(1, scalar_charter());
    assert!(harness.answer(main, Box::new([served(b"finish", verdict(b"approve"))])).is_empty());
    let emitted = harness.next();
    let [Request::Answer { answer: run::Answer::Accepted { spent, turns, .. }, .. }] = emitted.as_ref() else {
        panic!("same-turn accepted finish wins scalar crossing: {emitted:?}")
    };
    assert_eq!((spent.units, spent.turns, *turns), (130, 1, 1));
    assert_eq!(harness.turns.len(), 1);
    assert_eq!(harness.turns.iter().next().expect("final record").spent, 130);
}

#[test]
fn overflowing_completion_runs_no_calls_and_tells_no_turn() {
    let mut harness = Harness::with(&scalar_limits());
    let selected = Charter {
        llm: Llm { prices: run::Prices { input: u64::MAX, cached: 0, output: 0, unit: 1 }, ..scalar_charter().llm },
        ..scalar_charter()
    };
    let (_, main, _) = harness.admit(1, selected);
    let completion = Completion {
        content: Box::new([served(b"call", Ask::Wait)]),
        stop: Stop::ToolUse,
        usage: Usage { input_tokens: 2, ..Usage::ZERO },
    };
    let emitted = harness.step(Event::Completed { owner: main, completion });
    let [Request::Answer { answer: run::Answer::Failed { failure, spent, turns }, .. }] = emitted.as_ref() else {
        panic!("overflow settles with a typed answer: {emitted:?}");
    };
    assert_eq!(*failure, run::Failure::Budget(run::Exhausted::Overflow(run::Overflow::Spend)));
    assert_eq!((spent.units, spent.turns, *turns), (0, 0, 0));
    assert!(harness.turns.is_empty());
    assert_eq!(harness.domain.flights(), 0);
}
