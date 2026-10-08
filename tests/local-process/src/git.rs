//! Simulator-free child output for the fake checkout's git decisions. The
//! world answers command effects through skein-fake-checkout, then binds this
//! Host to the child's pipes. It knows neither Sim nor Machine.

use skein_io::kernel;
use skein_lib::{Queue, Time, Token, Wall};
use skein_world::Host;

/// A git child sending its bounded stdout before closing both output pipes.
pub struct Child {
    output: Option<kernel::Fd>,
    error: Option<kernel::Fd>,
    bytes: Box<[u8]>,
    written: u32,
    pending: bool,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    code: u8,
}

impl Child {
    #[must_use]
    pub fn new(pipes: &[(u32, kernel::Fd)], bytes: Box<[u8]>, code: u8) -> Self {
        assert!(bytes.len() <= 32_768);
        assert_eq!(pipes.len(), 2);
        Self {
            output: Some(pipes.iter().find(|(child, _)| *child == 1).expect("stdout").1),
            error: Some(pipes.iter().find(|(child, _)| *child == 2).expect("stderr").1),
            bytes,
            written: 0,
            pending: false,
            completions: Queue::with_capacity(32),
            submissions: Queue::with_capacity(32),
            code,
        }
    }
    #[must_use]
    pub fn code(&self) -> u8 {
        self.code
    }
}

impl Host for Child {
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn iterate(&mut self, _: Time, _: Wall) {
        while let Some(complete) = self.completions.pop() {
            assert!(self.pending);
            self.pending = false;
            match complete.result.expect("fake git pipe outcome") {
                kernel::Done::Count(count) => self.written = self.written.checked_add(count).expect("bounded stdout"),
                kernel::Done::Nothing => {
                    if complete.op == Token::new(2) {
                        self.output = None;
                    } else {
                        self.error = None;
                    }
                }
                done => panic!("unexpected git pipe completion {done:?}"),
            }
        }
        if self.pending {
            return;
        }
        let next = if let Some(output) = self.output {
            if usize::try_from(self.written).expect("small count") < self.bytes.len() {
                kernel::Submit {
                    op: Token::new(1),
                    kind: kernel::Op::PipeWrite { fd: output, bytes: self.bytes.clone(), from: self.written },
                }
            } else {
                kernel::Submit { op: Token::new(2), kind: kernel::Op::Close { fd: output } }
            }
        } else if let Some(error) = self.error {
            kernel::Submit { op: Token::new(3), kind: kernel::Op::Close { fd: error } }
        } else {
            return;
        };
        self.submissions.push(next);
        self.pending = true;
    }
    fn next_deadline(&self) -> Option<Time> {
        None
    }
    fn work_pending(&self, _: Time) -> bool {
        !self.pending && !self.is_empty()
    }
    fn is_empty(&self) -> bool {
        self.output.is_none() && self.error.is_none() && !self.pending
    }
    fn operations(&self) -> u32 {
        u32::from(self.pending)
    }
    fn worst_case(&self) -> u64 {
        65_536
    }
}
