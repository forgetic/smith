//! Durable chat files for the local shell (protocol/hosts.md, section 5.3).
//! The shell owns paths and descriptors and never interprets the typed chat.
//! Each replacement is written, synced, renamed and followed by directory
//! sync before its domain acknowledgement is returned. A fresh-chat journal
//! completes removal of prior records after a cut; loading refuses oversized
//! files, too many records, and symbolic links.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use skein_lib::Token;
use smith_local_domain::{self as local, DeliveryRecord};
use smith_local_protocol as protocol;
use smith_protocol_channel::Endpoints;

/// The shared shell owns bounded chat files and durable replacement ordering.
pub struct Store {
    directory: PathBuf,
    endpoints: Endpoints,
    delivery_bytes: u32,
    serial: u64,
    cut: Option<u32>,
    boundaries: u32,
}

/// Aggregate bytes and entries admitted at startup, including stale temporaries.
pub const STORE_BYTES: u64 = 64 * 1024 * 1024;

const RECORDS: usize = 4096;

impl Store {
    pub fn new(directory: PathBuf, endpoints: Endpoints, delivery_bytes: u32) -> io::Result<Self> {
        if directory.as_os_str().len() > 4096 || delivery_bytes == 0 || delivery_bytes > 1 << 20 {
            return Err(io::Error::other("chat store configuration exceeds its bounds"));
        }
        fs::create_dir_all(&directory)?;
        let mut store = Self { directory, endpoints, delivery_bytes, serial: 0, cut: None, boundaries: 0 };
        store.recover_fresh()?;
        Ok(store)
    }

    /// Encoding, startup bytes, decoded records and paths can coexist.
    #[must_use]
    pub fn worst_case() -> Option<u64> {
        STORE_BYTES.checked_mul(4)?.checked_add(u64::try_from(RECORDS).ok()?.checked_mul(4096)?)
    }

    pub fn load(&self) -> Result<local::Event, local::StoreFailure> {
        let mut total = 0_u64;
        if self.directory.join("fresh").exists() {
            return Err(local::StoreFailure::Read);
        }
        let mut state = match read_optional(&self.directory.join("state"), 33, &mut total)? {
            Some(bytes) => Some(protocol::decode_state(&bytes).map_err(|_| local::StoreFailure::Read)?),
            None => None,
        };
        let mut turns = Vec::new();
        let mut deliveries = Vec::new();
        let entries = fs::read_dir(&self.directory).map_err(|_| local::StoreFailure::Read)?;
        for (index, entry) in entries.enumerate() {
            if index >= RECORDS * 2 {
                return Err(local::StoreFailure::Read);
            }
            let entry = entry.map_err(|_| local::StoreFailure::Read)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { return Err(local::StoreFailure::Read) };
            if turn_file(name) {
                if turns.len() + deliveries.len() >= RECORDS {
                    return Err(local::StoreFailure::Read);
                }
                let bytes = read_optional(&entry.path(), STORE_BYTES, &mut total)?.ok_or(local::StoreFailure::Read)?;
                turns.push((name.to_owned(), bytes.into_boxed_slice()));
            } else if delivery_file(name) {
                if turns.len() + deliveries.len() >= RECORDS {
                    return Err(local::StoreFailure::Read);
                }
                let bytes = read_optional(&entry.path(), u64::from(self.delivery_bytes), &mut total)?
                    .ok_or(local::StoreFailure::Read)?;
                let record =
                    protocol::decode_delivery(&bytes, self.delivery_bytes).map_err(|_| local::StoreFailure::Read)?;
                deliveries.push(record);
            }
        }
        turns.sort_by(|left, right| left.0.cmp(&right.0));
        let mut ordered = Vec::with_capacity(turns.len());
        for (_, bytes) in turns {
            ordered.push(bytes);
        }
        let (transcript, read) = protocol::decode_turns(&ordered, &smith_transcript::CEILINGS, &self.endpoints)
            .map_err(|_| local::StoreFailure::Read)?;
        if transcript.is_some() {
            let Some(saved) = &mut state else { return Err(local::StoreFailure::Read) };
            saved.read = read;
        }
        deliveries.sort_by_key(|record| (record.name.activation, record.name.completion, record.name.position));
        Ok(local::Event::Loaded { state, transcript, deliveries: deliveries.into_boxed_slice() })
    }

    pub fn save_state(&mut self, state: local::ChatState, fresh: bool) -> Result<local::Event, local::StoreFailure> {
        if fresh {
            let mut journal = protocol::save_state(state);
            journal.name = b"fresh".as_slice().into();
            self.replace(journal).map_err(|_| local::StoreFailure::Write)?;
            self.recover_fresh().map_err(|_| local::StoreFailure::Write)?;
        } else {
            self.replace(protocol::save_state(state)).map_err(|_| local::StoreFailure::Write)?;
        }
        Ok(local::Event::StateSaved)
    }

    pub fn save_turn(
        &mut self,
        number: u32,
        read: Option<Token>,
        turn: smith_domain::Turn,
    ) -> Result<local::Event, local::StoreFailure> {
        let file = protocol::save_turn(number, read, &turn, &smith_transcript::CEILINGS, &self.endpoints)
            .map_err(|_| local::StoreFailure::Write)?;
        self.replace(file).map_err(|_| local::StoreFailure::Write)?;
        Ok(local::Event::TurnSaved { number })
    }

    pub fn save_delivery(&mut self, record: &DeliveryRecord) -> Result<local::Event, local::StoreFailure> {
        let file = protocol::save_delivery(record, self.delivery_bytes).map_err(|_| local::StoreFailure::Write)?;
        self.replace(file).map_err(|_| local::StoreFailure::Write)?;
        Ok(local::Event::DeliverySaved { name: record.name })
    }

    fn replace(&mut self, replacement: protocol::File) -> io::Result<()> {
        let name = std::str::from_utf8(&replacement.name)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "record filename is not UTF-8"))?;
        let mut created = None;
        for _ in 0..RECORDS {
            self.serial = self.serial.checked_add(1).ok_or_else(|| io::Error::other("store serial exhausted"))?;
            let candidate = self.directory.join(format!(".{name}.{}.tmp", self.serial));
            match OpenOptions::new().write(true).create_new(true).open(&candidate) {
                Ok(file) => {
                    created = Some((candidate, file));
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        let (temp, mut file) = created.ok_or_else(|| io::Error::other("too many stale record temporaries"))?;
        self.boundary()?;
        let target = self.directory.join(name);
        let result = (|| {
            let split = replacement.bytes.len() / 2;
            file.write_all(&replacement.bytes[..split])?;
            self.boundary()?;
            file.write_all(&replacement.bytes[split..])?;
            self.boundary()?;
            file.sync_all()?;
            self.boundary()?;
            fs::rename(&temp, &target)?;
            self.boundary()?;
            sync_directory(&self.directory)?;
            self.boundary()
        })();
        if result.is_err() && self.cut != Some(0) {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn discard_old_records(&mut self) -> io::Result<()> {
        for (index, entry) in fs::read_dir(&self.directory)?.enumerate() {
            if index >= RECORDS * 2 {
                return Err(io::Error::other("too many chat directory entries"));
            }
            let entry = entry?;
            let name = entry.file_name();
            if let Some(name) = name.to_str()
                && (turn_file(name) || delivery_file(name))
            {
                fs::remove_file(entry.path())?;
                self.boundary()?;
            }
        }
        sync_directory(&self.directory)?;
        self.boundary()
    }

    fn recover_fresh(&mut self) -> io::Result<()> {
        let path = self.directory.join("fresh");
        let Some(bytes) =
            read_optional(&path, 33, &mut 0).map_err(|_| io::Error::other("fresh journal read failed"))?
        else {
            return Ok(());
        };
        let state = protocol::decode_state(&bytes).map_err(|_| io::Error::other("invalid fresh journal"))?;
        self.discard_old_records()?;
        self.replace(protocol::save_state(state))?;
        fs::remove_file(path)?;
        self.boundary()?;
        sync_directory(&self.directory)?;
        self.boundary()
    }

    /// Cut after a numbered file boundary; reopening recovers the completed prefix.
    /// This deterministic fault source is shared by process worlds and store tests.
    pub fn crash_after(&mut self, boundary: Option<u32>) {
        assert!(boundary != Some(0), "crash boundary numbering starts at one");
        self.cut = boundary;
        self.boundaries = 0;
    }

    /// File boundaries completed since the most recent fault-source reset.
    #[must_use]
    pub fn boundaries(&self) -> u32 {
        self.boundaries
    }

    fn boundary(&mut self) -> io::Result<()> {
        self.boundaries =
            self.boundaries.checked_add(1).ok_or_else(|| io::Error::other("file boundary counter exhausted"))?;
        if let Some(remaining) = &mut self.cut {
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "injected file crash cut"));
            }
        }
        Ok(())
    }
}

fn read_optional(path: &Path, bound: u64, total: &mut u64) -> Result<Option<Vec<u8>>, local::StoreFailure> {
    let file = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(local::StoreFailure::Read),
    };
    let metadata = file.metadata().map_err(|_| local::StoreFailure::Read)?;
    if !metadata.is_file() || metadata.len() > bound {
        return Err(local::StoreFailure::Read);
    }
    let room = STORE_BYTES.checked_sub(*total).ok_or(local::StoreFailure::Read)?.min(bound);
    let mut bytes = Vec::new();
    file.take(room.checked_add(1).ok_or(local::StoreFailure::Read)?)
        .read_to_end(&mut bytes)
        .map_err(|_| local::StoreFailure::Read)?;
    let count = u64::try_from(bytes.len()).map_err(|_| local::StoreFailure::Read)?;
    if count > room {
        return Err(local::StoreFailure::Read);
    }
    *total = total.checked_add(count).ok_or(local::StoreFailure::Read)?;
    Ok(Some(bytes))
}

fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn turn_file(name: &str) -> bool {
    name.len() == 15 && name.ends_with(".turn") && name.as_bytes()[..10].iter().all(u8::is_ascii_digit)
}

fn delivery_file(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 42
        && bytes[20] == b'.'
        && bytes[31] == b'.'
        && bytes[..20].iter().all(u8::is_ascii_digit)
        && bytes[21..31].iter().all(u8::is_ascii_digit)
        && bytes[32..].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    use skein_lib::List;
    use smith_domain::{Turn, session};
    use smith_protocol_channel::Endpoint;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn directory() -> PathBuf {
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("smith-local-store-{}-{number}", std::process::id()))
    }

    fn endpoints() -> Endpoints {
        let mut entries = List::with_capacity(1);
        entries
            .push(Endpoint { name: Box::from(&b"main"[..]), number: 2, dialect: 1, account: 0 })
            .expect("one endpoint");
        Endpoints::new(entries)
    }

    fn turn() -> Turn {
        Turn {
            version: session::record::VERSION,
            endpoint: session::llm::Endpoint(2),
            dialect: 1,
            sequence: 1,
            usage: session::llm::Usage::ZERO,
            spent: 0,
            messages: Box::new([]),
        }
    }

    #[test]
    fn synced_turn_recovers_the_read_fence_across_store_reopen() {
        let path = directory();
        let mut store = Store::new(path.clone(), endpoints(), 4096).expect("store opens");
        store
            .save_state(local::ChatState { activation: 1, next_message: 2, read: None }, false)
            .expect("state durable");
        store.save_turn(1, Some(Token::new(2)), turn()).expect("turn durable");
        drop(store);
        let reopened = Store::new(path.clone(), endpoints(), 4096).expect("store reopens");
        let local::Event::Loaded { state: Some(state), transcript: Some(transcript), deliveries } =
            reopened.load().expect("records load")
        else {
            panic!("saved chat must load");
        };
        assert_eq!(state.read, Some(Token::new(2)));
        assert_eq!(transcript.turns.len(), 1);
        assert!(deliveries.is_empty());
        fs::remove_dir_all(path).expect("remove test store");
    }

    #[test]
    fn incomplete_temporary_replacement_is_ignored() {
        let path = directory();
        let store = Store::new(path.clone(), endpoints(), 4096).expect("store opens");
        fs::write(path.join(".state.1.tmp"), b"partial").expect("crash cut file");
        let local::Event::Loaded { state: None, transcript: None, deliveries } =
            store.load().expect("temporary bytes ignored")
        else {
            panic!("temporary replacement is invisible");
        };
        assert!(deliveries.is_empty());
        fs::remove_dir_all(path).expect("remove test store");
    }

    #[test]
    fn a_crashed_temporary_file_does_not_block_the_next_durable_save() {
        let path = directory();
        let mut store = Store::new(path.clone(), endpoints(), 4096).expect("store opens");
        fs::write(path.join(".state.1.tmp"), b"partial").expect("crash cut file");
        store
            .save_state(local::ChatState { activation: 1, next_message: 1, read: None }, false)
            .expect("new save uses a fresh temporary file");
        assert!(path.join(".state.1.tmp").exists());
        assert!(matches!(store.load(), Ok(local::Event::Loaded { state: Some(_), .. })));
        fs::remove_dir_all(path).expect("remove test store");
    }

    fn state(activation: u64) -> local::ChatState {
        local::ChatState { activation, next_message: 2, read: Some(Token::new(2)) }
    }

    fn delivery() -> DeliveryRecord {
        let receipt = smith_domain::run::Receipt::new(0, b"commit abc".as_slice().into()).expect("receipt");
        DeliveryRecord {
            name: smith_domain::run::CallName { activation: 1, completion: 1, position: 0 },
            state: local::DeliveryState::Answer(smith_domain::run::Delivery::Delivered(
                smith_domain::run::Delivered::new(Box::new([receipt.clone()])).expect("delivery"),
            )),
            landed: Box::new([receipt]),
        }
    }

    fn baseline(path: &Path) -> Store {
        let mut store = Store::new(path.into(), endpoints(), 4096).expect("store");
        store.save_state(state(1), false).expect("metadata acknowledged");
        store.save_turn(1, Some(Token::new(2)), turn()).expect("turn acknowledged");
        store.save_delivery(&delivery()).expect("answered call acknowledged");
        store
    }

    #[test]
    fn every_write_and_sync_cut_keeps_acknowledged_turns_and_answered_calls() {
        for cut in 1..=6 {
            for record in 0..3 {
                let path = directory();
                let mut store = baseline(&path);
                store.crash_after(Some(cut));
                let result = match record {
                    0 => store.save_state(state(2), false),
                    1 => {
                        let mut next = turn();
                        next.sequence = 2;
                        store.save_turn(2, Some(Token::new(3)), next)
                    }
                    2 => {
                        let mut next = delivery();
                        next.name.completion = 2;
                        store.save_delivery(&next)
                    }
                    _ => unreachable!(),
                };
                assert_eq!(
                    result.err(),
                    Some(local::StoreFailure::Write),
                    "cut {cut}, record {record} has no acknowledgement"
                );
                assert_eq!(store.boundaries(), cut);
                drop(store);
                let store = Store::new(path.clone(), endpoints(), 4096).expect("reopen cut");
                let local::Event::Loaded { transcript: Some(transcript), deliveries, .. } =
                    store.load().expect("complete prefixes load")
                else {
                    panic!("acknowledged history")
                };
                assert_eq!(transcript.turns[0].sequence, 1);
                assert_eq!(deliveries[0], delivery());
                assert_eq!(transcript.turns.len(), if record == 1 && cut >= 5 { 2 } else { 1 });
                assert_eq!(deliveries.len(), if record == 2 && cut >= 5 { 2 } else { 1 });
                fs::remove_dir_all(path).expect("cleanup");
            }
        }
    }

    #[test]
    fn every_fresh_reset_cut_recovers_one_complete_history() {
        for cut in 1..=17 {
            let path = directory();
            let mut store = baseline(&path);
            store.crash_after(Some(cut));
            assert_eq!(store.save_state(state(2), true).err(), Some(local::StoreFailure::Write), "cut {cut}");
            assert_eq!(store.boundaries(), cut);
            drop(store);
            let store = Store::new(path.clone(), endpoints(), 4096).expect("fresh journal recovers");
            let local::Event::Loaded { state: Some(saved), transcript, deliveries } =
                store.load().expect("one complete history")
            else {
                panic!("metadata")
            };
            if cut < 5 {
                assert_eq!(saved.activation, 1);
                assert_eq!(transcript.expect("original history").turns.len(), 1);
                assert_eq!(deliveries, Box::new([delivery()]) as Box<[DeliveryRecord]>);
            } else {
                assert_eq!(saved.activation, 2);
                assert!(transcript.is_none());
                assert!(deliveries.is_empty());
                assert!(!path.join("fresh").exists());
            }
            fs::remove_dir_all(path).expect("cleanup");
        }
        let path = directory();
        let mut store = baseline(&path);
        store.crash_after(None);
        assert!(matches!(store.save_state(state(2), true), Ok(local::Event::StateSaved)));
        assert_eq!(store.boundaries(), 17, "all journal, writes, syncs and removals covered");
        drop(store);
        let store = Store::new(path.clone(), endpoints(), 4096).expect("acknowledged reset reopens");
        assert!(matches!(store.load(), Ok(local::Event::Loaded { transcript: None, .. })));
        fs::remove_dir_all(path).expect("cleanup");
    }

    #[test]
    fn oversized_and_symbolic_records_are_refused_before_decoding() {
        use std::os::unix::fs::symlink;
        let path = directory();
        let store = Store::new(path.clone(), endpoints(), 4096).expect("store");
        let oversized = File::create(path.join("0000000001.turn")).expect("sparse file");
        oversized.set_len(STORE_BYTES + 1).expect("bounded sparse fixture");
        assert_eq!(store.load().err(), Some(local::StoreFailure::Read));
        fs::remove_file(path.join("0000000001.turn")).expect("remove");
        fs::write(path.join("other"), protocol::save_state(state(1)).bytes).expect("other");
        symlink("other", path.join("state")).expect("symbolic state");
        assert_eq!(store.load().err(), Some(local::StoreFailure::Read));
        fs::remove_dir_all(path).expect("cleanup");
    }
}
