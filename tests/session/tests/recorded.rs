use skein_lib::Token;
use smith_domain_session::{self as session, llm, record};
use smith_session_world::recorded::{self, World, opening, scenario, transcript};

#[test]
fn concrete_turns_resume_verbatim_without_local_tickets() {
    let old = scenario(1, 256);
    let mut resumed = World::new(1, 256);
    resumed.open(opening(Some(transcript(&old)), 100));
    assert!(resumed.end.is_none());
    let prompt = &resumed.prompts[0];
    assert_eq!(prompt.endpoint, llm::Endpoint(7));
    assert_eq!(prompt.messages[1].content[0], llm::Block::Opaque { bytes: recorded::OPAQUE.into() });
    assert!(matches!(prompt.messages[1].content[1], llm::Block::ToolCall { call: llm::Decoded::Historical, .. }));
    assert_eq!(prompt.messages[2].content[0], old.turns[0].messages[2].content[0]);
    assert_eq!(
        prompt.messages.last().expect("the scenario supplied a value").content[0],
        llm::Block::Text { text: b"wake".as_slice().into(), replay: None }
    );
    resumed.complete(
        Box::new([llm::Block::Text { text: b"resumed".as_slice().into(), replay: None }]),
        llm::Stop::EndTurn,
        llm::Usage::ZERO,
    );
    assert_eq!(resumed.turns[0].sequence, 3);
    assert_eq!(resumed.turns[0].spent, 0, "old activation spend is not charged again");
    resumed.close();
}

#[test]
fn transcript_refusals_precede_tools_and_completions() {
    type Change = Box<dyn Fn(&mut record::Transcript)>;
    let old = scenario(2, 256);
    let changes: Vec<(record::Refusal, Change)> = vec![
        (record::Refusal::Version, Box::new(|t| t.version = 99)),
        (record::Refusal::Endpoint, Box::new(|t| t.endpoint = llm::Endpoint(8))),
        (record::Refusal::Dialect, Box::new(|t| t.dialect = 24)),
        (record::Refusal::Version, Box::new(|t| t.turns[0].version = 99)),
        (record::Refusal::Endpoint, Box::new(|t| t.turns[0].endpoint = llm::Endpoint(8))),
        (record::Refusal::Dialect, Box::new(|t| t.turns[0].dialect = 24)),
        (record::Refusal::Malformed, Box::new(|t| t.turns[1].sequence = 9)),
        (
            record::Refusal::Malformed,
            Box::new(|t| {
                t.turns[0].messages[2].content[0] =
                    llm::Block::ToolResult { id: b"unmatched".as_slice().into(), result: llm::Returned::NotRun }
            }),
        ),
        (record::Refusal::Unresolved, Box::new(|t| t.turns[0].messages[1].content = recorded::called())),
        (
            record::Refusal::Unresolved,
            Box::new(|t| {
                let llm::Block::ToolCall { call, .. } = &mut t.turns[0].messages[1].content[1] else {
                    panic!("the saved scenario contains the provider call");
                };
                *call = llm::Decoded::Invalid { problem: llm::Problem::UnknownTool };
            }),
        ),
        (
            record::Refusal::Unresolved,
            Box::new(|t| {
                let llm::Block::ToolCall { call, .. } = &mut t.turns[0].messages[1].content[1] else {
                    panic!("the saved scenario contains the provider call");
                };
                *call = llm::Decoded::Owned {
                    call: smith_domain_tools::Call::List {
                        path: smith_domain_tools::Path { absolute: true, parts: Box::default() },
                    },
                };
            }),
        ),
    ];
    for (reason, change) in changes {
        let mut history = transcript(&old);
        change(&mut history);
        let mut world = World::new(2, 256);
        world.open(opening(Some(history), 100));
        assert_eq!(world.end, Some(session::End::TranscriptRefused { reason }));
        assert!(world.prompts.is_empty());
        world.domain.reclaim();
        assert_eq!(world.domain.kits(), 0);
    }
}

#[test]
fn oversized_history_and_fresh_specs_are_refused_at_the_entrance() {
    let old = scenario(3, 256);
    let mut world = World::new(3, 256);
    world.env.limits.messages = 5;
    world.open(opening(Some(transcript(&old)), 100));
    assert_eq!(world.end, Some(session::End::TranscriptRefused { reason: record::Refusal::TooLarge }));
    let mut world = World::new(3, 256);
    world.env.limits.session_bytes = 512;
    world.open(opening(Some(transcript(&old)), 100));
    assert_eq!(world.end, Some(session::End::TranscriptRefused { reason: record::Refusal::TooLarge }));
    let mut world = World::new(3, 256);
    world.env.limits.spend = 99;
    world.open(opening(None, 100));
    assert_eq!(world.end, Some(session::End::Invalid));
    let mut world = World::new(3, 256);
    let mut spec = opening(None, 100);
    spec.prices.unit = 0;
    world.open(spec);
    assert_eq!(world.end, Some(session::End::Invalid));
}

#[test]
fn unit_budget_stops_after_the_crossing_turn_settles_and_child_counts_once() {
    let mut world = World::new(4, 256);
    world.open(opening(None, 20));
    world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    let owner = world.delegated[0];
    world.step(session::Event::Answered { owner, text: b"answer".as_slice().into(), error: false, spent: 9 });
    assert_eq!(world.end, Some(session::End::Budget { spent: session::Dimension::Unit }));
    assert_eq!(world.prompts.len(), 1);
    assert_eq!(world.turns[0].spent, 25);
    let trace = world.trace.clone();
    // Redelivery before reclaim and after reclaim is harmless, with no charge.
    for reclaim in [false, true] {
        if reclaim {
            world.domain.reclaim();
        }
        session::step(
            &mut world.domain,
            &world.env,
            session::Event::Answered { owner, text: b"duplicate".as_slice().into(), error: false, spent: 9 },
            &mut world.out,
        );
        assert!(world.out.is_empty());
    }
    assert_eq!(trace, world.trace);
    world.close();
}

#[test]
fn pricing_rounds_the_combined_completion_and_rejects_overflow() {
    assert_eq!(recorded::PRICES.price(recorded::USAGE), Some(16));
    assert_eq!(
        record::Prices { input: 1, cached: 1, output: 1, unit: 3 }.price(llm::Usage {
            input_tokens: 1,
            output_tokens: 1,
            ..llm::Usage::ZERO
        }),
        Some(1)
    );
    assert_eq!(
        record::Prices { input: u64::MAX, cached: 0, output: 0, unit: 1 }
            .price(llm::Usage { input_tokens: 2, ..llm::Usage::ZERO }),
        None
    );
    let mut world = World::new(5, 256);
    let mut spec = opening(None, u64::MAX);
    spec.prices = record::Prices { input: u64::MAX, cached: 0, output: 0, unit: 1 };
    world.open(spec);
    world.complete(
        Box::new([llm::Block::Text { text: b"done".as_slice().into(), replay: None }]),
        llm::Stop::EndTurn,
        llm::Usage { input_tokens: 2, ..llm::Usage::ZERO },
    );
    assert_eq!(world.spend, []);
    assert_eq!(world.own_spend, []);
    assert!(world.turns.is_empty());
    assert_eq!(world.end, Some(session::End::PriceOverflow));
    world.close();
}

#[test]
fn closing_preserves_withdrawn_and_late_answers_and_provider_completions() {
    for wins in [false, true] {
        let mut world = World::new(6, 256);
        world.open(opening(None, 100));
        world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
        let owner = world.delegated[0];
        world.step(session::Event::Close { session: world.session.expect("the scenario supplied a value") });
        if wins {
            world.step(session::Event::Answered {
                owner,
                text: b"late child".as_slice().into(),
                error: false,
                spent: 9,
            });
            assert_eq!(world.turns[0].spent, 25);
        } else {
            world.step(session::Event::AnswerCancelled { owner, spent: 9 });
            assert_eq!(world.turns[0].spent, 25, "withdrawn child spend is included");
            assert_eq!(
                world.turns[0].messages[2].content[0],
                llm::Block::ToolResult { id: b"provider-call".as_slice().into(), result: llm::Returned::Withdrawn }
            );
        }
        assert_eq!(world.end, Some(session::End::Closed));
        let mut resumed = World::new(6, 0);
        resumed.open(opening(Some(transcript(&world)), 100));
        assert!(resumed.end.is_none());
        resumed.close();
        world.close();
    }
    let mut world = World::new(6, 256);
    world.open(opening(None, 100));
    world.step(session::Event::Close { session: world.session.expect("the scenario supplied a value") });
    world.complete(recorded::called(), llm::Stop::EndTurn, recorded::USAGE);
    assert_eq!(world.turns[0].spent, 16);
    assert_eq!(world.turns[0].messages[1].content[0], llm::Block::Opaque { bytes: recorded::OPAQUE.into() });
    world.close();
}

#[test]
fn committed_call_results_after_a_yield_are_restored_without_tickets() {
    let mut world = World::new(7, 256);
    world.open(opening(None, 100));
    world.complete(recorded::called(), llm::Stop::EndTurn, recorded::USAGE);
    world.close();
    let history = transcript(&world);
    let tail = history.turns.last().expect("saved turn");
    let position = tail
        .messages
        .last()
        .expect("saved assistant")
        .content
        .iter()
        .position(|block| matches!(block, llm::Block::ToolCall { id, .. } if id.as_ref() == b"provider-call"))
        .expect("saved call");
    let sequence = tail.sequence;
    let mut opened = opening(Some(history), 100);
    opened.answered = Box::new([session::record::Answered {
        origin: session::record::Origin { sequence, position: u32::try_from(position).expect("position") },
        result: llm::Returned::Text { text: b"committed answer".as_slice().into(), error: false, replay: None },
    }]);
    let mut resumed = World::new(7, 256);
    resumed.open(opened);
    assert_eq!(resumed.prompts[0].messages.len(), 3);
    assert_eq!(
        resumed.prompts[0].messages[2].content[0],
        llm::Block::ToolResult {
            id: b"provider-call".as_slice().into(),
            result: llm::Returned::Text { text: b"committed answer".as_slice().into(), error: false, replay: None }
        }
    );
    resumed.close();
}

#[test]
fn an_answer_name_outside_the_last_turn_is_malformed() {
    let mut first = World::new(31, 256);
    first.open(opening(None, 100));
    first.complete(recorded::called(), llm::Stop::EndTurn, recorded::USAGE);
    first.close();
    let mut opened = opening(Some(transcript(&first)), 100);
    opened.answered = Box::new([session::record::Answered {
        origin: session::record::Origin { sequence: 1, position: 99 },
        result: llm::Returned::Text { text: b"wrong call".as_slice().into(), error: false, replay: None },
    }]);
    let mut resumed = World::new(31, 256);
    resumed.open(opened);
    assert_eq!(resumed.end, Some(session::End::TranscriptRefused { reason: record::Refusal::Malformed }));
    assert!(resumed.prompts.is_empty());
}

#[test]
fn replay_and_facts_capacity_change_no_decision() {
    let first = scenario(19, 256);
    let replayed = scenario(19, 256);
    assert_eq!(first.trace, replayed.trace);
    assert_eq!(first.snapshots, replayed.snapshots, "the complete frozen domain states replay too");
    assert_eq!(first.trace, scenario(19, 0).trace);
}

#[test]
fn yielded_historical_calls_resume_with_concrete_not_run_results() {
    let mut world = World::new(29, 256);
    world.open(opening(None, 100));
    world.complete(recorded::called(), llm::Stop::EndTurn, recorded::USAGE);
    world.close();
    let mut resumed = World::new(29, 256);
    resumed.open(opening(Some(transcript(&world)), 100));
    assert_eq!(
        resumed.prompts[0].messages[2].content[0],
        llm::Block::ToolResult { id: b"provider-call".as_slice().into(), result: llm::Returned::NotRun }
    );
    resumed.close();
}

#[test]
fn closing_a_resting_batch_keeps_its_result_and_marks_only_unstarted_calls() {
    use smith_domain_tools::{Call, Part, Path};
    let mut world = World::new(30, 256);
    world.open(opening(None, 100));
    let mut blocks = recorded::called().into_vec();
    blocks.insert(
        0,
        llm::Block::ToolCall {
            id: b"denied-read".as_slice().into(),
            name: b"read".as_slice().into(),
            input: br#"{"path":"."}"#.as_slice().into(),
            call: llm::Decoded::Owned {
                call: Call::Read {
                    path: Path { absolute: false, parts: Box::new([Part::Current]) },
                    skip: 0,
                    lines: None,
                },
            },
            replay: None,
        },
    );
    world.complete(blocks.into(), llm::Stop::ToolUse, recorded::USAGE);
    world.domain.reclaim();
    assert!(world.domain.is_ready(), "the tools' entrance refusal defers the next batch");
    assert!(world.delegated.is_empty());
    world.close();
    let results = &world.turns[0].messages[2].content;
    assert!(matches!(
        results[0],
        llm::Block::ToolResult {
            result: llm::Returned::Owned { outcome: smith_domain_tools::Outcome::NotGranted },
            ..
        }
    ));
    assert_eq!(
        results[1],
        llm::Block::ToolResult { id: b"provider-call".as_slice().into(), result: llm::Returned::NotRun }
    );
}

#[test]
fn cumulative_child_spend_overflow_is_a_typed_failure() {
    let mut world = World::new(31, 256);
    world.open(opening(None, u64::MAX));
    world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    world.step(session::Event::Answered {
        owner: world.delegated[0],
        text: b"child".as_slice().into(),
        error: false,
        spent: u64::MAX,
    });
    assert_eq!(world.spend, [16]);
    assert_eq!(world.end, Some(session::End::PriceOverflow));
    world.close();
}

#[test]
fn owned_io_cancellation_keeps_terminal_results_in_the_turn() {
    use smith_domain_tools::{Authority, Call, Done, Grants, Name, Op, Outcome, Part, Path, Repo, Version};
    for wins in [false, true] {
        let mut world = World::new(32, 256);
        let mut spec = opening(None, 100);
        let name = || Name::new(b"repo".as_slice().into()).expect("a repository mount");
        spec.spec.authority = Authority {
            cwd: Box::new([name()]),
            repos: Box::new([Repo { mount: Box::new([name()]), root: Token::new(7), writable: false }]),
            grants: Grants { inspect: true, modify: false, shell: false },
            env: Box::default(),
        };
        world.open(spec);
        world.complete(
            Box::new([
                llm::Block::Opaque { bytes: recorded::OPAQUE.into() },
                llm::Block::ToolCall {
                    id: b"owned-read".as_slice().into(),
                    name: b"read".as_slice().into(),
                    input: br#"{"path":"data"}"#.as_slice().into(),
                    call: llm::Decoded::Owned {
                        call: Call::Read {
                            path: Path {
                                absolute: false,
                                parts: Box::new([Part::Name {
                                    name: Name::new(b"data".as_slice().into()).expect("a file name"),
                                }]),
                            },
                            skip: 0,
                            lines: None,
                        },
                    },
                    replay: None,
                },
            ]),
            llm::Stop::ToolUse,
            recorded::USAGE,
        );
        let (owner, op) = &world.operations[0];
        let owner = *owner;
        let Op::Load { at, .. } = op else {
            panic!("a read asks io to load the file");
        };
        assert_eq!((at.root, at.path.as_ref()), (Token::new(7), b"data".as_slice()));
        world.step(session::Event::Close { session: world.session.expect("admitted session") });
        assert_eq!(world.cancelled_operations, [owner]);
        assert!(world.turns.is_empty(), "the turn waits for the owned io terminal");
        let done = if wins {
            Done::Loaded { content: b"file bytes\n".as_slice().into(), version: Version::new([1, 0, 0, 0]) }
        } else {
            Done::Cancelled
        };
        world.step(session::Event::Done { owner, done });
        assert_eq!(world.end, Some(session::End::Closed));
        assert_eq!(world.turns[0].spent, 16);
        assert_eq!(world.turns[0].messages[1].content[0], llm::Block::Opaque { bytes: recorded::OPAQUE.into() });
        let llm::Block::ToolResult { result: llm::Returned::Owned { outcome }, .. } =
            &world.turns[0].messages[2].content[0]
        else {
            panic!("the actual owned result is recorded");
        };
        if wins {
            assert_eq!(
                *outcome,
                Outcome::Read {
                    content: b"file bytes\n".as_slice().into(),
                    skipped: 0,
                    lines: 1,
                    total: 1,
                    cut: false
                }
            );
        } else {
            assert_eq!(*outcome, Outcome::Cancelled);
        }
        let mut resumed = World::new(32, 0);
        resumed.open(opening(Some(transcript(&world)), 100));
        assert!(resumed.end.is_none(), "the settled owned terminal is resumable");
        resumed.close();
        world.close();
    }
}

#[test]
fn repeated_provider_ids_keep_distinct_origins_and_restore_includes_history_prefix() {
    let mut first = World::new(61, 256);
    first.open(opening(None, 1000));
    first.complete(recorded::called(), llm::Stop::ToolUse, llm::Usage::ZERO);
    let owner = first.delegated[0];
    first.step(session::Event::Answered { owner, text: b"first".as_slice().into(), error: false, spent: 0 });
    first.complete(recorded::called(), llm::Stop::ToolUse, llm::Usage::ZERO);
    let owner = first.delegated[0];
    first.step(session::Event::Answered { owner, text: b"second".as_slice().into(), error: false, spent: 0 });
    assert_eq!(
        first.origins,
        [record::Origin { sequence: 1, position: 1 }, record::Origin { sequence: 2, position: 1 }]
    );
    first.close();
    let mut resumed = World::new(62, 256);
    resumed.open(opening(Some(transcript(&first)), 1000));
    resumed.complete(recorded::called(), llm::Stop::ToolUse, llm::Usage::ZERO);
    assert_eq!(resumed.origins, [record::Origin { sequence: 3, position: 1 }]);
    let owner = resumed.delegated[0];
    resumed.step(session::Event::Answered { owner, text: b"third".as_slice().into(), error: false, spent: 0 });
    resumed.close();
}

// Receiving-credit controls use the actual V2 domain and its observed turns,
// rather than supplied answers as evidence. Contract: domain/session.md, 3/5.
fn initial_owned(opening: &record::Opening) -> u64 {
    let spec = &opening.spec;
    u64::try_from(spec.model.len() + spec.system.len() + spec.prompt.len()).expect("small fixture")
        + u64::try_from(core::mem::size_of::<llm::Block>()).expect("block size")
        + u64::try_from(spec.delegated.len() * core::mem::size_of::<llm::Descriptor>()).expect("descriptors")
}

fn fill_initial(spec: &mut record::Opening, total: u64) {
    let current = initial_owned(spec);
    assert!(total >= current);
    let add = usize::try_from(total - current).expect("small fixture");
    let mut system = spec.spec.system.clone().into_vec();
    system.extend(vec![b's'; add]);
    spec.spec.system = system.into();
    assert_eq!(initial_owned(spec), total);
}

#[test]
fn cap_filled_provider_credit_preserves_replay_completion_that_wins_cancel() {
    let mut world = World::new(101, 0);
    world.env.limits.session_bytes = 16_384;
    world.env.limits.completion_bytes = 2048;
    world.env.limits.completion_blocks = 3;
    let reserve = session::completion_reserve(&world.env.limits).expect("bounded caps");
    let mut spec = opening(None, u64::MAX);
    fill_initial(&mut spec, world.env.limits.session_bytes - reserve);
    world.open(spec);
    assert_eq!(world.prompts.len(), 1);
    let call = recorded::called().into_vec().remove(1);
    // called() starts with opaque reasoning; the second block is its actual call.
    let call_payload = match &call {
        llm::Block::ToolCall { id, name, input, .. } => {
            assert_eq!(id.as_ref(), b"provider-call");
            assert_eq!(name.as_ref(), b"sub_agent");
            assert_eq!(input.as_ref(), br#"{"task":"review"}"#);
            id.len() + name.len() + input.len()
        }
        llm::Block::Text { .. }
        | llm::Block::Refusal { .. }
        | llm::Block::Opaque { .. }
        | llm::Block::ToolResult { .. } => panic!("fixture call"),
    };
    let fixed = 3 * core::mem::size_of::<llm::Block>() + call_payload + b"refused".len() + 8;
    let opaque = vec![0xff; 2048 - fixed].into_boxed_slice();
    let completion: Box<[llm::Block]> = Box::new([
        llm::Block::Opaque { bytes: opaque },
        llm::Block::Refusal {
            text: b"refused".as_slice().into(),
            replay: Some(llm::Replay { bytes: b"\0tag\xffext".as_slice().into() }),
        },
        call,
    ]);
    // The replay envelope is eight bytes and the full owned content reaches C.
    assert_eq!(b"\0tag\xffext".len(), 8);
    world.step(session::Event::Close { session: world.session.expect("admitted") });
    assert!(world.end.is_none(), "Cancel emission does not surrender receiving credit");
    world.complete(completion.clone(), llm::Stop::ToolUse, recorded::USAGE);
    assert_eq!(world.end, Some(session::End::Closed));
    assert_eq!(world.turns.len(), 1);
    assert_eq!(world.turns[0].usage, recorded::USAGE);
    let mut expected = completion.into_vec();
    match &mut expected[2] {
        llm::Block::ToolCall { call, .. } => *call = llm::Decoded::Historical,
        llm::Block::Text { .. }
        | llm::Block::Refusal { .. }
        | llm::Block::Opaque { .. }
        | llm::Block::ToolResult { .. } => panic!("fixture call"),
    }
    assert_eq!(world.turns[0].messages[1].content.as_ref(), expected.as_slice());
    assert_eq!(
        world.turns[0].messages[2].content.as_ref(),
        &[llm::Block::ToolResult { id: b"provider-call".as_slice().into(), result: llm::Returned::NotRun }]
    );
    world.close();
}

#[test]
fn one_byte_or_one_message_less_refuses_before_provider_and_tools() {
    for short_slot in [false, true] {
        let mut world = World::new(102, 0);
        world.env.limits.session_bytes = 16_384;
        world.env.limits.completion_bytes = 256;
        world.env.limits.completion_blocks = 1;
        let reserve = session::completion_reserve(&world.env.limits).expect("bounded caps");
        let mut spec = opening(None, u64::MAX);
        let extra = u64::from(!short_slot);
        fill_initial(&mut spec, world.env.limits.session_bytes - reserve + extra);
        if short_slot {
            world.env.limits.messages = 2;
        }
        world.open(spec);
        assert_eq!(world.end, Some(session::End::TranscriptRefused { reason: record::Refusal::TooLarge }));
        assert!(world.prompts.is_empty() && world.operations.is_empty() && world.delegated.is_empty());
        assert_eq!(world.domain.kits(), 0);
    }
}

#[test]
fn full_history_batch_credit_keeps_maximum_late_results_or_prevents_every_effect() {
    use smith_domain_tools::Effect;
    for short in [false, true] {
        let mut world = World::new(103, 0);
        world.env.limits.session_bytes = 16_384;
        world.env.limits.completion_bytes = 512;
        world.env.limits.completion_blocks = 2;
        world.env.limits.delegated_result_bytes = 2048;
        world.env.limits.parallel_tools = 2;
        let mut spec = opening(None, u64::MAX);
        spec.spec.delegated[0].effect = Effect::Read;
        let mut calls = vec![];
        let mut held = 0_u64;
        for id in [b"first".as_slice(), b"second".as_slice()] {
            calls.push(llm::Block::ToolCall {
                id: id.into(),
                name: b"served".as_slice().into(),
                input: b"{}".as_slice().into(),
                call: llm::Decoded::Delegated {
                    source: smith_domain_session::ToolSource::Run,
                    ticket: Token::new(19),
                    effect: Effect::Read,
                },
                replay: Some(llm::Replay { bytes: b"\0token\xff".as_slice().into() }),
            });
            held += u64::try_from(
                2 * core::mem::size_of::<llm::Block>()
                    + 2 * id.len()
                    + b"served".len()
                    + b"{}".len()
                    + b"\0token\xff".len(),
            )
            .expect("small fixture");
        }
        fill_initial(&mut spec, world.env.limits.session_bytes - held - 4096 + u64::from(short));
        world.open(spec);
        assert_eq!(world.prompts.len(), 1);
        world.complete(calls.into(), llm::Stop::ToolUse, llm::Usage::ZERO);
        if short {
            assert!(world.delegated.is_empty(), "whole batch refused before its first effect");
            assert_eq!(world.end, Some(session::End::TranscriptFull));
        } else {
            assert_eq!(world.delegated.len(), 2);
            world.step(session::Event::Close { session: world.session.expect("admitted") });
            assert!(world.end.is_none());
            for byte in [b'a', b'b'] {
                let owner = world.delegated[0];
                world.step(session::Event::Answered {
                    owner,
                    text: vec![byte; 2048].into(),
                    error: byte == b'b',
                    spent: 0,
                });
            }
            assert_eq!(world.end, Some(session::End::Closed));
        }
        let turn = world.turns.last().expect("actual final turn survives full history");
        for (index, id) in [b"first".as_slice(), b"second".as_slice()].into_iter().enumerate() {
            let result = if short {
                llm::Returned::NotRun
            } else {
                llm::Returned::Text {
                    text: vec![if index == 0 { b'a' } else { b'b' }; 2048].into(),
                    error: index == 1,
                    replay: None,
                }
            };
            assert_eq!(turn.messages[2].content[index], llm::Block::ToolResult { id: id.into(), result });
        }
        world.close();
    }
}

#[test]
fn exact_failure_classes_and_transport_evidence_survive_policy_without_diagnostic_retention() {
    for failure in [llm::Failure::Limit, llm::Failure::Protocol, llm::Failure::Cancelled] {
        for evidence in [llm::Evidence::Unsent, llm::Evidence::Unknown, llm::Evidence::Response] {
            for closing in [false, true] {
                let mut world = World::new(104, 256);
                world.env.limits.session_bytes = 16_384;
                world.env.limits.completion_bytes = 128;
                world.env.limits.completion_blocks = 1;
                world.env.limits.failure_bytes = 512;
                let reserve = session::completion_reserve(&world.env.limits).expect("bounded terminal cap");
                let mut spec = opening(None, u64::MAX);
                fill_initial(&mut spec, world.env.limits.session_bytes - reserve);
                world.open(spec);
                assert_eq!(world.prompts.len(), 1);
                if closing {
                    world.step(session::Event::Close { session: world.session.expect("admitted") });
                    assert!(world.end.is_none());
                }
                world.step(session::Event::Failed {
                    owner: world.completing.expect("one actual provider terminal"),
                    failure,
                    evidence,
                    detail: vec![0xff; 512].into(),
                });
                let expected = if closing { session::End::Closed } else { session::End::Failed { failure, evidence } };
                assert_eq!(world.end, Some(expected));
                assert_eq!(world.prompts.len(), 1, "new classes cannot start retry");
                assert!(world.turns.is_empty(), "failure does not fabricate assistant content");
                assert!(!format!("{:?}", world.domain).contains("255, 255"), "policy drops actual detail");
                world.close();
            }
        }
    }
}

#[test]
fn fullest_history_keeps_maximum_owned_read_list_search_and_shell_after_cancel() {
    use smith_domain_tools::{self as tools, Authority, Call, Grants, Name, Repo};
    for kind in 0..4 {
        for short in [false, true] {
            let mut world = World::new(110 + kind, 0);
            world.env.limits.session_bytes = 16_384;
            world.env.limits.completion_bytes = 512;
            world.env.limits.completion_blocks = 1;
            world.env.limits.tools.read_bytes = 4096;
            world.env.limits.tools.list_entries = 8;
            world.env.limits.tools.list_bytes = 4096;
            world.env.limits.tools.search_hits = 4;
            world.env.limits.tools.search_bytes = 4096;
            world.env.limits.tools.shell_head = 2048;
            world.env.limits.tools.shell_tail = 2048;
            let (call, done, expected) = maximum_owned_result(kind);
            let credit = tools::result_worst_case(&call, &world.env.limits.tools).expect("checked complete result cap");
            let id = b"actual-owned".as_slice();
            let name = b"owned".as_slice();
            let input = b"{}".as_slice();
            let skeleton =
                u64::try_from(2 * core::mem::size_of::<llm::Block>() + 2 * id.len() + name.len() + input.len())
                    .expect("bounded cells")
                    + call.owned_bytes().expect("checked complete call")
                    - u64::try_from(core::mem::size_of::<Call>()).expect("inline call");
            let mut spec = opening(None, u64::MAX);
            let mount = || Name::new(b"repo".as_slice().into()).expect("mount");
            spec.spec.authority = Authority {
                cwd: Box::new([mount()]),
                repos: Box::new([Repo { mount: Box::new([mount()]), root: Token::new(7), writable: true }]),
                grants: Grants { inspect: true, modify: true, shell: true },
                env: Box::new([]),
            };
            fill_initial(&mut spec, world.env.limits.session_bytes - skeleton - credit + u64::from(short));
            world.open(spec);
            assert_eq!(world.prompts.len(), 1, "provider credit secured before actual call");
            world.complete(
                Box::new([llm::Block::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    input: input.into(),
                    call: llm::Decoded::Owned { call },
                    replay: None,
                }]),
                llm::Stop::ToolUse,
                llm::Usage::ZERO,
            );
            if short {
                assert!(world.operations.is_empty(), "one-byte tight result cap refuses before any IO effect");
                assert_eq!(world.end, Some(session::End::TranscriptFull));
                assert_eq!(
                    world.turns[0].messages[2].content[0],
                    llm::Block::ToolResult { id: id.into(), result: llm::Returned::NotRun }
                );
            } else {
                assert_eq!(world.operations.len(), 1);
                let owner = world.operations[0].0;
                world.step(session::Event::Close { session: world.session.expect("actual admitted handle") });
                assert_eq!(world.cancelled_operations, [owner]);
                assert!(world.turns.is_empty(), "close retains the actual pending IO/result right");
                world.step(session::Event::Done { owner, done });
                assert_eq!(world.end, Some(session::End::Closed));
                assert_eq!(
                    world.turns[0].messages[2].content[0],
                    llm::Block::ToolResult { id: id.into(), result: llm::Returned::Owned { outcome: expected } }
                );
            }
            world.close();
        }
    }
}

/// Exact maximum lower terminal and independently expected semantic result.
fn maximum_owned_result(
    kind: u64,
) -> (smith_domain_tools::Call, smith_domain_tools::Done, smith_domain_tools::Outcome) {
    use smith_domain_tools::{Call, Done, Entry, Exit, Hit, Kind, Name, Outcome, Part, Path, Version};
    let path = || Path {
        absolute: false,
        parts: Box::new([Part::Name { name: Name::new(b"file".as_slice().into()).expect("file name") }]),
    };
    match kind {
        0 => (
            Call::Read { path: path(), skip: 0, lines: None },
            Done::Loaded { content: vec![b'r'; 4096].into(), version: Version::new([1; 4]) },
            Outcome::Read { content: vec![b'r'; 4096].into(), skipped: 0, lines: 1, total: 1, cut: false },
        ),
        1 => {
            let each = (4096 - 8 * core::mem::size_of::<Entry>()) / 8;
            let entries: Box<[Entry]> = (0..8)
                .map(|index| Entry {
                    name: Name::new(vec![b'a' + index; each].into()).expect("nonempty bounded name"),
                    kind: Kind::File,
                })
                .collect();
            assert_eq!(8 * core::mem::size_of::<Entry>() + 8 * each, 4096);
            (
                Call::List { path: path() },
                Done::Scanned { entries: entries.clone(), more: 17 },
                Outcome::Listed { entries, more: 17 },
            )
        }
        2 => {
            let hits: Box<[Hit]> = (0..4)
                .map(|line| Hit { path: b"f".as_slice().into(), line: line + 1, text: vec![b's'; 1023].into() })
                .collect();
            (
                Call::Search { path: path(), pattern: b"s".as_slice().into(), glob: None },
                Done::Found { hits: hits.clone(), more: 19, timed_out: true },
                Outcome::Found { hits, more: 19, timed_out: true },
            )
        }
        3 => (
            Call::Shell { command: b"echo owned".as_slice().into(), timeout: None },
            Done::Exited {
                exit: Exit::Code { code: 7 },
                head: vec![b'h'; 2048].into(),
                tail: vec![b't'; 2048].into(),
                dropped: 23,
            },
            Outcome::Exited {
                exit: Exit::Code { code: 7 },
                head: vec![b'h'; 2048].into(),
                tail: vec![b't'; 2048].into(),
                dropped: 23,
            },
        ),
        _ => unreachable!("four owning result kinds"),
    }
}
