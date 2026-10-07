//! Bounded `rg --json` match extraction (domain/tools.md, section 5).
//! The tokenizer validates each emitted JSON line; only match records count.

use alloc::boxed::Box;

use skein_io::kernel;
use skein_json::{Token as JsonToken, tokenizer};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Env, List, Queue, Stack, Time, Wall, bytes};
use smith_domain::tools;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    Root,
    Data,
    Path,
    Lines,
    Other,
}

#[derive(Debug)]
struct Match {
    context: Stack<Context>,
    key: Option<Box<[u8]>>,
    matched: bool,
    path: Option<Box<[u8]>>,
    text: Option<Box<[u8]>>,
    line: Option<u32>,
}

impl Match {
    fn new() -> Match {
        Match { context: Stack::with_capacity(8), key: None, matched: false, path: None, text: None, line: None }
    }

    fn token(&mut self, token: JsonToken) -> Option<()> {
        match token {
            JsonToken::ObjectStart => {
                let context = match self.context.top() {
                    None => Context::Root,
                    Some(Context::Root) if self.key.as_deref() == Some(b"data") => Context::Data,
                    Some(Context::Data) if self.key.as_deref() == Some(b"path") => Context::Path,
                    Some(Context::Data) if self.key.as_deref() == Some(b"lines") => Context::Lines,
                    Some(Context::Root | Context::Data | Context::Path | Context::Lines | Context::Other) => {
                        Context::Other
                    }
                };
                self.context.push(context).ok()?;
                self.key = None;
            }
            JsonToken::ObjectEnd | JsonToken::ArrayEnd => {
                self.context.pop()?;
                self.key = None;
            }
            JsonToken::ArrayStart => {
                self.context.push(Context::Other).ok()?;
                self.key = None;
            }
            JsonToken::Key(key) => self.key = Some(key),
            JsonToken::String(value) => {
                match self.context.top() {
                    Some(Context::Root) if self.key.as_deref() == Some(b"type") => {
                        self.matched = value.as_ref() == b"match";
                    }
                    Some(Context::Path) if self.key.as_deref() == Some(b"text") => self.path = Some(value),
                    Some(Context::Lines) if self.key.as_deref() == Some(b"text") => self.text = Some(value),
                    None | Some(Context::Root | Context::Data | Context::Path | Context::Lines | Context::Other) => {}
                }
                self.key = None;
            }
            JsonToken::Number(value) => {
                match self.context.top() {
                    Some(Context::Data) if self.key.as_deref() == Some(b"line_number") => {
                        self.line = decimal(&value);
                    }
                    None | Some(Context::Root | Context::Data | Context::Path | Context::Lines | Context::Other) => {}
                }
                self.key = None;
            }
            JsonToken::True | JsonToken::False | JsonToken::Null => self.key = None,
        }
        Some(())
    }

    fn finish(self) -> Option<tools::Hit> {
        if !self.matched {
            return None;
        }
        let path = self.path?;
        let mut text = self.text?;
        let mut end = text.len();
        if end > 0 && text.get(end.checked_sub(1)?) == Some(&b'\n') {
            end = end.checked_sub(1)?;
        }
        if text.get(end.saturating_sub(1)) == Some(&b'\r') {
            end = end.checked_sub(1)?;
        }
        text = Box::from(text.get(..end)?);
        Some(tools::Hit { path, line: self.line?, text })
    }
}

/// Parsed lines and the bounded ordered prefix of search hits.
#[derive(Debug)]
pub(crate) struct Search {
    hits: List<tools::Hit>,
    bytes: u32,
    used: u32,
    more: u64,
    line: List<u8>,
    overflow: bool,
}

impl Search {
    pub(crate) fn new(hits: u32, bytes: u32, line_bytes: u32) -> Search {
        Search {
            hits: List::with_capacity(hits),
            bytes,
            used: 0,
            more: 0,
            line: List::with_capacity(line_bytes),
            overflow: false,
        }
    }

    pub(crate) fn push(&mut self, byte: u8) {
        if byte == b'\n' {
            self.end_line();
        } else if self.line.push(byte).is_err() {
            self.overflow = true;
        }
    }

    fn end_line(&mut self) {
        if self.overflow {
            if bytes::find(self.line.as_slice(), b"\"type\":\"match\"").is_some() {
                self.more = self.more.checked_add(1).expect("match count fits u64");
            }
        } else if let Some(hit) = parse(self.line.as_slice()) {
            self.retain(hit);
        }
        self.line.clear();
        self.overflow = false;
    }

    #[expect(clippy::disallowed_methods, reason = "the protocol machine validates rg text")]
    fn retain(&mut self, mut hit: tools::Hit) {
        let Some(path_len) = u32::try_from(hit.path.len()).ok() else {
            self.more = self.more.checked_add(1).expect("match count fits");
            return;
        };
        let mut replacing = None;
        if self.hits.room() == 0 {
            let last = self.hits.len().checked_sub(1);
            let Some(last) = last else {
                self.more = self.more.checked_add(1).expect("match count fits");
                return;
            };
            if !less(&hit, self.hits.get(last).expect("last hit")) {
                self.more = self.more.checked_add(1).expect("match count fits");
                return;
            }
            replacing = Some(last);
        }
        let released = match replacing {
            Some(index) => {
                let old = self.hits.get(index).expect("replacement slot");
                u32::try_from(old.path.len().checked_add(old.text.len()).expect("owned hit length fits"))
                    .expect("bounded hit bytes")
            }
            None => 0,
        };
        let room = self
            .bytes
            .checked_sub(self.used.checked_sub(released).expect("released bytes were held"))
            .expect("used bytes within limit");
        if path_len > room {
            self.more = self.more.checked_add(1).expect("match count fits");
            return;
        }
        let text_room = room.checked_sub(path_len).expect("path fits");
        let text_len = hit.text.len().min(usize::try_from(text_room).expect("u32 fits usize"));
        let mut end = text_len;
        for _ in 0_u8..4 {
            if end == 0 || core::str::from_utf8(hit.text.get(..end).expect("text prefix")).is_ok() {
                break;
            }
            end = end.checked_sub(1).expect("positive end");
        }
        hit.text = Box::from(hit.text.get(..end).expect("text prefix"));
        let kept = path_len.checked_add(u32::try_from(end).expect("bounded text")).expect("path and text fit");
        self.used = self
            .used
            .checked_sub(released)
            .expect("released bytes held")
            .checked_add(kept)
            .expect("retained bytes fit");
        match replacing {
            Some(index) => {
                *self.hits.get_mut(index).expect("replacement slot") = hit;
                self.more = self.more.checked_add(1).expect("evicted match count fits");
            }
            None => self.hits.push(hit).expect("room checked"),
        }
        let mut at = self.hits.len().checked_sub(1).expect("one retained hit");
        for _ in 0..self.hits.len() {
            if at == 0 {
                break;
            }
            let before = at.checked_sub(1).expect("positive index");
            let left = self.hits.get(before).expect("earlier hit").clone();
            let right = self.hits.get(at).expect("later hit").clone();
            if !less(&right, &left) {
                break;
            }
            *self.hits.get_mut(before).expect("earlier slot") = right;
            *self.hits.get_mut(at).expect("later slot") = left;
            at = before;
        }
    }

    pub(crate) fn finish(
        mut self,
        exit: Option<kernel::Exit>,
        timed_out: bool,
        head: Box<[u8]>,
        tail: Box<[u8]>,
        dropped: u64,
    ) -> tools::Done {
        if !self.line.is_empty() || self.overflow {
            self.end_line();
        }
        match exit {
            Some(kernel::Exit::Code(0 | 1)) => {
                tools::Done::Found { hits: self.hits.into_boxed(), more: self.more, timed_out }
            }
            Some(kernel::Exit::Code(2)) if !self.hits.is_empty() || self.more > 0 => {
                tools::Done::Found { hits: self.hits.into_boxed(), more: self.more, timed_out }
            }
            Some(kernel::Exit::Code(code)) => {
                if timed_out {
                    tools::Done::Found { hits: self.hits.into_boxed(), more: self.more, timed_out: true }
                } else {
                    tools::Done::Exited { exit: tools::Exit::Code { code }, head, tail, dropped }
                }
            }
            Some(kernel::Exit::Signal(signal)) => {
                if timed_out {
                    tools::Done::Found { hits: self.hits.into_boxed(), more: self.more, timed_out: true }
                } else {
                    tools::Done::Exited {
                        exit: tools::Exit::Signal { signal: u8::try_from(signal).unwrap_or(u8::MAX) },
                        head,
                        tail,
                        dropped,
                    }
                }
            }
            None => tools::Done::Failed { fault: tools::Fault::Other },
        }
    }
}

fn less(left: &tools::Hit, right: &tools::Hit) -> bool {
    left.path < right.path || (left.path == right.path && left.line < right.line)
}

fn parse(input: &[u8]) -> Option<tools::Hit> {
    let length = u32::try_from(input.len()).ok()?;
    let limits = tokenizer::Limits { depth: 8, string: length.max(1), number: 20, chunk: 64, length: length.max(1) };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut machine = tokenizer::Tokenizer::new(&limits);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut found = Match::new();
    let mut at = 0_usize;
    let ticks = input.len().checked_mul(4)?.checked_add(16)?;
    for _ in 0..ticks {
        match machine.waiting() {
            tokenizer::Waiting::Next => {
                tokenizer::down(&mut machine, &env, tokenizer::Request::Next, &mut above, &mut below);
            }
            tokenizer::Waiting::Bytes => {
                let demand = below.pop()?;
                let read = match demand {
                    Down::Demand { read, .. } => read,
                    Down::Send(_) | Down::Finish => return None,
                };
                let remaining = input.get(at..)?;
                let count = match read {
                    Read::Fill(n) => usize::try_from(n).ok()?,
                    Read::Scan { until, max } => {
                        let max = usize::try_from(max).ok()?;
                        let scanned = remaining.get(..remaining.len().min(max))?;
                        match bytes::find(scanned, until.as_bytes()) {
                            Some(position) => position.checked_add(until.as_bytes().len())?,
                            None => max,
                        }
                    }
                    Read::Nothing | Read::Line { .. } => return None,
                };
                if count <= remaining.len() {
                    let delivery = Box::from(remaining.get(..count)?);
                    at = at.checked_add(count)?;
                    tokenizer::up(&mut machine, &env, Up::Bytes(delivery), &mut above, &mut below);
                } else {
                    tokenizer::up(&mut machine, &env, Up::End, &mut above, &mut below);
                }
            }
            tokenizer::Waiting::Close | tokenizer::Waiting::Nothing => return None,
        }
        if let Some(event) = above.pop() {
            match event {
                tokenizer::Event::Token(token) => found.token(token)?,
                tokenizer::Event::Done => return found.finish(),
                tokenizer::Event::Failed(_) | tokenizer::Event::Closed => return None,
            }
        }
    }
    None
}

fn decimal(input: &[u8]) -> Option<u32> {
    let mut value = 0_u32;
    for byte in input {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(*byte).checked_sub(u32::from(b'0'))?)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{Search, parse};
    use alloc::boxed::Box;
    use skein_io::kernel;
    use smith_domain::tools;

    #[test]
    fn parses_rg_match_and_retains_ordered_prefix() {
        let line = br#"{"type":"match","data":{"path":{"text":"src/a.rs"},"lines":{"text":"hello\n"},"line_number":7,"absolute_offset":0,"submatches":[]}}"#;
        assert_eq!(
            parse(line),
            Some(tools::Hit { path: Box::from(&b"src/a.rs"[..]), line: 7, text: Box::from(&b"hello"[..]) })
        );
        let mut search = Search::new(1, 64, 256);
        for byte in line {
            search.push(*byte);
        }
        search.push(b'\n');
        assert_eq!(
            search.finish(Some(kernel::Exit::Code(0)), false, Box::new([]), Box::new([]), 0),
            tools::Done::Found {
                hits: Box::new([tools::Hit {
                    path: Box::from(&b"src/a.rs"[..]),
                    line: 7,
                    text: Box::from(&b"hello"[..])
                }]),
                more: 0,
                timed_out: false
            }
        );
    }
}
