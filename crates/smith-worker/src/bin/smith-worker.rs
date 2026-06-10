use std::sync::Arc;

use smith_worker::{
    CodingExecutor, CodingExecutorConfig, ExecutorSelection, ParseOutcome, StubExecutor, USAGE,
    role_identities_from_env, run_worker,
};

fn main() {
    let outcome = smith_worker::config::parse(std::env::args().skip(1));
    match outcome {
        Ok(ParseOutcome::Help) => {
            println!("usage: {USAGE}");
        }
        Ok(ParseOutcome::Run(config)) => match config.executor.clone() {
            ExecutorSelection::Stub => {
                let runtime = match runtime() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("smith-worker: building runtime failed: {error}");
                        std::process::exit(2);
                    }
                };
                let executor = Arc::new(StubExecutor::success());
                if let Err(error) = runtime.block_on(run_worker(config, executor)) {
                    eprintln!("smith-worker: {error}");
                    std::process::exit(2);
                }
            }
            ExecutorSelection::Coding(surface) => {
                let roles = config
                    .capabilities
                    .iter()
                    .map(|capability| capability.role.clone());
                let role_identities = match role_identities_from_env(roles, std::env::vars()) {
                    Ok(role_identities) => role_identities,
                    Err(error) => {
                        eprintln!("smith-worker: {error}");
                        std::process::exit(2);
                    }
                };
                let executor = Arc::new(CodingExecutor::new(CodingExecutorConfig {
                    workspace_root: surface.workspace_root,
                    git_base_url: surface.git_base_url,
                    agent_command: surface.agent_command,
                    role_identities,
                }));
                let runtime = match runtime() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("smith-worker: building runtime failed: {error}");
                        std::process::exit(2);
                    }
                };
                if let Err(error) = runtime.block_on(run_worker(config, executor)) {
                    eprintln!("smith-worker: {error}");
                    std::process::exit(2);
                }
            }
        },
        Err(error) => {
            eprintln!("smith-worker: {error}\nusage: {USAGE}");
            std::process::exit(2);
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
