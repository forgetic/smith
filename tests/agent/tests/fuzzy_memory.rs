//! Memory stays within the worst case (programming-model.md, section 6.3), measured by
//! a counting allocator: the top level driven at random through runs on
//! charters as large as they may be, their conversations' completions (calls
//! to the tools, finishes and sub-agents among them, at sizes up to what a
//! session holds), what io and the host answer, and cancels; its peak
//! measured in every entry point, as the loop calls them.

use skein_lib::{Duration, Env, Queue, ReplyTo, Rng, Time, Token, Wall};
use skein_world::domain::heap::{self, Meter};
use smith_agent_world::TIGHT;
use smith_domain::llm::{Completion, Decoded, Failure, Problem, Prompt, Said, Served, Stop, Usage};
use smith_domain::run::charter::{Endpoint, Families, Grants, Llm, Tools};
use smith_domain::run::outcome::{
    Change, ChangeSpec, Declared, DeclaredFailure, Field, Item, OutcomeSpec, Report, TextSpec,
};
use smith_domain::run::outcome::{Verdict, VerdictRule};
use smith_domain::run::{self, Ask, Charter};
use smith_domain::run::{Directory, Workspace};
use smith_domain::tools::{Call, Done, Entry, Exit, Fault, Hit, Kind, Name, Op, Part, Path, Version};
use smith_domain::{Domain, Event, Limits, Request, fire, max_out, resume, step, worst_case};

#[global_allocator]
static HEAP: heap::Counting = heap::Counting;

fn bytes(len: u64) -> Box<[u8]> {
    vec![b'x'; usize::try_from(len).expect("a test length fits")].into_boxed_slice()
}

fn name(text: &[u8]) -> Name {
    Name::new(text.into()).expect("a name")
}

fn path(names: &[&[u8]]) -> Path {
    Path { absolute: false, parts: names.iter().map(|text| Part::Name { name: name(text) }).collect() }
}

fn index(rng: &mut Rng, len: usize) -> usize {
    usize::try_from(rng.below(u64::try_from(len).expect("small"))).expect("an index")
}

/// Small limits, so that every bound is met often: few runs and sessions,
/// few bytes each.
const LIMITS: Limits = Limits {
    accounts: 4,
    endpoints: 3,
    decoded_call_bytes: 4096,
    skew: Duration::ZERO,
    run: run::Limits {
        runs: 2,
        conversations: 4,
        run_bytes: 2048,
        brief_sections: 4,
        run_conversations: 3,
        calls: 4,
        answer_bytes: 128,
        guide_bytes: 128,
        outcome_bytes: 512,
        check_tail: 64,
        facts: 32,
        ..TIGHT.run
    },
    session: smith_domain::session::Limits {
        sessions: 4,
        messages: 12,
        session_bytes: 1_048_576,
        completion_bytes: 4096,
        completion_blocks: 16,
        failure_bytes: 512,
        delegated_result_bytes: 131_072,
        parallel_tools: 2,
        facts: 32,
        tools: smith_domain::tools::Limits {
            kits: 4,
            calls: 2,
            file_bytes: 512,
            read_bytes: 256,
            list_entries: 8,
            list_bytes: 4096,
            shell_head: 64,
            shell_tail: 64,
            search_hits: 4,
            search_bytes: 128,
            facts: 32,
            ..TIGHT.session.tools
        },
        ..TIGHT.session
    },
};

/// A charter with unchanged aggregate context filler distributed across role,
/// maximum Section cells, titles and bodies, granting everything and wanting a
/// change that passes its checks, a verdict, report or declared failure.
fn workspace() -> Workspace {
    let repository = Directory {
        name: (*b"temper").into(),
        root: Token::new(1),
        writable: true,
        git: true,
        conflicts: Box::new([]),
    };
    Workspace { directories: Box::new([repository]) }
}

fn charter(context_bytes: u64) -> Charter {
    let all = Tools { inspect: true, modify: true, shell: true };
    let rule = VerdictRule {
        name: (*b"request-changes").into(),
        text_max: 1024,
        fields: Box::new([]),
        items: smith_domain::run::outcome::ItemSpec {
            min: 1,
            max: 4,
            kinds: {
                let required: Box<[Box<[u8]>]> = Box::new([(*b"path").into()]);
                let kinds: Box<[Box<[u8]>]> = Box::new([(*b"nit").into()]);
                kinds
                    .into_vec()
                    .into_iter()
                    .map(|kind| smith_domain::run::outcome::ItemRule {
                        kind,
                        fields: required
                            .iter()
                            .map(|name| smith_domain::run::outcome::FieldRule { name: name.clone(), max: 1024 })
                            .collect(),
                    })
                    .collect()
            },
        },
    };
    let count = usize::try_from(LIMITS.run.brief_sections).expect("bounded receiving section count");
    let payload = context_bytes
        - u64::try_from(std::mem::size_of::<run::Section>() * count).expect("bounded owning Section array fits u64");
    let instructions = bytes(payload / 4);
    let mut remainder = payload - u64::try_from(instructions.len()).expect("bounded instructions payload fits u64");
    let mut sections = Vec::with_capacity(count);
    for section in 0..count {
        let remaining = u64::try_from(count - section).expect("bounded Section suffix count fits u64");
        let title = bytes(remainder / remaining / 3);
        let text = bytes(remainder / remaining - u64::try_from(title.len()).expect("bounded title payload fits u64"));
        remainder -= u64::try_from(title.len() + text.len()).expect("bounded Section payloads fit u64");
        sections.push(run::Section { title, text });
    }
    assert_eq!(remainder, 0, "unchanged aggregate filler owns every byte");
    Charter {
        instructions,
        brief: run::Brief { sections: sections.into_boxed_slice() },

        grants: Grants { wait: true, deliver: None, tools: all, agents: true, host_tools: Box::new([]) },
        outcome: OutcomeSpec {
            change: Some(ChangeSpec {
                checks_must_pass: true,
                fields: Box::new([
                    smith_domain::run::outcome::FieldRule { name: b"title".as_slice().into(), max: 1024 },
                    smith_domain::run::outcome::FieldRule { name: b"body".as_slice().into(), max: 1024 },
                ]),
            }),
            verdicts: Box::new([rule]),
            report: Some(TextSpec { max: 512, fields: Box::new([]) }),
            failure: Some(TextSpec { max: 512, fields: Box::new([]) }),
        },
        budget: run::Budget { turns: 12, ..TIGHT.run.budget },
        llm: Llm {
            prices: run::Prices { input: 0, cached: 0, output: 0, unit: 1 },
            account: 0,
            endpoint: Endpoint(0),
            model: (*b"m").into(),
            max_tokens: 256,
            dialect: 1,
        },
        models: Box::new([]),
        conventions: Some(smith_domain::run::Conventions {
            guide: b"AGENTS.md".as_slice().into(),
            checks: b".temper/pre-pr".as_slice().into(),
        }),
        resume: false,
        waiting: skein_lib::Duration::from_secs(30),
    }
}

/// What the domain asked for and the driver has yet to end, as the driver
/// keeps it: tokens and kinds, nothing the domain allocated.
#[derive(Clone, Copy, Debug)]
enum Asked {
    /// A call to an LLM, which may finish and ask for sub-agents if
    /// offered; and whether it was cancelled.
    Complete {
        owner: Token,
        finish: bool,
        agents: bool,
        receiving: Receiving,
        cancelled: bool,
    },
    Io {
        owner: Token,
        op: OpKind,
        cancelled: bool,
    },
    Read {
        owner: Token,
    },
    Probe {
        owner: Token,
    },
    Check {
        owner: Token,
        aborted: bool,
    },
    Delivery {
        owner: Token,
        host_run: Token,
        stopped: bool,
    },
}

/// The families of requests that may be cancelled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Llm,
    Io,
    Check,
    Delivery,
}

#[derive(Clone, Copy, Debug)]
enum OpKind {
    Load,
    Scan,
    Store,
    Spawn,
    Search { hits: u32, byte_cap: u32 },
}

/// Receiving contract observed on this actual root request, independently of
/// stored history capacity. Every terminal must fit all three allowances.
#[derive(Clone, Copy, Debug)]
struct Receiving {
    bytes: u64,
    blocks: u32,
    decoded: u64,
}

/// The driver: what is in flight, and the runs, in containers allocated
/// before the meter's base that never grow past their capacity.
struct Driver {
    rng: Rng,
    asked: Vec<Asked>,
    runs: Vec<(Token, Token)>,
    workers: u64,
    /// Requests seen, by kind: completions, io, looks, checks, pushes,
    /// Requests: completions, io, looks, checks, pushes, answers; then actual
    /// full searches, passing/failing checks, delivered/stale/failed delivery,
    /// delivery after parent cancellation, full provider terminals, invalid semantic
    /// feedback, refused starts, provider terminals winning cancellation, and
    /// invalid decoded-call feedback.
    seen: [u32; 18],
}

impl Driver {
    /// Takes what an entry point asked for, keeping what the driver needs to
    /// end it, and drops the rest.
    fn drain(&mut self, out: &mut Queue<Request>) {
        while let Some(request) = out.pop() {
            match request {
                Request::HostCall { .. } | Request::WithdrawHost { .. } => {
                    panic!("random legacy driver has no host declarations")
                }
                Request::Admitted { host_run, run } => self.runs.push((host_run, run)),
                Request::Answer { answer, .. } => {
                    self.seen[5] += 1;
                    if matches!(answer, run::Answer::Refused(_)) {
                        self.seen[15] += 1;
                    }
                }
                Request::MessageRefused { .. }
                | Request::Turn { .. }
                | Request::Waiting { .. }
                | Request::Checking { .. }
                | Request::ChecksEnded { .. }
                | Request::Rejected { .. }
                | Request::Exhausted { .. } => {}
                Request::Deliver { owner, host_run, .. } => {
                    self.seen[4] += 1;
                    self.ask(Asked::Delivery { owner, host_run, stopped: false });
                }
                Request::Complete {
                    owner,
                    prompt,
                    max_completion_bytes,
                    max_completion_blocks,
                    decoded_call_bytes,
                    ..
                } => {
                    self.seen[0] += 1;
                    let (finish, agents) = served(&prompt);
                    self.observe_prompt(&prompt);
                    let receiving = Receiving {
                        bytes: max_completion_bytes,
                        blocks: max_completion_blocks,
                        decoded: decoded_call_bytes,
                    };
                    self.ask(Asked::Complete { owner, finish, agents, receiving, cancelled: false });
                }
                Request::Cancel { owner } => self.cancel(Family::Llm, owner),
                Request::Io { owner, op, deadline: _ } => {
                    self.seen[1] += 1;
                    let op = match op {
                        Op::Load { .. } => OpKind::Load,
                        Op::Scan { .. } => OpKind::Scan,
                        Op::Store { .. } => OpKind::Store,
                        Op::Spawn { .. } => OpKind::Spawn,
                        Op::Search { hits, bytes: byte_cap, .. } => OpKind::Search { hits, byte_cap },
                    };
                    self.ask(Asked::Io { owner, op, cancelled: false });
                }
                Request::CancelIo { owner } => self.cancel(Family::Io, owner),
                Request::Read { owner, .. } => {
                    self.seen[2] += 1;
                    self.ask(Asked::Read { owner });
                }
                Request::Probe { owner, .. } => {
                    self.seen[2] += 1;
                    self.ask(Asked::Probe { owner });
                }
                Request::Check { owner, .. } => {
                    self.seen[3] += 1;
                    self.ask(Asked::Check { owner, aborted: false });
                }
                Request::Abort { owner } => self.cancel(Family::Check, owner),
            }
        }
    }

    fn observe_prompt(&mut self, prompt: &Prompt) {
        for message in &prompt.messages {
            for block in &message.content {
                if let smith_domain::llm::Block::ToolResult { result, .. } = block {
                    match result {
                        smith_domain::llm::Returned::Text { text, error: true, .. }
                            if text.starts_with(b"rejected") || text.starts_with(b"refused") =>
                        {
                            self.seen[14] += 1;
                        }
                        smith_domain::llm::Returned::Invalid { .. } => self.seen[17] += 1,
                        smith_domain::llm::Returned::Text { .. }
                        | smith_domain::llm::Returned::Served { .. }
                        | smith_domain::llm::Returned::Owned { .. }
                        | smith_domain::llm::Returned::Withdrawn
                        | smith_domain::llm::Returned::NotRun => {}
                    }
                }
            }
        }
    }

    fn ask(&mut self, asked: Asked) {
        assert!(self.asked.len() < self.asked.capacity(), "the driver's room is allocated before the base");
        self.asked.push(asked);
    }

    /// Marks what `owner` asked for of `family` as cancelled, if it is in
    /// flight: tokens of different families may be equal.
    fn cancel(&mut self, family: Family, owner: Token) {
        for asked in &mut self.asked {
            let (of, cancelled) = match asked {
                Asked::Complete { owner, cancelled, .. } => (Family::Llm, (owner, cancelled)),
                Asked::Io { owner, cancelled, .. } => (Family::Io, (owner, cancelled)),
                Asked::Check { owner, aborted } => (Family::Check, (owner, aborted)),
                Asked::Delivery { owner, stopped, .. } => (Family::Delivery, (owner, stopped)),
                Asked::Read { .. } | Asked::Probe { .. } => continue,
            };
            if of == family && *cancelled.0 == owner {
                *cancelled.1 = true;
            }
        }
    }

    /// The next event: a start, an end of something in flight, or a cancel.
    fn event(&mut self, limits: &Limits) -> Option<Event> {
        let roll = self.rng.below(12);
        if roll == 0 || self.asked.is_empty() {
            if self.rng.chance(200) {
                return self.cancel_run();
            }
            self.workers += 1;
            let host_run = Token::new(self.workers);
            // Most charters as large as a run may hold, some a byte larger.
            let brief = limits.run.run_bytes - 900 + self.rng.below(901);
            return Some(Event::Start {
                answered: Box::default(),
                workspace: Some(workspace()),
                grants: Box::new([smith_domain::Grant {
                    name: smith_domain::GrantName { account: 0, generation: 0 },
                    valid: Duration::from_secs(100_000),
                }]),
                reply_to: ReplyTo::new(host_run),
                host_run,
                activation: self.workers,
                window: smith_domain::Window { turns: u32::MAX, bytes: u64::MAX },
                charter: charter(brief),
                transcript: None,
            });
        }
        if roll == 1 && !self.runs.is_empty() && self.rng.chance(100) {
            return self.cancel_run();
        }
        let asked = self.asked.swap_remove(index(&mut self.rng, self.asked.len()));
        Some(match asked {
            Asked::Complete { owner, cancelled: true, .. } if self.rng.chance(700) => Event::Cancelled { owner },
            Asked::Complete { owner, finish, agents, receiving, cancelled } => {
                if self.rng.below(10) == 0 {
                    Event::Failed {
                        owner,
                        failure: Failure::Overloaded,
                        evidence: smith_domain::llm::Evidence::Unknown,
                        detail: Box::default(),
                    }
                } else {
                    let completion = self.completion(limits, finish, agents, receiving);
                    let (owned, decoded) = completion_owned(&completion);
                    assert!(owned <= receiving.bytes && decoded <= receiving.decoded);
                    assert!(completion.content.len() <= usize::try_from(receiving.blocks).expect("bounded blocks"));
                    if owned == receiving.bytes {
                        self.seen[13] += 1;
                    }
                    if cancelled {
                        self.seen[16] += 1;
                    }
                    Event::Completed { owner, completion }
                }
            }
            Asked::Io { owner, cancelled: true, .. } if self.rng.chance(700) => {
                Event::Done { owner, done: Done::Cancelled }
            }
            Asked::Io { owner, op, .. } => Event::Done { owner, done: self.done(limits, op) },
            Asked::Read { owner } => {
                let read = match self.rng.below(4) {
                    0 => run::Read::Missing,
                    _ => run::Read::Text { text: bytes(u64::from(limits.run.guide_bytes)), whole: false },
                };
                Event::Read { owner, read }
            }
            Asked::Probe { owner } => Event::Probed { owner, executable: self.rng.chance(700) },
            Asked::Check { owner, aborted: true } if self.rng.chance(700) => Event::Aborted { owner },
            Asked::Check { owner, .. } => {
                let exit = run::Exit::Code { code: u8::from(self.rng.chance(500)) };
                self.seen[if exit == (run::Exit::Code { code: 0 }) { 7 } else { 8 }] += 1;
                let ran = run::Ran { exit, output: bytes(u64::from(limits.run.check_tail)), cut: 1000 };
                Event::Checked { owner, ran }
            }
            Asked::Delivery { owner, stopped, .. } => {
                let choice = index(&mut self.rng, 3);
                let push = [
                    smith_agent_world::delivered(),
                    run::Delivery::Stale,
                    run::Delivery::Failed(run::DeliveryFailure::new(0, run::DeliveryReason::Unknown)),
                ][choice]
                    .clone();
                self.seen[9 + choice] += 1;
                if stopped {
                    self.seen[12] += 1;
                }
                Event::Delivered { owner, delivery: push }
            }
        })
    }

    fn cancel_run(&mut self) -> Option<Event> {
        if self.runs.is_empty() {
            return None;
        }
        let pending_delivery = if self.rng.chance(500) {
            self.asked.iter().find_map(|asked| match asked {
                Asked::Delivery { host_run, .. } => {
                    self.runs.iter().find(|(candidate, _)| candidate == host_run).copied()
                }
                Asked::Complete { .. }
                | Asked::Io { .. }
                | Asked::Read { .. }
                | Asked::Probe { .. }
                | Asked::Check { .. } => None,
            })
        } else {
            None
        };
        // Actual submissions make this race reachable; the other half retains
        // arbitrary stale parent cancellation handles from the original sweep.
        let (host_run, run) = pending_delivery.unwrap_or_else(|| self.runs[index(&mut self.rng, self.runs.len())]);
        for asked in &mut self.asked {
            if let Asked::Delivery { host_run: delivery_worker, stopped, .. } = asked
                && *delivery_worker == host_run
            {
                *stopped = true;
            }
        }
        Some(Event::Cancel { run })
    }

    /// A bounded terminal, independently pricing provider and decoded
    /// ownership; history capacity cannot enlarge the advertised receiving cap.
    fn completion(&mut self, limits: &Limits, finish: bool, agents: bool, receiving: Receiving) -> Completion {
        let completion_cell_bytes = completion_cell();
        let usage = Usage { input_tokens: 100, output_tokens: 20, cache_read_tokens: 50, cache_write_tokens: 50 };
        if self.rng.chance(150) {
            let available =
                receiving.bytes.checked_sub(completion_cell_bytes).expect("admitted receiving cap holds one block");
            let size = if self.rng.chance(500) { available } else { self.rng.below(available + 1) };
            let text = Said::Text { text: bytes(size), replay: None };
            return Completion { content: Box::new([text]), stop: Stop::EndTurn, usage };
        }
        let classification = u64::try_from(core::mem::size_of::<Decoded>()).expect("fixed cell");
        let minimum = completion_cell_bytes + classification + 6;
        let count = (1 + self.rng.below(2))
            .min(u64::from(receiving.blocks))
            .min(receiving.bytes / minimum)
            .min(receiving.decoded / classification);
        assert!(count > 0, "admitted receiving contract holds one call and its classification");
        let decoded_cap = receiving.decoded / count;
        let block_cap = receiving.bytes / count;
        let most = block_cap.min(decoded_cap) / 4;
        let content = (0..count)
            .map(|at| {
                let size = self.rng.below(most + 1);
                let mut call = self.call(limits, size, finish, agents);
                let mut owned = call.owned_bytes().expect("bounded decoded fixture");
                if owned > decoded_cap || completion_cell_bytes + 6 + owned > block_cap {
                    call = Decoded::Invalid { problem: Problem::TooLarge };
                    owned = classification;
                }
                let input_cap = block_cap - completion_cell_bytes - 6 - owned;
                Said::ToolCall {
                    id: format!("c{at}").into_bytes().into(),
                    name: bytes(4),
                    input: bytes(self.rng.below(input_cap + 1)),
                    call,
                    replay: None,
                }
            })
            .collect();
        Completion { content, stop: Stop::ToolUse, usage }
    }

    fn call(&mut self, limits: &Limits, size: u64, finish: bool, agents: bool) -> Decoded {
        let lib = || path(&[b"src", b"lib.rs"]);
        let call = match self.rng.below(9) {
            0 => Call::Read { path: lib(), skip: 0, lines: None },
            1 => Call::List { path: path(&[b"src"]) },
            2 => Call::Search { path: path(&[b"src"]), pattern: bytes(3), glob: None },
            3 => Call::Write { path: lib(), content: bytes(size) },
            4 => Call::Edit { path: lib(), old: bytes(1), new: bytes(size), all: false },
            5 => Call::Shell { command: bytes(8), timeout: None },
            6 if finish => return Decoded::Served { ask: self.finish(limits) },
            7 if agents => {
                let families = Families {
                    tools: Tools { inspect: true, modify: self.rng.chance(500), shell: false },

                    agents: self.rng.chance(500),
                };
                return Decoded::Served { ask: Ask::SubAgent { brief: bytes(size), families, llm: None, share: None } };
            }
            _ => return Decoded::Invalid { problem: Problem::Missing { field: bytes(size.min(16)) } },
        };
        Decoded::Owned { call }
    }

    /// A finish of every form, with ownership near or beyond the aggregate cap.
    fn finish(&mut self, limits: &Limits) -> Ask {
        let most = limits.run.outcome_bytes;
        let outcome = match self.rng.below(4) {
            0 => Declared::Report(Report {
                text: bytes(self.rng.below(most.saturating_add(1))),
                fields: Box::new([Field { name: b"extra".as_slice().into(), value: bytes(self.rng.below(most)) }]),
            }),
            1 => Declared::Failure(DeclaredFailure {
                reason: bytes(self.rng.below(most.saturating_add(1))),
                fields: Box::new([]),
            }),
            2 => Declared::Change(Change {
                fields: Box::new([
                    smith_domain::run::outcome::Field { name: b"title".as_slice().into(), value: bytes(1) },
                    smith_domain::run::outcome::Field {
                        name: b"body".as_slice().into(),
                        value: bytes(self.rng.below(most)),
                    },
                ]),
            }),
            _ => {
                let child = || Item {
                    kind: (*b"nit").into(),
                    fields: Box::new([Field { name: (*b"path").into(), value: bytes(8) }]),
                };
                let children = (0..self.rng.below(3)).map(|_| child()).collect();
                let name = (*b"request-changes").into();
                Declared::Verdict(Verdict {
                    name,
                    text: bytes(self.rng.below(most / 2)),
                    items: children,
                    fields: Box::new([]),
                })
            }
        };
        Ask::Finish { outcome }
    }

    /// A terminal io may end an operation of `op` with, as large as the
    /// tools take.
    fn done(&mut self, limits: &Limits, op: OpKind) -> Done {
        let tools = &limits.session.tools;
        let version = Version::new([self.rng.below(3), 0, 0, 0]);
        if self.rng.chance(100) {
            return [Done::Failed { fault: Fault::Other }, Done::TimedOut][index(&mut self.rng, 2)].clone();
        }
        match op {
            OpKind::Load => Done::Loaded { content: bytes(self.rng.below(u64::from(tools.file_bytes) + 1)), version },
            OpKind::Scan => {
                let entries = (0..tools.list_entries)
                    .map(|entry| Entry { name: name(format!("e{entry}").as_bytes()), kind: Kind::File });
                Done::Scanned { entries: entries.collect(), more: 3 }
            }
            OpKind::Store => Done::Stored { version },
            OpKind::Spawn => {
                let (head, tail) = (u64::from(tools.shell_head), u64::from(tools.shell_tail));
                Done::Exited { exit: Exit::Code { code: 1 }, head: bytes(head), tail: bytes(tail), dropped: 100 }
            }
            OpKind::Search { hits, byte_cap } => {
                assert!(hits > 0, "the fixture requests a nonempty search result");
                let paths = 6 * u64::from(hits);
                let remaining = u64::from(byte_cap).checked_sub(paths).expect("requested cap holds every hit path");
                let per_hit = remaining / u64::from(hits);
                let extra = remaining % u64::from(hits);
                let found: Box<[Hit]> = (0..hits)
                    .map(|line| Hit { path: bytes(6), line, text: bytes(per_hit + u64::from(u64::from(line) < extra)) })
                    .collect();
                assert_eq!(found.len(), usize::try_from(hits).expect("bounded requested hit count"));
                assert_eq!(
                    found.iter().map(|hit| hit.path.len() + hit.text.len()).sum::<usize>(),
                    usize::try_from(byte_cap).expect("bounded requested path plus text cap"),
                    "actual lower search result attains the full aggregate path and text bound"
                );
                self.seen[6] += 1;
                Done::Found { hits: found, more: 1, timed_out: false }
            }
        }
    }
}

/// Full translated terminal cells reserve the larger root/session layout.
fn completion_cell() -> u64 {
    u64::try_from(core::mem::size_of::<Said>().max(core::mem::size_of::<smith_domain::session::llm::Block>()))
        .expect("fixed translated block cell")
}

/// Price every original provider field, replay and decoded owning value, plus
/// all translated cells. This boundary check precedes the actual step call.
fn completion_owned(completion: &Completion) -> (u64, u64) {
    let mut owned = completion_cell() * u64::try_from(completion.content.len()).expect("bounded actual cells");
    let mut decoded = 0;
    for block in &completion.content {
        let payload = match block {
            Said::Opaque { bytes } => u64::try_from(bytes.len()).expect("bounded opaque bytes"),
            Said::Text { text, replay } | Said::Refusal { text, replay } => {
                u64::try_from(text.len()).expect("bounded text")
                    + replay.as_ref().map_or(0, |replay| u64::try_from(replay.bytes.len()).expect("bounded replay"))
            }
            Said::ToolCall { id, name, input, call, replay } => {
                let call_bytes = call.owned_bytes().expect("bounded decoded ownership");
                decoded += call_bytes;
                u64::try_from(id.len() + name.len() + input.len()).expect("bounded provider fields")
                    + replay.as_ref().map_or(0, |replay| u64::try_from(replay.bytes.len()).expect("bounded replay"))
                    + call_bytes
            }
        };
        owned += payload;
    }
    (owned, decoded)
}

/// Whether `prompt` offers the run's finish and sub-agents.
fn served(prompt: &Prompt) -> (bool, bool) {
    let mut offered = (false, false);
    for tool in &prompt.served {
        match tool {
            Served::Host(_) => panic!("random legacy driver has no host declarations"),
            Served::Wait | Served::Deliver => {}
            Served::Finish => offered.0 = true,
            Served::SubAgent => offered.1 = true,
        }
    }
    offered
}

/// Drives a domain under `limits` for `rounds` rounds from `seed`, as the
/// loop would: each round resumes what is ready, takes an event, and fires
/// what is due, then reaches the reclaim point; checking the peak of the heap
/// in every entry point against the worst case.
fn churn(limits: &Limits, seed: u64, rounds: u32) -> [u32; 18] {
    let limits = *limits;
    let bound = worst_case(&limits).expect("the test limits fit");
    let mut env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(max_out(&limits));
    let mut driver = Driver {
        rng: Rng::new(seed),
        asked: Vec::with_capacity(4096),
        runs: Vec::with_capacity(usize::try_from(rounds).expect("small")),
        workers: 0,
        seen: [0; 18],
    };
    let meter = Meter::new();
    let mut domain = Domain::new(&limits, smith_domain::Config { endpoints: Box::new([Endpoint(0)]) }, seed);
    let mut measure = |domain: &mut Domain, env: &Env<Limits>, driver: &mut Driver, call: Point| {
        meter.start();
        match call {
            Point::Resume => resume(domain, env, &mut out),
            Point::Fire => fire(domain, env, &mut out),
            Point::Step(event) => step(domain, env, event, &mut out),
        }
        let measured = meter.end();
        driver.drain(&mut out);
        while domain.pop_fact().is_some() {}
        meter.check(measured, bound, &limits);
    };
    for _ in 0..rounds {
        env.now = env.now.saturating_add(Duration::from_millis(driver.rng.below(3000)));
        for _ in 0..1000 {
            if !domain.is_ready() {
                break;
            }
            measure(&mut domain, &env, &mut driver, Point::Resume);
        }
        if let Some(event) = driver.event(&limits) {
            measure(&mut domain, &env, &mut driver, Point::Step(event));
        }
        for _ in 0..1000 {
            if !domain.is_due(env.now) {
                break;
            }
            measure(&mut domain, &env, &mut driver, Point::Fire);
        }
        domain.reclaim();
    }
    driver.seen
}

/// An entry point the loop calls.
#[expect(clippy::large_enum_variant, reason = "the memory driver passes an owned bounded terminal to the step")]
enum Point {
    Resume,
    Fire,
    Step(Event),
}

#[test]
fn a_domain_driven_at_random_stays_within_its_worst_case_at_every_entry_point() {
    let wider = Limits {
        accounts: LIMITS.accounts,
        endpoints: LIMITS.endpoints,
        decoded_call_bytes: LIMITS.decoded_call_bytes,
        skew: LIMITS.skew,
        run: run::Limits { runs: 3, conversations: 8, run_conversations: 4, calls: 8, ..LIMITS.run },
        session: smith_domain::session::Limits {
            sessions: 8,
            messages: 24,
            session_bytes: 1_048_576,
            completion_bytes: 4096,
            completion_blocks: 16,
            failure_bytes: 512,
            delegated_result_bytes: 131_072,
            parallel_tools: 3,
            tools: smith_domain::tools::Limits { kits: 8, calls: 3, ..LIMITS.session.tools },
            ..LIMITS.session
        },
    };
    let mut seen = [0; 18];
    for seed in 0..12 {
        for limits in [LIMITS, wider] {
            eprintln!("agent memory replay: seed={seed}, rounds=3000, limits={limits:?}");
            let counted = churn(&limits, seed, 3_000);
            for (all, one) in seen.iter_mut().zip(counted) {
                *all += one;
            }
        }
    }
    eprintln!("agent memory actual boundary coverage: {seen:?}");
    assert!(seen[..7].iter().all(|count| *count > 10), "every kind of request was made: {seen:?}");
    assert!(seen[7..].iter().all(|count| *count > 0), "actual terminal and refusal classes occurred: {seen:?}");
}
