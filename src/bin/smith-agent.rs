//! The spawned agent's glue: configuration path, inherited channel and exit.
//! It keeps no service state or credentials; `smith_agent_shell::run` owns
//! startup and progression. Contract: protocol/agent.md, section 6.

use skein_io::kernel::Exit;
use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let Some(configuration) = arguments.next() else {
        eprintln!("smith-agent: usage: smith-agent CONFIG.json");
        return ExitCode::FAILURE;
    };
    if arguments.next().is_some() {
        eprintln!("smith-agent: usage: smith-agent CONFIG.json");
        return ExitCode::FAILURE;
    }
    match smith_agent_shell::run(Path::new(&configuration), Box::new(std::io::stderr())) {
        Ok(Exit::Code(code)) => ExitCode::from(code),
        Ok(Exit::Signal(_)) | Err(_) => ExitCode::FAILURE,
    }
}
