use std::sync::Arc;

use smith_temper_agent::{AuthChoice, DEFAULT_MAX_ITERATIONS, ProviderConfig, resolve_config_dir};
use smith_worker::{
    AgentAuthChoice, AgentSurface, CodingExecutor, CodingExecutorConfig, ExecutorSelection,
    ExternalCommandRunner, ParseOutcome, PiAgentRunner, SmithAgentSurface, StubExecutor, USAGE,
    role_identities_from_env, run_worker,
};

fn main() {
    let outcome = smith_worker::config::parse(std::env::args().skip(1));
    match outcome {
        Ok(ParseOutcome::Help) => {
            println!("usage: {USAGE}");
        }
        Ok(ParseOutcome::Run(config)) => {
            if let Err(error) = run(config) {
                eprintln!("smith-worker: {error}");
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("smith-worker: {error}\nusage: {USAGE}");
            std::process::exit(2);
        }
    }
}

/// Builds the selected executor and runs the worker on the asupersync runtime.
///
/// The runtime is single-threaded (libuv-shaped) and hosts both the worker loop
/// and the in-process `pi`-SDK coding jobs it dispatches — the coding loop's
/// file/bash tools require an asupersync reactor, so there is exactly one
/// runtime for the whole process.
fn run(config: smith_worker::WorkerConfig) -> Result<(), String> {
    match config.executor.clone() {
        ExecutorSelection::Stub => {
            let executor = Arc::new(StubExecutor::success());
            smith_io_engine::block_on(async move {
                run_worker(config, executor)
                    .await
                    .map_err(|error| error.to_string())
            })
        }
        ExecutorSelection::Coding(surface) => {
            let role_identities = {
                let roles = config
                    .capabilities
                    .iter()
                    .map(|capability| capability.role.clone());
                role_identities_from_env(roles, std::env::vars())?
            };
            let executor_config = CodingExecutorConfig {
                workspace_root: surface.workspace_root,
                git_base_url: surface.git_base_url,
                role_identities,
            };

            match surface.agent {
                AgentSurface::Smith(smith) => {
                    // Resolve provider/credentials up front (eager preflight) so
                    // a missing login fails the worker now, not every job.
                    let provider = build_provider(&smith)?;
                    let max_iterations = smith.max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS);
                    let config_dir = resolve_config_dir(smith.config_dir.as_deref());
                    let runner =
                        Arc::new(PiAgentRunner::new(provider, max_iterations, config_dir));
                    let executor = Arc::new(CodingExecutor::new(executor_config, runner));
                    smith_io_engine::block_on(async move {
                        run_worker(config, executor)
                            .await
                            .map_err(|error| error.to_string())
                    })
                }
                AgentSurface::ExternalCommand(command) => {
                    let runner = Arc::new(ExternalCommandRunner::new(command));
                    let executor = Arc::new(CodingExecutor::new(executor_config, runner));
                    smith_io_engine::block_on(async move {
                        run_worker(config, executor)
                            .await
                            .map_err(|error| error.to_string())
                    })
                }
            }
        }
    }
}

/// Resolves the provider config for the in-process Smith agent, performing the
/// same eager credential preflight the former `smith-coding-agent` binary did.
fn build_provider(smith: &SmithAgentSurface) -> Result<ProviderConfig, String> {
    let auth = match smith.auth {
        AgentAuthChoice::DeepSeek => AuthChoice::DeepSeek,
        AgentAuthChoice::ChatGptOAuth => AuthChoice::ChatGptOAuth,
        AgentAuthChoice::AnthropicOAuth => AuthChoice::AnthropicOAuth,
    };
    ProviderConfig::from_auth(auth, smith.codex_model.clone(), smith.auth_file.clone())
        .map_err(|error| error.to_string())
}
