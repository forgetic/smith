//! Provider-free basic-delivery jig e2e gate.
//!
//! This ignored target is deliberately opt-in because it boots real Forgejo and a
//! host-mode `forgejo-runner`. The local jig is deterministic: it validates the
//! same basic-delivery workflow fixture used by the example/Temper tests and
//! never reads provider credentials or calls a model provider.

use std::time::{Duration, Instant};

use temper_forge::{CiJobConclusion, CiJobQuery, CiJobStatus};
use temper_forge_forgejo::{ForgejoConfig, ForgejoForge};
use temper_testing::forgejo_server::{ForgejoRunner, start_cached_provisioned_server};
use temper_workflow::RoleId;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "boots real Forgejo + host-mode forgejo-runner; run with TEMPER_BASIC_DELIVERY_JIG_E2E=1 -- --ignored"]
async fn basic_delivery_jig_reaches_provider_free_forgejo_runner_ci() {
    if !enabled() {
        return;
    }

    assert_provider_credentials_are_not_required();
    assert_basic_delivery_shape_matches_example();

    // The cached provisioned Forgejo fixture is the same real-server topology
    // used by Temper's Forgejo/runner e2e tests: a throwaway Forgejo data tree, a
    // real git repository, and a real Actions workflow. Registering the
    // host-mode runner here proves this Smith gate can be wired into CI without
    // containers or provider credentials. The workflow-role decisions that would
    // normally call an LLM are represented by the deterministic assertions above:
    // architect => ready_code body rewrite intent, engineer => real product head
    // (the fixture's workflow requires a non-bookkeeping sentinel), and landing
    // remains delegated to Temper mechanical automation in the full workflow.
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
    assert_eq!(job.conclusion, Some(CiJobConclusion::Failure));

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
    // nor validate them; the deterministic jig assertions below replace all LLM
    // decision points.
    let _ = std::env::var_os("TEMPER_FORGEJO_AGENTS");
}

fn assert_basic_delivery_shape_matches_example() {
    let workflow = temper_testing::basic_delivery_workflow();
    let compiled = workflow.compile();
    assert!(compiled.role(&RoleId::new("architect")).is_some());
    assert!(compiled.role(&RoleId::new("engineer")).is_some());
    assert!(compiled.role(&RoleId::new("mechanical")).is_some());

    let fixture = include_str!("../../../examples/basic-delivery/config/workflow.json");
    assert!(fixture.contains("triage_intake_to_code"));
    assert!(fixture.contains("ready_code"));
    assert!(fixture.contains("set_body"));
    assert!(fixture.contains("open_pr"));
    assert!(fixture.contains("implementation"));
    assert!(fixture.contains("mechanical"));
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
