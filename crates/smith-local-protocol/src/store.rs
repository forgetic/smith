//! Typed local chat records and durable-file requests (protocol/hosts.md,
//! section 5.3). The caller stores each `File` with Skein file `Store`, whose
//! terminal follows the file sync, rename, and directory sync. This module
//! retains no file descriptor or secret. `save_state`, `save_turn`, and the
//! decoders translate the domain's concrete records without interpreting a
//! conversation.

use alloc::boxed::Box;
use skein_lib::{List, Reader, Token, Writer};
use smith_domain::Transcript;
use smith_local_domain::ChatState;
use smith_protocol_channel::{self as channel, Endpoints};
use smith_transcript as transcript;

const STATE_BYTES: usize = 4 + 8 + 8 + 1 + 8;
const TURN_HEADER: usize = 4 + 1 + 8;

/// One file replacement; the service acknowledges it only after a durable store terminal.
#[derive(Debug, PartialEq, Eq)]
pub struct File {
    /// Path relative to the selected chat directory.
    pub name: Box<[u8]>,
    /// Whole bounded replacement bytes.
    pub bytes: Box<[u8]>,
}

/// Why a retained chat record cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The record is malformed, truncated, or from a different version.
    Malformed,
    /// A typed turn cannot be encoded under these configured endpoints.
    Turn,
    /// The record exceeds the configured file bound.
    TooLarge,
}

/// The metadata file after the caller has durably stored it.
#[must_use]
pub fn save_state(state: ChatState) -> File {
    let mut writer = Writer::new(STATE_BYTES);
    writer.put(b"SMLS").expect("fixed state header fits");
    writer.put(&state.activation.to_be_bytes()).expect("fixed activation fits");
    writer.put(&state.next_message.to_be_bytes()).expect("fixed message counter fits");
    match state.read {
        Some(read) => {
            writer.put(&[1]).expect("fixed read flag fits");
            writer.put(&read.raw().to_be_bytes()).expect("fixed read token fits");
        }
        None => {
            writer.put(&[0]).expect("fixed read flag fits");
            writer.put(&0_u64.to_be_bytes()).expect("fixed absent token fits");
        }
    }
    File { name: Box::from(&b"state"[..]), bytes: writer.finish() }
}

/// Read a complete local metadata file.
pub fn decode_state(bytes: &[u8]) -> Result<ChatState, StoreError> {
    if bytes.len() != STATE_BYTES {
        return Err(StoreError::Malformed);
    }
    let mut reader = Reader::new(bytes);
    if reader.bytes(4) != Some(&b"SMLS"[..]) {
        return Err(StoreError::Malformed);
    }
    let activation = reader.u64().ok_or(StoreError::Malformed)?;
    let next_message = reader.u64().ok_or(StoreError::Malformed)?;
    let flag = reader.u8().ok_or(StoreError::Malformed)?;
    let raw = reader.u64().ok_or(StoreError::Malformed)?;
    let read = match flag {
        0 if raw == 0 => None,
        1 => Some(Token::new(raw)),
        0 | 2..=u8::MAX => return Err(StoreError::Malformed),
    };
    if !reader.is_empty() {
        return Err(StoreError::Malformed);
    }
    Ok(ChatState { activation, next_message, read })
}

/// Fixed-width numeric path for one turn, sortable in transcript order.
#[must_use]
pub fn turn_name(place: u32) -> Box<[u8]> {
    let mut digits = [b'0'; 10];
    let mut number = place;
    for index in (0..10).rev() {
        let digit = number.checked_rem(10).expect("nonzero base");
        let cell = digits.get_mut(index).expect("decimal position within ten digits");
        *cell = b'0'.checked_add(u8::try_from(digit).expect("decimal digit fits")).expect("digit fits");
        number = number.checked_div(10).expect("nonzero base");
    }
    let mut writer = Writer::new(15);
    writer.put(&digits).expect("ten digits fit");
    writer.put(b".turn").expect("turn suffix fits");
    writer.finish()
}

/// Encode one concrete turn for a durable numbered file.
pub fn save_turn(
    place: u32,
    read: Option<Token>,
    turn: &smith_domain::Turn,
    limits: &transcript::v3::Limits,
    endpoints: &Endpoints,
) -> Result<File, StoreError> {
    if place == 0 || place != turn.sequence {
        return Err(StoreError::Malformed);
    }
    let Ok(bytes) = channel::encode_turn(turn, limits, endpoints) else { return Err(StoreError::Turn) };
    let mut writer = Writer::new(TURN_HEADER.checked_add(bytes.len()).ok_or(StoreError::TooLarge)?);
    writer.put(b"SMLT").expect("bounded turn header");
    match read {
        Some(read) => {
            writer.put(&[1]).expect("bounded turn flag");
            writer.put(&read.raw().to_be_bytes()).expect("bounded read token");
        }
        None => {
            writer.put(&[0]).expect("bounded turn flag");
            writer.put(&0_u64.to_be_bytes()).expect("bounded read token");
        }
    }
    writer.put(&bytes).expect("bounded turn body");
    Ok(File { name: turn_name(place), bytes: writer.finish() })
}

/// Decode ordered numbered files as one concrete history before returning Loaded.
pub fn decode_turns(
    files: &[Box<[u8]>],
    limits: &transcript::v3::Limits,
    endpoints: &Endpoints,
) -> Result<(Option<Transcript>, Option<Token>), StoreError> {
    let Ok(count) = u32::try_from(files.len()) else { return Err(StoreError::TooLarge) };
    let mut bodies = List::with_capacity(count);
    let mut last_read = None;
    for file in files {
        let mut reader = Reader::new(file);
        if reader.bytes(4) != Some(&b"SMLT"[..]) {
            return Err(StoreError::Malformed);
        }
        let flag = reader.u8().ok_or(StoreError::Malformed)?;
        let raw = reader.u64().ok_or(StoreError::Malformed)?;
        last_read = match flag {
            0 if raw == 0 => None,
            1 => Some(Token::new(raw)),
            0 | 2..=u8::MAX => return Err(StoreError::Malformed),
        };
        bodies
            .push(Box::from(reader.bytes(reader.remaining()).ok_or(StoreError::Malformed)?))
            .expect("bounded turn count");
    }
    match channel::decode_transcript(bodies.as_slice(), limits, endpoints) {
        Ok(transcript) => Ok((transcript, last_read)),
        Err(_) => Err(StoreError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_metadata_round_trips_with_read_fence() {
        let state = ChatState { activation: 42, next_message: 9, read: Some(Token::new(8)) };
        let file = save_state(state);
        assert_eq!(file.name.as_ref(), b"state");
        assert_eq!(decode_state(&file.bytes), Ok(state));
        let truncated = file.bytes.get(..STATE_BYTES.saturating_sub(1)).expect("state has bytes");
        assert_eq!(decode_state(truncated), Err(StoreError::Malformed));
    }

    #[test]
    fn numeric_turn_names_sort_in_place_order() {
        assert_eq!(turn_name(1).as_ref(), b"0000000001.turn");
        assert_eq!(turn_name(42).as_ref(), b"0000000042.turn");
        assert!(turn_name(9) < turn_name(10));
    }

    #[test]
    fn one_saved_turn_reloads_as_concrete_history() {
        let mut entries = List::with_capacity(1);
        entries
            .push(channel::Endpoint { name: Box::from(&b"main"[..]), number: 2, dialect: 1, account: 0 })
            .expect("one endpoint");
        let endpoints = Endpoints::new(entries);
        let turn = smith_domain::Turn {
            version: smith_domain::session::record::VERSION,
            endpoint: smith_domain::session::llm::Endpoint(2),
            dialect: 1,
            sequence: 1,
            usage: smith_domain::session::llm::Usage::ZERO,
            spent: 0,
            messages: Box::new([]),
        };
        let file =
            save_turn(1, Some(Token::new(9)), &turn, &transcript::CEILINGS, &endpoints).expect("bounded saved turn");
        assert_eq!(file.name.as_ref(), b"0000000001.turn");
        let (loaded, read) = decode_turns(&[file.bytes], &transcript::CEILINGS, &endpoints).expect("valid transcript");
        let loaded = loaded.expect("one turn");
        assert_eq!(read, Some(Token::new(9)));
        assert_eq!(loaded.turns.as_ref(), &[turn]);
    }
}
