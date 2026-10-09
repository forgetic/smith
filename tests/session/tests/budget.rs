//! Session charges only completions and child bills that fit exactly.
use skein_lib::Token;
use smith_domain_session::{self as session, BudgetDenial, Dimension, End, llm, record};
use smith_session_world::recorded::{USAGE, World, called, opening};

fn text() -> Box<[llm::Block]> {
    Box::new([llm::Block::Text { text: b"actual completion".as_slice().into(), replay: None }])
}

fn inert(world: &mut World, event: session::Event) {
    let before = format!("{:?}", world.domain);
    session::step(&mut world.domain, &world.env, event, &mut world.out);
    assert!(world.out.is_empty());
    assert_eq!(format!("{:?}", world.domain), before);
}

#[test]
fn child_bill_is_charged_once_to_the_inclusive_spend() {
    let mut world = World::new(41, 256);
    world.open(opening(None, 100));
    world.complete(called(), llm::Stop::ToolUse, USAGE);
    let owner = world.delegated[0];
    world.step(session::Event::Answered { owner, text: b"child".as_slice().into(), error: false, spent: 9 });
    assert_eq!(world.own_spend, [16, 16]);
    assert_eq!(world.spend, [16, 25]);
    assert_eq!(world.turns[0].spent, 25);
    inert(&mut world, session::Event::AnswerCancelled { owner, spent: u64::MAX });
    world.close();
}

#[test]
fn child_bill_that_does_not_fit_ends_without_a_turn() {
    let mut world = World::new(42, 256);
    world.open(opening(None, u64::MAX));
    world.complete(called(), llm::Stop::ToolUse, USAGE);
    let owner = world.delegated[0];
    world.step(session::Event::Answered { owner, text: b"child".as_slice().into(), error: false, spent: u64::MAX });
    assert_eq!(world.end, Some(End::PriceOverflow));
    assert_eq!(world.spend, [16]);
    assert!(world.turns.is_empty());
    assert_eq!(world.terminal_usage, Some((1, USAGE)));
    world.close();
}

#[test]
fn price_overflow_rejects_the_completion_before_its_turn() {
    let mut world = World::new(43, 256);
    let mut spec = opening(None, u64::MAX);
    spec.prices = record::Prices { input: u64::MAX, cached: 0, output: 0, unit: 1 };
    world.open(spec);
    world.complete(text(), llm::Stop::EndTurn, llm::Usage { input_tokens: Some(2), ..llm::Usage::ZERO });
    assert_eq!(world.end, Some(End::PriceOverflow));
    assert!(world.spend.is_empty() && world.used.is_empty() && world.turns.is_empty());
    assert_eq!(world.terminal_usage, Some((0, llm::Usage::ZERO)));
    world.close();
}

#[test]
fn raw_overflow_keeps_only_the_charged_completion() {
    let mut world = World::new(44, 256);
    let mut spec = opening(None, u64::MAX);
    spec.prices = record::Prices { input: 0, cached: 0, output: 0, unit: 1 };
    spec.spec.budget.input = u64::MAX;
    world.env.limits.budget = spec.spec.budget;
    world.open(spec);
    let first = llm::Usage { input_tokens: Some(u64::MAX - 1), ..llm::Usage::ZERO };
    world.complete(text(), llm::Stop::EndTurn, first);
    world.step(session::Event::Continue {
        session: world.session.expect("session"),
        content: b"continue".as_slice().into(),
    });
    world.complete(text(), llm::Stop::EndTurn, llm::Usage { input_tokens: Some(2), ..llm::Usage::ZERO });
    assert_eq!(world.end, Some(End::UsageOverflow));
    assert_eq!(world.used, [first]);
    assert_eq!(world.turns.len(), 1);
    assert_eq!(world.terminal_usage, Some((1, first)));
    world.close();
}

#[test]
fn unsent_budget_denial_settles_current_reservation() {
    for (reason, dimension) in [(BudgetDenial::Turns, Dimension::Turns), (BudgetDenial::Spend, Dimension::Unit)] {
        let mut world = World::new(45, 256);
        world.open(opening(None, 100));
        inert(&mut world, session::Event::BudgetDenied { owner: Token::new(u64::MAX), reason });
        let owner = world.completing.expect("reserved completion");
        world.step(session::Event::BudgetDenied { owner, reason });
        assert_eq!(world.end, Some(End::Budget { spent: dimension }));
        assert_eq!(world.terminal_usage, Some((0, llm::Usage::ZERO)));
        assert!(world.spend.is_empty() && world.used.is_empty() && world.turns.is_empty());
        inert(&mut world, session::Event::BudgetDenied { owner, reason });
        world.close();
    }
}

#[test]
fn zero_price_unit_is_rejected_before_a_request() {
    let mut world = World::new(46, 256);
    let mut spec = opening(None, 100);
    spec.prices = record::Prices { input: 0, cached: 0, output: 0, unit: 0 };
    world.open(spec);
    assert_eq!(world.end, Some(End::Invalid));
    assert!(world.prompts.is_empty() && world.spend.is_empty() && world.turns.is_empty());
    world.domain.reclaim();
}

fn maximum(world: &World) -> u64 {
    let prompt = world.prompts.last().expect("one completion is pending");
    session::preview_reservation(
        &world.domain,
        world.completing.expect("the completion owns its reservation"),
        prompt.input_bytes().expect("bounded prompt"),
        world.env.limits.protocol_allowance,
        prompt.max_tokens,
    )
    .expect("maximum fits the session budget")
}

#[test]
fn a_completion_that_costs_less_than_its_maximum_returns_the_rest() {
    let usage = llm::Usage { output_tokens: Some(1), ..llm::Usage::ZERO };
    let mut probe = World::new(47, 256);
    probe.open(opening(None, u64::MAX));
    let first_maximum = maximum(&probe);
    probe.complete(called(), llm::Stop::ToolUse, usage);
    let owner = probe.delegated[0];
    probe.step(session::Event::Answered { owner, text: b"child".as_slice().into(), error: false, spent: 0 });
    let second_maximum = maximum(&probe);
    let actual = record::Prices { input: 7, cached: 3, output: 11, unit: 10 }.price(usage).expect("priced usage");
    let budget = second_maximum.checked_add(actual).expect("small fixture budget");
    assert!(first_maximum <= budget);
    assert!(first_maximum.checked_add(second_maximum).expect("small fixture maximums") > budget);
    probe.close();

    let mut world = World::new(48, 256);
    world.open(opening(None, budget));
    assert_eq!(maximum(&world), first_maximum);
    world.complete(called(), llm::Stop::ToolUse, usage);
    assert_eq!(world.own_spend, [actual]);
    let owner = world.delegated[0];
    world.step(session::Event::Answered { owner, text: b"child".as_slice().into(), error: false, spent: 0 });
    assert_eq!(maximum(&world), second_maximum, "unused maximum is available for the next completion");
    assert_eq!(world.prompts.len(), 2);
    world.close();
}

#[test]
fn missing_input_or_output_charges_the_full_reservation_and_preserves_raw_reports() {
    for mask in 1..4 {
        let mut world = World::new(70 + mask, 256);
        world.open(opening(None, 100));
        let owner = world.completing.expect("admitted call");
        assert_eq!(session::reserve_completion(&mut world.domain, owner, 9, 3, 4), Some(13));
        let raw = llm::Usage {
            input_tokens: if mask & 1 == 0 { Some(1) } else { None },
            output_tokens: if mask & 2 == 0 { Some(1) } else { None },
            cache_read_tokens: Some(0),
            cache_write_tokens: None,
            reasoning_tokens: None,
        };
        assert_eq!(session::preview_completion(&world.domain, owner, raw), Ok(13));
        world.complete(text(), llm::Stop::EndTurn, raw);
        assert_eq!(world.used, [raw]);
        assert_eq!(world.spend, [13]);
        assert_eq!(world.own_spend, [13]);
        assert_eq!((world.turns[0].usage, world.turns[0].spent), (raw, 13));
        let fact =
            world.facts.iter().find(|fact| matches!(fact.kind, session::FactKind::Used { .. })).expect("used fact");
        assert_eq!(fact.kind, session::FactKind::Used { opener: Token::new(31), usage: raw });
        assert_eq!(fact.response.as_ref().map(|info| (info.number, info.spent)), Some((1, Some(13))));
        world.close();
        assert_eq!(world.terminal_usage, Some((1, raw)));
    }
}

#[test]
fn reported_zero_and_reasoning_do_not_charge_the_reserved_maximum_again() {
    let mut world = World::new(74, 256);
    world.open(opening(None, 100));
    let owner = world.completing.expect("admitted call");
    assert_eq!(session::reserve_completion(&mut world.domain, owner, 9, 3, 4), Some(13));
    let raw =
        llm::Usage { input_tokens: Some(0), output_tokens: Some(0), reasoning_tokens: Some(0), ..llm::Usage::NONE };
    world.complete(text(), llm::Stop::EndTurn, raw);
    assert_eq!(world.spend, [0]);
    assert_eq!(world.used, [raw]);
    world.close();
    let mut world = World::new(75, 256);
    world.open(opening(None, 100));
    let raw =
        llm::Usage { input_tokens: Some(0), output_tokens: Some(2), reasoning_tokens: Some(2), ..llm::Usage::NONE };
    world.complete(text(), llm::Stop::EndTurn, raw);
    assert_eq!(world.spend, [3]);
    assert_eq!(world.used, [raw]);
    world.close();
}

#[test]
fn reported_counts_after_an_unreported_turn_still_exhaust_the_token_budget() {
    let mut world = World::new(76, 256);
    let mut spec = opening(None, 100);
    spec.spec.budget.input = 2;
    world.open(spec);
    let owner = world.completing.expect("first call");
    assert_eq!(session::reserve_completion(&mut world.domain, owner, 9, 3, 4), Some(13));
    world.complete(text(), llm::Stop::EndTurn, llm::Usage::NONE);
    world.step(session::Event::Continue {
        session: world.session.expect("admitted"),
        content: b"continue".as_slice().into(),
    });
    world.complete(text(), llm::Stop::EndTurn, llm::Usage { input_tokens: Some(2), ..llm::Usage::ZERO });
    world.step(session::Event::Continue {
        session: world.session.expect("admitted"),
        content: b"continue".as_slice().into(),
    });
    assert_eq!(world.end, Some(End::Budget { spent: Dimension::Input }));
    assert!(world.completing.is_none());
    assert_eq!(world.spend, [13, 15]);
    assert_eq!(world.terminal_usage, Some((2, llm::Usage::NONE)));
    world.close();
}
