//! The admitted message queue, one outstanding offer and the read fence.
//! The run calls admission, offering, told and unread; this module knows no
//! session or provider state (domain/run.md, section 6; protocol/limits.md, 3.9).

use alloc::boxed::Box;

use skein_lib::{Queue, Token};

use crate::facts::{FactKind, Facts};
use crate::{Limits, MessageRefusal};

/// Admitted messages in arrival order and their outstanding terminal rights.
#[derive(Debug)]
pub(crate) struct Inbox {
    queued: Queue<Queued>,
    offered: Queue<Token>,
    read: Option<Token>,
}

/// One rendered message retained until offered; the host's name remains opaque.
#[derive(Debug)]
pub(crate) struct Queued {
    name: Token,
    text: Box<[u8]>,
}

/// Whether the run still takes messages; ending begins once its answer is decided.
pub(crate) enum Entrance {
    /// The run may admit messages within its configured bounds.
    Open,

    /// Every further message ends refused.
    Ending,
}

/// The bytes a message takes when rendered as its label, colon, space and text.
#[must_use]
pub fn rendered_bytes(label: &[u8], text: &[u8]) -> Option<u64> {
    u64::try_from(label.len()).ok()?.checked_add(2)?.checked_add(u64::try_from(text.len()).ok()?)
}

impl Inbox {
    pub(crate) fn new(limits: &Limits) -> Inbox {
        Inbox {
            queued: Queue::with_capacity(limits.messages),
            offered: Queue::with_capacity(limits.messages),
            read: None,
        }
    }

    pub(crate) fn fence(&self) -> Option<Token> {
        self.read
    }

    pub(crate) fn has_queued(&self) -> bool {
        !self.queued.is_empty()
    }

    /// Move the admitted Start batch ahead of every later relayed message.
    pub(crate) fn carry(&mut self, facts: &mut Facts, token: Token, messages: Box<[crate::Message]>) {
        for message in messages {
            let bytes = message
                .label
                .len()
                .checked_add(2)
                .expect("validated label")
                .checked_add(message.text.len())
                .expect("validated rendered message");
            let mut writer = skein_lib::Writer::new(bytes);
            writer.put(&message.label).expect("measured label");
            writer.put(b": ").expect("measured separator");
            writer.put(&message.text).expect("measured text");
            self.queued.push(Queued { name: message.name, text: writer.finish() });
            facts.push(FactKind::MessageReceived {
                run: token,
                name: message.name,
                bytes: u64::try_from(bytes).expect("bounded message"),
            });
        }
    }

    /// Retain one relayed message, or refuse before retaining or rendering it.
    pub(crate) fn admit(
        &mut self,
        name: Token,
        label: Box<[u8]>,
        text: Box<[u8]>,
        entrance: Entrance,
        limits: &Limits,
    ) -> Result<u64, MessageRefusal> {
        let rendered = match label.len().checked_add(2) {
            Some(bytes) => bytes.checked_add(text.len()),
            None => None,
        };
        let bytes = match rendered {
            Some(bytes) => u64::try_from(bytes).expect("owned lengths fit"),
            None => u64::MAX,
        };
        let too_large = match rendered {
            Some(bytes) => u64::try_from(bytes).expect("owned lengths fit") > u64::from(limits.message_bytes),
            None => true,
        };
        let mut named = self.read == Some(name);
        for offered in &self.offered {
            if *offered == name {
                named = true;
            }
        }
        for queued in &self.queued {
            if queued.name == name {
                named = true;
            }
        }
        let held = self.queued.len().saturating_add(self.offered.len());
        let ending = match entrance {
            Entrance::Open => false,
            Entrance::Ending => true,
        };
        let reason = if ending {
            Some(MessageRefusal::Ending)
        } else if too_large {
            Some(MessageRefusal::TooLarge)
        } else if named {
            Some(MessageRefusal::NameInUse)
        } else if held >= limits.messages {
            Some(MessageRefusal::Full)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(reason);
        }
        let mut writer = skein_lib::Writer::new(rendered.expect("checked rendered size"));
        writer.put(&label).expect("checked rendered size");
        writer.put(b": ").expect("checked rendered size");
        writer.put(&text).expect("checked rendered size");
        let text = writer.finish();
        self.queued.push(Queued { name, text });
        Ok(bytes)
    }

    /// Build one ordered offer and retain each name until an actual told turn.
    pub(crate) fn offer(&mut self, limits: &Limits) -> Option<Box<[u8]>> {
        assert!(self.offered.is_empty(), "a previous offer ends at its told turn");
        let mut length = 0_usize;
        let mut count = 0_u32;
        for message in &self.queued {
            if count >= limits.offer_messages {
                break;
            }
            let separator = if count == 0 { 0 } else { 2 };
            let next = match length.checked_add(separator) {
                Some(bytes) => match bytes.checked_add(message.text.len()) {
                    Some(bytes) => bytes,
                    None => break,
                },
                None => break,
            };
            if next > usize::try_from(limits.offer_bytes).expect("receiving cap fits") {
                break;
            }
            length = next;
            count = count.checked_add(1).expect("bounded inbox count");
        }
        if count == 0 {
            return None;
        }
        let mut writer = skein_lib::Writer::new(length);
        for at in 0..count {
            let message = self.queued.pop().expect("measured ordered prefix");
            if at > 0 {
                writer.put(b"\n\n").expect("measured separators");
            }
            writer.put(&message.text).expect("measured payload");
            self.offered.push(message.name);
        }
        Some(writer.finish())
    }

    /// Settle each offered name as read by this told turn and advance the fence.
    pub(crate) fn told(&mut self, token: Token, turn: u32, facts: &mut Facts) {
        let offered = self.offered.len();
        for _ in 0..offered {
            let name = self.offered.pop().expect("bounded offered prefix");
            self.read = Some(name);
            facts.push(FactKind::MessageRead { run: token, name, turn });
        }
        facts.push(FactKind::MessageFence { run: token, turn, read: self.read });
    }

    /// End every outstanding message unread once the run's answer is decided.
    pub(crate) fn unread(&mut self, token: Token, facts: &mut Facts) {
        let offered = self.offered.len();
        for _ in 0..offered {
            let name = self.offered.pop().expect("retained offered prefix");
            facts.push(FactKind::MessageUnread { run: token, name });
        }
        let queued = self.queued.len();
        for _ in 0..queued {
            let message = self.queued.pop().expect("retained inbox prefix");
            facts.push(FactKind::MessageUnread { run: token, name: message.name });
        }
    }
}

pub(crate) fn messages_valid(messages: &[crate::Message], limits: &Limits) -> bool {
    if messages.len() > usize::try_from(limits.messages).expect("u32 fits") {
        return false;
    }
    for (at, message) in messages.iter().enumerate() {
        let length = match message.label.len().checked_add(2) {
            Some(bytes) => match bytes.checked_add(message.text.len()) {
                Some(bytes) => bytes,
                None => return false,
            },
            None => return false,
        };
        if length > usize::try_from(limits.message_bytes).expect("u32 fits") {
            return false;
        }
        for earlier in messages.get(..at).expect("enumerated prefix") {
            if earlier.name == message.name {
                return false;
            }
        }
    }
    true
}
