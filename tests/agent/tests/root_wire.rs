//! Original root Start, opaque host effect and cancellation over actual byte peers.
//! Observations stay outside the domains: native continued arguments have explicit
//! handwritten expectations; incoming host input remains byte exact.
//! Contract: domain/client.md, sections 1, 4, 5 and 6; domain/run.md,
//! sections 5.2, 10, 13 and 14; testing-strategy.md, sections 2.3 and 6.

use skein_fake_llm_domain::api::{Finish, Line, Part, Query, Script, Turn};
use skein_lib::{Time, Wall};
use skein_world::domain::Span;
use smith_agent_world::{
    Job, Settings, World,
    wire::{self, Configuration, Observed},
};
use smith_domain::{run, session::llm};
use smith_protocol_llm::{self as adapter, Limits};

const BODY: &[u8] = br#"{ "opaque" : {"future":[1,true,null]}, "extra":"unchanged" }"#;
const ID: &[u8] = b"call_0000000000000001";
const FEEDBACK: &[u8] = b"opaque host answer: first decision";

fn scripts() -> Box<[Script]> {
    Box::new([Script {
        cue: b"@hosttools".as_slice().into(),
        turns: Box::new([
            Turn {
                lines: Box::new([Line::Call { name: b"host_action".as_slice().into(), arguments: BODY.into() }]),
                finish: Finish::ToolCalls,
                tokens: 8,
            },
            Turn {
                lines: Box::new([Line::Text { text: b"actual continued result".as_slice().into() }]),
                finish: Finish::Stop,
                tokens: 4,
            },
        ]),
    }])
}

fn feedback(query: &Query, arguments: &[u8]) -> bool {
    let [_, assistant, user] = query.messages.as_ref() else { return false };
    let [Part::ToolCall { id, name, arguments: actual }] = assistant.parts.as_ref() else { return false };
    let [Part::ToolOutput { id: returned, output, is_error }] = user.parts.as_ref() else { return false };
    id.as_ref() == ID
        && returned.as_ref() == ID
        && name.as_ref() == b"host_action"
        && actual.as_ref() == arguments
        && output.as_ref() == FEEDBACK
        && !is_error
}

fn host_boundary(world: &World, expected_arguments: &[u8]) {
    let first = &world.prompts()[0];
    assert!(first.system.windows(b"@hosttools".len()).any(|bytes| bytes == b"@hosttools"));
    assert!(
        first
            .system
            .windows(b"The answer is in src/lib.rs; the checks want it to be 43.\n".len())
            .any(|bytes| bytes == b"The answer is in src/lib.rs; the checks want it to be 43.\n"),
        "the real discovered guide reaches the first actual system text: {first:?}"
    );
    let read =
        world.trace().iter().position(|line| line.contains("agent -> Read {")).expect("actual discovery Read request");
    let returned =
        world.trace().iter().position(|line| line.contains("agent <- Read {")).expect("actual discovery Read terminal");
    let complete = world
        .trace()
        .iter()
        .position(|line| line.contains("agent -> Complete {"))
        .expect("actual root Complete request");
    assert!(
        read < returned && returned < complete,
        "original discovery request and terminal precede actual Complete: {:?}",
        world.trace()
    );
    let host = first.tools.iter().find(|tool| tool.name.as_ref() == b"host_action").expect("declared opaque host tool");
    assert_eq!(host.description.as_ref(), b"Opaque host write");
    assert_eq!(host.parameters.as_ref(), br#"{"type":"object"}"#);
    let [submission] = world.host_submissions() else { panic!("one actual host submission") };
    assert_eq!(submission.tool.as_ref(), b"host_action");
    assert_eq!(submission.effect, run::HostEffect::Write);
    assert_eq!(submission.input.bytes(), BODY);
    let [(_, _, run::HostReply::Answered(answer))] = world.host_terminals() else { panic!("one actual host answer") };
    assert_eq!(answer.text(), FEEDBACK);
    assert!(!answer.error());
    assert!(feedback(&world.prompts()[1], expected_arguments));
    let mut missing = world.prompts()[1].clone();
    missing.messages[2].parts = Box::new([]);
    assert!(!feedback(&missing, expected_arguments));
    let mut rewritten = world.prompts()[1].clone();
    rewritten.messages[2].parts =
        Box::new([Part::ToolOutput { id: ID.into(), output: b"rewritten".as_slice().into(), is_error: false }]);
    assert!(!feedback(&rewritten, expected_arguments));
}

fn retired_boundary(world: &World) {
    assert!(matches!(world.answer(), run::Answer::Failed { failure: run::Failure::Cancelled, .. }));
    let [first_binding, second_binding] = world.wire_bindings() else { panic!("retired outside bindings") };
    let owner = first_binding.owner;
    assert_eq!(
        first_binding.observed.iter().map(|(_, _, event)| *event).collect::<Vec<_>>(),
        [Observed::Completed(owner), Observed::Reusable, Observed::Close, Observed::Closed]
    );
    assert_eq!(
        second_binding.observed.iter().map(|(_, _, event)| *event).collect::<Vec<_>>(),
        [Observed::Close, Observed::Cancelled(owner), Observed::Closed]
    );
    assert!(first_binding.retired.is_some());
    assert_eq!(second_binding.retired.expect("actual lower Closed").1, Wall::from_nanos(3000));
    assert!(second_binding.observed.iter().all(|(_, wall, _)| *wall == Wall::from_nanos(3000)));
    assert!(world.turns().iter().flat_map(|turn| &turn.messages).flat_map(|message| &message.content).any(|block| {
        matches!(block, llm::Block::ToolCall { id, name, input, .. } if id.as_ref() == ID && name.as_ref() == b"host_action" && input.as_ref() == BODY)
    }), "root-owned concrete Turn retains the incoming delta bytes");
}

fn root_story(configuration: Configuration) {
    let expected_arguments = configuration.continuation_arguments.clone();
    let mut bounds = Limits { client: skein_llm_world::limits(), tool_bytes: 32768, result_bytes: 32768 };
    bounds.client.http.request = 16384;
    bounds.client.dialect.request_bytes = 16384;
    let mut settings =
        Settings { job: Job::HostTools, writable: false, network: Span::millis(1, 1), ..Settings::calm(71) };
    settings.limits.session.completion_bytes =
        adapter::completion_worst_case(&bounds.client, settings.limits.decoded_call_bytes)
            .expect("complete actual translated reservation");
    settings.limits.session.completion_blocks = bounds.client.dialect.parts;
    let expected_receiving = settings.limits;
    let mut world = World::with_wire(settings, configuration, bounds, scripts());
    world.wall_at(Wall::from_nanos(9000));
    for _ in 0..100_000 {
        assert!(!world.drive(1), "continued query precedes parent cancellation");
        if world.prompts().len() == 2 {
            break;
        }
    }
    assert_eq!(world.prompts().len(), 2, "both queries were parsed by actual byte peers");
    assert!(world.now() < Time::from_nanos(1_000_000_000), "queued wire bytes prevent jumping to the run deadline");
    host_boundary(&world, &expected_arguments);
    let [first_binding, second_binding] = world.wire_bindings() else { panic!("two actual physical bindings") };
    assert_eq!(first_binding.owner, second_binding.owner, "the root reuses the logical callback");
    assert_eq!(second_binding.retained_at_start, 1, "new call starts while the old physical Close is owed");
    assert_eq!(first_binding.receiving.max_completion_bytes, expected_receiving.session.completion_bytes);
    assert_eq!(first_binding.receiving.max_completion_blocks, expected_receiving.session.completion_blocks);
    assert_eq!(first_binding.receiving.max_failure_bytes, expected_receiving.session.failure_bytes);
    assert_eq!(first_binding.receiving.decoded_call_bytes, expected_receiving.decoded_call_bytes);
    assert_eq!(first_binding.started.1, Wall::from_nanos(9000));
    assert_eq!(second_binding.started.1, Wall::from_nanos(9000));
    world.wall_at(Wall::from_nanos(3000));
    world.cancel_run();
    world.run(100_000);
    retired_boundary(&world);
    assert!(
        feedback(&world.prompts()[1], &expected_arguments),
        "settlement preserves the positive outside observation"
    );
}

#[test]
fn actual_root_wire_host_feedback_and_reused_owner_close_survive_parent_cancel() {
    for configuration in wire::configurations() {
        root_story(configuration);
    }
}
