//! Bounded line and signal translation for one person at a terminal
//! (protocol/hosts.md, section 5.1). Only completed lines reach the local
//! domain; a second interrupt requests process stop from the service.

use alloc::boxed::Box;
use skein_lib::{List, Queue};
use smith_local_domain as local;

/// Receiving and display bounds of one local terminal.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum bytes in one continued message.
    pub line_bytes: u32,
    /// Maximum bytes in one shown notice.
    pub show_bytes: u32,
}

/// One output to the local service.
#[derive(Debug)]
pub enum Event {
    /// A completed person line for the local domain.
    Domain(local::Event),
    /// A second interrupt before an answer stops the child process.
    StopProcess,
    /// A bounded notice to write on the terminal.
    Write(Box<[u8]>),
}

/// Maximum output cells emitted by one entry point.
#[must_use]
pub const fn max_out() -> u32 {
    1
}

/// Checked retained upper bound for the terminal.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    if limits.line_bytes == 0 || limits.show_bytes == 0 {
        return None;
    }
    List::<u8>::worst_case(limits.line_bytes)?.checked_add(u64::from(limits.show_bytes))
}

/// One line accumulator and the live run's interrupt count.
#[derive(Debug)]
pub struct Terminal {
    limits: Limits,
    line: List<u8>,
    discarding: bool,
    cancelled: bool,
}

impl Terminal {
    /// Build the bounded terminal translator.
    #[must_use]
    pub fn new(limits: Limits) -> Option<Self> {
        worst_case(&limits)?;
        Some(Self { line: List::with_capacity(limits.line_bytes), limits, discarding: false, cancelled: false })
    }

    /// Feed one byte from standard input; only a complete line emits an event.
    pub fn feed(&mut self, byte: u8, out: &mut Queue<Event>) {
        if byte == b'\n' {
            self.finish_line(out);
            return;
        }
        if self.discarding {
            return;
        }
        if self.line.push(byte).is_err() {
            self.line = List::with_capacity(self.limits.line_bytes);
            self.discarding = true;
        }
    }

    /// Translate a terminal interrupt; the second one asks the service to stop the child.
    pub fn interrupt(&mut self, out: &mut Queue<Event>) {
        if self.cancelled {
            out.push(Event::StopProcess);
        } else {
            self.cancelled = true;
            out.push(Event::Domain(local::Event::Interrupt));
        }
    }

    /// A new run can be interrupted independently of the previous answer.
    pub fn answer_finished(&mut self) {
        self.cancelled = false;
    }

    /// Translate terminal closure after any complete line.
    pub fn closed(&mut self, out: &mut Queue<Event>) {
        out.push(Event::Domain(local::Event::Closed));
    }

    /// Forward bounded local-domain text to the terminal.
    pub fn show(&self, text: Box<[u8]>, out: &mut Queue<Event>) {
        let cap = usize::try_from(self.limits.show_bytes).expect("u32 fits usize");
        let kept = text.get(..cap).unwrap_or(text.as_ref());
        out.push(Event::Write(Box::from(kept)));
    }

    fn finish_line(&mut self, out: &mut Queue<Event>) {
        if self.discarding {
            self.discarding = false;
            out.push(Event::Write(Box::from(&b"Line is too long\n"[..])));
            return;
        }
        if self.line.as_slice().last() == Some(&b'\r') {
            self.pop_last();
        }
        if self.line.as_slice().last() == Some(&b'\\') {
            self.pop_last();
            return;
        }
        let completed = core::mem::replace(&mut self.line, List::with_capacity(self.limits.line_bytes));
        out.push(Event::Domain(local::Event::Line { text: completed.into_boxed() }));
    }

    fn pop_last(&mut self) {
        let count = self.line.as_slice().len().saturating_sub(1);
        let mut kept = List::with_capacity(self.limits.line_bytes);
        for byte in self.line.as_slice().get(..count).unwrap_or_default() {
            kept.push(*byte).expect("prefix fits line capacity");
        }
        self.line = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal() -> Terminal {
        Terminal::new(Limits { line_bytes: 32, show_bytes: 32 }).expect("valid terminal")
    }

    #[test]
    fn a_line_and_its_continuation_are_one_message() {
        let mut terminal = terminal();
        let mut out = Queue::with_capacity(max_out());
        for byte in b"hello\\\nworld\n" {
            terminal.feed(*byte, &mut out);
        }
        match out.pop().expect("one line") {
            Event::Domain(local::Event::Line { text }) => assert_eq!(text.as_ref(), b"helloworld"),
            Event::Domain(_) | Event::StopProcess | Event::Write(_) => panic!("unexpected terminal event"),
        }
        assert!(out.is_empty());
    }

    #[test]
    fn a_second_interrupt_stops_the_process() {
        let mut terminal = terminal();
        let mut out = Queue::with_capacity(max_out());
        terminal.interrupt(&mut out);
        match out.pop().expect("first interrupt") {
            Event::Domain(local::Event::Interrupt) => {}
            Event::Domain(_) | Event::StopProcess | Event::Write(_) => panic!("expected cancel"),
        }
        terminal.interrupt(&mut out);
        match out.pop().expect("second interrupt") {
            Event::StopProcess => {}
            Event::Domain(_) | Event::Write(_) => panic!("expected process stop"),
        }
        terminal.answer_finished();
        terminal.interrupt(&mut out);
        match out.pop().expect("new run interrupt") {
            Event::Domain(local::Event::Interrupt) => {}
            Event::Domain(_) | Event::StopProcess | Event::Write(_) => panic!("expected cancel"),
        }
    }

    #[test]
    fn an_overlong_line_is_discarded_until_newline() {
        let mut terminal = Terminal::new(Limits { line_bytes: 3, show_bytes: 32 }).expect("valid terminal");
        let mut out = Queue::with_capacity(max_out());
        for byte in b"abcd\n" {
            terminal.feed(*byte, &mut out);
        }
        match out.pop().expect("too long notice") {
            Event::Write(_) => {}
            Event::Domain(_) | Event::StopProcess => panic!("expected notice"),
        }
        for byte in b"ok\n" {
            terminal.feed(*byte, &mut out);
        }
        match out.pop().expect("following line") {
            Event::Domain(local::Event::Line { text }) => assert_eq!(text.as_ref(), b"ok"),
            Event::Domain(_) | Event::StopProcess | Event::Write(_) => panic!("expected line"),
        }
    }
}
