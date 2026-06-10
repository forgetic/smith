use std::sync::Arc;

use smith_worker::{ParseOutcome, StubExecutor, USAGE, run_worker};

fn main() {
    let outcome = smith_worker::config::parse(std::env::args().skip(1));
    match outcome {
        Ok(ParseOutcome::Help) => {
            println!("usage: {USAGE}");
        }
        Ok(ParseOutcome::Run(config)) => {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("smith-worker: building runtime failed: {error}");
                    std::process::exit(2);
                }
            };

            let stub = Arc::new(StubExecutor::success());
            if let Err(error) = runtime.block_on(run_worker(config, stub)) {
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
