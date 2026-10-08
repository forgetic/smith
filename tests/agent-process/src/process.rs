//! Shared hosted agent adapter and its outside facts (testing.md, sections
//! 2.1 and 5). It knows only inherited resources and the service's public
//! boundary; no simulator or peer state. `make` adopts startup descriptors.
use crate as fixture;
use skein_io::kernel;
use skein_lib::{Queue, Time, Token, Wall};
use skein_world::{Host, Inherited};
use smith_agent_service as agent;

/// The agent service and retained outside facts, ending after descriptor settlement.
pub struct Agent {
    service: agent::Service,
    paused: bool,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    stderr_closed: bool,
    facts: Vec<smith_domain::Fact>,
    keep_facts: bool,
}

impl Host for Agent {
    fn iterate(&mut self, now: Time, wall: Wall) {
        if self.paused {
            return;
        }
        while let Some(complete) = self.completions.pop() {
            if complete.op == Token::new(u64::MAX - 1) {
                assert!(complete.result.is_ok(), "unused stderr closes");
                self.stderr_closed = true;
            } else {
                self.service.completions().push(complete);
            }
        }
        agent::iterate(&mut self.service, now, wall);
        while let Some(fact) = self.service.pop_trace_fact() {
            if self.keep_facts {
                assert!(self.facts.len() < 1024, "finite scenario facts");
                self.facts.push(fact);
            }
        }
        while let Some(submission) = self.service.submissions().pop() {
            self.submissions.push(submission);
        }
    }

    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }

    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }

    fn work_pending(&self, now: Time) -> bool {
        !self.paused
            && (agent::work_pending(&self.service, now) || !self.completions.is_empty() || !self.submissions.is_empty())
    }

    fn next_deadline(&self) -> Option<Time> {
        if self.paused { None } else { agent::next_deadline(&self.service) }
    }

    fn is_empty(&self) -> bool {
        agent::done(&self.service).is_some()
            && self.stderr_closed
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }

    fn exit(&self) -> Option<kernel::Exit> {
        if self.is_empty() {
            agent::done(&self.service).map(|success| kernel::Exit::Code(u8::from(!success)))
        } else {
            None
        }
    }

    fn worst_case(&self) -> u64 {
        agent::worst_case(&fixture::limits())
            .and_then(|bound| bound.checked_add(16_777_216))
            .and_then(|bound| {
                bound.checked_add(u64::try_from(size_of::<smith_domain::Fact>()).ok()?.checked_mul(1024)?)
            })
            .and_then(|bound| bound.checked_add(u64::try_from(size_of::<Self>()).ok()?))
            .expect("complete agent bound")
    }

    fn operations(&self) -> u32 {
        fixture::limits().routes.checked_add(1).expect("agent operations")
    }
}

impl Agent {
    /// Referee-controlled scheduling pause, consumed without allocating.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Discard outside facts without changing service decisions.
    pub fn discard_facts(&mut self) {
        self.keep_facts = false;
    }

    /// Facts emitted by the service, retained separately from its state.
    #[must_use]
    pub fn facts(&self) -> &[smith_domain::Fact] {
        &self.facts
    }
}

/// Adopt the three child pipes and signal source in a hosted factory.
#[must_use]
pub fn make(_spawn: &kernel::Spawn, inherited: &Inherited) -> Agent {
    let mut service = fixture::service(7);
    let input = inherited.pipes.iter().find(|(child, _)| *child == 0).expect("stdin").1;
    let output = inherited.pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1;
    service.adopt_streams(input, output, inherited.signal).expect("child resources");
    let error = inherited.pipes.iter().find(|(child, _)| *child == 2).expect("stderr").1;
    let mut submissions = Queue::with_capacity(256);
    submissions.push(kernel::Submit { op: Token::new(u64::MAX - 1), kind: kernel::Op::Close { fd: error } });
    Agent {
        service,
        paused: false,
        completions: Queue::with_capacity(256),
        submissions,
        stderr_closed: false,
        facts: Vec::with_capacity(1024),
        keep_facts: true,
    }
}
