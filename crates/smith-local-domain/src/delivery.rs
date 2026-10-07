//! One in-place delivery, in workspace order (domain/host.md, section 8;
//! domain/run.md, sections 8.2–8.4). It keeps the child call's name, bounded
//! message, survey and receipts while IO decides git state. It never reads a
//! tree or knows git syntax. The parent saves the intent before the first git
//! write, then saves the answer before returning it to the child.

use alloc::boxed::Box;
use skein_lib::{List, Map, Time, Token};
use smith_domain::run::{self, outcome};

use crate::{DeliveryRecord, IntentDirectory};

/// Return the first durable answer for a repeated call name.
pub(crate) fn cached(records: &Map<run::CallName, DeliveryRecord>, name: run::CallName) -> Option<run::Delivery> {
    records.get(&name)?.answer(name)
}

/// Operation awaited from the caller, or the durable store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    Plain,
    Status,
    Markers,
    Commit,
    Push,
}

/// Survey reads every writable directory before a commit; execution uses that snapshot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Stage {
    Survey,
    Execute,
}

/// One child delivery call and the directories already made durable.
#[derive(Debug)]
pub(crate) struct InPlace {
    pub(crate) name: run::CallName,
    pub(crate) owner: Token,
    pub(crate) deadline: Time,
    pub(crate) next: u32,
    pub(crate) receipts: List<run::Receipt>,
    pub(crate) message: Box<[u8]>,
    pub(crate) step: Step,
    pub(crate) stage: Stage,
    pub(crate) directories: List<IntentDirectory>,
    pub(crate) marker_head: Option<Box<[u8]>>,
}

impl InPlace {
    pub(crate) fn new(name: run::CallName, owner: Token, deadline: Time, message: Box<[u8]>) -> Self {
        Self {
            name,
            owner,
            deadline,
            next: 0,
            receipts: List::with_capacity(run::MAX_DIRECTORIES),
            message,
            step: Step::Status,
            stage: Stage::Survey,
            directories: List::with_capacity(run::MAX_DIRECTORIES),
            marker_head: None,
        }
    }

    pub(crate) fn receipt(&mut self, directory: u32, text: Box<[u8]>) {
        let receipt = run::Receipt::new(directory, text).expect("bounded host receipt");
        self.receipts.push(receipt).expect("admitted directory count fits receipts");
    }

    pub(crate) fn directory(&mut self, directory: u32, changed: bool, head: Option<Box<[u8]>>) {
        self.directories.push(IntentDirectory { directory, changed, head }).expect("admitted directory count");
    }

    pub(crate) fn result(self) -> run::Delivery {
        let receipts = self.receipts.into_boxed();
        if receipts.is_empty() {
            run::Delivery::Nothing
        } else {
            run::Delivery::Delivered(run::Delivered::new(receipts).expect("ordered unique writable directories"))
        }
    }
}

/// Render the host's configured title first, then all other declared fields
/// in their result order. The run already bounded each field and their total.
#[must_use]
pub fn commit_message(change: &outcome::Change, title_field: &[u8], rules: &[outcome::FieldRule]) -> Option<Box<[u8]>> {
    let mut title = None;
    let mut size = 0_usize;
    for field in &change.fields {
        if field.name.as_ref() == title_field {
            title = Some(&field.value);
        }
        size = size.checked_add(field.value.len())?.checked_add(2)?;
    }
    let title = title?;
    if title.is_empty() {
        return None;
    }
    let cap = u32::try_from(size).ok()?;
    let mut bytes = List::with_capacity(cap);
    append(&mut bytes, title);
    for rule in rules {
        if rule.name.as_ref() == title_field {
            continue;
        }
        for field in &change.fields {
            if field.name == rule.name {
                append(&mut bytes, b"\n\n");
                append(&mut bytes, &field.value);
            }
        }
    }
    for field in &change.fields {
        let mut required = false;
        for rule in rules {
            if field.name == rule.name {
                required = true;
            }
        }
        if field.name.as_ref() != title_field && !required {
            append(&mut bytes, b"\n\n");
            append(&mut bytes, &field.value);
        }
    }
    Some(bytes.into_boxed())
}

fn append(out: &mut List<u8>, bytes: &[u8]) {
    for byte in bytes {
        out.push(*byte).expect("precomputed message bound");
    }
}

/// Append a delivery key as a git trailer, so a committed effect can be found.
pub(crate) fn named_message(message: Box<[u8]>, name: run::CallName) -> Option<Box<[u8]>> {
    let capacity = u32::try_from(message.len()).ok()?.checked_add(80)?;
    let mut out = List::with_capacity(capacity);
    append(&mut out, &message);
    append(&mut out, b"\n\nSmith-Delivery: ");
    decimal(&mut out, name.activation);
    append(&mut out, b"/");
    decimal(&mut out, u64::from(name.completion));
    append(&mut out, b"/");
    decimal(&mut out, u64::from(name.position));
    Some(out.into_boxed())
}

/// Append decimal digits without allocating outside the caller's bound.
pub(crate) fn decimal(out: &mut List<u8>, mut number: u64) {
    let mut digits = [0_u8; 20];
    let mut count = 0_usize;
    for _ in 0_u8..20_u8 {
        *digits.get_mut(count).expect("twenty decimal digits fit") =
            b'0'.checked_add(u8::try_from(number % 10).expect("one digit")).expect("ascii digit");
        count = count.checked_add(1).expect("twenty digits fit");
        number /= 10;
        if number == 0 {
            break;
        }
    }
    for digit in digits.get(..count).expect("twenty digits fit").iter().rev() {
        if out.room() == 0 {
            break;
        }
        out.push(*digit).expect("room checked");
    }
}

/// Keep an interrupted-delivery diagnostic within the sealed 512-byte tail.
pub(crate) fn append_bounded(out: &mut List<u8>, bytes: &[u8]) {
    for byte in bytes {
        if out.room() == 0 {
            break;
        }
        out.push(*byte).expect("room checked");
    }
}

#[cfg(test)]
mod tests {
    use super::{cached, commit_message};
    use crate::DeliveryRecord;
    use smith_domain::run::outcome::{Change, Field, FieldRule};
    use smith_domain::run::{CallName, Delivery};

    #[test]
    fn the_configured_title_precedes_other_result_fields() {
        let change = Change {
            fields: Box::new([
                Field { name: b"body".as_slice().into(), value: b"Second".as_slice().into() },
                Field { name: b"title".as_slice().into(), value: b"First".as_slice().into() },
                Field { name: b"context".as_slice().into(), value: b"Third".as_slice().into() },
            ]),
        };
        let rules = [
            FieldRule { name: b"title".as_slice().into(), max: 32 },
            FieldRule { name: b"body".as_slice().into(), max: 32 },
        ];
        assert_eq!(commit_message(&change, b"title", &rules).as_deref(), Some(b"First\n\nSecond\n\nThird".as_slice()));
        assert_eq!(commit_message(&change, b"missing", &rules), None);
    }

    #[test]
    fn a_delivery_asked_again_under_its_name_gets_its_first_answer() {
        let name = CallName { activation: 2, completion: 3, position: 1 };
        let record =
            DeliveryRecord { name, state: crate::DeliveryState::Answer(Delivery::Nothing), landed: Box::new([]) };
        let mut records = skein_lib::Map::with_capacity(1);
        records.insert(name, record).expect("one entry");
        assert_eq!(cached(&records, name), Some(Delivery::Nothing));
        assert_eq!(cached(&records, CallName { activation: 2, completion: 4, position: 1 }), None);
    }
}
