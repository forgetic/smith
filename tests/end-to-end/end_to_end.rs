pub mod support;

use smith_local_process_world::{Placement, referee::CheckoutRead};
use smith_real_world::{Scratch, World};

#[test]
fn the_shipped_local_and_agent_answer_on_a_terminal() {
    let scratch = Scratch::new();
    let scenario = World::new(31, &scratch, Placement::Spawned, b"First answer").scenario;
    let seen = support::run(&scratch, scenario);
    assert_eq!(seen.queries.len(), 2);
}

#[test]
fn a_first_run_signs_in_and_ends_with_a_commit_in_place() {
    let scratch = Scratch::new();
    let mut scenario = World::changed(32, &scratch, Placement::Spawned).scenario;
    scenario.launch.authenticated = true;
    scenario.authentication = Some(smith_local_process_world::world::Authentication::SignIn);
    std::fs::remove_file(scenario.launch.token_directory.join("0.json")).expect("first sign-in has no token");
    let seen = support::run(&scratch, scenario);
    let oauth = seen.oauth.expect("issuer and browser observations");
    assert_eq!(oauth.posts, 1);
    assert_eq!(oauth.pages, 1);
    assert!(oauth.browser_replied && oauth.saved_before_query);
    let checkout = scratch.checkout();
    let head = checkout.head().expect("committed head");
    let files = checkout.files(&head);
    assert_eq!(files[b"original.txt".as_slice()], b"edited\n");
    assert_eq!(files[b"result.txt".as_slice()], b"new\n");
    assert_eq!(files[b"command.txt".as_slice()], b"ran\n");
    assert_eq!(files[b"checks-ran.txt".as_slice()], b"checked\n");
}

#[test]
fn a_second_run_resumes_the_chat_from_its_files() {
    let scratch = Scratch::new();
    let first = support::run(&scratch, World::new(33, &scratch, Placement::Spawned, b"First answer").scenario);
    let second = support::run(&scratch, World::new(34, &scratch, Placement::Spawned, b"Second answer").scenario);
    assert!(second.queries[0].messages.len() > first.queries[0].messages.len());
    assert!(
        second.queries[0].messages.iter().flat_map(|message| message.parts.iter()).any(|part| {
            matches!(part, skein_fake_llm_domain::api::Part::ToolCall { name, arguments, .. }
            if name.as_ref() == b"finish" && arguments.windows(12).any(|bytes| bytes == b"First answer"))
        }),
        "the second actual agent received the first run's durable history"
    );
}

#[test]
fn the_startup_refusals_of_both_commands_are_observed_outside() {
    let scratch = Scratch::new();
    for command in ["agent", "local"] {
        let arguments = if command == "agent" {
            vec![command.into(), "".into()]
        } else {
            vec![command.into(), "".into(), scratch.path().into()]
        };
        let errors = support::refusal(&scratch, arguments);
        assert!(String::from_utf8_lossy(&errors).contains(if command == "agent" {
            "configuration metadata"
        } else {
            "settings metadata"
        }));
    }
    let path = scratch.path().join("agent-config.json");
    std::fs::write(
        &path,
        br#"{"profile":{"name":"standard","declared":{"memory":1099511627776},"policy":{"group_stop":10}},"endpoints":[],"environment":[]}"#,
    )
    .expect("valid agent fixture");
    let errors = support::refusal(&scratch, vec!["agent".into(), path.clone().into()]);
    assert_eq!(
        String::from_utf8_lossy(&errors).lines().last(),
        Some("smith: the run could not answer: Some(ChannelEnded)")
    );
    std::fs::write(&path, br#"{"profile":{"name":"standard","declared":{"memory":1099511627776},"policy":{"group_stop":10}},"endpoints":[{"name":"invalid","number":1,"dialect":1,"account":0,"provider":"codex","connect":10000,"handshake":10000,"models":[{"name":"fake","window":8192,"output":1024,"reasoning_item":2048,"head":60000,"idle":30000}],"address":"192.0.2.1:8080","transport":"plaintext"}],"environment":[]}"#).expect("invalid transport fixture");
    let errors = support::refusal(&scratch, vec!["agent".into(), path.into()]);
    assert_eq!(
        String::from_utf8_lossy(&errors).lines().last(),
        Some("smith: plaintext endpoint address must be loopback")
    );
    assert!(!scratch.path().join("chat").exists(), "refusals leave no chat");
}

#[test]
fn the_live_outcome_referee_runs_on_the_shared_fake_binary_harness() {
    let report = Scratch::new();
    support::live_run::run_fake(&report, &World::new(35, &report, Placement::Spawned, b"First answer").scenario);
    let change = Scratch::new();
    support::live_run::run_fake(&change, &World::changed(36, &change, Placement::Spawned).scenario);
}

#[test]
fn smith_agent_refuses_a_malformed_configuration_before_its_channel_opens() {
    let scratch = Scratch::new();
    let path = scratch.path().join("malformed-agent.json");
    std::fs::write(&path, br#"{"profile":"standard"}"#).expect("malformed configuration fixture");
    let errors =
        support::refusal_program(std::path::Path::new(env!("CARGO_BIN_EXE_smith-agent")), &scratch, vec![path.into()]);
    let line = std::str::from_utf8(&errors).expect("UTF-8 refusal");
    assert!(line.starts_with("smith: configuration JSON:"));
    assert_eq!(line.lines().count(), 1, "startup writes one refusal");
}
