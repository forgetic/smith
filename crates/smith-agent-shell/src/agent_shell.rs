//! Shared agent startup and shell pass (protocol/agent.md, sections 4–6).
//! It keeps the service, trace writer, inherited mount descriptors and error
//! output. It never keeps credentials outside the service or knows the driving
//! kernel, simulator or neighbours. `Agent::read` reads startup settings;
//! `Agent::new` adopts caller resources; `Host::iterate` runs the same pass in
//! the binary and worlds. `run` supplies the binary's kernel and clock.

use std::io::Write;
use std::path::Path;

use skein_io::kernel::{Complete, Exit, Fd, Op, Submit};
use skein_lib::{Queue, Time, Token, Wall};
use skein_shell::{Clock, Config as KernelConfig, Host, Kernel, drive};
use smith_agent_service as service;

use crate::{config, trace};

/// Channel and signal descriptors handed by the caller to one agent invocation.
pub struct Resources {
    pub input: Fd,
    pub output: Fd,
    pub signals: Fd,
    pub seed: u64,
    /// An inherited stderr descriptor whose close the invocation owns; the
    /// supplied writer owns the diagnostic bytes.
    pub error: Option<Fd>,
    /// When present, roots are adopted in Start order; unused roots close at
    /// the invocation's end instead of returning to the caller.
    pub roots: Option<Box<[Fd]>>,
}

/// The binary's agent and shell effects, hosted by a kernel or world until settlement.
pub struct Agent {
    service: service::Service,
    trace: Option<trace::Trace>,
    roots: Option<std::vec::IntoIter<Fd>>,
    errors: Box<dyn Write>,
    error: Option<Fd>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    closing: u32,
    result: Option<Result<(), String>>,
    memory: u64,
    started: bool,
    reported: bool,
    worst: u64,
    operations: u32,
}

struct Prepared {
    service: service::Service,
    trace: Option<trace::Trace>,
    worst: u64,
    operations: u32,
    queue: u32,
}

impl Agent {
    /// Read the binary's configuration and build the same invocation worlds host.
    /// Every startup refusal is written once to the supplied error output.
    pub fn read(path: &Path, resources: Resources, mut errors: Box<dyn Write>) -> Result<Self, String> {
        let configuration = read_configuration(path, errors.as_mut())?;
        Self::new(configuration, resources, errors)
    }

    /// Build a configured service and adopt its streams; `drain` announces startup.
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
        Ok(Self {
            service: prepared.service,
            trace: prepared.trace,
            roots: resources.roots.map(|roots| roots.into_vec().into_iter()),
            errors,
            error: resources.error,
            completions: Queue::with_capacity(prepared.queue),
            submissions: Queue::with_capacity(prepared.queue),
            closing: 0,
            result: None,
            memory,
            started: false,
            reported: false,
            worst: prepared.worst,
            operations: prepared.operations,
        })
    }

    /// The settled invocation's success or failure, already logged to error output.
    #[must_use]
    pub fn result(&self) -> Option<Result<(), &str>> {
        if self.is_empty() {
            self.result.as_ref().map(|result| result.as_ref().copied().map_err(String::as_str))
        } else {
            None
        }
    }

    fn close(&mut self, fd: Fd) {
        self.closing = self.closing.checked_add(1).expect("bounded inherited closes");
        self.submissions.push(Submit {
            op: Token::new(u64::MAX.checked_sub(u64::from(self.closing)).expect("bounded close token")),
            kind: Op::Close { fd },
        });
    }

    fn finish(&mut self, answered: bool) {
        self.result = Some(if answered {
            Ok(())
        } else {
            let why = format!("the run could not answer: {:?}", service::failure(&self.service));
            Err(why)
        });
    }
}

fn prepare(configuration: config::Configuration, resources: &Resources) -> Result<Prepared, String> {
    let root_count = resources.roots.as_ref().map_or(0, |roots| roots.len());
    if root_count > usize::try_from(configuration.service.limits.domain.run.directories).expect("directory bound") {
        return Err("inherited agent roots exceed the directory limit".into());
    }
    let extra_operations = u32::try_from(root_count)
        .map_err(|_| "inherited root count overflowed")?
        .checked_add(u32::from(resources.error.is_some()))
        .ok_or("inherited operations overflowed")?;
    let queue =
        configuration.service.limits.queue.checked_add(extra_operations).ok_or("agent queue calculation overflowed")?;
    let operations = configuration
        .service
        .limits
        .routes
        .checked_add(extra_operations)
        .ok_or("agent operations calculation overflowed")?;
    let reserve = if configuration.trace.is_some() { trace::MEMORY_RESERVE } else { 0 };
    let worst = service::worst_case(&configuration.service.limits, &configuration.service.llm_endpoints)
        .and_then(|bytes| bytes.checked_add(reserve))
        .and_then(|bytes| bytes.checked_add(Queue::<Complete>::worst_case(queue)?))
        .and_then(|bytes| bytes.checked_add(Queue::<Submit>::worst_case(queue)?))
        .and_then(|bytes| bytes.checked_add(u64::try_from(size_of::<Agent>()).ok()?))
        .and_then(|bytes| bytes.checked_add(u64::try_from(root_count.checked_mul(size_of::<Fd>())?).ok()?))
        .ok_or("agent memory calculation overflowed")?;
    if worst > configuration.memory {
        return Err("agent shell exceeds profile.declared.memory".into());
    }
    let trace = match configuration.trace {
        Some(config) => Some(trace::Trace::open(config)?),
        None => None,
    };
    let mut service = service::Service::new(configuration.service, resources.seed)
        .map_err(|error| format!("agent configuration cannot start: {error:?}"))?;
    service
        .adopt_streams(resources.input, resources.output, resources.signals)
        .map_err(|fd| format!("agent stream {} cannot be adopted", fd.raw()))?;
    Ok(Prepared { service, trace, worst, operations, queue })
}

impl Host for Agent {
    fn drain(&mut self) {
        if !self.started {
            self.started = true;
            diagnostic(
                self.errors.as_mut(),
                &format!("agent started; worst case {} of {} configured bytes", self.worst, self.memory),
            );
        }
        if self.result.is_some() && !self.reported {
            self.reported = true;
            let trace_dropped = self.trace.as_ref().map_or(0, trace::Trace::dropped);
            let lost = self
                .service
                .lost_channel_facts()
                .saturating_add(self.service.lost_trace_facts())
                .saturating_add(self.service.lost_trace_prompts())
                .saturating_add(trace_dropped);
            if lost > 0 {
                diagnostic(self.errors.as_mut(), &format!("{lost} observations were dropped"));
            }
            if let Some(Err(why)) = &self.result {
                diagnostic(self.errors.as_mut(), why);
            }
        }
    }

    fn iterate(&mut self, now: Time, wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            if complete.op.raw() >= u64::MAX.saturating_sub(u64::from(self.operations)) {
                assert!(complete.result.is_ok(), "inherited descriptor closes");
                self.closing = self.closing.checked_sub(1).expect("submitted inherited close");
            } else {
                self.service.completions().push(complete);
            }
        }
        if self.result.is_some() {
            return;
        }
        service::iterate(&mut self.service, now, wall);
        while let Some(fact) = self.service.pop_trace_fact() {
            if let Some(trace) = &mut self.trace {
                trace.fact(&fact);
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
            while let Some(fd) = self.roots.as_mut().and_then(Iterator::next) {
                self.close(fd);
            }
            if let Some(fd) = self.error.take() {
                self.close(fd);
            }
        }
        while let Some(submission) = self.service.submissions().pop() {
            self.submissions.push(submission);
        }
    }

    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }

    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }

    fn work_pending(&self, now: Time) -> bool {
        (self.result.is_none() && service::work_pending(&self.service, now))
            || !self.completions.is_empty()
            || !self.submissions.is_empty()
    }

    fn next_deadline(&self) -> Option<Time> {
        if self.result.is_none() { service::next_deadline(&self.service) } else { None }
    }

    fn is_empty(&self) -> bool {
        self.result.is_some() && self.closing == 0 && self.completions.is_empty() && self.submissions.is_empty()
    }

    fn exit(&self) -> Option<Exit> {
        self.result().map(|result| Exit::Code(u8::from(result.is_err())))
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
pub fn run(path: &Path, mut errors: Box<dyn Write>) -> Result<Exit, String> {
    let configuration = read_configuration(path, errors.as_mut())?;
    let seed = skein_shell::seed()
        .map_err(|errno| format!("kernel seed failed (errno {errno})"))
        .inspect_err(|why| diagnostic(errors.as_mut(), why))?;
    let signals = skein_shell::open_termination_signals()
        .map_err(|errno| format!("termination signals cannot open (errno {errno})"))
        .inspect_err(|why| diagnostic(errors.as_mut(), why))?;
    let mut agent = Agent::new(
        configuration,
        Resources { input: Fd::new(0), output: Fd::new(1), signals, seed, error: None, roots: None },
        errors,
    )?;
    let mut kernel = Kernel::open(KernelConfig { operations: agent.operations() })
        .map_err(|error| format!("agent kernel cannot open: {error}"))
        .inspect_err(|why| diagnostic(agent.errors.as_mut(), why))?;
    Ok(drive(&mut kernel, &Clock::new(), &mut agent))
}
