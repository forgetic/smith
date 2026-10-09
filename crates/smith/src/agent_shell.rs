//! Shared agent startup and shell pass (protocol/agent.md, sections 4–6).
//! It keeps the service, trace writer, inherited mount descriptors and error
//! output. It never keeps credentials outside the service or knows the driving
//! kernel, simulator or neighbours. `Agent::read` reads startup settings;
//! `Agent::new` adopts caller resources; `Host::iterate` runs the same pass in
//! the binary and worlds. `run` supplies the binary's kernel and clock.

use std::io::Write;
use std::path::Path;

use skein_io::kernel::{Complete, Fd, Submit};
use skein_lib::{Queue, Time, Wall};
use skein_shell::{Clock, Config as KernelConfig, Kernel, Now, Wait};
use skein_world::Host;
use smith_agent_service as service;

use crate::{config, trace};

/// Channel and signal descriptors handed by the caller to one agent invocation.
pub struct Resources {
    pub input: Fd,
    pub output: Fd,
    pub signals: Fd,
    pub seed: u64,
    /// When present, roots are adopted in Start order instead of opened on
    /// the machine. The caller retains responsibility for any unused roots.
    pub roots: Option<Box<[Fd]>>,
}

/// The binary's agent and shell effects, hosted by a kernel or world until settlement.
pub struct Agent {
    service: service::Service,
    trace: Option<trace::Trace>,
    roots: Option<std::vec::IntoIter<Fd>>,
    errors: Box<dyn Write>,
    result: Option<Result<(), String>>,
    worst: u64,
    operations: u32,
}

struct Prepared {
    service: service::Service,
    trace: Option<trace::Trace>,
    worst: u64,
    operations: u32,
}

impl Agent {
    /// Read the binary's configuration and build the same invocation worlds host.
    /// Every startup refusal is written once to the supplied error output.
    pub fn read(path: &Path, resources: Resources, mut errors: Box<dyn Write>) -> Result<Self, String> {
        let configuration = read_configuration(path, errors.as_mut())?;
        Self::new(configuration, resources, errors)
    }

    /// Build a configured service, adopt its streams and announce startup.
    /// Worlds inject their clock through `iterate` and deterministic resource seeds.
    pub fn new(
        configuration: config::Configuration,
        resources: Resources,
        mut errors: Box<dyn Write>,
    ) -> Result<Self, String> {
        let memory = configuration.memory;
        let prepared = match prepare(configuration, &resources) {
            Ok(prepared) => prepared,
            Err(why) => {
                diagnostic(errors.as_mut(), &why);
                return Err(why);
            }
        };
        diagnostic(
            errors.as_mut(),
            &format!("agent started; worst case {} of {memory} configured bytes", prepared.worst),
        );
        Ok(Self {
            service: prepared.service,
            trace: prepared.trace,
            roots: resources.roots.map(|roots| roots.into_vec().into_iter()),
            errors,
            result: None,
            worst: prepared.worst,
            operations: prepared.operations,
        })
    }

    /// The settled invocation's success or failure, already logged to error output.
    pub fn result(&self) -> Option<Result<(), &str>> {
        self.result.as_ref().map(|result| result.as_ref().copied().map_err(String::as_str))
    }

    fn finish(&mut self, answered: bool) {
        let trace_dropped = self.trace.as_mut().map_or(0, trace::Trace::finish);
        let domain_dropped = self.service.lost_domain_observations();
        let channel_dropped = self.service.lost_channel_facts();
        let facts_dropped = self.service.lost_trace_facts();
        let prompts_dropped = self.service.lost_trace_prompts();
        let lost = domain_dropped
            .saturating_add(channel_dropped)
            .saturating_add(facts_dropped)
            .saturating_add(prompts_dropped)
            .saturating_add(trace_dropped);
        if lost > 0 {
            diagnostic(
                self.errors.as_mut(),
                &format!(
                    "{lost} observations were dropped (domain: {domain_dropped}, channel: {channel_dropped}, \
                     trace facts: {facts_dropped}, trace prompts: {prompts_dropped}, writer: {trace_dropped})"
                ),
            );
        }
        self.result = Some(if answered {
            Ok(())
        } else {
            let why = format!("the run could not answer: {:?}", service::failure(&self.service));
            diagnostic(self.errors.as_mut(), &why);
            Err(why)
        });
    }
}

fn prepare(configuration: config::Configuration, resources: &Resources) -> Result<Prepared, String> {
    let root_count = resources.roots.as_ref().map_or(0, |roots| roots.len());
    if root_count > usize::try_from(configuration.service.limits.domain.run.directories).expect("directory bound") {
        return Err("inherited agent roots exceed the directory limit".into());
    }
    let reserve = if configuration.trace.is_some() { trace::MEMORY_RESERVE } else { 0 };
    let worst = service::worst_case(&configuration.service.limits)
        .and_then(|bytes| bytes.checked_add(reserve))
        .and_then(|bytes| bytes.checked_add(u64::try_from(std::mem::size_of::<Agent>()).ok()?))
        .and_then(|bytes| bytes.checked_add(u64::try_from(root_count.checked_mul(std::mem::size_of::<Fd>())?).ok()?))
        .ok_or("agent memory calculation overflowed")?;
    if worst > configuration.memory {
        return Err("agent shell exceeds memory_bytes".into());
    }
    let operations = configuration.service.limits.routes;
    let trace = match configuration.trace {
        Some(config) => Some(trace::Trace::open(config)?),
        None => None,
    };
    let mut service = service::Service::new(configuration.service, resources.seed)
        .map_err(|error| format!("agent configuration cannot start: {error:?}"))?;
    service
        .adopt_streams(resources.input, resources.output, resources.signals)
        .map_err(|fd| format!("agent stream {} cannot be adopted", fd.raw()))?;
    Ok(Prepared { service, trace, worst, operations })
}

impl Host for Agent {
    fn iterate(&mut self, now: Time, wall: Wall) {
        if self.result.is_some() {
            return;
        }
        service::iterate(&mut self.service, now, wall);
        while let Some(fact) = self.service.pop_trace_fact() {
            if let Some(trace) = &mut self.trace {
                trace.fact(fact, now.as_nanos());
            }
        }
        while let Some(content) = self.service.pop_trace_content() {
            if let Some(trace) = &mut self.trace {
                trace.content(content, now.as_nanos());
            }
        }
        if let Some((owner, prompt)) = self.service.pop_trace_prompt()
            && let Some(trace) = &mut self.trace
        {
            trace.prompt(owner, &prompt, now.as_nanos());
        }
        while let Some(path) = self.service.root_to_open() {
            let root = match &mut self.roots {
                Some(roots) => roots.next().ok_or(()),
                None => match std::str::from_utf8(path) {
                    Ok(path) => skein_shell::open_root(Path::new(path)).map_err(|_| ()),
                    Err(_) => Err(()),
                },
            };
            self.service.root_opened(root);
        }
        if let Some(answered) = service::done(&self.service) {
            self.finish(answered);
        }
    }

    fn completions(&mut self) -> &mut Queue<Complete> {
        self.service.completions()
    }

    fn submissions(&mut self) -> &mut Queue<Submit> {
        self.service.submissions()
    }

    fn work_pending(&self, now: Time) -> bool {
        self.result.is_none() && service::work_pending(&self.service, now)
    }

    fn next_deadline(&self) -> Option<Time> {
        service::next_deadline(&self.service)
    }

    fn is_empty(&self) -> bool {
        service::done(&self.service).is_some()
    }

    fn worst_case(&self) -> u64 {
        self.worst
    }

    fn operations(&self) -> u32 {
        self.operations
    }
}

fn read_configuration(path: &Path, errors: &mut dyn Write) -> Result<config::Configuration, String> {
    config::read(path).inspect_err(|why| diagnostic(errors, why))
}

fn diagnostic(errors: &mut dyn Write, line: &str) {
    // Diagnostics are best effort, as trace records are; a broken operator
    // output cannot prevent the channel's answer or its lower settlement.
    drop(writeln!(errors, "smith: {line}"));
}

/// Run the shipped agent command; startup and final failures are already logged.
pub fn run(path: &Path, mut errors: Box<dyn Write>) -> Result<(), String> {
    let configuration = read_configuration(path, errors.as_mut())?;
    let seed = skein_shell::seed()
        .map_err(|errno| format!("kernel seed failed (errno {errno})"))
        .inspect_err(|why| diagnostic(errors.as_mut(), why))?;
    let signals = skein_shell::open_termination_signals()
        .map_err(|errno| format!("termination signals cannot open (errno {errno})"))
        .inspect_err(|why| diagnostic(errors.as_mut(), why))?;
    let mut agent = Agent::new(
        configuration,
        Resources { input: Fd::new(0), output: Fd::new(1), signals, seed, roots: None },
        errors,
    )?;
    let mut kernel = Kernel::open(KernelConfig { operations: agent.operations() })
        .map_err(|error| format!("agent kernel cannot open: {error}"))
        .inspect_err(|why| diagnostic(agent.errors.as_mut(), why))?;
    let clock = Clock::new();
    loop {
        kernel.reap(agent.completions());
        let Now { now, wall } = clock.now();
        agent.iterate(now, wall);
        if let Some(result) = agent.result() {
            return result.map_err(str::to_owned);
        }
        let wait = if agent.work_pending(now) {
            Wait::No
        } else {
            match agent.next_deadline() {
                Some(deadline) => Wait::Until(deadline),
                None => Wait::Forever,
            }
        };
        kernel.submit(agent.submissions(), wait);
    }
}
