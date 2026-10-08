//! Lines, names and text shown to one terminal (domain/host.md, section 8).

use alloc::boxed::Box;
use skein_lib::{List, Token};
use smith_domain::{self as agent, session, tools};

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
                        session::llm::Block::ToolCall { name, .. } => {
                            append_text(&mut shown, b"\nTool: ");
                            append_text(&mut shown, name);
                            append_text(&mut shown, b"\n");
                        }
                        session::llm::Block::ToolResult { result, .. } => {
                            append_text(&mut shown, tool_end(result));
                        }
                        session::llm::Block::Opaque { .. } => {}
                    }
                }
            }
        }
    }
    shown.into_boxed()
}

fn tool_end(result: &session::llm::Returned) -> &'static [u8] {
    match result {
        session::llm::Returned::Owned { outcome } => match outcome {
            tools::Outcome::Read { .. }
            | tools::Outcome::Listed { .. }
            | tools::Outcome::Found { timed_out: false, .. }
            | tools::Outcome::Written { .. }
            | tools::Outcome::Edited { .. }
            | tools::Outcome::Exited { exit: tools::Exit::Code { code: 0 }, .. } => b"Tool finished\n",
            tools::Outcome::Found { timed_out: true, .. }
            | tools::Outcome::Exited { .. }
            | tools::Outcome::NotGranted
            | tools::Outcome::Outside
            | tools::Outcome::ReadOnly
            | tools::Outcome::TooLong
            | tools::Outcome::NotFound
            | tools::Outcome::NotFile
            | tools::Outcome::Linked
            | tools::Outcome::Protected
            | tools::Outcome::NotDirectory
            | tools::Outcome::TooLarge { .. }
            | tools::Outcome::NotRead
            | tools::Outcome::Stale
            | tools::Outcome::NoMatch
            | tools::Outcome::Ambiguous { .. }
            | tools::Outcome::Unchanged
            | tools::Outcome::Failed { .. }
            | tools::Outcome::TimedOut
            | tools::Outcome::Cancelled
            | tools::Outcome::Busy
            | tools::Outcome::NulByte => b"Tool failed\n",
        },
        session::llm::Returned::Text { error: false, .. } => b"Tool finished\n",
        session::llm::Returned::Text { error: true, .. }
        | session::llm::Returned::Invalid { .. }
        | session::llm::Returned::NotRun
        | session::llm::Returned::Withdrawn => b"Tool failed\n",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_shows_the_tool_name_and_its_terminal() {
        let turn = agent::Turn {
            version: 1,
            endpoint: session::llm::Endpoint(1),
            dialect: 1,
            sequence: 1,
            usage: session::llm::Usage::ZERO,
            spent: 0,
            messages: Box::new([session::llm::Message {
                role: session::llm::Role::Assistant,
                content: Box::new([
                    session::llm::Block::Text { text: Box::from(&b"Working"[..]), replay: None },
                    session::llm::Block::ToolCall {
                        id: Box::from(&b"call-1"[..]),
                        name: Box::from(&b"inspect"[..]),
                        input: Box::from(&b"{}"[..]),
                        call: session::llm::Decoded::Historical,
                        replay: None,
                    },
                    session::llm::Block::ToolResult {
                        id: Box::from(&b"call-1"[..]),
                        result: session::llm::Returned::Text {
                            text: Box::from(&b"done"[..]),
                            error: false,
                            replay: None,
                        },
                    },
                ]),
            }]),
        };
        assert_eq!(turn_text(&turn, 128).as_ref(), b"Working\nTool: inspect\nTool finished\n");
    }
}
