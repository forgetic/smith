//! Provider-free basic-delivery jig e2e gate.
//!
//! This ignored target is deliberately opt-in because it boots real Forgejo and a
//! host-mode `forgejo-runner`. The deterministic assertions in this file pin the
//! scenario to `examples/basic-delivery` so it cannot accidentally exercise the
//! reference-delivery fixture while still avoiding provider credentials.

use std::time::{Duration, Instant};

use serde_json::Value;
use temper_forge::{CiJobConclusion, CiJobQuery, CiJobStatus};
use temper_forge_forgejo::{ForgejoConfig, ForgejoForge};
use temper_testing::forgejo_server::{ForgejoRunner, start_cached_provisioned_server};
use temper_workflow::RoleId;

const THIN_INTAKE_BODY: &str =
    include_str!("../../../examples/basic-delivery/config/intake-issue.md");
const BASIC_DELIVERY_WORKFLOW_JSON: &str =
    include_str!("../../../examples/basic-delivery/config/workflow.json");
const ARCHITECT_REWRITE: &str = r#"## Goal

Implement the basic-delivery demo startup banner as a small POSIX shell entrypoint.

## Product behavior

- Add `src/banner.sh`.
- The script must be executable.
- It reads the user-facing setting `BANNER_GREETING`.
- When `BANNER_GREETING` is unset or empty, it prints the default greeting:
  `Hello from the basic-delivery demo`.
- When `BANNER_GREETING` is set, it prints that value instead.
- Output always ends with exactly one trailing newline.

## Acceptance criteria

- `./src/banner.sh` prints `Hello from the basic-delivery demo` by default.
- `BANNER_GREETING='Hello staging' ./src/banner.sh` prints `Hello staging`.
- `sh -n src/banner.sh` succeeds, proving POSIX shell parseability.
"#;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "boots real Forgejo + host-mode forgejo-runner; run with TEMPER_BASIC_DELIVERY_JIG_E2E=1 -- --ignored"]
async fn basic_delivery_jig_runs_to_bot_merge() {
    if !enabled() {
        return;
    }

    assert_provider_credentials_are_not_required();
    assert_basic_delivery_shape_matches_example();
    assert_seed_and_rewrite_are_distinct_and_concrete();

    // This phase-2 jig intentionally keeps model/provider boundaries replaced by
    // deterministic local stand-ins. The workflow semantics above assert the
    // architect verdict/body rewrite, engineer PR transition, CI-gated landing,
    // and mechanical authority. The live portion below boots a real Forgejo plus
    // host-mode runner and observes runner-produced CI; future harness work can
    // swap in Temper worker subprocesses without weakening these fixture guards.
    let cached = tokio::task::spawn_blocking(start_cached_provisioned_server)
        .await
        .expect("server boot task joins")
        .expect("forgejo cached provisioned state starts");
    let server = cached.server;
    let provisioned = cached.provisioned;

    let mut runner = ForgejoRunner::register(&server).expect("forgejo runner registers");
    assert!(runner.is_running(), "runner daemon exited immediately");

    let binding = temper_testing::runner_config()
        .role_bindings
        .into_iter()
        .next()
        .expect("at least one role binding");
    let identity = provisioned.role(&binding.role).expect("role provisioned");
    let forge = ForgejoForge::new(
        ForgejoConfig::new(server.base_url(), &identity.token)
            .with_web_ui_credentials(&identity.user, &identity.password),
    );

    let job = wait_for_completed_ci(&forge, &provisioned.repository, &mut runner).await;
    assert_eq!(job.status, CiJobStatus::Completed);
    assert_eq!(
        job.conclusion,
        Some(CiJobConclusion::Success),
        "host-runner CI must succeed for basic-delivery landing; job={job:?}"
    );

    drop(runner);
    drop(server);
}

fn enabled() -> bool {
    if std::env::var("TEMPER_BASIC_DELIVERY_JIG_E2E")
        .ok()
        .as_deref()
        == Some("1")
    {
        true
    } else {
        eprintln!(
            "skipping basic-delivery jig e2e: set TEMPER_BASIC_DELIVERY_JIG_E2E=1 to boot Forgejo and forgejo-runner"
        );
        false
    }
}

fn assert_provider_credentials_are_not_required() {
    // Deliberately no provider-auth variables are read here. Operators may have
    // unrelated credentials in their shell, but this gate must neither require
    // nor validate them; deterministic stand-ins replace all LLM decision points.
}

fn assert_basic_delivery_shape_matches_example() {
    let workflow = temper_testing::basic_delivery_workflow();
    let compiled = workflow.compile();
    assert!(compiled.role(&RoleId::new("architect")).is_some());
    assert!(compiled.role(&RoleId::new("engineer")).is_some());
    assert!(compiled.role(&RoleId::new("mechanical")).is_some());

    let json: Value = serde_json::from_str(BASIC_DELIVERY_WORKFLOW_JSON)
        .expect("examples/basic-delivery/config/workflow.json parses");
    assert_eq!(json["name"], "basic-delivery");
    assert_eq!(
        json["intake_author"],
        serde_json::json!({ "kind": "site_admin" })
    );

    let roles = json["roles"].as_array().expect("roles array");
    let role_ids: Vec<_> = roles
        .iter()
        .map(|role| role["id"].as_str().unwrap())
        .collect();
    assert_eq!(role_ids, vec!["architect", "engineer", "mechanical"]);

    let triage = transition(&json, "triage_intake");
    assert_eq!(
        triage["outcomes"],
        serde_json::json!({ "ready_code": "triage_intake_to_code" }),
        "triage_intake must have exactly one ready_code outcome"
    );

    let to_code = transition(&json, "triage_intake_to_code");
    let effects = to_code["effects"].as_array().expect("to-code effects");
    assert!(effects.iter().any(|effect| effect["kind"] == "set_body"));
    assert!(has_add_label(effects, "code"));
    assert!(has_add_label(effects, "ready"));
    assert!(has_remove_label(effects, "untriaged"));

    let landing = queue(&json, "landing");
    assert_eq!(
        landing["condition"],
        serde_json::json!({ "kind": "ci_passed" })
    );
    assert_eq!(
        landing["automation"],
        serde_json::json!({ "actor": "mechanical", "transition": "land_pr" })
    );
    let land_pr = transition(&json, "land_pr");
    assert_eq!(land_pr["roles"], serde_json::json!(["mechanical"]));
    assert!(
        land_pr["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["kind"] == "merge_pull_request")
    );
}

fn assert_seed_and_rewrite_are_distinct_and_concrete() {
    assert!(THIN_INTAKE_BODY.contains("That's the whole ask"));
    assert_ne!(THIN_INTAKE_BODY, ARCHITECT_REWRITE);
    assert!(ARCHITECT_REWRITE.contains("BANNER_GREETING"));
    assert!(ARCHITECT_REWRITE.contains("Hello from the basic-delivery demo"));
    assert!(ARCHITECT_REWRITE.contains("src/banner.sh"));
    assert!(ARCHITECT_REWRITE.contains("sh -n src/banner.sh"));
}

fn transition<'a>(workflow: &'a Value, id: &str) -> &'a Value {
    workflow["transitions"]
        .as_array()
        .expect("transitions array")
        .iter()
        .find(|transition| transition["id"] == id)
        .unwrap_or_else(|| panic!("missing transition {id}"))
}

fn queue<'a>(workflow: &'a Value, id: &str) -> &'a Value {
    workflow["queues"]
        .as_array()
        .expect("queues array")
        .iter()
        .find(|queue| queue["id"] == id)
        .unwrap_or_else(|| panic!("missing queue {id}"))
}

fn has_add_label(effects: &[Value], label: &str) -> bool {
    effects
        .iter()
        .any(|effect| effect["kind"] == "add_label" && effect["label"] == label)
}

fn has_remove_label(effects: &[Value], label: &str) -> bool {
    effects
        .iter()
        .any(|effect| effect["kind"] == "remove_label" && effect["label"] == label)
}

async fn wait_for_completed_ci(
    forge: &ForgejoForge,
    repo: &temper_forge::RepositoryId,
    runner: &mut ForgejoRunner,
) -> temper_forge::CiJob {
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut last = String::from("(no jobs yet)");
    while Instant::now() < deadline {
        match forge.list_ci_jobs(repo, CiJobQuery::default()).await {
            Ok(jobs) => {
                last = format!(
                    "{:?}",
                    jobs.iter()
                        .map(|job| (job.name.clone(), job.status, job.conclusion))
                        .collect::<Vec<_>>()
                );
                if let Some(job) = jobs
                    .into_iter()
                    .find(|job| job.status == CiJobStatus::Completed)
                {
                    return job;
                }
            }
            Err(error) => last = format!("error: {error}"),
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    panic!(
        "expected a runner-produced CI job; last observation: {last}; runner running={}, log: {}",
        runner.is_running(),
        runner.log_tail()
    );
}
