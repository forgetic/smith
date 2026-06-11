//! Jig-backed end-to-end test for the native sans-IO sub-agent loop.
//!
//! Drives a real [`smith_agent::run_sub_agent`] (pure `AgentMachine` + asupersync
//! `AgentShell` + pi-SDK provider + pi-SDK tools) against a local scripted
//! `jig_server::FakeLlm`, entirely in-process on the asupersync runtime. The
//! fake instructs the agent to call the `write` tool to create a file, then to
//! finish; the test asserts the file landed, the loop ran a tool round, and the
//! run completed cleanly. This is the native-loop analog of
//! `smith-temper-agent`'s `jig_coding_agent.rs` (which drives pi's own loop).
//!
//! Hermetic and fast — no live provider — so it runs by default.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jig_core::{Reply, Script, StopReason, Turn};
use jig_server::FakeLlm;
use pi::provider::StreamOptions;
use pi::sdk::{create_read_tool, create_write_tool};
use pi::tools::ToolRegistry;
use smith_agent::{AgentStop, SubAgent, run_sub_agent};
use smith_temper_agent::ProviderConfig;

#[test]
fn sub_agent_runs_a_tool_loop_and_completes() {
    let observed_continuation = Arc::new(AtomicUsize::new(0));
    let fake = sub_agent_fake(Arc::clone(&observed_continuation));

    let checkout = TempCheckout::new("sub-agent-tool-loop");

    let provider = ProviderConfig::new(
        "jig-openai-compatible",
        "jig-sub-agent-tool-loop",
        "https://example.invalid/unused-production-url",
        "sk-jig-test",
    )
    .with_base_url_override(fake.base_url())
    .build_provider()
    .expect("build jig provider");

    let tools = ToolRegistry::from_tools(vec![
        create_read_tool(checkout.path()),
        create_write_tool(checkout.path()),
    ]);

    let outcome = smith_io_engine::block_on(async move {
        run_sub_agent(SubAgent {
            system_prompt: Some(
                "You are a sub-agent. Use the write tool to create the requested file."
                    .to_string(),
            ),
            user_message: "Create NOTES.md whose first line is exactly `project notes`."
                .to_string(),
            tools,
            max_iterations: 6,
            provider,
            stream_options: StreamOptions {
                api_key: Some("sk-jig-test".to_string()),
                ..StreamOptions::default()
            },
        })
        .await
    })
    .expect("sub-agent runs");

    assert_eq!(outcome.stop, AgentStop::Completed, "run should complete cleanly");

    // The write tool actually created the file in the checkout.
    let notes = fs::read_to_string(checkout.path().join("NOTES.md")).expect("NOTES.md was written");
    assert_eq!(
        notes.lines().next(),
        Some("project notes"),
        "NOTES.md first line must match the requested content"
    );

    // The loop did a tool round (more than one model turn) and the fake saw the
    // tool-result continuation.
    assert!(
        fake.requests().len() > 1,
        "expected a tool loop, got a single model turn"
    );
    assert!(
        observed_continuation.load(Ordering::SeqCst) >= 1,
        "fake provider did not observe a tool-result continuation turn"
    );

    // The final conversation ends with the model's terminal text message.
    assert!(
        outcome
            .final_message
            .content
            .iter()
            .any(|block| matches!(block, pi::model::ContentBlock::Text(_))),
        "final message should carry the model's closing text"
    );
}

#[test]
fn sub_agent_reports_budget_exhaustion_when_model_loops_forever() {
    // The fake always asks for another tool call; the agent must stop at the
    // iteration budget rather than loop unboundedly.
    let fake = FakeLlm::start(Script::rule(|_view| Reply {
        turns: vec![Turn::ToolCall {
            id: "call_again".to_string(),
            name: "read".to_string(),
            args: serde_json::json!({ "path": "NOTES.md" }),
        }],
        usage: Default::default(),
        stop: StopReason::ToolCalls,
    }))
    .expect("start fake LLM");

    let checkout = TempCheckout::new("sub-agent-budget");
    fs::write(checkout.path().join("NOTES.md"), "seed\n").expect("seed file");

    let provider = ProviderConfig::new(
        "jig-openai-compatible",
        "jig-sub-agent-budget",
        "https://example.invalid/unused",
        "sk-jig-test",
    )
    .with_base_url_override(fake.base_url())
    .build_provider()
    .expect("build jig provider");

    let tools = ToolRegistry::from_tools(vec![create_read_tool(checkout.path())]);

    let outcome = smith_io_engine::block_on(async move {
        run_sub_agent(SubAgent {
            system_prompt: None,
            user_message: "read forever".to_string(),
            tools,
            max_iterations: 3,
            provider,
            stream_options: StreamOptions {
                api_key: Some("sk-jig-test".to_string()),
                ..StreamOptions::default()
            },
        })
        .await
    })
    .expect("sub-agent runs");

    assert_eq!(outcome.stop, AgentStop::BudgetExhausted);
}

fn sub_agent_fake(observed_continuation: Arc<AtomicUsize>) -> FakeLlm {
    FakeLlm::start(Script::rule(move |view| {
        if view.prior_tool_results == 0 {
            Reply {
                turns: vec![Turn::ToolCall {
                    id: "call_write_notes".to_string(),
                    name: "write".to_string(),
                    args: serde_json::json!({
                        "path": "NOTES.md",
                        "content": "project notes\n"
                    }),
                }],
                usage: Default::default(),
                stop: StopReason::ToolCalls,
            }
        } else {
            observed_continuation.fetch_add(1, Ordering::SeqCst);
            Reply::text("Created NOTES.md with project notes.")
        }
    }))
    .expect("start fake LLM")
}

/// A throwaway checkout directory removed on drop.
struct TempCheckout {
    path: PathBuf,
}

impl TempCheckout {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("smith-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create checkout dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempCheckout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
