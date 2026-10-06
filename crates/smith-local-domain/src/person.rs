//! Lines, names and text shown to one terminal (domain/host.md, section 8).

use alloc::boxed::Box;
use skein_lib::{List, Token};
use smith_domain::{self as agent, session};

/// One person line whose saved name precedes delivery to the agent.
#[derive(Debug)]
pub(crate) struct Line {
    pub(crate) name: Token,
    pub(crate) text: Box<[u8]>,
}

/// Render assistant text blocks up to a UTF-8 boundary under the display cap.
pub(crate) fn turn_text(turn: &agent::Turn, cap: u32) -> Box<[u8]> {
    let mut shown = List::with_capacity(cap);
    for message in &turn.messages {
        match message.role {
            session::llm::Role::User => {}
            session::llm::Role::Assistant => {
                for block in &message.content {
                    match block {
                        session::llm::Block::Text { text, replay: _ }
                        | session::llm::Block::Refusal { text, replay: _ } => append_text(&mut shown, text),
                        session::llm::Block::Opaque { .. }
                        | session::llm::Block::ToolCall { .. }
                        | session::llm::Block::ToolResult { .. } => {}
                    }
                }
            }
        }
    }
    shown.into_boxed()
}

/// Copy attested UTF-8 text within the person's display cap.
pub(crate) fn bounded_text(text: &[u8], cap: u32) -> Box<[u8]> {
    let mut shown = List::with_capacity(cap);
    append_text(&mut shown, text);
    shown.into_boxed()
}

fn append_text(shown: &mut List<u8>, text: &[u8]) {
    let mut skip = 0_u8;
    for (index, byte) in text.iter().enumerate() {
        if skip > 0 {
            skip = skip.checked_sub(1).expect("continuation count is positive");
            continue;
        }
        let width = if *byte < 0x80 {
            1_usize
        } else if *byte < 0xe0 {
            2_usize
        } else if *byte < 0xf0 {
            3_usize
        } else {
            4_usize
        };
        let Some(bytes) = text.get(index..index.saturating_add(width)) else { return };
        if shown.room() < u32::try_from(width).expect("UTF-8 character width fits u32") {
            return;
        }
        for value in bytes {
            shown.push(*value).expect("character room checked before writing");
        }
        skip = u8::try_from(width.saturating_sub(1)).expect("UTF-8 character width fits u8");
    }
}
