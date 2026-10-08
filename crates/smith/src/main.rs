//! The single-run agent shell: JSON startup, inherited channel pipes, and
//! Skein's one-thread kernel loop. It owns no domain decisions or credentials.
//! Contract: protocol/agent.md, sections 4 and 6; skein shell.md, section 6.
//!
//! Command: `smith agent CONFIG.json`.

mod config;
mod limits;
mod local_settings;
mod local_shell;
pub mod local_store;
mod trace;

use std::env;
use std::path::Path;
use std::process::ExitCode;

use skein_io::kernel::Fd;
use skein_shell::{Clock, Config as KernelConfig, Kernel, Now, Wait};
use smith_agent_service as service;

const USAGE: &str = "usage: smith agent CONFIG.json | smith local SETTINGS.json STATE_DIR [WORKSPACE_SETTINGS.json]";

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    let result = if args.len() == 3 && args[1] == "agent" {
        run(Path::new(&args[2]))
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

fn run(path: &Path) -> Result<(), String> {
    let configuration = config::read(path)?;
    let worst = service::worst_case(&configuration.service.limits).ok_or("agent memory calculation overflowed")?;
    let operations = configuration.service.limits.routes;
    let mut trace = match configuration.trace {
        Some(config) => Some(trace::Trace::open(config)?),
        None => None,
    };
    let seed = skein_shell::seed().map_err(|errno| format!("kernel seed failed (errno {errno})"))?;
    let mut service = service::Service::new(configuration.service, seed)
        .map_err(|error| format!("agent configuration cannot start: {error:?}"))?;
    let signal_fd = skein_shell::open_termination_signals()
        .map_err(|errno| format!("termination signals cannot open (errno {errno})"))?;
    let mut kernel =
        Kernel::open(KernelConfig { operations }).map_err(|error| format!("agent kernel cannot open: {error}"))?;
    service
        .adopt_streams(Fd::new(0), Fd::new(1), signal_fd)
        .map_err(|fd| format!("agent stream {} cannot be adopted", fd.raw()))?;
    let clock = Clock::new();
    eprintln!("smith: agent started; worst case {worst} of {} configured bytes", configuration.memory);
    loop {
        kernel.reap(service.completions());
        let Now { now, wall } = clock.now();
        service::iterate(&mut service, now, wall);
        while let Some(fact) = service.pop_trace_fact() {
            if let Some(trace) = &mut trace {
                trace.fact(fact, now.as_nanos());
            }
        }
        while let Some(content) = service.pop_trace_content() {
            if let Some(trace) = &mut trace {
                trace.content(content, now.as_nanos());
            }
        }
        if let Some((owner, prompt)) = service.pop_trace_prompt()
            && let Some(trace) = &mut trace
        {
            trace.prompt(owner, &prompt, now.as_nanos());
        }
        while let Some(path) = service.root_to_open() {
            let root = match std::str::from_utf8(path) {
                Ok(path) => skein_shell::open_root(Path::new(path)).map_err(|_| ()),
                Err(_) => Err(()),
            };
            service.root_opened(root);
        }
        if let Some(answered) = service::done(&service) {
            let trace_dropped = match &trace {
                Some(trace) => trace.dropped(),
                None => 0,
            };
            let lost = service
                .lost_channel_facts()
                .saturating_add(service.lost_trace_facts())
                .saturating_add(service.lost_trace_prompts())
                .saturating_add(trace_dropped);
            if lost > 0 {
                eprintln!("smith: {lost} observations were dropped");
            }
            return if answered {
                Ok(())
            } else {
                Err(format!("the run could not answer: {:?}", service::failure(&service)))
            };
        }
        let wait = if service::work_pending(&service, now) {
            Wait::No
        } else {
            match service::next_deadline(&service) {
                Some(deadline) => Wait::Until(deadline),
                None => Wait::Forever,
            }
        };
        kernel.submit(service.submissions(), wait);
    }
}
