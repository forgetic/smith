//! Durable chat files for the local shell (protocol/hosts.md, section 5.3).
//! The shell owns paths and descriptors and never interprets the typed chat.
//! Each replacement is written, synced, renamed and followed by directory
//! sync before its domain acknowledgement is returned.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use skein_lib::Token;
use smith_local_domain::{self as local, DeliveryRecord};
use smith_local_protocol as protocol;
use smith_protocol_channel::Endpoints;

pub struct Store {
    directory: PathBuf,
    endpoints: Endpoints,
    delivery_bytes: u32,
    serial: u64,
}

impl Store {
    pub fn new(directory: PathBuf, endpoints: Endpoints, delivery_bytes: u32) -> io::Result<Self> {
        fs::create_dir_all(&directory)?;
        Ok(Self { directory, endpoints, delivery_bytes, serial: 0 })
    }

    pub fn load(&self) -> Result<local::Event, local::StoreFailure> {
        let mut state = match read_optional(&self.directory.join("state"))? {
            Some(bytes) => Some(protocol::decode_state(&bytes).map_err(|_| local::StoreFailure::Read)?),
            None => None,
        };
        let mut turns = Vec::new();
        let mut deliveries = Vec::new();
        let entries = fs::read_dir(&self.directory).map_err(|_| local::StoreFailure::Read)?;
        for entry in entries {
            let entry = entry.map_err(|_| local::StoreFailure::Read)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { return Err(local::StoreFailure::Read) };
            if turn_file(name) {
                let bytes = fs::read(entry.path()).map_err(|_| local::StoreFailure::Read)?;
                turns.push((name.to_owned(), bytes.into_boxed_slice()));
            } else if delivery_file(name) {
                let bytes = fs::read(entry.path()).map_err(|_| local::StoreFailure::Read)?;
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
        self.replace(protocol::save_state(state)).map_err(|_| local::StoreFailure::Write)?;
        if fresh {
            self.discard_old_records().map_err(|_| local::StoreFailure::Write)?;
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
        let (temp, mut file) = loop {
            self.serial = self.serial.checked_add(1).ok_or_else(|| io::Error::other("store serial exhausted"))?;
            let candidate = self.directory.join(format!(".{name}.{}.tmp", self.serial));
            match OpenOptions::new().write(true).create_new(true).open(&candidate) {
                Ok(file) => break (candidate, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let target = self.directory.join(name);
        let result = (|| {
            file.write_all(&replacement.bytes)?;
            file.sync_all()?;
            fs::rename(&temp, &target)?;
            sync_directory(&self.directory)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn discard_old_records(&self) -> io::Result<()> {
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            if let Some(name) = name.to_str()
                && (turn_file(name) || delivery_file(name))
            {
                fs::remove_file(entry.path())?;
            }
        }
        sync_directory(&self.directory)
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, local::StoreFailure> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(local::StoreFailure::Read),
    }
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
}
