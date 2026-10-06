use alloc::boxed::Box;
use skein_lib::Duration;
use smith_domain::run::{self, charter::Endpoint, outcome};

use crate::{Config, Contract, Domain, Invalid, Limits, charter};

fn limits() -> Limits {
    Limits {
        agent: smith_agent_world::LIMITS,
        endpoints: Box::new([Endpoint(4)]),
        chat_bytes: 64,
        text_bytes: 1024,
        models: 2,
        line_bytes: 256,
        show_bytes: 1024,
        lines: 8,
        unsaved: 2,
        facts: 16,
    }
}

fn config() -> Config {
    Config {
        chat: Box::from(&b"main"[..]),
        instructions: Box::from(&b"Assist the person"[..]),
        brief: run::charter::Brief {
            sections: Box::new([run::charter::Section {
                title: Box::from(&b"Context"[..]),
                text: Box::from(&b"Work carefully"[..]),
            }]),
        },
        models: Box::new([run::charter::Llm {
            prices: run::Prices { input: 1, cached: 1, output: 1, unit: 1 },
            dialect: 1,
            account: 7,
            endpoint: Endpoint(4),
            model: Box::from(&b"small"[..]),
            max_tokens: 128,
        }]),
        budget: run::Budget { turns: 8, spend: 1, time: Duration::from_secs(60) },
        conventions: Some(run::Conventions { guide: Box::from(&b"GUIDE.md"[..]), checks: Box::from(&b"check"[..]) }),
        contract: Contract::Report(outcome::TextSpec { max: 256, fields: Box::new([]) }),
        deliver: None,
        title_field: Box::from(&b"title"[..]),
        waiting: Duration::from_secs(30),
        resume: true,
        accounts: Box::new([7]),
        workspace: None,
    }
}

#[test]
fn report_charter_copies_every_local_choice_without_workspace_tools() {
    let config = config();
    let limits = limits();
    assert_eq!(config.validate(&limits), Ok(()));
    let charter = charter(&config);
    assert!(charter.resume, "configured resume is retained");
    assert_eq!(charter.waiting, config.waiting);
    assert_eq!(charter.instructions, config.instructions);
    assert_eq!(charter.brief.sections, config.brief.sections);
    assert_eq!(charter.conventions, config.conventions);
    assert_eq!(charter.budget, config.budget);
    assert_eq!(charter.llm, config.models[0]);
    assert!(charter.models.is_empty(), "only the first model is the main model");
    assert!(charter.outcome.report.is_some(), "report is selected");
    assert!(charter.outcome.change.is_none(), "change was not selected");
    assert!(charter.grants.wait, "local chat may wait for the person");
    assert!(!charter.grants.tools.inspect && !charter.grants.tools.modify && !charter.grants.tools.shell);
    assert!(!charter.grants.agents, "sub-agent authority is not configured");
    assert!(charter.grants.host_tools.is_empty(), "local host has no tools");
    assert!(Domain::new(config, &limits, 11).is_ok(), "validated local domain builds its agent");
}

#[test]
fn change_and_second_model_map_to_distinct_charter_choices() {
    let mut config = config();
    let mut second = config.models[0].clone();
    second.model = Box::from(&b"large"[..]);
    config.models = Box::new([config.models[0].clone(), second.clone()]);
    config.contract = Contract::Change(outcome::ChangeSpec { checks_must_pass: true, fields: Box::new([]) });
    config.resume = false;
    config.waiting = Duration::ZERO;
    let charter = charter(&config);
    assert_eq!(charter.models.as_ref(), &[second]);
    assert!(!charter.resume, "fresh run was configured");
    assert_eq!(charter.waiting, Duration::ZERO);
    assert!(charter.outcome.report.is_none(), "report was not selected");
    assert!(charter.outcome.change.is_some(), "change was selected");
}

#[test]
fn invalid_local_configuration_is_refused_before_agent_construction() {
    let limits = limits();
    let mut invalid = config();
    invalid.chat = Box::new([]);
    assert_eq!(invalid.validate(&limits), Err(Invalid::Chat));
    assert_eq!(Domain::new(invalid, &limits, 1).err(), Some(Invalid::Chat));

    let mut invalid = config();
    invalid.instructions = Box::from([b'x'; 1025]);
    assert_eq!(invalid.validate(&limits), Err(Invalid::Text));

    let mut invalid = config();
    invalid.models = Box::new([]);
    assert_eq!(invalid.validate(&limits), Err(Invalid::Models));

    let mut invalid = config();
    invalid.accounts = Box::new([7, 7]);
    assert_eq!(invalid.validate(&limits), Err(Invalid::Accounts));

    let mut invalid = config();
    invalid.models[0].endpoint = Endpoint(99);
    assert_eq!(invalid.validate(&limits), Err(Invalid::Endpoint));
}
