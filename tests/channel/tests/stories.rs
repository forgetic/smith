//! Channel opening stories (protocol/channel.md, sections 2 and 10).
use skein_channel::{Closed, StreamMode};
use smith_channel::CEILINGS;
use smith_channel_world::{Observation, World};

#[test]
fn the_halves_open_on_the_highest_version_both_speak() {
    for mode in [StreamMode::One, StreamMode::Two] {
        let mut world = World::new(CEILINGS, CEILINGS, mode);
        world.settle();
        assert!(world.observations().contains(&Observation::HostOpened(1)));
        assert!(world.observations().contains(&Observation::AgentOpened(1)));
    }
}

#[test]
fn no_version_in_common_ends_the_channel_before_a_start() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.agent_hears_foreign_version();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(1))));
    assert!(!world.observations().contains(&Observation::AgentOpened(1)));
}

#[test]
fn a_host_whose_terms_are_too_small_is_refused_at_the_opening() {
    let mut host = CEILINGS;
    host.turn_body = 1;
    let mut world = World::new(host, CEILINGS, StreamMode::Two);
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(2))));
}

#[test]
fn a_start_that_decodes_reaches_the_agent() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    assert!(world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_charter_in_an_unread_version_is_refused_as_invalid_with_no_turns() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from([0, 2]));
    world.settle();
    assert!(world.observations().contains(&Observation::InvalidStart {
        why: smith_channel::InvalidStart::CharterVersion,
        turns: 0,
        spent: 0,
    }));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_malformed_charter_is_refused_with_its_own_reason() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from([0, 1]));
    world.settle();
    assert!(world.observations().contains(&Observation::InvalidStart {
        why: smith_channel::InvalidStart::MalformedCharter,
        turns: 0,
        spent: 0,
    }));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn a_start_record_that_does_not_decode_breaks_the_channel_rules() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.agent_hears_malformed_start();
    world.settle();
    assert!(world.observations().contains(&Observation::AgentEnded(Closed::RefusedHere(256))));
    assert!(!world.observations().contains(&Observation::AgentStart));
}

#[test]
fn the_agent_resolves_the_charters_endpoint_before_admission() {
    let mut entries = skein_lib::List::with_capacity(1);
    entries
        .push(smith_protocol_channel::Endpoint { name: Box::default(), number: 17, dialect: 23, account: 29 })
        .expect("one endpoint");
    let endpoints = smith_protocol_channel::Endpoints::new(entries);
    let bytes = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin");
    let charter =
        smith_protocol_channel::decode_charter(bytes, &smith_charter::CEILINGS, &endpoints).expect("v1 charter");
    assert_eq!(charter.llm.endpoint.0, 17);
    assert_eq!(charter.llm.dialect, 23);
    assert_eq!(charter.llm.account, 29);
    assert!(charter.brief.sections.is_empty());
    assert!(charter.models.is_empty());
    let unknown = smith_protocol_channel::decode_charter(
        bytes,
        &smith_charter::CEILINGS,
        &smith_protocol_channel::Endpoints::new(skein_lib::List::with_capacity(0)),
    );
    assert!(matches!(unknown, Err(smith_domain::run::Invalid::Endpoint)));
}

#[test]
fn the_full_charter_keeps_contracts_tools_and_model_prices() {
    let bytes = include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_full.bin");
    let wire = smith_charter::Charter::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(bytes))
        .expect("full charter");
    let mut entries = skein_lib::List::with_capacity(1 + wire.models().len());
    entries
        .push(smith_protocol_channel::Endpoint {
            name: Box::from(wire.main().endpoint()),
            number: 17,
            dialect: 23,
            account: 29,
        })
        .expect("main endpoint");
    for model in wire.models() {
        entries
            .push(smith_protocol_channel::Endpoint {
                name: Box::from(model.endpoint()),
                number: 17,
                dialect: 23,
                account: 29,
            })
            .expect("model endpoint");
    }
    let charter = smith_protocol_channel::decode_charter(
        bytes,
        &smith_charter::CEILINGS,
        &smith_protocol_channel::Endpoints::new(entries),
    )
    .expect("translated charter");
    assert_eq!(charter.instructions.as_ref(), wire.instructions());
    assert_eq!(charter.brief.sections.len(), usize::try_from(wire.brief().len()).expect("bounded count"));
    assert_eq!(charter.grants.host_tools.len(), usize::try_from(wire.tools().host().len()).expect("bounded count"));
    assert_eq!(
        charter.outcome.verdicts.len(),
        usize::try_from(wire.contract().verdicts().len()).expect("bounded count")
    );
    assert_eq!(charter.models.len(), usize::try_from(wire.models().len()).expect("bounded count"));
    assert_eq!(charter.llm.prices.input, wire.main().prices().input());
    assert_eq!(charter.llm.prices.cached, wire.main().prices().cached());
    assert_eq!(charter.llm.prices.output, wire.main().prices().output());
    assert_eq!(charter.llm.prices.unit, wire.main().prices().unit());
    assert_eq!(charter.budget.turns, wire.budget().turns());
    assert_eq!(charter.budget.spend, wire.budget().spend());
    assert_eq!(charter.budget.time, wire.budget().time());
}

#[test]
fn an_accepted_report_keeps_its_fields_in_charter_v1() {
    let result = smith_domain::run::outcome::Declared::Report(smith_domain::run::outcome::Report {
        text: Box::from(*b"done"),
        fields: Box::from([smith_domain::run::outcome::Field {
            name: Box::from(*b"source"),
            value: Box::from(*b"agent"),
        }]),
    });
    let bytes = smith_protocol_channel::encode_result(&result, &smith_charter::CEILINGS).expect("bounded result");
    let record = smith_charter::RunResult::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(&bytes))
        .expect("result decodes");
    assert!(matches!(record.form(), smith_charter::Form::Report));
    assert_eq!(record.text(), b"done");
    assert_eq!(record.fields().get(0).expect("one field").name(), b"source");
    assert_eq!(record.fields().get(0).expect("one field").text(), b"agent");
}

#[test]
fn an_accepted_verdict_keeps_its_label_and_items() {
    let result = smith_domain::run::outcome::Declared::Verdict(smith_domain::run::outcome::Verdict {
        name: Box::from(*b"ready"),
        text: Box::from(*b"reviewed"),
        fields: Box::default(),
        items: Box::from([smith_domain::run::outcome::Item {
            kind: Box::from(*b"path"),
            fields: Box::from([smith_domain::run::outcome::Field {
                name: Box::from(*b"name"),
                value: Box::from(*b"file"),
            }]),
        }]),
    });
    let bytes = smith_protocol_channel::encode_result(&result, &smith_charter::CEILINGS).expect("bounded result");
    let record = smith_charter::RunResult::decode(&smith_charter::CEILINGS, &mut skein_lib::Reader::new(&bytes))
        .expect("result decodes");
    assert!(matches!(record.form(), smith_charter::Form::Verdict));
    assert_eq!(record.label().as_deref(), Some(&b"ready"[..]));
    assert_eq!(record.items().get(0).expect("one item").kind(), b"path");
    assert_eq!(record.items().get(0).expect("one item").fields().get(0).expect("one field").text(), b"file");
}

#[test]
fn a_run_goes_from_start_to_answer() {
    let mut world = World::new(CEILINGS, CEILINGS, StreamMode::Two);
    world.settle();
    world.send_start(Box::from(
        &include_bytes!("../../../crates/smith-charter/golden/v1/record_charter_smallest.bin")[..],
    ));
    world.settle();
    assert!(world.observations().contains(&Observation::AgentStart));
    world.agent_admits_and_parks();
    world.settle();
    assert!(world.observations().contains(&Observation::HostAdmitted));
    assert!(world.observations().contains(&Observation::HostParked { turns: 0, spent: 0 }));
}

#[test]
fn a_model_failure_keeps_transport_evidence_and_cooldown() {
    let answer = smith_domain::run::Answer::Failed {
        failure: smith_domain::run::Failure::Model(smith_domain::run::Fault::Completion {
            failure: smith_domain::run::CompletionFailure::RateLimited {
                retry_after: skein_lib::Duration::from_nanos(42),
            },
            evidence: smith_domain::run::CompletionEvidence::Response,
        }),
        spent: smith_domain::run::Spend::ZERO,
        turns: 2,
    };
    let record =
        smith_protocol_channel::answer_record(answer, &CEILINGS, &smith_charter::CEILINGS).expect("bounded failure");
    assert_eq!(record.turns(), 2);
    let smith_channel::RunResult::Failed(failed) = record.result() else {
        panic!("expected failure");
    };
    let smith_channel::RunFailure::Model(model) = failed.reason() else {
        panic!("expected model failure");
    };
    let smith_channel::ModelFault::Completion(fault) = model.value() else {
        panic!("expected completion");
    };
    assert!(matches!(fault.failure(), smith_channel::CompletionFailure::RateLimited));
    assert!(matches!(fault.evidence(), smith_channel::CompletionEvidence::Response));
    assert_eq!(fault.retry_after(), &Some(skein_lib::Duration::from_nanos(42)));
}
