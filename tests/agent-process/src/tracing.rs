//! A Smith-specific observer of the service's native local trace output.
//! Skein owns scheduling, kernel operations, settlement and replay. This
//! adapter adopts Smith's inherited streams and records only emitted facts;
//! it never reads the engine's decisions or manufactures an observation.

use skein_io::kernel::{self, Complete, Fd, Op, Submit};
use skein_lib::{Queue, Time, Token, Wall};
use skein_shell::Host;
use smith_agent_service::{self as agent, Service};

pub(super) struct Tracing {
    service: Service,
    roots: std::vec::IntoIter<Fd>,
    unused: Vec<Fd>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    closing: u32,
    result: Option<bool>,
    pub(super) facts: Vec<smith_domain::Fact>,
    pub(super) channel_loss: u64,
    pub(super) trace_loss: u64,
}

impl Tracing {
    pub(super) fn new(configuration: agent::Config, inherited: &skein_world::Inherited) -> Self {
        let queue = configuration.limits.queue;
        let mut service = Service::new(configuration, 7).expect("bounded observed service");
        let pipe = |child| inherited.pipes.iter().find(|(number, _)| *number == child).expect("inherited pipe").1;
        service.adopt_streams(pipe(0), pipe(1), inherited.signal).expect("service streams");
        Self {
            service,
            roots: inherited.roots.iter().map(|(_, root)| *root).collect::<Vec<_>>().into_iter(),
            unused: vec![pipe(2)],
            completions: Queue::with_capacity(queue),
            submissions: Queue::with_capacity(queue),
            closing: 0,
            result: None,
            facts: Vec::new(),
            channel_loss: 0,
            trace_loss: 0,
        }
    }

    fn close(&mut self, fd: Fd) {
        self.closing += 1;
        self.submissions.push(Submit { op: Token::new(u64::MAX - u64::from(self.closing)), kind: Op::Close { fd } });
    }
}

impl Host for Tracing {
    fn iterate(&mut self, now: Time, wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            if complete.op.raw() >= u64::MAX - 3 {
                assert!(complete.result.is_ok(), "unused inherited close");
                self.closing -= 1;
            } else {
                self.service.completions().push(complete);
            }
        }
        if self.result.is_some() {
            return;
        }
        agent::iterate(&mut self.service, now, wall);
        while let Some(fact) = self.service.pop_trace_fact() {
            assert!(self.facts.len() < 256, "finite observed scenario");
            self.facts.push(fact);
        }
        while self.service.pop_trace_content().is_some() {}
        drop(self.service.pop_trace_prompt());
        while self.service.root_to_open().is_some() {
            self.service.root_opened(self.roots.next().ok_or(()));
        }
        self.channel_loss = self.service.lost_channel_facts();
        self.trace_loss = self.service.lost_trace_facts();
        if let Some(answered) = agent::done(&self.service) {
            self.result = Some(answered);
            let mut unused = std::mem::take(&mut self.unused);
            unused.extend(self.roots.by_ref());
            for fd in unused {
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
        (self.result.is_none() && agent::work_pending(&self.service, now))
            || !self.completions.is_empty()
            || !self.submissions.is_empty()
    }

    fn next_deadline(&self) -> Option<Time> {
        if self.result.is_none() { agent::next_deadline(&self.service) } else { None }
    }

    fn is_empty(&self) -> bool {
        self.result.is_some() && self.closing == 0 && self.completions.is_empty() && self.submissions.is_empty()
    }

    fn exit(&self) -> Option<kernel::Exit> {
        if self.is_empty() { self.result.map(|answered| kernel::Exit::Code(u8::from(!answered))) } else { None }
    }

    fn worst_case(&self) -> u64 {
        134_217_728
    }

    fn operations(&self) -> u32 {
        crate::limits().routes + 4
    }
}
