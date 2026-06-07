//! Online request-body oracle for Smith's real outbound provider requests.
//!
//! This integration test is `#[ignore]`d and every leg is separately
//! environment-gated, so the default developer/CI test path compiles it but does
//! not make network calls or require credentials. When opted in, it captures the
//! actual request emitted by `smith_temper_agent::run_decision` through
//! `pi_agent_rust` with `jig-record`, including Smith's Anthropic OAuth Claude
//! Code identity workaround, then compares the captured request body with jig's
//! authoritative `fixtures/<dialect>/single-text/request.template.json` using
//! `jig_core::conform::grammar::grammar_findings`.
//!
//! ```sh
//! # DeepSeek/OpenAI-compatible API-key leg
//! TEMPER_DEEPSEEK_REQUEST_ORACLE=1 \
//! TEMPER_DEEPSEEK_API_KEY=... \
//!   cargo test -p smith-temper-agent --test jig_request_oracle --features test-provider-base-url-override -- --ignored --nocapture
//!
//! # Anthropic OAuth leg (requires `pi /login anthropic` first)
//! TEMPER_ANTHROPIC_OAUTH=1 \
//!   cargo test -p smith-temper-agent --test jig_request_oracle --features test-provider-base-url-override -- --ignored --nocapture
//!
//! # ChatGPT/Codex OAuth leg (requires `pi /login openai-codex` first)
//! TEMPER_CHATGPT_OAUTH=1 \
//!   cargo test -p smith-temper-agent --test jig_request_oracle --features test-provider-base-url-override -- --ignored --nocapture
//! ```

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use jig_core::conform::grammar::{GrammarFinding, grammar_findings};
use jig_record::{bind, proxy_once};
use serde::Deserialize;
use serde_json::Value;
use smith_temper_agent::{AuthChoice, ProviderConfig, run_decision};
use temper_workflow::{RawWorkflowSpec, RoleId, RoleManifest};

#[derive(Debug, Deserialize)]
struct RoleDecision {
    action: String,
    #[allow(dead_code)]
    #[serde(default)]
    reason: String,
}

/// Reviewed request-grammar divergences.
///
/// Keep this empty by default. Add entries only after reviewing an intentional
/// provider/client difference; unreviewed findings should fail the oracle.
const REVIEWED_FINDINGS: &[(&str, &str)] = &[];

#[test]
#[ignore = "online request oracle; opt in with provider env gates"]
fn smith_requests_match_jig_authoritative_templates() {
    run_deepseek_openai_leg();
    run_anthropic_oauth_leg();
    run_chatgpt_codex_oauth_leg();
}

fn run_deepseek_openai_leg() {
    if std::env::var("TEMPER_DEEPSEEK_REQUEST_ORACLE")
        .ok()
        .as_deref()
        != Some("1")
    {
        eprintln!(
            "skipping DeepSeek request oracle live validation: set \
             TEMPER_DEEPSEEK_REQUEST_ORACLE=1 (requires real DeepSeek/OpenAI-compatible \
             credentials and makes real provider calls)"
        );
        return;
    }

    let provider = match ProviderConfig::from_auth(AuthChoice::DeepSeek, None, None) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!(
                "skipping DeepSeek/OpenAI request oracle: no DeepSeek key available ({error}); \
                 set TEMPER_DEEPSEEK_API_KEY or TEMPER_DEEPSEEK_API_KEY_PATH"
            );
            return;
        }
    };
    run_leg(
        "deepseek/openai",
        "openai",
        Some("api.deepseek.com"),
        provider,
    );
}

fn run_anthropic_oauth_leg() {
    if std::env::var("TEMPER_ANTHROPIC_OAUTH").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping Anthropic OAuth request oracle: set TEMPER_ANTHROPIC_OAUTH=1 and run `pi /login anthropic` first"
        );
        return;
    }
    let provider = match ProviderConfig::from_auth(AuthChoice::AnthropicOAuth, None, None) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!(
                "skipping Anthropic OAuth request oracle: provider preflight failed ({error}); run `pi /login anthropic` first"
            );
            return;
        }
    };
    run_leg("anthropic/oauth", "anthropic", None, provider);
}

fn run_chatgpt_codex_oauth_leg() {
    if std::env::var("TEMPER_CHATGPT_OAUTH").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping ChatGPT/Codex OAuth request oracle: set TEMPER_CHATGPT_OAUTH=1 and run `pi /login openai-codex` first"
        );
        return;
    }
    let provider = match ProviderConfig::from_auth(AuthChoice::ChatGptOAuth, None, None) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!(
                "skipping ChatGPT/Codex OAuth request oracle: provider preflight failed ({error}); run `pi /login openai-codex` first"
            );
            return;
        }
    };
    run_leg("chatgpt/codex", "codex", None, provider);
}

fn run_leg(
    label: &str,
    dialect: &str,
    upstream_host_override: Option<&'static str>,
    provider: ProviderConfig,
) {
    let Some(template_path) = authoritative_template_path(dialect) else {
        eprintln!(
            "skipping {label} request oracle: sibling ../jig checkout or template fixture is absent"
        );
        return;
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let recorder = start_recorder(&runtime, upstream_host_override).expect("start jig recorder");
    let provider = provider.with_base_url_override(recorder.base_url.clone());

    let role = fixture_role_manifest();
    let context = fixture_role_context(&role);
    let decision: RoleDecision = runtime
        .block_on(run_decision(&provider, &role.prompt.render(), &context))
        .expect("Smith decision succeeds through recorder and real upstream");
    assert_eq!(decision.action, "advance");

    let (request, _response, _route) = runtime
        .block_on(recorder.capture)
        .expect("recorder task joins")
        .expect("recorder captures one provider request");
    let subject_body: Value =
        serde_json::from_slice(&request.body).expect("captured request body is JSON");
    let authoritative_body = load_template_body(&template_path);
    assert_no_unreviewed_findings(label, grammar_findings(&subject_body, &authoritative_body));
}

struct Recorder {
    base_url: String,
    capture: tokio::task::JoinHandle<
        io::Result<(
            jig_record::ClientRequest,
            jig_record::UpstreamResponse,
            jig_record::Route,
        )>,
    >,
}

fn start_recorder(
    runtime: &tokio::runtime::Runtime,
    upstream_host_override: Option<&'static str>,
) -> io::Result<Recorder> {
    let (tx, rx) = mpsc::channel();
    let capture = runtime.spawn(async move {
        let listener = bind().await?;
        tx.send(listener.local_addr()?)
            .expect("send recorder address");
        proxy_once(&listener, upstream_host_override).await
    });
    let addr = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("recorder reports loopback address");
    Ok(Recorder {
        base_url: format!("http://{addr}"),
        capture,
    })
}

fn authoritative_template_path(dialect: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../jig/fixtures")
        .join(dialect)
        .join("single-text/request.template.json");
    path.exists().then_some(path)
}

fn load_template_body(path: &Path) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(path).expect("read authoritative request template"),
    )
    .expect("authoritative request template is JSON")
}

fn assert_no_unreviewed_findings(label: &str, findings: Vec<GrammarFinding>) {
    let unexpected: Vec<_> = findings
        .into_iter()
        .filter(|finding| {
            !REVIEWED_FINDINGS
                .iter()
                .any(|(path, detail)| finding.path == *path && finding.detail == *detail)
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "{label} request grammar diverged from jig authoritative template:\n  {}",
        unexpected
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn fixture_role_manifest() -> RoleManifest {
    let json = r#"{
        "name": "jig-request-oracle-role-smoke",
        "roles": [{
            "id": "banana",
            "prompt": {
                "guidance": "When the work item is a task in the todo queue with the todo label, choose the advance action. Reply with one JSON object."
            },
            "queues": ["todo"]
        }],
        "labels": [{"id": "task"}, {"id": "todo"}, {"id": "done"}],
        "artifact_kinds": [{"id": "task", "target": "issue", "identifying_labels": ["task"]}],
        "queues": [{"id": "todo", "artifact": "task", "labels": ["todo"]}],
        "transitions": [{
            "id": "advance",
            "artifact": "task",
            "roles": ["banana"],
            "effects": [{"kind": "remove_label", "label": "todo"}, {"kind": "add_label", "label": "done"}]
        }]
    }"#;
    let spec: RawWorkflowSpec = serde_json::from_str(json).expect("workflow json parses");
    spec.validate()
        .expect("workflow validates")
        .compile()
        .role(&RoleId::new("banana"))
        .expect("banana role manifest exists")
        .clone()
}

fn fixture_role_context(role: &RoleManifest) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "work_item": {"repository": "forgejo:acme/service", "queue": "todo", "role": role.id.as_str(), "kind": "task", "artifact": {"type": "issue", "number": 1, "title": "Advance a generic task", "body": "This synthetic task is ready for the generic action.", "labels": ["task", "todo"], "state": "Open"}},
        "allowed_actions": ["no_action", "advance"],
        "authorized_actions": [{"action": "advance", "transition": "advance", "artifact": "task", "requires_gates": []}]
    })).expect("context serializes")
}
