//! Both real domains meet across the channel halves, with the process and
//! provider kept as scripted neighbours (protocol/channel.md, section 10).

use skein_channel::StreamMode;
use skein_lib::{Duration, Env, Queue, Reader, ReplyTo, Time, Token, Wall, Writer};
use smith_channel::CEILINGS;
use smith_channel_world::{Observation, World};
use smith_domain::{self as agent, Event as AgentEvent, Request as AgentRequest};
use smith_host_domain::{self as host, Down, Up};

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one composed story follows both domains and both wire halves to final cleanup"
)]
#[expect(clippy::wildcard_enum_match_arm, reason = "the story rejects every request outside its scripted path")]
fn the_host_start_enters_the_agent_domain_and_its_answer_returns_to_the_host_domain() {
    let smallest = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin");
    let source =
        smith_charter::Charter::decode(&smith_charter::CEILINGS, &mut Reader::new(smallest)).expect("golden charter");
    let mut parts = source.into_parts();
    parts.budget = smith_charter::Budget::new(
        &smith_charter::CEILINGS,
        smith_charter::BudgetParts { turns: 1, spend: 1, time: Duration::from_secs(60) },
    )
    .expect("workable budget");
    parts.contract = smith_charter::Contract::new(
        &smith_charter::CEILINGS,
        smith_charter::ContractParts {
            report: Some(
                smith_charter::TextRule::new(
                    &smith_charter::CEILINGS,
                    smith_charter::TextRuleParts { max: 128, fields: skein_lib::List::with_capacity(0) },
                )
                .expect("report rule"),
            ),
            verdicts: skein_lib::List::with_capacity(0),
            change: None,
            failure: None,
        },
    )
    .expect("outcome contract");
    let mut llm = parts.main.into_parts();
    llm.model = Box::from(*b"fake");
    llm.max_tokens = 1024;
    llm.prices = smith_charter::Prices::new(
        &smith_charter::CEILINGS,
        smith_charter::PricesParts { input: 0, cached: 0, output: 0, unit: 1 },
    )
    .expect("prices");
    parts.main = smith_charter::Llm::new(&smith_charter::CEILINGS, llm).expect("model");
    let record = smith_charter::Charter::new(&smith_charter::CEILINGS, parts).expect("valid charter record");
    let mut writer = Writer::new(usize::try_from(record.measure()).expect("small charter"));
    record.encode(&mut writer).expect("measured charter");
    let charter = writer.finish();
    let limits = smith_agent_world::LIMITS;
    let largest_turn = agent::max_turn_bytes(&limits).expect("bounded concrete turn");
    let mut host_limits = smith_host_world::limits();
    host_limits.charter_bytes = u64::try_from(charter.len()).expect("small charter");
    host_limits.turn_bytes = largest_turn;
    host_limits.unacknowledged_bytes = largest_turn;
    let mut host_world = smith_host_world::World::new(1, host_limits);
    let mut start = smith_host_world::start();
    start.charter = charter;
    start.transcript = None;
    start.answered = Box::default();
    start.grants = Box::from([host::Grant { account: 0, generation: 1, valid: Duration::from_secs(7200) }]);
    host_world.spawn(start);
    host_world.spawned();
    let first = host_world.seen.down.pop().expect("host domain first Send");
    let Down::Start { start, window } = first else { panic!("host first word must be Start") };
    let mut wire = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    wire.settle();
    wire.send_domain_start(
        start,
        window,
        smith_host_protocol::Values { paths: Box::default(), credentials: Box::from([Box::from(*b"secret")]) },
    );
    wire.settle();
    assert!(wire.observations().contains(&Observation::HostSent(Token::new(2))));
    host_world.sent();
    let decoded = wire.take_agent_start().expect("translated Start reached agent side");
    let mut domain =
        agent::Domain::new(&limits, agent::Config { endpoints: Box::new([agent::run::charter::Endpoint(0)]) }, 7);
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut out = Queue::with_capacity(agent::max_out(&limits));
    agent::step(
        &mut domain,
        &env,
        AgentEvent::Start {
            reply_to: ReplyTo::new(Token::new(1)),
            host_run: Token::new(7),
            activation: decoded.activation,
            window: decoded.window,
            charter: decoded.charter,
            workspace: None,
            transcript: decoded.transcript,
            answered: decoded.answered,
            grants: decoded.grants.iter().map(|grant| agent::Grant { name: grant.name, valid: grant.valid }).collect(),
        },
        &mut out,
    );
    let mut run = None;
    let mut completion = None;
    while let Some(request) = out.pop() {
        match request {
            AgentRequest::Admitted { host_run, run: admitted } => {
                assert_eq!(host_run, Token::new(7));
                run = Some(admitted);
                wire.agent_admits();
            }
            AgentRequest::Complete { owner, .. } => completion = Some(owner),
            other => panic!("unexpected initial agent request: {other:?}"),
        }
    }
    let run = run.expect("real agent domain admitted the decoded Start");
    let completion = completion.expect("scripted provider owes a terminal");
    wire.settle();
    assert!(wire.observations().contains(&Observation::HostAdmitted));
    host_world.up(Up::Admitted);
    assert!(host_world.seen.admitted);
    host_world.event(host::Event::Stop { agent: host_world.agent() });
    let down = host_world.seen.down.pop().expect("host domain cancelled live run");
    assert_eq!(down, Down::Cancel);
    wire.host_cancels();
    wire.settle();
    host_world.sent();
    assert!(wire.observations().contains(&Observation::AgentCancel));
    agent::step(&mut domain, &env, AgentEvent::Cancel { run }, &mut out);
    assert!(out.pop().is_none(), "cancel first enters the child handoff");
    domain.reclaim();
    assert!(domain.is_ready());
    agent::resume(&mut domain, &env, &mut out);
    assert_eq!(out.pop(), Some(AgentRequest::Cancel { owner: completion }));
    assert!(out.pop().is_none());
    agent::step(&mut domain, &env, AgentEvent::Cancelled { owner: completion }, &mut out);
    let answer = match out.pop().expect("real agent domain answers after provider settlement") {
        AgentRequest::Answer { to, answer } => {
            assert_eq!(to, ReplyTo::new(Token::new(1)));
            answer
        }
        other => panic!("expected domain answer, got {other:?}"),
    };
    assert!(out.pop().is_none());
    assert!(matches!(answer, agent::run::Answer::Failed { failure: agent::run::Failure::Cancelled, turns: 0, .. }));
    wire.agent_answers(answer);
    wire.settle();
    let translated = wire.take_host_answer().expect("host protocol decoded final answer");
    assert!(matches!(translated.result, host::RunResult::Failed { failure: host::RunFailure::Cancelled }));
    host_world.up(Up::Answer { answer: translated });
    host_world.cleanup();
    host_world.settled();
    assert!(matches!(
        host_world.seen.answer.as_ref().map(|answer| &answer.result),
        Some(host::RunResult::Failed { failure: host::RunFailure::Cancelled })
    ));
    assert_eq!(host_world.seen.gone, Some(host::End::Stopped));
}
