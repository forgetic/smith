//! Bounded provider stream state and event codecs. The decoder keeps only ordered blocks, usage and terminal progress.
//! event accepts injected decoded events; end settles truncation or failure once. The caller reserves `MAX_OUT` output slots.
//!
//! Contract: domain/session.md, sections 4 and 12; programming-model.md, sections 4.4, 4.5 and 6.3.

use crate::{
    DecodeError, Failure, Json, Limits, ProviderError, RateLimit, Request, Stop, Usage, classify, common, json, request,
};
use alloc::boxed::Box;
use core::mem;
use skein_json::{Token, writer::Encoder};
use skein_lib::{List, Queue, Wall, Writer, bytes};

/// One provider response item, classified before bounded stream accumulation.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Item {
    /// One ordered provider message item, optionally naming its replay identity and phase.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Message {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        id: Box<[u8]>,
        /// Optional provider phase label, preserved on replay.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        phase: Option<Box<[u8]>>,
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        text: Box<[u8]>,
        /// Whether the provider message is a refusal rather than an ordinary answer.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        refusal: bool,
    },
    /// Provider function call, retaining call and item identities with its arguments.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    FunctionCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        id: Box<[u8]>,
        /// Provider-issued tool-call identity, echoed on its output.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        call_id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        name: Box<[u8]>,
        /// Provider-written JSON tool arguments, retained within the enclosing byte cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        arguments: Box<[u8]>,
    },
    /// Provider-owned replay data, preserved verbatim and never interpreted by the domain.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Opaque {
        /// Provider-owned JSON replay value, bounded by document limits and preserved without interpretation.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        value: Json,
    },
}

/// Decoded provider or peer event; the enclosing entry point checks order and correlation.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Event {
    /// Provider created the response stream.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Created {
        /// Optional provider echo of the accepted request, retained only when configured.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        echo: Option<Request>,
    },
    /// Provider response is still progressing.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    InProgress {
        /// Optional provider echo of the accepted request, retained only when configured.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        echo: Option<Request>,
    },
    /// Provider begins the named response item.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Added {
        /// Provider block or item index used to correlate stream events.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        index: u32,
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        id: Box<[u8]>,
        /// Typed entry classification or byte label required by the enclosing contract.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        kind: Box<[u8]>,
    },
    /// The named provider item is complete.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Done {
        /// Provider block or item index used to correlate stream events.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        index: u32,
        /// Provider response item, retained in provider order.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        item: Item,
    },
    /// One successful completion terminal with stop reason and accepted usage.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Completed {
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        stop: Stop,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        usage: Usage,
    },
    /// One failed terminal with bounded diagnostics.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Failed {
        /// Whether the returned tool result represents a failure.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        error: ProviderError,
    },
    /// Provider transport progress with no new accepted result.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Progress,
    /// Unrecognized provider metadata, retained or ignored according to this decoder.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Unknown,
}

/// One bounded block accepted from the provider stream or neutral fake API.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    /// Owned bounded text in its original provider position.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Text {
        /// Owned UTF-8 text, bounded by the enclosing message or output cap.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        text: Box<[u8]>,
    },
    /// Provider-owned replay data, preserved verbatim and never interpreted by the domain.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Opaque {
        /// Owned payload bytes charged against the enclosing session limit.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        bytes: Box<[u8]>,
    },
    /// Provider tool call with its original identity, name and input retained.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    ToolCall {
        /// Provider-issued tool-call identifier, preserved verbatim in its result.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        id: Box<[u8]>,
        /// Boundary name, compared byte for byte; it carries no authority by itself.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        name: Box<[u8]>,
        /// Provider-written tool-argument bytes, retained exactly for replay.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        input: Box<[u8]>,
        /// The provider call arguments exceeded their configured cap; they cannot be executed.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        too_large: bool,
    },
}

/// Stream-decoder observation: bounded content, progress, or exactly one terminal.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Output {
    /// A bounded accepted content block; it is not a terminal by itself.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Part(Part),
    /// One successful completion terminal with stop reason and accepted usage.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Completed {
        /// Why the provider stopped this completion, independently of its content.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        stop: Stop,
        /// Provider-reported token usage, charged exactly once when its completion ends.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        usage: Usage,
    },
    /// One failed terminal with bounded diagnostics.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Failed {
        /// Typed reason why the pending operation produced no successful value.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        failure: Failure,
        /// Bounded diagnostic text retained for this failure.
        ///
        /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
        detail: Box<[u8]>,
    },
    /// Provider transport progress with no new accepted result.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    Progress,
}

/// Maximum records emitted by one entry point; the caller reserves this much output room before calling.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub const MAX_OUT: u32 = 3;

/// Bounded provider-stream state; events are injected and no transport or domain policy is owned here.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
#[derive(Debug)]
pub struct StreamDecoder {
    opened: List<Opened>,
    next: u32,
    parts: u32,
    bytes: u64,
    tools: bool,
    refusal: bool,
    over: bool,
    terminal: Option<Terminal>,
}

#[derive(Debug)]
enum Opened {
    Active { id: Box<[u8]>, kind: Box<[u8]> },
    Ready(Prepared),
    Emitted,
}

#[derive(Debug)]
struct Prepared {
    first: Part,
    second: Option<Part>,
    tool: bool,
    refusal: bool,
}

#[derive(Clone, Copy, Debug)]
struct Terminal {
    stop: Stop,
    usage: Usage,
}

impl StreamDecoder {
    /// Constructs empty bounded state under the supplied immutable limits; no IO or clocks are consulted.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn new(limits: &Limits) -> StreamDecoder {
        StreamDecoder {
            opened: List::with_capacity(limits.parts),
            next: 0,
            parts: 0,
            bytes: 0,
            tools: false,
            refusal: false,
            over: false,
            terminal: None,
        }
    }

    /// One event, with the caller reserving `MAX_OUT` output slots. Completed
    /// items may arrive in any order; only the next ordered item is emitted.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub fn event(&mut self, event: Event, limits: &Limits, wall: Wall, out: &mut Queue<Output>) {
        if self.over {
            return;
        }
        if self.terminal.is_some() {
            self.ready(out);
            return;
        }
        let before = out.len();
        let result = self.accept(event, limits, wall, out);
        match result {
            Ok(()) => {
                if out.len() == before {
                    out.push(Output::Progress);
                }
            }
            Err(DecodeError::TooLarge) => {
                self.over = true;
                self.opened.clear();
                out.push(Output::Completed { stop: Stop::MaxTokens, usage: Usage::ZERO });
            }
            Err(DecodeError::Malformed | DecodeError::Missing | DecodeError::WrongType) => {
                self.fail(Failure::Unavailable, bytes::copy_of(b"malformed ChatGPT stream"), out);
            }
        }
    }

    /// True when another bounded ready call can emit. The owner puts this
    /// decoder on its ready list and drains it before reading another event.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub fn has_ready(&self) -> bool {
        if self.over {
            return false;
        }
        match self.opened.get(self.next) {
            Some(Opened::Ready(_)) => true,
            Some(Opened::Active { .. }) => false,
            Some(Opened::Emitted) | None => self.terminal.is_some(),
        }
    }

    /// Emits at most one ordered item (two parts) and its final terminal.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub fn ready(&mut self, out: &mut Queue<Output>) {
        if self.over {
            return;
        }
        for _slot in 0..self.opened.len() {
            let Some(slot) = self.opened.get_mut(self.next) else {
                break;
            };
            match slot {
                Opened::Active { .. } => break,
                Opened::Emitted => self.next = self.next.saturating_add(1),
                Opened::Ready(_) => {
                    let state = mem::replace(slot, Opened::Emitted);
                    match state {
                        Opened::Ready(prepared) => {
                            out.push(Output::Part(prepared.first));
                            if let Some(second) = prepared.second {
                                out.push(Output::Part(second));
                            }
                        }
                        Opened::Active { .. } | Opened::Emitted => unreachable!("the slot was ready"),
                    }
                    self.next = self.next.saturating_add(1);
                    break;
                }
            }
        }
        if self.next == self.opened.len()
            && let Some(terminal) = self.terminal.take()
        {
            self.over = true;
            self.opened.clear();
            let stop = match terminal.stop {
                Stop::MaxTokens | Stop::Refusal => terminal.stop,
                Stop::EndTurn | Stop::ToolUse => {
                    if self.refusal {
                        Stop::Refusal
                    } else if self.tools {
                        Stop::ToolUse
                    } else {
                        terminal.stop
                    }
                }
            };
            out.push(Output::Completed { stop, usage: terminal.usage });
        }
    }

    /// Reports end of stream; an incomplete stream emits one failure and a completed stream emits no second terminal.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    pub fn end(&mut self, out: &mut Queue<Output>) {
        if !self.over {
            if self.terminal.is_some() {
                self.ready(out);
            } else {
                self.fail(Failure::Unavailable, bytes::copy_of(b"incomplete ChatGPT stream"), out);
            }
        }
    }

    /// Whether this stream has already emitted its single terminal, successfully or with a failure.
    ///
    /// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.over
    }

    fn accept(
        &mut self,
        event: Event,
        limits: &Limits,
        wall: Wall,
        out: &mut Queue<Output>,
    ) -> Result<(), DecodeError> {
        match event {
            Event::Created { .. } | Event::InProgress { .. } | Event::Progress | Event::Unknown => {
                out.push(Output::Progress);
            }
            Event::Added { index, id, kind } => {
                if index != self.opened.len() {
                    return Err(DecodeError::Malformed);
                }
                if self.opened.push(Opened::Active { id, kind }).is_err() {
                    return Err(DecodeError::TooLarge);
                }
                out.push(Output::Progress);
            }
            Event::Done { index, item } => {
                let slot = self.opened.get_mut(index).ok_or(DecodeError::Malformed)?;
                let state = mem::replace(slot, Opened::Emitted);
                let prepared = match state {
                    Opened::Active { id, kind } => prepare(item, &id, &kind, limits)?,
                    Opened::Ready(_) | Opened::Emitted => return Err(DecodeError::Malformed),
                };
                let mut count: u32 = 1;
                let mut size = part_size(&prepared.first);
                if let Some(second) = &prepared.second {
                    count = count.saturating_add(1);
                    size = size.saturating_add(part_size(second));
                }
                self.reserve(count, size, limits)?;
                self.tools = self.tools || prepared.tool;
                self.refusal = self.refusal || prepared.refusal;
                *self.opened.get_mut(index).expect("the slot exists") = Opened::Ready(prepared);
                self.ready(out);
            }
            Event::Completed { stop, usage } => {
                for index in 0..self.opened.len() {
                    match self.opened.get_mut(index).expect("within the slots") {
                        slot @ Opened::Active { .. } => match stop {
                            Stop::EndTurn | Stop::ToolUse => return Err(DecodeError::Malformed),
                            Stop::MaxTokens | Stop::Refusal => *slot = Opened::Emitted,
                        },
                        Opened::Ready(_) | Opened::Emitted => {}
                    }
                }
                self.terminal = Some(Terminal { stop, usage });
                self.ready(out);
            }
            Event::Failed { error } => {
                let failure = classify(0, Some(&error), RateLimit::NONE, wall);
                self.fail(failure, common::clipped(&error.message, limits.detail_bytes), out);
            }
        }
        Ok(())
    }

    fn reserve(&mut self, parts: u32, bytes: usize, limits: &Limits) -> Result<(), DecodeError> {
        let parts = self.parts.checked_add(parts).ok_or(DecodeError::TooLarge)?;
        let bytes =
            self.bytes.checked_add(u64::try_from(bytes).expect("usize fits u64")).ok_or(DecodeError::TooLarge)?;
        if parts > limits.parts || bytes > u64::from(limits.answer_bytes) {
            return Err(DecodeError::TooLarge);
        }
        self.parts = parts;
        self.bytes = bytes;
        Ok(())
    }

    fn fail(&mut self, failure: Failure, detail: Box<[u8]>, out: &mut Queue<Output>) {
        self.over = true;
        self.terminal = None;
        self.opened.clear();
        out.push(Output::Failed { failure, detail });
    }
}

fn part_size(part: &Part) -> usize {
    match part {
        Part::Text { text } => text.len(),
        Part::Opaque { bytes } => bytes.len(),
        Part::ToolCall { id, name, input, .. } => id.len().saturating_add(name.len()).saturating_add(input.len()),
    }
}

fn prepare(item: Item, expected_id: &[u8], expected_kind: &[u8], limits: &Limits) -> Result<Prepared, DecodeError> {
    match item {
        Item::Message { id, phase, text, refusal } => {
            if expected_id != id.as_ref() || expected_kind != b"message" {
                return Err(DecodeError::Malformed);
            }
            let head = message_head(&id, phase.as_deref(), limits)?;
            Ok(Prepared {
                first: Part::Opaque { bytes: head },
                second: Some(Part::Text { text }),
                tool: false,
                refusal,
            })
        }
        Item::FunctionCall { id, call_id, name, arguments } => {
            if expected_id != id.as_ref()
                || expected_kind != b"function_call"
                || bytes::find(&call_id, b"|").is_some()
                || bytes::find(&id, b"|").is_some()
            {
                return Err(DecodeError::Malformed);
            }
            let merged = joined_id(&call_id, &id)?;
            let too_large = arguments.len() > usize::try_from(limits.input_bytes).expect("u32 fits usize");
            let input = if too_large { bytes::copy_of(b"") } else { arguments };
            Ok(Prepared {
                first: Part::ToolCall { id: merged, name, input, too_large },
                second: None,
                tool: true,
                refusal: false,
            })
        }
        Item::Opaque { value } => {
            let tokens = value.as_tokens();
            if json::text_ref(json::value_at(tokens, json::required(tokens, b"id")?)?)? != expected_id
                || json::text_ref(json::value_at(tokens, json::required(tokens, b"type")?)?)? != expected_kind
            {
                return Err(DecodeError::Malformed);
            }
            let bytes = value.to_bytes(limits)?;
            if bytes.len() > usize::try_from(limits.opaque_bytes).expect("u32 fits usize") {
                return Err(DecodeError::TooLarge);
            }
            Ok(Prepared { first: Part::Opaque { bytes }, second: None, tool: false, refusal: false })
        }
    }
}

fn joined_id(call_id: &[u8], item_id: &[u8]) -> Result<Box<[u8]>, DecodeError> {
    let len = call_id
        .len()
        .checked_add(1)
        .ok_or(DecodeError::TooLarge)?
        .checked_add(item_id.len())
        .ok_or(DecodeError::TooLarge)?;
    let mut writer = Writer::new(len);
    writer.put(call_id).expect("measured");
    writer.put(b"|").expect("measured");
    writer.put(item_id).expect("measured");
    Ok(writer.finish())
}

fn message_head(id: &[u8], phase: Option<&[u8]>, limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    let bounded =
        skein_json::writer::Limits { depth: limits.depth, length: limits.opaque_bytes.min(limits.document_bytes) };
    let mut measure = Encoder::measure(&bounded);
    write_head(&mut measure, id, phase);
    let len = common::measured(measure)?;
    let mut write = Encoder::write(len, &bounded);
    write_head(&mut write, id, phase);
    Ok(write.finish())
}

fn write_head(out: &mut Encoder, id: &[u8], phase: Option<&[u8]>) {
    out.object_start();
    out.key(b"id");
    out.string(id);
    if let Some(phase) = phase {
        out.key(b"phase");
        out.string(phase);
    }
    out.object_end();
}

/// Decodes one provider stream event within document limits; stream-order validation belongs to `StreamDecoder`.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn decode_event(value: &Json, limits: &Limits) -> Result<Event, DecodeError> {
    let tokens = value.as_tokens();
    let kind = json::text_ref(json::value_at(tokens, json::required(tokens, b"type")?)?)?;
    match kind {
        b"response.created" => Ok(Event::Created { echo: None }),
        b"response.in_progress" => Ok(Event::InProgress { echo: None }),
        b"response.output_item.added" => {
            let item = json::value_at(tokens, json::required(tokens, b"item")?)?;
            Ok(Event::Added {
                index: index(tokens)?,
                id: json::text(json::value_at(item, json::required(item, b"id")?)?)?,
                kind: json::text(json::value_at(item, json::required(item, b"type")?)?)?,
            })
        }
        b"response.output_item.done" => Ok(Event::Done {
            index: index(tokens)?,
            item: read_item(json::value_at(tokens, json::required(tokens, b"item")?)?, limits)?,
        }),
        b"response.completed" | b"response.incomplete" | b"response.done" => {
            let response = json::value_at(tokens, json::required(tokens, b"response")?)?;
            let status = json::text_ref(json::value_at(response, json::required(response, b"status")?)?)?;
            if status == b"failed" {
                return Ok(Event::Failed {
                    error: read_error(json::value_at(response, json::required(response, b"error")?)?, limits)?,
                });
            }
            let stop = match status {
                b"completed" => {
                    if kind == b"response.incomplete" {
                        return Err(DecodeError::Malformed);
                    }
                    Stop::EndTurn
                }
                b"incomplete" => {
                    let details = json::value_at(response, json::required(response, b"incomplete_details")?)?;
                    match json::text_ref(json::value_at(details, json::required(details, b"reason")?)?)? {
                        b"max_output_tokens" => Stop::MaxTokens,
                        b"content_filter" => Stop::Refusal,
                        _ => return Err(DecodeError::WrongType),
                    }
                }
                _ => return Err(DecodeError::Malformed),
            };
            Ok(Event::Completed {
                stop,
                usage: read_usage(json::value_at(response, json::required(response, b"usage")?)?)?,
            })
        }
        b"response.failed" => {
            let response = json::value_at(tokens, json::required(tokens, b"response")?)?;
            Ok(Event::Failed {
                error: read_error(json::value_at(response, json::required(response, b"error")?)?, limits)?,
            })
        }
        b"error" => Ok(Event::Failed { error: decode_error(value, limits)? }),
        b"response.output_text.delta"
        | b"response.function_call_arguments.delta"
        | b"response.reasoning_summary_text.delta"
        | b"response.reasoning_summary_part.added"
        | b"response.function_call_arguments.done"
        | b"response.output_text.done"
        | b"response.content_part.added"
        | b"response.content_part.done"
        | b"response.reasoning_summary_text.done"
        | b"response.reasoning_summary_part.done" => Ok(Event::Progress),
        _ => Ok(Event::Unknown),
    }
}

fn index(tokens: &[Token]) -> Result<u32, DecodeError> {
    match u32::try_from(json::unsigned(json::value_at(tokens, json::required(tokens, b"output_index")?)?)?) {
        Ok(n) => Ok(n),
        Err(_) => Err(DecodeError::TooLarge),
    }
}

fn read_item(tokens: &[Token], limits: &Limits) -> Result<Item, DecodeError> {
    match json::text_ref(json::value_at(tokens, json::required(tokens, b"type")?)?)? {
        b"function_call" => Ok(Item::FunctionCall {
            id: json::text(json::value_at(tokens, json::required(tokens, b"id")?)?)?,
            call_id: json::text(json::value_at(tokens, json::required(tokens, b"call_id")?)?)?,
            name: json::text(json::value_at(tokens, json::required(tokens, b"name")?)?)?,
            arguments: json::text(json::value_at(tokens, json::required(tokens, b"arguments")?)?)?,
        }),
        b"message" => {
            let values = json::value_at(tokens, json::required(tokens, b"content")?)?;
            let mut text = List::with_capacity(limits.answer_bytes);
            let mut refusal = false;
            for &offset in &json::array(values, limits.parts)? {
                let part = json::value_at(values, offset)?;
                match json::text_ref(json::value_at(part, json::required(part, b"type")?)?)? {
                    b"output_text" => common::append(
                        &mut text,
                        json::text_ref(json::value_at(part, json::required(part, b"text")?)?)?,
                    )?,
                    b"refusal" => {
                        refusal = true;
                        common::append(
                            &mut text,
                            json::text_ref(json::value_at(part, json::required(part, b"refusal")?)?)?,
                        )?;
                    }
                    _ => {}
                }
            }
            Ok(Item::Message {
                id: json::text(json::value_at(tokens, json::required(tokens, b"id")?)?)?,
                phase: request::optional_text(tokens, b"phase")?,
                text: text.into_boxed(),
                refusal,
            })
        }
        _ => Ok(Item::Opaque { value: Json::from_tokens(tokens, limits)? }),
    }
}

fn read_usage(tokens: &[Token]) -> Result<Usage, DecodeError> {
    let input = json::unsigned(json::value_at(tokens, json::required(tokens, b"input_tokens")?)?)?;
    let cached = match json::optional_at(tokens, json::field(tokens, b"input_tokens_details")?)? {
        Some(value) => match json::optional_at(value, json::field(value, b"cached_tokens")?)? {
            Some(value) => json::unsigned(value)?,
            None => 0,
        },
        None => 0,
    };
    Ok(Usage {
        input_tokens: input.checked_sub(cached).ok_or(DecodeError::Malformed)?,
        cache_read_tokens: cached,
        output_tokens: json::unsigned(json::value_at(tokens, json::required(tokens, b"output_tokens")?)?)?,
        cache_write_tokens: 0,
    })
}

/// Decodes bounded peer-error metadata; syntax, type and byte-cap failures remain typed.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn decode_error(value: &Json, limits: &Limits) -> Result<ProviderError, DecodeError> {
    let tokens = value.as_tokens();
    let error = match json::optional_at(tokens, json::field(tokens, b"error")?)? {
        Some(value) => value,
        None => tokens,
    };
    read_error(error, limits)
}

fn read_error(tokens: &[Token], limits: &Limits) -> Result<ProviderError, DecodeError> {
    let kind = match json::optional_at(tokens, json::field(tokens, b"code")?)? {
        Some([Token::String(code)]) => code.clone(),
        Some([Token::Null]) | None => json::text(json::value_at(tokens, json::required(tokens, b"type")?)?)?,
        Some(_) => return Err(DecodeError::WrongType),
    };
    let message = common::clipped(
        json::text_ref(json::value_at(tokens, json::required(tokens, b"message")?)?)?,
        limits.detail_bytes,
    );
    let resets_in_seconds = match json::optional_at(tokens, json::field(tokens, b"resets_in_seconds")?)? {
        Some(value) => Some(json::unsigned(value)?),
        None => None,
    };
    let resets_at = match json::optional_at(tokens, json::field(tokens, b"resets_at")?)? {
        Some(value) => Some(json::unsigned(value)?),
        None => None,
    };
    Ok(ProviderError { kind, message, resets_in_seconds, resets_at })
}

/// Encodes a bounded typed peer-error body; no network or retry policy is executed.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn encode_error(error: &ProviderError, limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    encode_event(&Event::Failed { error: error.clone() }, limits)
}

/// Encodes one typed fake-provider stream event within document limits.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn encode_event(event: &Event, limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    let bounded = limits.writer_limits();
    let mut measure = Encoder::measure(&bounded);
    write_event(&mut measure, event, None);
    let len = common::measured(measure)?;
    let mut write = Encoder::write(len, &bounded);
    write_event(&mut write, event, None);
    Ok(write.finish())
}

fn write_error(out: &mut Encoder, error: &ProviderError) {
    out.object_start();
    out.key(b"code");
    out.string(&error.kind);
    out.key(b"message");
    out.string(&error.message);
    if let Some(reset) = error.resets_in_seconds {
        out.key(b"resets_in_seconds");
        out.unsigned(reset);
    }
    if let Some(reset) = error.resets_at {
        out.key(b"resets_at");
        out.unsigned(reset);
    }
    out.object_end();
}

fn write_event(out: &mut Encoder, event: &Event, completion_echo: Option<&Request>) {
    out.object_start();
    out.key(b"type");
    match event {
        Event::Created { echo } | Event::InProgress { echo } => {
            out.string(match event {
                Event::Created { .. } => b"response.created",
                Event::InProgress { .. } => b"response.in_progress",
                Event::Added { .. }
                | Event::Done { .. }
                | Event::Completed { .. }
                | Event::Failed { .. }
                | Event::Progress
                | Event::Unknown => unreachable!("only echo events reach this arm"),
            });
            out.key(b"response");
            out.object_start();
            out.key(b"status");
            out.string(b"in_progress");
            if let Some(request) = echo {
                out.key(b"instructions");
                out.string(&request.instructions);
                out.key(b"tools");
                request::write_tools(out, &request.tools);
            }
            out.object_end();
        }
        Event::Added { index, id, kind } => {
            out.string(b"response.output_item.added");
            out.key(b"output_index");
            out.unsigned(u64::from(*index));
            out.key(b"item");
            out.object_start();
            out.key(b"id");
            out.string(id);
            out.key(b"type");
            out.string(kind);
            out.object_end();
        }
        Event::Done { index, item } => {
            out.string(b"response.output_item.done");
            out.key(b"output_index");
            out.unsigned(u64::from(*index));
            out.key(b"item");
            write_item(out, item);
        }
        Event::Completed { stop, usage } => {
            out.string(match stop {
                Stop::MaxTokens | Stop::Refusal => b"response.incomplete",
                Stop::EndTurn | Stop::ToolUse => b"response.completed",
            });
            out.key(b"response");
            out.object_start();
            out.key(b"status");
            out.string(match stop {
                Stop::MaxTokens | Stop::Refusal => b"incomplete",
                Stop::EndTurn | Stop::ToolUse => b"completed",
            });
            match stop {
                Stop::MaxTokens | Stop::Refusal => {
                    out.key(b"incomplete_details");
                    out.object_start();
                    out.key(b"reason");
                    out.string(match stop {
                        Stop::MaxTokens => b"max_output_tokens",
                        Stop::Refusal => b"content_filter",
                        Stop::EndTurn | Stop::ToolUse => unreachable!("cut stop"),
                    });
                    out.object_end();
                }
                Stop::EndTurn | Stop::ToolUse => {}
            }
            if let Some(request) = completion_echo {
                out.key(b"instructions");
                out.string(&request.instructions);
                out.key(b"tools");
                request::write_tools(out, &request.tools);
            }
            out.key(b"usage");
            out.object_start();
            out.key(b"input_tokens");
            out.unsigned(usage.input_tokens.saturating_add(usage.cache_read_tokens));
            out.key(b"output_tokens");
            out.unsigned(usage.output_tokens);
            out.key(b"input_tokens_details");
            out.object_start();
            out.key(b"cached_tokens");
            out.unsigned(usage.cache_read_tokens);
            out.object_end();
            out.object_end();
            out.object_end();
        }
        Event::Failed { error } => {
            out.string(b"error");
            out.key(b"error");
            write_error(out, error);
        }
        Event::Progress => out.string(b"response.output_text.delta"),
        Event::Unknown => out.string(b"future_event"),
    }
    out.object_end();
}

fn write_item(out: &mut Encoder, item: &Item) {
    match item {
        Item::Opaque { value } => value.write(out),
        Item::FunctionCall { id, call_id, name, arguments } => {
            out.object_start();
            out.key(b"type");
            out.string(b"function_call");
            out.key(b"id");
            out.string(id);
            out.key(b"call_id");
            out.string(call_id);
            out.key(b"name");
            out.string(name);
            out.key(b"arguments");
            out.string(arguments);
            out.object_end();
        }
        Item::Message { id, phase, text, refusal } => {
            out.object_start();
            out.key(b"type");
            out.string(b"message");
            out.key(b"id");
            out.string(id);
            if let Some(phase) = phase {
                out.key(b"phase");
                out.string(phase);
            }
            out.key(b"content");
            out.array_start();
            out.object_start();
            out.key(b"type");
            out.string(if *refusal { b"refusal" } else { b"output_text" });
            out.key(if *refusal { b"refusal" } else { b"text" });
            out.string(text);
            out.object_end();
            out.array_end();
            out.object_end();
        }
    }
}

pub(crate) fn decoder_worst_case(limits: &Limits) -> Option<u64> {
    let slots = List::<Opened>::worst_case(limits.parts)?;
    let identifiers = u64::from(limits.parts)
        .checked_mul(u64::from(limits.string_bytes.min(limits.document_bytes)))?
        .checked_mul(2)?;
    slots.checked_add(identifiers)?.checked_add(u64::from(limits.answer_bytes))
}

/// Fake-server completion with the instructions/tools echo that the real Codex
/// route sends for the third time. The client decoder deliberately ignores it.
///
/// Contract: domain/session.md, sections 4, 9 and 12; programming-model.md, section 4.4.
pub fn encode_completion(
    stop: Stop,
    usage: Usage,
    request: &Request,
    limits: &Limits,
) -> Result<Box<[u8]>, DecodeError> {
    let event = Event::Completed { stop, usage };
    let bounded = limits.writer_limits();
    let mut measure = Encoder::measure(&bounded);
    write_event(&mut measure, &event, Some(request));
    let len = common::measured(measure)?;
    let mut write = Encoder::write(len, &bounded);
    write_event(&mut write, &event, Some(request));
    Ok(write.finish())
}
