//! The durable pre-effect intent and post-effect decision for a local
//! delivery (protocol/hosts.md, sections 5.3 and 5.5; domain/host.md,
//! section 2). Neither this codec nor its caller may discard an intent after
//! a crash: the local domain reconciles it before the next run. The file is
//! replaced through Skein's synced whole-file store before the effect starts
//! or its terminal is told to the agent.

use alloc::boxed::Box;
use skein_lib::{List, Reader};
use smith_domain::run;
use smith_local_domain::{DeliveryIntent, DeliveryRecord, DeliveryState, IntentDirectory, PushTarget};

use crate::{File, StoreError};

const MAGIC: &[u8] = b"SMLD\x01";

/// Encode a bounded delivery record under its stable call name.
pub fn save_delivery(record: &DeliveryRecord, max_bytes: u32) -> Result<File, StoreError> {
    let mut out = List::with_capacity(max_bytes);
    put(&mut out, MAGIC)?;
    put_u64(&mut out, record.name.activation)?;
    put_u32(&mut out, record.name.completion)?;
    put_u32(&mut out, record.name.position)?;
    match &record.state {
        DeliveryState::Intent(intent) => {
            put_u8(&mut out, 0)?;
            put_intent(&mut out, intent)?;
        }
        DeliveryState::Answer(answer) => {
            put_u8(&mut out, 1)?;
            put_answer(&mut out, answer)?;
        }
    }
    put_receipts(&mut out, &record.landed)?;
    Ok(File { name: delivery_name(record.name), bytes: out.into_boxed() })
}

/// Decode one complete bounded delivery record after a durable load.
pub fn decode_delivery(bytes: &[u8], max_bytes: u32) -> Result<DeliveryRecord, StoreError> {
    if bytes.len() > usize::try_from(max_bytes).expect("u32 fits usize") {
        return Err(StoreError::TooLarge);
    }
    let mut reader = Reader::new(bytes);
    if reader.bytes(5) != Some(MAGIC) {
        return Err(StoreError::Malformed);
    }
    let name = run::CallName {
        activation: number_u64(&mut reader)?,
        completion: number_u32(&mut reader)?,
        position: number_u32(&mut reader)?,
    };
    if name.activation == 0 || name.completion == 0 {
        return Err(StoreError::Malformed);
    }
    let state = match number_u8(&mut reader)? {
        0 => DeliveryState::Intent(read_intent(&mut reader)?),
        1 => DeliveryState::Answer(read_answer(&mut reader)?),
        2..=u8::MAX => return Err(StoreError::Malformed),
    };
    let landed = read_receipts(&mut reader)?;
    if !reader.is_empty() {
        return Err(StoreError::Malformed);
    }
    Ok(DeliveryRecord { name, state, landed })
}

fn delivery_name(name: run::CallName) -> Box<[u8]> {
    let mut out = [b'0'; 42];
    let mut cursor = 0_usize;
    decimal(&mut out, &mut cursor, name.activation, 20);
    let dot = out.get_mut(cursor).expect("name separator position");
    *dot = b'.';
    cursor = cursor.saturating_add(1);
    decimal(&mut out, &mut cursor, u64::from(name.completion), 10);
    let dot = out.get_mut(cursor).expect("name separator position");
    *dot = b'.';
    cursor = cursor.saturating_add(1);
    decimal(&mut out, &mut cursor, u64::from(name.position), 10);
    Box::from(out)
}

fn decimal(out: &mut [u8], cursor: &mut usize, value: u64, width: usize) {
    let mut number = value;
    let end = cursor.saturating_add(width);
    for index in (*cursor..end).rev() {
        let cell = out.get_mut(index).expect("fixed decimal slot");
        *cell = b'0'.checked_add(u8::try_from(number % 10).expect("digit fits")).expect("ASCII digit fits");
        number /= 10;
    }
    *cursor = end;
}

fn put(out: &mut List<u8>, bytes: &[u8]) -> Result<(), StoreError> {
    for byte in bytes {
        if out.push(*byte).is_err() {
            return Err(StoreError::TooLarge);
        }
    }
    Ok(())
}

fn put_u8(out: &mut List<u8>, value: u8) -> Result<(), StoreError> {
    put(out, &[value])
}
fn put_u32(out: &mut List<u8>, value: u32) -> Result<(), StoreError> {
    put(out, &value.to_be_bytes())
}
fn put_u64(out: &mut List<u8>, value: u64) -> Result<(), StoreError> {
    put(out, &value.to_be_bytes())
}

fn put_bytes(out: &mut List<u8>, bytes: &[u8]) -> Result<(), StoreError> {
    let Ok(len) = u32::try_from(bytes.len()) else { return Err(StoreError::TooLarge) };
    put_u32(out, len)?;
    put(out, bytes)
}

fn put_optional(out: &mut List<u8>, value: Option<&[u8]>) -> Result<(), StoreError> {
    match value {
        Some(bytes) => {
            put_u8(out, 1)?;
            put_bytes(out, bytes)
        }
        None => put_u8(out, 0),
    }
}

fn put_intent(out: &mut List<u8>, intent: &DeliveryIntent) -> Result<(), StoreError> {
    put_count(out, intent.directories.len())?;
    for entry in &intent.directories {
        put_u32(out, entry.directory)?;
        put_u8(out, u8::from(entry.changed))?;
        put_optional(out, entry.head.as_deref())?;
        match &entry.push {
            Some(push) => {
                put_u8(out, 1)?;
                put_bytes(out, &push.remote)?;
                put_bytes(out, &push.branch)?;
            }
            None => put_u8(out, 0)?,
        }
    }
    Ok(())
}

fn put_receipts(out: &mut List<u8>, receipts: &[run::Receipt]) -> Result<(), StoreError> {
    put_count(out, receipts.len())?;
    for receipt in receipts {
        put_u32(out, receipt.directory())?;
        put_bytes(out, receipt.text())?;
    }
    Ok(())
}

fn put_answer(out: &mut List<u8>, answer: &run::Delivery) -> Result<(), StoreError> {
    match answer {
        run::Delivery::Delivered(delivered) => {
            put_u8(out, 0)?;
            put_receipts(out, delivered.receipts())
        }
        run::Delivery::Nothing => put_u8(out, 1),
        run::Delivery::Refused(refused) => {
            put_u8(out, 2)?;
            match refused.marker() {
                Some(marker) => {
                    put_u8(out, 1)?;
                    put_u32(out, marker.directory())?;
                    put_bytes(out, marker.path())?;
                }
                None => put_u8(out, 0)?,
            }
            put_bytes(out, refused.explanation())
        }
        run::Delivery::Failed(failed) => {
            put_u8(out, 3)?;
            put_u32(out, failed.directory)?;
            put_u8(out, reason_tag(failed.reason))?;
            put_bytes(out, failed.diagnostic.output())?;
            put_u64(out, failed.diagnostic.cut())
        }
        run::Delivery::Stale => put_u8(out, 4),
    }
}

fn reason_tag(reason: run::DeliveryReason) -> u8 {
    match reason {
        run::DeliveryReason::Unreachable => 0,
        run::DeliveryReason::RefusedByTarget => 1,
        run::DeliveryReason::TimedOut => 2,
        run::DeliveryReason::Broken => 3,
        run::DeliveryReason::TooLarge => 4,
        run::DeliveryReason::Missing => 5,
        run::DeliveryReason::Busy => 6,
        run::DeliveryReason::Unavailable => 7,
        run::DeliveryReason::Cancelled => 8,
        run::DeliveryReason::Unknown => 9,
    }
}

fn read_reason(tag: u8) -> Result<run::DeliveryReason, StoreError> {
    match tag {
        0 => Ok(run::DeliveryReason::Unreachable),
        1 => Ok(run::DeliveryReason::RefusedByTarget),
        2 => Ok(run::DeliveryReason::TimedOut),
        3 => Ok(run::DeliveryReason::Broken),
        4 => Ok(run::DeliveryReason::TooLarge),
        5 => Ok(run::DeliveryReason::Missing),
        6 => Ok(run::DeliveryReason::Busy),
        7 => Ok(run::DeliveryReason::Unavailable),
        8 => Ok(run::DeliveryReason::Cancelled),
        9 => Ok(run::DeliveryReason::Unknown),
        10..=u8::MAX => Err(StoreError::Malformed),
    }
}

fn put_count(out: &mut List<u8>, count: usize) -> Result<(), StoreError> {
    if count > usize::try_from(run::MAX_DIRECTORIES).expect("fixed directory cap fits") {
        return Err(StoreError::TooLarge);
    }
    put_u32(out, u32::try_from(count).expect("bounded count fits"))
}

fn number_u8(reader: &mut Reader<'_>) -> Result<u8, StoreError> {
    reader.u8().ok_or(StoreError::Malformed)
}
fn number_u32(reader: &mut Reader<'_>) -> Result<u32, StoreError> {
    reader.u32().ok_or(StoreError::Malformed)
}
fn number_u64(reader: &mut Reader<'_>) -> Result<u64, StoreError> {
    reader.u64().ok_or(StoreError::Malformed)
}
fn read_bytes(reader: &mut Reader<'_>) -> Result<Box<[u8]>, StoreError> {
    let count = number_u32(reader)?;
    Ok(Box::from(reader.bytes(count).ok_or(StoreError::Malformed)?))
}
fn read_optional(reader: &mut Reader<'_>) -> Result<Option<Box<[u8]>>, StoreError> {
    match number_u8(reader)? {
        0 => Ok(None),
        1 => Ok(Some(read_bytes(reader)?)),
        2..=u8::MAX => Err(StoreError::Malformed),
    }
}

fn read_count(reader: &mut Reader<'_>) -> Result<u32, StoreError> {
    let count = number_u32(reader)?;
    if count > run::MAX_DIRECTORIES {
        return Err(StoreError::Malformed);
    }
    Ok(count)
}

fn read_intent(reader: &mut Reader<'_>) -> Result<DeliveryIntent, StoreError> {
    let count = read_count(reader)?;
    let mut directories = List::with_capacity(count);
    let mut previous = None;
    for _ in 0..count {
        let directory = number_u32(reader)?;
        if directory >= run::MAX_DIRECTORIES || !increasing(previous, directory) {
            return Err(StoreError::Malformed);
        }
        previous = Some(directory);
        let changed = match number_u8(reader)? {
            0 => false,
            1 => true,
            2..=u8::MAX => return Err(StoreError::Malformed),
        };
        let head = read_optional(reader)?;
        match &head {
            Some(bytes) if bytes.is_empty() || bytes.len() > run::Receipt::CAPACITY => {
                return Err(StoreError::Malformed);
            }
            Some(_) | None => {}
        }
        let push = match number_u8(reader)? {
            0 => None,
            1 => Some(PushTarget { remote: read_bytes(reader)?, branch: read_bytes(reader)? }),
            2..=u8::MAX => return Err(StoreError::Malformed),
        };
        directories.push(IntentDirectory { directory, changed, head, push }).expect("decoded count reserved");
    }
    Ok(DeliveryIntent { directories: directories.into_boxed() })
}

fn read_receipts(reader: &mut Reader<'_>) -> Result<Box<[run::Receipt]>, StoreError> {
    let count = read_count(reader)?;
    let mut receipts = List::with_capacity(count);
    let mut previous = None;
    for _ in 0..count {
        let directory = number_u32(reader)?;
        if !increasing(previous, directory) {
            return Err(StoreError::Malformed);
        }
        previous = Some(directory);
        let text = read_bytes(reader)?;
        let receipt = run::Receipt::new(directory, text).ok_or(StoreError::Malformed)?;
        receipts.push(receipt).expect("decoded count reserved");
    }
    Ok(receipts.into_boxed())
}

fn increasing(previous: Option<u32>, current: u32) -> bool {
    match previous {
        Some(prior) => prior < current,
        None => true,
    }
}

fn read_answer(reader: &mut Reader<'_>) -> Result<run::Delivery, StoreError> {
    match number_u8(reader)? {
        0 => Ok(run::Delivery::Delivered(run::Delivered::new(read_receipts(reader)?).ok_or(StoreError::Malformed)?)),
        1 => Ok(run::Delivery::Nothing),
        2 => {
            let marker = match number_u8(reader)? {
                0 => None,
                1 => Some(run::Marker::new(number_u32(reader)?, read_bytes(reader)?).ok_or(StoreError::Malformed)?),
                2..=u8::MAX => return Err(StoreError::Malformed),
            };
            let explanation = read_bytes(reader)?;
            Ok(run::Delivery::Refused(run::DeliveryRefusal::new(marker, explanation).ok_or(StoreError::Malformed)?))
        }
        3 => {
            let directory = number_u32(reader)?;
            let reason = read_reason(number_u8(reader)?)?;
            let output = read_bytes(reader)?;
            if output.len() > run::Diagnostic::CAPACITY {
                return Err(StoreError::Malformed);
            }
            let cut = number_u64(reader)?;
            Ok(run::Delivery::Failed(run::DeliveryFailure {
                directory,
                reason,
                diagnostic: run::Diagnostic::new(&output, cut),
            }))
        }
        4 => Ok(run::Delivery::Stale),
        5..=u8::MAX => Err(StoreError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name() -> run::CallName {
        run::CallName { activation: 7, completion: 3, position: 1 }
    }

    #[test]
    fn pre_effect_intent_survives_file_round_trip() {
        let record = DeliveryRecord {
            name: name(),
            state: DeliveryState::Intent(DeliveryIntent {
                directories: Box::new([
                    IntentDirectory {
                        directory: 0,
                        changed: true,
                        head: Some(Box::from(&b"abc"[..])),
                        push: Some(PushTarget { remote: Box::from(&b"origin"[..]), branch: Box::from(&b"main"[..]) }),
                    },
                    IntentDirectory { directory: 1, changed: true, head: None, push: None },
                ]),
            }),
            landed: Box::new([]),
        };
        let file = save_delivery(&record, 4096).expect("bounded intent");
        assert_eq!(save_delivery(&record, 8), Err(StoreError::TooLarge));
        assert_eq!(decode_delivery(&file.bytes, 4096), Ok(record));
    }

    #[test]
    fn landed_answer_and_receipts_survive_file_round_trip() {
        let receipt = run::Receipt::new(0, Box::from(&b"commit abc"[..])).expect("bounded receipt");
        let record = DeliveryRecord {
            name: name(),
            state: DeliveryState::Answer(run::Delivery::Delivered(
                run::Delivered::new(Box::new([receipt.clone()])).expect("one receipt"),
            )),
            landed: Box::new([receipt]),
        };
        let file = save_delivery(&record, 4096).expect("bounded answer");
        assert_eq!(decode_delivery(&file.bytes, 4096), Ok(record));
        assert_eq!(decode_delivery(&file.bytes, 8), Err(StoreError::TooLarge));
    }

    #[test]
    fn failed_answer_keeps_diagnostic_cut_and_refusal_keeps_marker() {
        let failed = DeliveryRecord {
            name: name(),
            state: DeliveryState::Answer(run::Delivery::Failed(run::DeliveryFailure {
                directory: 1,
                reason: run::DeliveryReason::TimedOut,
                diagnostic: run::Diagnostic::new(b"last bytes", 42),
            })),
            landed: Box::new([]),
        };
        let file = save_delivery(&failed, 4096).expect("bounded failure");
        assert_eq!(decode_delivery(&file.bytes, 4096), Ok(failed));

        let refused = DeliveryRecord {
            name: name(),
            state: DeliveryState::Answer(run::Delivery::Refused(
                run::DeliveryRefusal::new(
                    Some(run::Marker::new(0, Box::from(&b"src/main.rs"[..])).expect("relative marker")),
                    Box::from(&b"markers remain"[..]),
                )
                .expect("bounded refusal"),
            )),
            landed: Box::new([]),
        };
        let file = save_delivery(&refused, 4096).expect("bounded refusal");
        assert_eq!(decode_delivery(&file.bytes, 4096), Ok(refused));
    }
}
