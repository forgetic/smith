//! Local best-effort JSONL capture. A bounded channel and nonblocking send
//! keep filesystem latency outside the agent's service loop. The writer owns
//! its file and never receives grants or other credential values. Once the
//! run has settled, `finish` gives queued records one bounded drain period.
//! Contract: protocol/agent.md, section 5.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread;
use std::time::Duration;

use serde_json::json;
use smith_domain::{self as domain, tools};

const RECORDS: usize = 32;
const CALL_BYTES: usize = 16_384;
const TEXT_BYTES: usize = 65_536;
const RECORD_BYTES: usize = 400_000;
const SHUTDOWN_GRACE: Duration = Duration::from_millis(50);
// Hex payloads, JSON-owned copies and serialization growth fit this scratch
// bound under CALL_BYTES/TEXT_BYTES, including a maximally escaped completion.
const FORMAT_BYTES: u64 = 2_097_152;
/// Reserved record storage: bounded queue, one writer line and formatting scratch.
pub const MEMORY_RESERVE: u64 = match (RECORDS as u64).checked_add(1) {
    Some(records) => match records.checked_mul(RECORD_BYTES as u64) {
        Some(bytes) => match bytes.checked_add(FORMAT_BYTES) {
            Some(reserve) => reserve,
            None => panic!("trace formatting reserve fits"),
        },
        None => panic!("trace record reserve fits"),
    },
    None => panic!("trace record count fits"),
};

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
    sender: Option<SyncSender<Box<str>>>,
    drained: Option<Receiver<()>>,
    capture: Capture,
    dropped: u64,
    pending: Arc<AtomicU64>,
    write_failures: Arc<AtomicU64>,
}

impl Trace {
    pub fn open(config: TraceConfig) -> Result<Trace, String> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&config.path)
            .map_err(|error| format!("trace file {}: {error}", config.path.display()))?;
        let (sender, receiver) = sync_channel::<Box<str>>(RECORDS);
        let (writer_done, drained) = sync_channel(1);
        let pending = Arc::new(AtomicU64::new(0));
        let write_failures = Arc::new(AtomicU64::new(0));
        let writer_pending = Arc::clone(&pending);
        let writer_failures = Arc::clone(&write_failures);
        thread::Builder::new()
            .name("smith-trace".into())
            .spawn(move || {
                write_records(file, receiver, &writer_pending, &writer_failures);
                let _ = writer_done.send(());
            })
            .map_err(|error| format!("trace writer cannot start: {error}"))?;
        Ok(Trace {
            sender: Some(sender),
            drained: Some(drained),
            capture: config.capture,
            dropped: 0,
            pending,
            write_failures,
        })
    }

    fn offer(&mut self, record: String) {
        if record.len() > RECORD_BYTES {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let Some(sender) = &self.sender else {
            self.dropped = self.dropped.saturating_add(1);
            return;
        };
        self.pending.fetch_add(1, Ordering::Relaxed);
        // Shrink before retention: serialized String capacity may exceed its length.
        match sender.try_send(record.into_boxed_str()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.pending.fetch_sub(1, Ordering::Relaxed);
                self.dropped = self.dropped.saturating_add(1);
            }
        }
    }

    pub fn fact(&mut self, fact: domain::Fact, at_ns: u64) {
        self.offer(json!({ "type": "fact", "at_ns": at_ns, "fact": format!("{fact:?}") }).to_string());
    }

    pub fn prompt(&mut self, owner: skein_lib::Token, prompt: &[u8], at_ns: u64) {
        if prompt.len() > TEXT_BYTES {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.offer(
            json!({ "type": "prompt", "at_ns": at_ns, "owner": owner.raw(), "encoding": "hex", "bytes": hex(prompt) })
                .to_string(),
        );
    }

    pub fn content(&mut self, content: domain::Content, at_ns: u64) {
        match (self.capture, content) {
            (Capture::None, _) => {}
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
            (Capture::Calls, domain::Content::Text { .. } | domain::Content::Usage { .. }) => {}
        }
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
            .saturating_add(self.pending.load(Ordering::Relaxed))
            .saturating_add(self.write_failures.load(Ordering::Relaxed))
    }

    /// Stop capture and drain accepted records for at most 50 milliseconds.
    /// A stalled writer remains detached; pending records count as dropped.
    /// Repeated calls do not wait again.
    pub fn finish(&mut self) -> u64 {
        drop(self.sender.take());
        if let Some(drained) = self.drained.take() {
            let _ = drained.recv_timeout(SHUTDOWN_GRACE);
        }
        self.dropped()
    }
}

fn write_records(
    mut file: File,
    receiver: std::sync::mpsc::Receiver<Box<str>>,
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
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        value.push(char::from(DIGITS[usize::from(*byte >> 4)]));
        value.push(char::from(DIGITS[usize::from(*byte & 15)]));
    }
    value
}

fn done_bytes(done: &tools::Done) -> u64 {
    match done {
        tools::Done::Loaded { content, .. } => content.len() as u64,
        tools::Done::Scanned { entries, .. } => entries.iter().map(|entry| entry.name.as_bytes().len() as u64).sum(),
        tools::Done::Exited { head, tail, .. } => (head.len() + tail.len()) as u64,
        tools::Done::Found { hits, .. } => hits.iter().map(|hit| (hit.path.len() + hit.text.len()) as u64).sum(),
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

    fn trace(capture: Capture) -> (Trace, std::sync::mpsc::Receiver<Box<str>>) {
        let (sender, receiver) = sync_channel(2);
        (
            Trace {
                sender: Some(sender),
                drained: None,
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

    #[test]
    fn finishing_a_fast_writer_drains_all_accepted_records_before_returning() {
        let path = std::env::temp_dir().join(format!("smith-trace-drain-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut trace = Trace::open(TraceConfig { path: path.clone(), capture: Capture::Calls }).expect("trace writer");
        for index in 0..RECORDS {
            trace.content(
                domain::Content::Call {
                    owner: Token::new(u64::try_from(index).expect("bounded record count")),
                    id: b"call".as_slice().into(),
                    name: b"finish".as_slice().into(),
                    input: b"{}".as_slice().into(),
                },
                5,
            );
        }
        assert_eq!(trace.finish(), 0, "a fast writer loses no accepted records");
        assert_eq!(trace.finish(), 0, "finish is idempotent");
        let records = std::fs::read_to_string(&path).expect("drained file");
        assert_eq!(records.lines().count(), RECORDS);
        assert!(records.lines().all(|line| line.contains("66696e697368")));
        std::fs::remove_file(path).expect("remove trace fixture");
    }

    #[test]
    fn a_burst_to_an_actual_paused_writer_is_bounded_and_counts_only_overflow() {
        let path = std::env::temp_dir().join(format!("smith-trace-burst-{}.jsonl", std::process::id()));
        let file = File::create(&path).expect("trace fixture");
        let (sender, receiver) = sync_channel(RECORDS);
        let (writer_done, drained) = sync_channel(1);
        let (release, paused) = sync_channel(1);
        let pending = Arc::new(AtomicU64::new(0));
        let write_failures = Arc::new(AtomicU64::new(0));
        let writer_pending = Arc::clone(&pending);
        let writer_failures = Arc::clone(&write_failures);
        let writer = thread::spawn(move || {
            paused.recv().expect("release actual writer after burst");
            write_records(file, receiver, &writer_pending, &writer_failures);
            let _ = writer_done.send(());
        });
        let mut trace = Trace {
            sender: Some(sender),
            drained: Some(drained),
            capture: Capture::Everything,
            dropped: 0,
            pending,
            write_failures,
        };
        for index in 0..RECORDS * 2 {
            trace.offer(format!("{{\"burst\":{index}}}"));
        }
        assert_eq!(trace.pending.load(Ordering::Relaxed), RECORDS as u64);
        assert_eq!(trace.dropped, RECORDS as u64, "only records beyond bounded capacity are lost");
        release.send(()).expect("allow bounded drain");
        assert_eq!(trace.finish(), RECORDS as u64);
        writer.join().expect("fixture writer settles");
        let records = std::fs::read_to_string(&path).expect("burst file");
        assert_eq!(records.lines().count(), RECORDS);
        assert_eq!(records.lines().last(), Some(format!("{{\"burst\":{}}}", RECORDS - 1).as_str()));
        std::fs::remove_file(path).expect("remove burst fixture");
    }

    #[test]
    fn maximum_escaped_completion_fits_exact_record_storage_and_prompt_cap() {
        let (mut trace, receiver) = trace(Capture::Everything);
        trace.content(domain::Content::Text { owner: Token::new(1), text: vec![1; TEXT_BYTES].into() }, 5);
        let record = receiver.try_recv().expect("maximally escaped completion");
        assert!(record.len() <= RECORD_BYTES);
        let length = record.len();
        assert_eq!(record.into_string().capacity(), length, "boxed record retains only serialized bytes");
        trace.prompt(Token::new(1), &vec![0; TEXT_BYTES + 1], 6);
        assert!(receiver.try_recv().is_err());
        assert_eq!(trace.dropped, 1);
    }

    #[test]
    fn a_stalled_writer_has_one_bounded_shutdown_grace_and_reports_pending_loss() {
        let path = std::env::temp_dir().join(format!("smith-trace-stall-{}.jsonl", std::process::id()));
        let file = File::create(&path).expect("trace fixture");
        let (sender, receiver) = sync_channel(RECORDS);
        let (writer_done, drained) = sync_channel(1);
        let (release, stalled) = sync_channel(1);
        let pending = Arc::new(AtomicU64::new(0));
        let write_failures = Arc::new(AtomicU64::new(0));
        let writer_pending = Arc::clone(&pending);
        let writer_failures = Arc::clone(&write_failures);
        let writer = thread::spawn(move || {
            stalled.recv().expect("the test releases the stalled writer");
            write_records(file, receiver, &writer_pending, &writer_failures);
            let _ = writer_done.send(());
        });
        let mut trace = Trace {
            sender: Some(sender),
            drained: Some(drained),
            capture: Capture::Calls,
            dropped: 0,
            pending,
            write_failures,
        };
        trace.offer("queued record".into());
        let started = std::time::Instant::now();
        assert_eq!(trace.finish(), 1, "the unwritten record is reported as lost at shutdown");
        assert!(started.elapsed() < Duration::from_secs(1), "a stalled writer cannot hold process shutdown");
        assert!(trace.drained.is_none(), "the grace is consumed once");
        assert_eq!(trace.finish(), 1);
        assert!(std::fs::read_to_string(&path).expect("stalled file").is_empty());
        release.send(()).expect("release fixture writer after bounded finish returns");
        writer.join().expect("released fixture writer settles");
        std::fs::remove_file(path).expect("remove trace fixture");
    }
}
