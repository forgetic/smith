//! Record-size bounds derived from content lengths and list cardinalities.
//! Contract: protocol/limits.md, section 3.7; protocol/events.md, section 5.3.

use crate::records::{Block, Capture, Count, Event, Family, Field, Form, Message};
use alloc::boxed::Box;
use skein_lib::{List, Queue};

/// Maximum UTF-8 bytes in each metadata string, content field, and entries in each list.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// UTF-8 bytes in one metadata string or unknown listed value.
    pub string: u32,
    /// UTF-8 bytes in one captured content field.
    pub content: u32,
    /// Entries in each list, including prompt messages and their blocks.
    pub items: u32,
}

/// Largest encoded line of this record kind under the capture policy; overflow is refused.
#[must_use]
pub fn largest_size(event: &Event, capture: &Capture, limits: &Limits) -> Option<u32> {
    let length = match event {
        Event::SessionStarted(_) => size_session_started(capture, limits)?.checked_add(68)?,
        Event::SessionEnded(_) => size_session_ended(capture, limits)?.checked_add(66)?,
        Event::RunStarted(_) => size_run_started(capture, limits)?.checked_add(64)?,
        Event::RunCompleted(_) => size_run_completed(capture, limits)?.checked_add(66)?,
        Event::ConversationOpened(_) => size_conversation_opened(capture, limits)?.checked_add(72)?,
        Event::ConversationClosed(_) => size_conversation_closed(capture, limits)?.checked_add(72)?,
        Event::ResponseStarted(_) => size_response_started(capture, limits)?.checked_add(69)?,
        Event::ResponseCompleted(_) => size_response_completed(capture, limits)?.checked_add(71)?,
        Event::TextDelta(_) => size_text_delta(capture, limits)?.checked_add(63)?,
        Event::ToolStarted(_) => size_tool_started(capture, limits)?.checked_add(65)?,
        Event::ToolCompleted(_) => size_tool_completed(capture, limits)?.checked_add(67)?,
        Event::CheckStarted(_) => size_check_started(capture, limits)?.checked_add(66)?,
        Event::CheckCompleted(_) => size_check_completed(capture, limits)?.checked_add(68)?,
        Event::Notice(_) => size_notice(capture, limits)?.checked_add(59)?,
    };
    u32::try_from(length).ok()
}

/// Largest encoded line over all record kinds under full capture.
#[must_use]
pub fn largest_record(limits: &Limits) -> Option<u32> {
    let capture = &Capture::Everything;
    let mut length = 0_u64;
    length = length.max(size_session_started(capture, limits)?.checked_add(68)?);
    length = length.max(size_session_ended(capture, limits)?.checked_add(66)?);
    length = length.max(size_run_started(capture, limits)?.checked_add(64)?);
    length = length.max(size_run_completed(capture, limits)?.checked_add(66)?);
    length = length.max(size_conversation_opened(capture, limits)?.checked_add(72)?);
    length = length.max(size_conversation_closed(capture, limits)?.checked_add(72)?);
    length = length.max(size_response_started(capture, limits)?.checked_add(69)?);
    length = length.max(size_response_completed(capture, limits)?.checked_add(71)?);
    length = length.max(size_text_delta(capture, limits)?.checked_add(63)?);
    length = length.max(size_tool_started(capture, limits)?.checked_add(65)?);
    length = length.max(size_tool_completed(capture, limits)?.checked_add(67)?);
    length = length.max(size_check_started(capture, limits)?.checked_add(66)?);
    length = length.max(size_check_completed(capture, limits)?.checked_add(68)?);
    length = length.max(size_notice(capture, limits)?.checked_add(59)?);
    u32::try_from(length).ok()
}

/// Peak heap for parsing and encoding, including the line, owned record, and bounded buffers.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let length = largest_record(limits)?;
    let json = skein_json::tokenizer::Limits {
        depth: 16,
        string: limits.string.max(limits.content).max(64),
        number: 20,
        chunk: 256,
        length,
    };
    let writer = skein_json::writer::Limits { depth: 16, length };
    let mut record = 0_u64;
    record = record.max(heap_session_started(limits)?);
    record = record.max(heap_session_ended(limits)?);
    record = record.max(heap_run_started(limits)?);
    record = record.max(heap_run_completed(limits)?);
    record = record.max(heap_conversation_opened(limits)?);
    record = record.max(heap_conversation_closed(limits)?);
    record = record.max(heap_response_started(limits)?);
    record = record.max(heap_response_completed(limits)?);
    record = record.max(heap_text_delta(limits)?);
    record = record.max(heap_tool_started(limits)?);
    record = record.max(heap_tool_completed(limits)?);
    record = record.max(heap_check_completed(limits)?);
    record = record.max(heap_notice(limits)?);
    record
        .checked_add(List::<skein_json::Token>::worst_case(length)?)?
        .checked_add(u64::from(length).checked_mul(3)?)?
        .checked_add(skein_json::tokenizer::worst_case(&json)?)?
        .checked_add(skein_json::writer::worst_case(&writer)?)?
        .checked_add(Queue::<skein_json::tokenizer::Event>::worst_case(1)?)?
        .checked_add(Queue::<skein_lib::stream::Down>::worst_case(1)?)
}

fn heap_agent(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    Some(length)
}

fn heap_count(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    Some(length)
}

fn heap_model(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    Some(length)
}

fn heap_tools(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(
        List::<Family>::worst_case(limits.items)?
            .checked_add((u64::from(limits.string)).checked_mul(u64::from(limits.items))?)?,
    )?;
    length = length.checked_add(
        List::<Box<[u8]>>::worst_case(limits.items)?
            .checked_add((u64::from(limits.string)).checked_mul(u64::from(limits.items))?)?,
    )?;
    Some(length)
}

fn heap_completion_failure(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    Some(length)
}

fn heap_answer_failure(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(heap_completion_failure(limits)?)?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    Some(length)
}

fn heap_field(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.content))?;
    Some(length)
}

fn heap_run_result(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.content))?;
    length = length.checked_add(
        List::<Field>::worst_case(limits.items)?
            .checked_add((heap_field(limits)?).checked_mul(u64::from(limits.items))?)?,
    )?;
    Some(length)
}

fn heap_message(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(
        List::<Block>::worst_case(limits.items)?
            .checked_add((heap_block(limits)?).checked_mul(u64::from(limits.items))?)?,
    )?;
    Some(length)
}

fn heap_prompt(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.content))?;
    length = length.checked_add(
        List::<Box<[u8]>>::worst_case(limits.items)?
            .checked_add((u64::from(limits.string)).checked_mul(u64::from(limits.items))?)?,
    )?;
    length = length.checked_add(
        List::<Message>::worst_case(limits.items)?
            .checked_add((heap_message(limits)?).checked_mul(u64::from(limits.items))?)?,
    )?;
    Some(length)
}

fn heap_completion(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(
        List::<Block>::worst_case(limits.items)?
            .checked_add((heap_block(limits)?).checked_mul(u64::from(limits.items))?)?,
    )?;
    Some(length)
}

fn heap_session_started(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(heap_agent(limits)?)?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(0_u64)?;
    Some(length)
}

fn heap_session_ended(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(
        List::<Count>::worst_case(limits.items)?
            .checked_add((heap_count(limits)?).checked_mul(u64::from(limits.items))?)?,
    )?;
    length = length.checked_add(0_u64)?;
    length = length.checked_add(0_u64)?;
    length = length.checked_add(0_u64)?;
    Some(length)
}

fn heap_run_started(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(heap_model(limits)?)?;
    length = length.checked_add(0_u64)?;
    length = length.checked_add(heap_tools(limits)?)?;
    length = length.checked_add(
        List::<Form>::worst_case(limits.items)?
            .checked_add((u64::from(limits.string)).checked_mul(u64::from(limits.items))?)?,
    )?;
    Some(length)
}

fn heap_run_completed(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(heap_answer_failure(limits)?)?;
    length = length.checked_add(heap_run_result(limits)?)?;
    length = length.checked_add(0_u64)?;
    Some(length)
}

fn heap_conversation_opened(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(heap_model(limits)?)?;
    Some(length)
}

fn heap_conversation_closed(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(heap_completion_failure(limits)?)?;
    length = length.checked_add(0_u64)?;
    Some(length)
}

fn heap_response_started(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(heap_model(limits)?)?;
    length = length.checked_add(heap_prompt(limits)?)?;
    Some(length)
}

fn heap_response_completed(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(0_u64)?;
    length = length.checked_add(heap_completion_failure(limits)?)?;
    length = length.checked_add(heap_completion(limits)?)?;
    Some(length)
}

fn heap_text_delta(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.content))?;
    Some(length)
}

fn heap_tool_started(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.content))?;
    Some(length)
}

fn heap_tool_completed(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(0_u64)?;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.content))?;
    Some(length)
}

fn heap_check_completed(_limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(0_u64)?;
    Some(length)
}

fn heap_notice(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    length = length.checked_add(u64::from(limits.string))?;
    length = length.checked_add(u64::from(limits.string))?;
    Some(length)
}

fn heap_block(limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    {
        let mut candidate = 0_u64;
        candidate = candidate.checked_add(u64::from(limits.content))?;
        length = length.max(candidate);
    }
    {
        let mut candidate = 0_u64;
        candidate = candidate.checked_add(u64::from(limits.content))?;
        length = length.max(candidate);
    }
    {
        let mut candidate = 0_u64;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        candidate = candidate.checked_add(u64::from(limits.content))?;
        length = length.max(candidate);
    }
    {
        let mut candidate = 0_u64;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        length = length.max(candidate);
    }
    {
        let mut candidate = 0_u64;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        candidate = candidate.checked_add(u64::from(limits.content))?;
        length = length.max(candidate);
    }
    {
        let mut candidate = 0_u64;
        candidate = candidate.checked_add(u64::from(limits.string))?;
        candidate = candidate.checked_add(u64::from(limits.content))?;
        length = length.max(candidate);
    }
    {
        let candidate = 0_u64;
        length = length.max(candidate);
    }
    Some(length)
}

fn size_capture(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_delivery(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(11)).checked_mul(6)?.checked_add(2)
}

fn size_mode(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(5)).checked_mul(6)?.checked_add(2)
}

fn size_family(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_form(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(7)).checked_mul(6)?.checked_add(2)
}

fn size_status(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(8)).checked_mul(6)?.checked_add(2)
}

fn size_conversation_kind(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_conversation_end(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_outcome(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(9)).checked_mul(6)?.checked_add(2)
}

fn size_stop(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_source(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(9)).checked_mul(6)?.checked_add(2)
}

fn size_effect(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(5)).checked_mul(6)?.checked_add(2)
}

fn size_verdict(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(9)).checked_mul(6)?.checked_add(2)
}

fn size_delivered(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(9)).checked_mul(6)?.checked_add(2)
}

fn size_failure_class(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(16)).checked_mul(6)?.checked_add(2)
}

fn size_evidence(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_answer_class(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(10)).checked_mul(6)?.checked_add(2)
}

fn size_level(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(7)).checked_mul(6)?.checked_add(2)
}

fn size_notice_kind(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(19)).checked_mul(6)?.checked_add(2)
}

fn size_role(_capture: &Capture, limits: &Limits) -> Option<u64> {
    u64::from(limits.string.max(9)).checked_mul(6)?.checked_add(2)
}

fn size_agent(_capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(11)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(9)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    Some(length)
}

fn size_versions(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(10)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(14)?.checked_add(20_u64)?;
    Some(length)
}

fn size_loss(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(10)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    Some(length)
}

fn size_cpu(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(20_u64)?;
    length = length.checked_add(10)?.checked_add(20_u64)?;
    length = length.checked_add(17)?.checked_add(20_u64)?;
    length = length.checked_add(19)?.checked_add(20_u64)?;
    Some(length)
}

fn size_peak_rss(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(20_u64)?;
    length = length.checked_add(12)?.checked_add(20_u64)?;
    Some(length)
}

fn size_model(_capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(12)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(9)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(17)?.checked_add(20_u64)?;
    length = length.checked_add(10)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?.max(4))?;
    Some(length)
}

fn size_usage(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(21)?.checked_add(20_u64)?;
    length = length.checked_add(22)?.checked_add(20_u64)?;
    length = length.checked_add(17)?.checked_add(20_u64)?;
    length = length.checked_add(20)?.checked_add(20_u64)?;
    Some(length)
}

fn size_budget(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    Some(length)
}

fn size_tools(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(12)?.checked_add(
        (size_family(capture, limits)?).checked_add(1)?.checked_mul(u64::from(limits.items))?.checked_add(2)?,
    )?;
    length = length.checked_add(8)?.checked_add(5_u64)?;
    length = length.checked_add(11)?.checked_add(5_u64)?;
    length = length.checked_add(8)?.checked_add(
        (u64::from(limits.string).checked_mul(6)?.checked_add(2)?)
            .checked_add(1)?
            .checked_mul(u64::from(limits.items))?
            .checked_add(2)?,
    )?;
    Some(length)
}

fn size_completion_failure(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(9)?.checked_add(size_failure_class(capture, limits)?)?;
    length = length.checked_add(12)?.checked_add(size_evidence(capture, limits)?)?;
    length = length.checked_add(18)?.checked_add(20_u64)?;
    Some(length)
}

fn size_answer_failure(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(9)?.checked_add(size_answer_class(capture, limits)?)?;
    length = length.checked_add(14)?.checked_add(size_completion_failure(capture, limits)?)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(10)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    Some(length)
}

fn size_field(_capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
    Some(length)
}

fn size_run_result(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(size_form(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    if *capture == Capture::Everything {
        length = length.checked_add(8)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
    }
    if *capture == Capture::Everything {
        length = length.checked_add(10)?.checked_add(
            (size_field(capture, limits)?).checked_add(1)?.checked_mul(u64::from(limits.items))?.checked_add(2)?,
        )?;
    }
    Some(length)
}

fn size_message(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(size_role(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(
        (size_block(capture, limits)?).checked_add(1)?.checked_mul(u64::from(limits.items))?.checked_add(2)?,
    )?;
    Some(length)
}

fn size_prompt(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(10)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(9)?.checked_add(
        (u64::from(limits.string).checked_mul(6)?.checked_add(2)?)
            .checked_add(1)?
            .checked_mul(u64::from(limits.items))?
            .checked_add(2)?,
    )?;
    length = length.checked_add(12)?.checked_add(
        (size_message(capture, limits)?).checked_add(1)?.checked_mul(u64::from(limits.items))?.checked_add(2)?,
    )?;
    Some(length)
}

fn size_completion(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(10)?.checked_add(
        (size_block(capture, limits)?).checked_add(1)?.checked_mul(u64::from(limits.items))?.checked_add(2)?,
    )?;
    Some(length)
}

fn size_command_exit(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(20_u64)?;
    Some(length)
}

fn size_session_started(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(size_agent(capture, limits)?)?;
    length = length.checked_add(8)?.checked_add(size_mode(capture, limits)?)?;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(11)?.checked_add(size_capture(capture, limits)?)?;
    length = length.checked_add(12)?.checked_add(size_delivery(capture, limits)?)?;
    length = length.checked_add(12)?.checked_add(size_versions(capture, limits)?)?;
    Some(length)
}

fn size_session_ended(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(8)?.checked_add(20_u64)?;
    length = length.checked_add(12)?.checked_add(5_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(
        (u64::from(limits.string).checked_mul(6)?.checked_add(25)?)
            .checked_mul(u64::from(limits.items))?
            .checked_add(2)?,
    )?;
    length = length.checked_add(8)?.checked_add(size_loss(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(size_cpu(capture, limits)?)?;
    length = length.checked_add(18)?.checked_add(size_peak_rss(capture, limits)?)?;
    Some(length)
}

fn size_run_started(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(5_u64)?;
    length = length.checked_add(8)?.checked_add(size_model(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(size_budget(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(size_tools(capture, limits)?)?;
    length = length.checked_add(12)?.checked_add(
        (size_form(capture, limits)?).checked_add(1)?.checked_mul(u64::from(limits.items))?.checked_add(2)?,
    )?;
    Some(length)
}

fn size_run_completed(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(10)?.checked_add(size_status(capture, limits)?)?;
    length = length.checked_add(11)?.checked_add(size_answer_failure(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(size_run_result(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(size_usage(capture, limits)?)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    Some(length)
}

fn size_conversation_opened(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(size_conversation_kind(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(9)?.checked_add(size_model(capture, limits)?)?;
    Some(length)
}

fn size_conversation_closed(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(7)?.checked_add(size_conversation_end(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(11)?.checked_add(size_completion_failure(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(size_usage(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    Some(length)
}

fn size_response_started(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(12)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(size_model(capture, limits)?)?;
    length = length.checked_add(12)?.checked_add(20_u64)?;
    length = length.checked_add(17)?.checked_add(20_u64)?;
    if *capture == Capture::Everything {
        length = length.checked_add(10)?.checked_add(size_prompt(capture, limits)?)?;
    }
    Some(length)
}

fn size_response_completed(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(12)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(size_outcome(capture, limits)?)?;
    length = length.checked_add(8)?.checked_add(size_stop(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(size_usage(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(17)?.checked_add(20_u64)?;
    length = length.checked_add(17)?.checked_add(20_u64)?;
    length = length.checked_add(18)?.checked_add(20_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(size_completion_failure(capture, limits)?)?;
    length = length.checked_add(12)?.checked_add(20_u64)?;
    if *capture == Capture::Everything {
        length = length.checked_add(14)?.checked_add(size_completion(capture, limits)?)?;
    }
    Some(length)
}

fn size_text_delta(_capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(12)?.checked_add(20_u64)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
    Some(length)
}

fn size_tool_started(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(10)?.checked_add(size_source(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(size_effect(capture, limits)?)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    if *capture != Capture::None {
        length = length.checked_add(9)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
    }
    Some(length)
}

fn size_tool_completed(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(16)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
    length = length.checked_add(11)?.checked_add(size_verdict(capture, limits)?)?;
    length = length.checked_add(8)?.checked_add(size_command_exit(capture, limits)?)?;
    length = length.checked_add(9)?.checked_add(20_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    length = length.checked_add(12)?.checked_add(size_delivered(capture, limits)?)?;
    if *capture == Capture::Everything {
        length = length.checked_add(10)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
    }
    Some(length)
}

fn size_check_started(_capture: &Capture, _limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    Some(length)
}

fn size_check_completed(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(8)?.checked_add(size_command_exit(capture, limits)?)?;
    length = length.checked_add(10)?.checked_add(5_u64)?;
    length = length.checked_add(15)?.checked_add(20_u64)?;
    Some(length)
}

fn size_notice(capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 2_u64;
    length = length.checked_add(9)?.checked_add(size_level(capture, limits)?)?;
    length = length.checked_add(8)?.checked_add(size_notice_kind(capture, limits)?)?;
    length = length.checked_add(7)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    length = length.checked_add(11)?.checked_add(20_u64)?;
    Some(length)
}

fn size_block(_capture: &Capture, limits: &Limits) -> Option<u64> {
    let mut length = 0_u64;
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(8)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(14)?);
    }
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(8)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(17)?);
    }
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(9)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(14)?);
    }
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(9)?.checked_add(20_u64)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(19)?);
    }
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(9)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(13)?);
    }
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(8)?.checked_add(u64::from(limits.string).checked_mul(6)?.checked_add(2)?)?;
            length = length.checked_add(9)?.checked_add(5_u64)?;
            length = length.checked_add(8)?.checked_add(u64::from(limits.content).checked_mul(6)?.checked_add(2)?)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(16)?);
    }
    {
        let candidate = {
            let mut length = 2_u64;
            length = length.checked_add(9)?.checked_add(20_u64)?;
            Some(length)
        }?;
        length = length.max(candidate.checked_add(16)?);
    }
    Some(length)
}
