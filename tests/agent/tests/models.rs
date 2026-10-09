//! Model quantities are checked at the root entrance before provider work.
//! Contract: domain/run.md, section 4; protocol/charter.md, sections 2 and 5.

use skein_lib::{Duration, ReplyTo, Token};
use smith_agent_world::{Job, Settings, World, scripted_charter};
use smith_domain::{Event, Grant, GrantName, Window, run};

fn world(settings: &Settings, charter: run::Charter) -> World {
    World::with_start(
        *settings,
        Event::Start {
            messages: Box::default(),
            reply_to: ReplyTo::new(Token::new(1)),
            host_run: Token::new(1),
            activation: 1,
            window: Window { turns: u32::MAX, bytes: u64::MAX },
            charter,
            workspace: None,
            grants: Box::new([Grant {
                name: GrantName { account: 0, generation: 1 },
                valid: Duration::from_secs(7200),
            }]),
            transcript: None,
            answered: Box::new([]),
        },
    )
}

#[test]
fn a_charter_naming_an_undeclared_model_is_refused_at_the_entrance() {
    let settings = Settings { job: Job::Reporting, ..Settings::calm(1401) };
    let mut requested = scripted_charter(&settings);
    requested.llm.model = Box::from(&b"not-served"[..]);
    let mut world = world(&settings, requested);
    world.run(2000);
    assert_eq!(world.answer(), &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Llm)));
    assert!(world.prompts().is_empty());
    assert!(world.turns().is_empty());
}

#[test]
fn a_window_above_its_declaration_is_refused() {
    let settings = Settings { job: Job::Reporting, ..Settings::calm(1402) };
    let mut requested = scripted_charter(&settings);
    requested.llm.window = 8193;
    let mut world = world(&settings, requested);
    world.run(2000);
    assert_eq!(world.answer(), &run::Answer::Refused(run::Refusal::Invalid(run::Invalid::Llm)));
    assert!(world.prompts().is_empty());
    assert!(world.turns().is_empty());
}

#[test]
fn the_fake_provider_receives_the_charters_effective_output_cap() {
    let settings = Settings { job: Job::Reporting, ..Settings::calm(1403) };
    let mut requested = scripted_charter(&settings);
    requested.llm.window = 4096;
    requested.llm.output = 256;
    let mut world = world(&settings, requested);
    world.run(2000);
    assert!(matches!(world.answer(), run::Answer::Accepted { .. }));
    assert!(!world.prompts().is_empty());
    assert!(world.prompts().iter().all(|query| query.max_tokens == 256));
}
