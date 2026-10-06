//! Outside controls for activation billing, exact overflow evidence and an
//! unsent root permit. Expected charges are literals from the scripted peer's
//! actual usage; no production price helper supplies these expected values.

use skein_lib::Token;
use smith_domain_session::{self as session, BudgetDenial, Dimension, End, llm, record};
use smith_session_world::recorded::{self, World, opening, transcript};

fn text() -> Box<[llm::Block]> {
    Box::new([llm::Block::Text { text: b"actual completion".as_slice().into(), replay: None }])
}

fn nudge(world: &mut World) {
    world.step(session::Event::Continue {
        session: world.session.expect("admitted session"),
        content: b"continue".as_slice().into(),
    });
}

fn answer(world: &mut World, spent: u64, spend_overflow: bool, error: bool) -> Token {
    let owner = world.delegated[0];
    world.step(session::Event::AnsweredV2 {
        owner,
        text: b"actual child result".as_slice().into(),
        error,
        spent,
        spend_overflow,
    });
    owner
}

fn inert(world: &mut World, event: session::Event) {
    let before = format!("{:?}", world.domain);
    session::step(&mut world.domain, &world.env, event, &mut world.out);
    assert!(world.out.is_empty());
    assert_eq!(format!("{:?}", world.domain), before);
}

fn redeliver(world: &mut World, owner: Token) {
    for reclaim in [false, true] {
        if reclaim {
            world.domain.reclaim();
        }
        inert(
            world,
            session::Event::AnsweredV2 {
                owner,
                text: b"stale error bill".as_slice().into(),
                error: true,
                spent: u64::MAX,
                spend_overflow: true,
            },
        );
        inert(world, session::Event::AnswerCancelledV2 { owner, spent: u64::MAX, spend_overflow: true });
    }
}

#[test]
fn nested_actual_session_bills_remain_inclusive_without_changing_own_charges() {
    let mut grandchild = World::new(41, 256);
    grandchild.open(opening(None, 100));
    grandchild.complete(text(), llm::Stop::EndTurn, llm::Usage { output_tokens: 1, ..llm::Usage::ZERO });
    grandchild.close();
    assert_eq!(grandchild.own_spend, [(2, false)]); // ceil(11/10)

    let mut child = World::new(42, 256);
    child.open(opening(None, 100));
    child.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    let owner = answer(&mut child, grandchild.turns[0].spent, grandchild.turns[0].spend_overflow, false);
    redeliver(&mut child, owner);
    child.close();
    assert_eq!(child.own_spend, [(16, false), (16, false)]);
    assert_eq!(child.spend, [(16, false), (18, false)]);
    assert_eq!(child.turns[0].spent, 18);

    let mut parent = World::new(43, 256);
    parent.open(opening(None, 100));
    parent.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    let owner = answer(&mut parent, child.turns[0].spent, child.turns[0].spend_overflow, true);
    redeliver(&mut parent, owner);
    assert_eq!(parent.own_spend, [(16, false), (16, false)]);
    assert_eq!(parent.spend, [(16, false), (34, false)]);
    assert_eq!(parent.turns[0].spent, 34, "a failed child still contributes its actual inclusive bill");
    assert_eq!(parent.used, [(recorded::USAGE, false)], "child raw usage is not the parent's own usage");
    parent.close();
}

#[test]
fn exact_and_crossing_inclusive_caps_settle_the_actual_tool_before_denying_more_work() {
    for cap in [24, 25, 26] {
        let mut world = World::new(44, 256);
        world.open(opening(None, cap));
        world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
        assert!(world.end.is_none() && world.turns.is_empty(), "crossing completion tools still run");
        answer(&mut world, 9, false, false);
        assert_eq!(world.turns[0].spent, 25); // ceil((14*7 + 5*3 + 4*11)/10) + 9
        assert!(!world.turns[0].spend_overflow);
        assert_eq!(world.own_spend, [(16, false), (16, false)]);
        if cap == 26 {
            assert!(world.end.is_none());
            assert_eq!(world.prompts.len(), 2);
        } else {
            assert_eq!(world.end, Some(End::Budget { spent: Dimension::Unit }));
            assert_eq!(world.prompts.len(), 1);
        }
        world.close();
    }
}

#[test]
fn unknown_failed_and_withdrawn_child_bills_preserve_the_turn_and_own_price() {
    for withdrawn in [false, true] {
        let mut world = World::new(45, 256);
        world.open(opening(None, 100));
        world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
        let owner = world.delegated[0];
        if withdrawn {
            world.step(session::Event::Close { session: world.session.expect("admitted session") });
            world.step(session::Event::AnswerCancelledV2 { owner, spent: 4, spend_overflow: true });
            assert_eq!(
                world.turns[0].messages[2].content[0],
                llm::Block::ToolResult { id: b"provider-call".as_slice().into(), result: llm::Returned::Withdrawn },
            );
        } else {
            answer(&mut world, 4, true, true);
            assert_eq!(
                world.turns[0].messages[2].content[0],
                llm::Block::ToolResult {
                    id: b"provider-call".as_slice().into(),
                    result: llm::Returned::Text {
                        text: b"actual child result".as_slice().into(),
                        error: true,
                        replay: None,
                    },
                },
            );
        }
        assert_eq!(world.spend, [(16, false), (20, true)]);
        assert_eq!(world.own_spend, [(16, false), (16, false)]);
        assert_eq!(world.end, Some(End::PriceOverflow));
        assert_eq!(world.turns.len(), 1);
        assert_eq!(world.turns[0].usage, recorded::USAGE);
        assert_eq!(world.turns[0].spent, 20);
        assert!(world.turns[0].spend_overflow);
        assert_eq!(world.terminal_usage, Some((1, recorded::USAGE, false)));
        redeliver(&mut world, owner);
        world.close();
    }
}

#[test]
fn inclusive_sum_failure_does_not_freeze_a_representable_own_completion() {
    let mut world = World::new(46, 256);
    world.open(opening(None, u64::MAX));
    world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    answer(&mut world, u64::MAX - 17, false, false);
    assert_eq!(world.spend, [(16, false), (u64::MAX - 1, false)]);
    assert_eq!(world.prompts.len(), 2, "the preceding inclusive prefix fits strictly below the cap");
    let last = llm::Usage { output_tokens: 1, ..llm::Usage::ZERO };
    world.complete(text(), llm::Stop::EndTurn, last); // ceil(11/10)=2
    assert_eq!(world.own_spend, [(16, false), (16, false), (18, false)]);
    assert_eq!(world.spend[2], (u64::MAX - 1, true));
    assert_eq!(world.end, Some(End::PriceOverflow));
    assert_eq!(world.turns.len(), 2);
    assert_eq!(world.turns[1].sequence, 2);
    assert_eq!(world.turns[1].usage, last);
    assert!(world.turns[1].spend_overflow);
    world.close();
}

fn enormous_opening(world: &mut World, prices: record::Prices) -> record::Opening {
    let mut spec = opening(None, u64::MAX);
    spec.prices = prices;
    spec.spec.budget.input = u64::MAX;
    spec.spec.budget.output = u64::MAX;
    spec.spec.budget.cache_read = u64::MAX;
    spec.spec.budget.cache_write = u64::MAX;
    world.env.limits.budget = spec.spec.budget;
    spec
}

fn overflowing_usage(field: usize) -> (llm::Usage, llm::Usage) {
    let mut first = [1; 4];
    let second = [2; 4];
    first[field] = u64::MAX - 1;
    let usage = |values: [u64; 4]| llm::Usage {
        input_tokens: values[0],
        output_tokens: values[1],
        cache_read_tokens: values[2],
        cache_write_tokens: values[3],
    };
    (usage(first), usage(second))
}

fn assert_usage_failure(world: &World, first: llm::Usage, second: llm::Usage, expected: End) {
    assert_eq!(world.end, Some(expected));
    assert_eq!(world.used, [(first, false), (second, true)]);
    assert_eq!(world.terminal_usage, Some((2, first, true)), "every raw field retains the same whole prefix");
    assert_eq!(world.turns.len(), 2);
    assert_eq!(world.turns[1].sequence, 2);
    assert_eq!(world.turns[1].usage, second, "the accepted overflowing completion remains exact in history");
    assert!(world.facts.contains(&session::Fact::Used { opener: Token::new(31), usage: second, usage_overflow: true }));
    assert!(world.facts.contains(&session::Fact::Ended {
        opener: Token::new(31),
        end: expected,
        turns: 2,
        usage: first,
        usage_overflow: true,
    }));
}

#[test]
fn each_raw_counter_overflow_preserves_the_entire_prefix_and_actual_turn() {
    for field in 0..4 {
        let mut world = World::new(47, 256);
        let spec = enormous_opening(&mut world, record::Prices { input: 0, cached: 0, output: 0, unit: 1 });
        let (first, second) = overflowing_usage(field);
        world.open(spec);
        world.complete(text(), llm::Stop::EndTurn, first);
        nudge(&mut world);
        world.complete(text(), llm::Stop::EndTurn, second);
        assert_usage_failure(&world, first, second, End::UsageOverflow);
        assert_eq!(world.spend, [(0, false), (0, false)]);
        assert_eq!(world.own_spend, [(0, false), (0, false)]);
        assert!(!world.turns[1].spend_overflow);
        world.close();
    }
}

#[test]
fn simultaneous_price_and_usage_failure_reports_price_first_and_keeps_both_attestations() {
    let mut world = World::new(48, 256);
    let spec = enormous_opening(&mut world, record::Prices { input: 0, cached: 0, output: 1, unit: 1 });
    let (first, second) = overflowing_usage(1);
    world.open(spec);
    world.complete(text(), llm::Stop::EndTurn, first);
    nudge(&mut world);
    world.complete(text(), llm::Stop::EndTurn, second);
    assert_usage_failure(&world, first, second, End::PriceOverflow);
    assert_eq!(world.spend, [(u64::MAX - 1, false), (u64::MAX - 1, true)]);
    assert_eq!(world.own_spend, world.spend);
    assert!(world.turns[1].spend_overflow);
    world.close();
}

#[test]
fn restored_overflow_metadata_does_not_seed_a_new_activations_price_or_usage() {
    let mut old = World::new(49, 256);
    old.open(opening(None, 100));
    old.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    answer(&mut old, 4, true, false);
    old.close();
    let saved = transcript(&old);
    assert_eq!(saved.turns[0].spent, 20);
    assert!(saved.turns[0].spend_overflow);
    let mut restored = World::new(49, 256);
    restored.open(opening(Some(saved.clone()), 100));
    assert!(restored.spend.is_empty() && restored.used.is_empty());
    assert_eq!(restored.prompts[0].messages[1].content, saved.turns[0].messages[1].content);
    let usage = llm::Usage { output_tokens: 1, ..llm::Usage::ZERO };
    restored.complete(text(), llm::Stop::EndTurn, usage);
    assert_eq!(restored.own_spend, [(2, false)]);
    assert_eq!(restored.spend, [(2, false)]);
    assert_eq!(restored.turns[0].sequence, 2);
    assert_eq!(restored.turns[0].spent, 2);
    assert!(!restored.turns[0].spend_overflow);
    restored.close();
    assert_eq!(restored.terminal_usage, Some((1, usage, false)));
    assert!(saved.turns[0].spend_overflow, "historical metadata remains data");
}

#[test]
fn unsent_budget_denial_is_inert_when_stale_and_settles_only_the_current_reservation() {
    for (reason, dimension) in [(BudgetDenial::Turns, Dimension::Turns), (BudgetDenial::Spend, Dimension::Unit)] {
        let mut world = World::new(50, 256);
        world.open(opening(None, 100));
        inert(&mut world, session::Event::BudgetDenied { owner: Token::new(u64::MAX), reason });
        let owner = world.completing.expect("a reserved requested completion");
        let first = world.trace.len();
        world.step(session::Event::BudgetDenied { owner, reason });
        assert_eq!(world.end, Some(End::Budget { spent: dimension }));
        assert_eq!(world.terminal_usage, Some((0, llm::Usage::ZERO, false)));
        assert!(
            world.spend.is_empty() && world.own_spend.is_empty() && world.used.is_empty() && world.turns.is_empty()
        );
        assert_eq!(world.trace.len() - first, 1, "only the settled Ended crosses the boundary");
        assert!(world.trace[first].starts_with("Ended"));
        assert!(!world.domain.is_ready());
        assert_eq!(world.domain.next_deadline(), None);
        for reclaim in [false, true] {
            if reclaim {
                world.domain.reclaim();
            }
            inert(&mut world, session::Event::BudgetDenied { owner, reason });
        }
        world.close();
    }
}

#[test]
fn unsent_denial_cannot_terminate_a_tool_turn_and_unit_zero_is_rejected_before_a_request() {
    let mut world = World::new(51, 256);
    world.open(opening(None, 100));
    let owner = world.completing.expect("requested completion");
    world.complete(recorded::called(), llm::Stop::ToolUse, recorded::USAGE);
    inert(&mut world, session::Event::BudgetDenied { owner, reason: BudgetDenial::Spend });
    answer(&mut world, 9, false, false);
    assert_eq!(world.turns[0].spent, 25);
    assert_eq!(world.prompts.len(), 2);
    world.close();

    let mut rejected = World::new(52, 256);
    let mut spec = opening(None, 100);
    spec.prices = record::Prices { input: 0, cached: 0, output: 0, unit: 0 };
    rejected.open(spec);
    assert_eq!(rejected.end, Some(End::Invalid));
    assert!(rejected.prompts.is_empty() && rejected.spend.is_empty() && rejected.turns.is_empty());
    rejected.domain.reclaim();
    assert_eq!((rejected.domain.sessions(), rejected.domain.runs(), rejected.domain.kits()), (0, 0, 0));
}

#[test]
fn unknown_parallel_bill_freezes_the_prefix_but_waits_for_every_actual_child_result() {
    use smith_domain_tools::Effect;

    let mut world = World::new(53, 256);
    let mut spec = opening(None, 100);
    spec.spec.delegated = Box::new([
        llm::Descriptor { ticket: Token::new(991), effect: Effect::Read },
        llm::Descriptor { ticket: Token::new(992), effect: Effect::Read },
    ]);
    world.open(spec);
    let call = |id: &[u8], ticket| llm::Block::ToolCall {
        id: id.into(),
        name: b"subagent".as_slice().into(),
        input: br#"{"task":"review"}"#.as_slice().into(),
        call: llm::Decoded::Delegated { ticket, effect: Effect::Read },
        replay: None,
    };
    world.complete(
        Box::new([call(b"left", Token::new(991)), call(b"right", Token::new(992))]),
        llm::Stop::ToolUse,
        recorded::USAGE,
    );
    assert_eq!(world.delegated.len(), 2);
    let left = answer(&mut world, 4, true, false);
    assert!(world.end.is_none() && world.turns.is_empty(), "the other actual terminal is still owed");
    assert_eq!(world.spend, [(16, false), (20, true)]);
    redeliver(&mut world, left);
    answer(&mut world, 9, false, true);
    assert_eq!(world.spend, [(16, false), (20, true), (20, true)]);
    assert_eq!(world.own_spend, [(16, false), (16, false), (16, false)]);
    assert_eq!(world.end, Some(End::PriceOverflow));
    assert_eq!(world.turns.len(), 1);
    assert_eq!(world.turns[0].messages[2].content.len(), 2);
    for (index, id) in [b"left".as_slice(), b"right".as_slice()].into_iter().enumerate() {
        assert_eq!(
            world.turns[0].messages[2].content[index],
            llm::Block::ToolResult {
                id: id.into(),
                result: llm::Returned::Text {
                    text: b"actual child result".as_slice().into(),
                    error: index == 1,
                    replay: None,
                },
            },
        );
    }
    assert_eq!(world.prompts.len(), 1);
    world.close();
}

#[test]
fn unsent_close_settles_without_a_provider_terminal_but_a_started_close_keeps_its_actual_winner() {
    let mut unsent = World::new(54, 256);
    unsent.open(opening(None, 100));
    inert(&mut unsent, session::Event::UnsentClosed { owner: Token::new(u64::MAX) });
    let owner = unsent.completing.expect("unpublished requested completion");
    let first = unsent.trace.len();
    unsent.step(session::Event::UnsentClosed { owner });
    assert_eq!(unsent.end, Some(End::Closed));
    assert_eq!(unsent.terminal_usage, Some((0, llm::Usage::ZERO, false)));
    assert!(unsent.spend.is_empty() && unsent.used.is_empty() && unsent.turns.is_empty());
    assert_eq!(unsent.trace.len() - first, 1);
    assert!(unsent.trace[first].starts_with("Ended"));
    inert(&mut unsent, session::Event::UnsentClosed { owner });
    unsent.domain.reclaim();
    inert(&mut unsent, session::Event::UnsentClosed { owner });
    unsent.close();

    let mut started = World::new(55, 256);
    started.open(opening(None, 100));
    let owner = started.completing.expect("published completion with an actual terminal still owed");
    started.step(session::Event::Close { session: started.session.expect("admitted session") });
    inert(&mut started, session::Event::UnsentClosed { owner });
    inert(&mut started, session::Event::BudgetDenied { owner, reason: BudgetDenial::Spend });
    started.complete(text(), llm::Stop::EndTurn, recorded::USAGE);
    assert_eq!(started.end, Some(End::Closed));
    assert_eq!(started.spend, [(16, false)]);
    assert_eq!(started.own_spend, [(16, false)]);
    assert_eq!(started.turns[0].usage, recorded::USAGE);
    assert_eq!(started.turns[0].spent, 16);
    assert_eq!(started.terminal_usage, Some((1, recorded::USAGE, false)));
    started.close();
}
