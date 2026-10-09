//! The single-run agent shell: JSON startup, inherited channel pipes, and
//! Skein's one-thread kernel loop. It owns no domain decisions or credentials.
//! Contract: protocol/agent.md, sections 4 and 6; skein shell.md, section 6.
//!
//! Command: `smith agent CONFIG.json`.

use smith::local_shell;
use smith_agent_shell as agent_shell;

use std::env;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: smith agent CONFIG.json | smith local SETTINGS.json STATE_DIR [WORKSPACE_SETTINGS.json]";

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let result = if args.len() == 3 && args[1] == "agent" {
        return match agent_shell::run(Path::new(&args[2]), Box::new(std::io::stderr())) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    } else if (args.len() == 4 || args.len() == 5) && args[1] == "local" {
        local_shell::run(Path::new(&args[2]), Path::new(&args[3]), args.get(4).map(Path::new))
    } else {
        eprintln!("smith: {USAGE}");
        return ExitCode::FAILURE;
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("smith: {why}");
            ExitCode::FAILURE
        }
    }
}
