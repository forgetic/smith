//! Local best-effort JSONL capture. A bounded channel and nonblocking send
//! keep filesystem latency outside the agent's service loop. The writer owns
//! its file and never receives grants or other credential values.
//! Contract: protocol/agent.md, section 5.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::thread;

use serde_json::json;
use smith_domain::{self as domain, tools};

const RECORDS: usize = 32;
const CALL_BYTES: usize = 16_384;
const TEXT_BYTES: usize = 65_536;
const RECORD_BYTES: usize = 400_000;
/// Reserved bytes for the writer's bounded line queue and one formatted line.
pub const MEMORY_RESERVE: u64 = 13_600_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    None,
    Calls,
    Everything,
}

pub struct TraceConfig {
    pub path: PathBuf,
    pub capture: Capture,
}

pub struct Trace {
    sender: SyncSender<String>,
    capture: Capture,
    dropped: u64,
    pending: Arc<AtomicU64>,
    write_failures: Arc<AtomicU64>,
}

impl Trace {
    pub fn open(config: TraceConfig) -> Result<Trace, String> {
        let TraceConfig { path, capture } = config;
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|error| format!("trace file {}: {error}", path.display()))?;
        let (sender, receiver) = sync_channel::<String>(RECORDS);
        let pending = Arc::new(AtomicU64::new(0));
        let write_failures = Arc::new(AtomicU64::new(0));
        let writer_pending = Arc::clone(&pending);
        let background_failures = Arc::clone(&write_failures);
        #[expect(clippy::disallowed_methods, reason = "02-events replaces the trace thread with an io append stream")]
        thread::Builder::new()
            .name("smith-trace".into())
            .spawn(move || write_records(file, receiver, &writer_pending, &background_failures))
            .map_err(|error| format!("trace writer cannot start: {error}"))?;
        Ok(Trace { sender, capture, dropped: 0, pending, write_failures })
    }

    fn offer(&mut self, record: String) {
        if record.len() > RECORD_BYTES {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.pending.fetch_add(1, Ordering::Relaxed);
        match self.sender.try_send(record) {
            Ok(()) => {}
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.pending.fetch_sub(1, Ordering::Relaxed);
                self.dropped = self.dropped.saturating_add(1);
            }
        }
    }

    pub fn fact(&mut self, fact: domain::Fact, at_ns: u64) {
        self.offer(json!({ "type": "fact", "at_ns": at_ns, "fact": format!("{fact:?}") }).to_string());
    }

    pub fn prompt(&mut self, owner: skein_lib::Token, prompt: &[u8], at_ns: u64) {
        self.offer(
            json!({ "type": "prompt", "at_ns": at_ns, "owner": owner.raw(), "encoding": "hex", "bytes": hex(prompt) })
                .to_string(),
        );
    }

    pub fn content(&mut self, content: domain::Content, at_ns: u64) {
        match (self.capture, content) {
            (Capture::Calls | Capture::Everything, domain::Content::Call { owner, id, name, input }) => {
                if id.len() > CALL_BYTES || name.len() > CALL_BYTES || input.len() > CALL_BYTES {
                    self.dropped = self.dropped.saturating_add(1);
                    return;
                }
                self.offer(
                    json!({
                        "type": "call",
                        "at_ns": at_ns,
                        "owner": owner.raw(),
                        "encoding": "hex",
                        "id": hex(&id),
                        "name": hex(&name),
                        "input": hex(&input),
                    })
                    .to_string(),
                );
            }
            (Capture::Calls | Capture::Everything, domain::Content::Tool { owner, done }) => {
                self.offer(
                    json!({ "type": "tool", "at_ns": at_ns, "owner": owner.raw(), "result_bytes": done_bytes(&done) })
                        .to_string(),
                );
            }
            (Capture::Everything, domain::Content::Text { owner, text }) => {
                if text.len() > TEXT_BYTES {
                    self.dropped = self.dropped.saturating_add(1);
                    return;
                }
                let Ok(text) = std::str::from_utf8(&text) else {
                    self.dropped = self.dropped.saturating_add(1);
                    return;
                };
                self.offer(
                    json!({ "type": "completion", "at_ns": at_ns, "owner": owner.raw(), "text": text }).to_string(),
                );
            }
            (Capture::Everything, domain::Content::Usage { owner, usage }) => {
                self.offer(
                    json!({ "type": "usage", "at_ns": at_ns, "owner": owner.raw(), "usage": format!("{usage:?}") })
                        .to_string(),
                );
            }
            (Capture::None, _) | (Capture::Calls, domain::Content::Text { .. } | domain::Content::Usage { .. }) => {}
        }
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
            .saturating_add(self.pending.load(Ordering::Relaxed))
            .saturating_add(self.write_failures.load(Ordering::Relaxed))
    }
}

#[expect(clippy::disallowed_types, reason = "02-events replaces the trace file and thread with an io append stream")]
fn write_records(
    mut file: std::fs::File,
    receiver: std::sync::mpsc::Receiver<String>,
    pending: &AtomicU64,
    write_failures: &AtomicU64,
) {
    for record in receiver {
        let result = writeln!(file, "{record}");
        pending.fetch_sub(1, Ordering::Relaxed);
        if result.is_err() {
            write_failures.fetch_add(1, Ordering::Relaxed);
            break;
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        value.push(char::from(*DIGITS.get(usize::from(*byte >> 4)).expect("high nibble indexes hex digits")));
        value.push(char::from(*DIGITS.get(usize::from(*byte & 15)).expect("low nibble indexes hex digits")));
    }
    value
}

fn done_bytes(done: &tools::Done) -> u64 {
    match done {
        tools::Done::Loaded { content, .. } => u64::try_from(content.len()).expect("content length fits u64"),
        tools::Done::Scanned { entries, .. } => entries
            .iter()
            .map(|entry| u64::try_from(entry.name.as_bytes().len()).expect("entry length fits u64"))
            .fold(0, u64::saturating_add),
        tools::Done::Exited { head, tail, .. } => u64::try_from(head.len())
            .expect("head length fits u64")
            .saturating_add(u64::try_from(tail.len()).expect("tail length fits u64")),
        tools::Done::Found { hits, .. } => hits
            .iter()
            .map(|hit| {
                u64::try_from(hit.path.len())
                    .expect("path length fits u64")
                    .saturating_add(u64::try_from(hit.text.len()).expect("text length fits u64"))
            })
            .fold(0, u64::saturating_add),
        tools::Done::Stored { .. }
        | tools::Done::Conflict { .. }
        | tools::Done::Missing
        | tools::Done::NotFile
        | tools::Done::Linked
        | tools::Done::NotDirectory
        | tools::Done::TooLarge { .. }
        | tools::Done::Escapes
        | tools::Done::Failed { .. }
        | tools::Done::TimedOut
        | tools::Done::Cancelled => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skein_lib::Token;

    fn trace(capture: Capture) -> (Trace, std::sync::mpsc::Receiver<String>) {
        let (sender, receiver) = sync_channel(2);
        (
            Trace {
                sender,
                capture,
                dropped: 0,
                pending: Arc::new(AtomicU64::new(0)),
                write_failures: Arc::new(AtomicU64::new(0)),
            },
            receiver,
        )
    }

    #[test]
    fn facts_are_content_free_and_calls_policy_keeps_only_selected_content() {
        let (mut trace, receiver) = trace(Capture::Calls);
        trace.content(domain::Content::Text { owner: Token::new(1), text: b"private completion".to_vec().into() }, 5);
        assert!(receiver.try_recv().is_err());
        trace.content(
            domain::Content::Call {
                owner: Token::new(1),
                id: b"call".to_vec().into(),
                name: b"inspect".to_vec().into(),
                input: b"file".to_vec().into(),
            },
            6,
        );
        let record = receiver.try_recv().expect("call record");
        assert!(record.contains("696e7370656374"));
        assert!(record.contains("66696c65"));
        assert!(!record.contains("private completion"));
    }

    #[test]
    fn full_trace_keeps_completion_text_and_counts_backpressure() {
        let (mut trace, receiver) = trace(Capture::Everything);
        trace.content(domain::Content::Text { owner: Token::new(1), text: b"answer".to_vec().into() }, 5);
        assert!(receiver.try_recv().expect("completion").contains("answer"));
        trace.content(domain::Content::Text { owner: Token::new(1), text: b"first".to_vec().into() }, 5);
        trace.content(domain::Content::Text { owner: Token::new(1), text: b"second".to_vec().into() }, 5);
        trace.content(domain::Content::Text { owner: Token::new(1), text: b"third".to_vec().into() }, 5);
        assert_eq!(trace.dropped, 1);
    }

    #[test]
    fn prompt_record_preserves_binary_bytes_without_a_lossy_decode() {
        let (mut trace, receiver) = trace(Capture::Everything);
        trace.prompt(Token::new(9), &[0, 255, b'A'], 7);
        let record = receiver.try_recv().expect("prompt record");
        assert!(record.contains("00ff41"));
        assert!(record.contains("\"encoding\":\"hex\""));
    }
}
