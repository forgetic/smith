//! Boundary-only FIFO/read-fence and concrete-turn oracle. Actual parent input,
//! fake provider terminals and root output populate this state; no run/session
//! cells or source fixtures are inspected. Skein owns deadlines and judgments.
//! Contract: domain/run.md, sections 6 and 13; testing-strategy.md, section 7.

use std::collections::VecDeque;

use skein_fake_llm_domain::api::{Part, Query, Role};
use skein_lib::{Duration, Time, Token};
use skein_world::domain::{Expectations, Judge};
use smith_domain::{Turn, llm, session};

/// A external observation copied by the world for replay and mutation.
/// Contract: domain/run.md, sections 6 and 13; testing-strategy.md, section 7.
#[derive(Clone, Debug)]
pub enum Seen {
    /// Actual root admission, separately from physical Started. Contract: domain/host.md, section 3.
    Admitted,

    /// Actual named parent input, before any bounce. Contract: domain/run.md, section 6.
    Input { name: Token, text: Box<[u8]> },

    /// The fake sees this exact completion request. Contract: domain/run.md, section 13.
    Prompt { query: Query },

    /// The actual provider terminal won its request. Contract: domain/session.md, section 3.
    Completed { parts: Vec<Part> },

    /// Provider failure/cancellation genuinely settled, with no invented turn.
    /// Contract: domain/session.md, sections 3 and 6.
    CompletionEnded,

    /// Exact root output, including concrete result body and read metadata.
    /// Contract: domain/run.md, section 13.
    Turn {
        /// Actual one-based main output. Contract: domain/run.md, section 13.
        number: u32,

        /// Actual read fence. Contract: domain/run.md, section 6.
        read: Option<Token>,

        /// Actual global units/raw prefixes and independent attestations, copied
        /// from root metadata. Bounded by its numeric types, independent of the
        /// inclusive record body. Contract: domain/run.md, sections 9 and 13.
        spent: smith_domain::run::Spend,

        /// Actual settled record body. Contract: domain/run.md, section 13.
        turn: Turn,
    },

    /// Actual settled waiting notice. Contract: domain/run.md, section 6.
    Waiting { read: Option<Token> },

    /// Observed final root count and parking classification, never an input answer.
    /// Contract: domain/run.md, sections 6 and 13.
    Answer {
        /// Last actual told message fence, retained in the run terminal.
        read: Option<Token>,
        /// Actual settled main record count. Contract: domain/run.md, section 13.
        turns: u32,

        /// Actual idle terminal class. Contract: domain/run.md, section 6.
        parked: bool,

        /// Final actual global prefixes and independent overflow attestations;
        /// never reconstructed from historical records or subtree prices.
        /// Contract: domain/run.md, sections 9, 10 and 13.
        spent: smith_domain::run::Spend,
    },
}

/// Retained observations for one fresh main chat. It owns no application right.
/// Contract: domain/run.md, sections 6 and 13; testing-strategy.md, section 7.
#[derive(Debug)]
pub struct Meeting {
    queued: VecDeque<(Token, Box<[u8]>)>,
    offered: Option<(Vec<Token>, Box<[u8]>)>,
    read: Option<Token>,
    expected: Option<Vec<Part>>,
    calling: bool,
    answered: bool,
    wait_result: bool,
    waiting: Option<Time>,
    count: u32,
    sequence: u32,
    idle: Duration,
    begun: Option<Time>,
}

impl Meeting {
    /// Start from the actual historical sequence; live numbering always starts
    /// at one. The scripted parent's positive idle threshold is independent.
    /// Contract: domain/run.md, sections 6 and 13.
    #[must_use]
    pub fn new(sequence: u32, idle: Duration) -> Self {
        Self {
            queued: VecDeque::new(),
            offered: None,
            read: None,
            expected: None,
            calling: false,
            answered: false,
            wait_result: false,
            waiting: None,
            count: 0,
            sequence,
            idle,
            begun: None,
        }
    }

    fn prompt(&mut self, query: &Query, judge: &mut Judge<&'static str, ()>) {
        judge
            .check(!self.calling && self.expected.is_none(), "a new provider request follows its previous actual turn");
        self.calling = true;
        let last = query.messages.last().and_then(|message| {
            if message.role != Role::User {
                return None;
            }
            match message.parts.as_ref() {
                [Part::Text { text }] => Some(text.as_ref()),
                [] | [Part::Opaque { .. } | Part::ToolCall { .. } | Part::ToolOutput { .. }] | [_, _, ..] => None,
            }
        });
        if !self.queued.is_empty() {
            let mut joined = Vec::new();
            let mut count = 0;
            for (at, (_, text)) in self.queued.iter().enumerate() {
                if at > 0 {
                    joined.extend_from_slice(b"\n\n");
                }
                joined.extend_from_slice(text);
                if last == Some(joined.as_slice()) {
                    count = at + 1;
                    break;
                }
            }
            if count > 0 {
                judge.check(self.offered.is_none(), "only one person offer precedes its actual turn");
                let names = self.queued.drain(..count).map(|(name, _)| name).collect::<Vec<_>>();
                self.offered = Some((names, joined.into_boxed_slice()));
                self.waiting = None;
                judge.withdraw(&"idle park");
                self.wait_result = false;
            } else {
                let later = self.queued.iter().skip(1).any(|(_, text)| last == Some(text.as_ref()));
                judge.check(!later, "person prompts preserve actual FIFO arrival order");
                judge.check(self.waiting.is_none(), "waiting wakes with the oldest exact person bytes");
            }
        }
    }

    fn turn(&mut self, number: u32, read: Option<Token>, turn: &Turn, judge: &mut Judge<&'static str, ()>) {
        judge.check(
            number == self.count + 1 && turn.sequence == self.sequence + 1,
            "actual turns have consecutive live and historical numbering",
        );
        self.count = number;
        self.sequence = turn.sequence;
        let expected_read = self.offered.as_ref().map_or(self.read, |(names, _)| names.last().copied());
        judge.check(read == expected_read, "read advances only for the exact offered person input");
        let expected = self.expected.take();
        judge.check(expected.is_some() && !self.calling, "a concrete turn follows an actual provider terminal");
        let assistant = turn
            .messages
            .iter()
            .filter(|message| message.role == llm::Role::Assistant)
            .map(|message| recorded_parts(&message.content))
            .collect::<Vec<_>>();
        judge.check(
            assistant.len() == 1 && assistant.first() == expected.as_ref(),
            "concrete turn preserves the actual assistant bytes and provider call identities",
        );
        if let Some((names, text)) = self.offered.take() {
            judge.check(
                turn.messages.iter().any(|message| {
                    message.role == llm::Role::User
                        && matches!(
                message.content.as_ref(), [session::llm::Block::Text { text: actual, .. }] if actual == &text)
                }),
                "offered person text is recorded unchanged before its assistant",
            );
            self.read = names.last().copied();
        }
        if let Some(parts) = expected {
            for part in parts {
                match part {
                    Part::ToolCall { id, name, .. } => {
                        let result =
                            turn.messages.iter().flat_map(|message| &message.content).find_map(|block| match block {
                                session::llm::Block::ToolResult { id: actual, result } if actual == &id => Some(result),
                                session::llm::Block::Text { .. }
                                | session::llm::Block::Refusal { .. }
                                | session::llm::Block::Opaque { .. }
                                | session::llm::Block::ToolCall { .. }
                                | session::llm::Block::ToolResult { .. } => None,
                            });
                        judge.check(result.is_some(), "every actual tool call has its concrete terminal before Turn");
                        if name.as_ref() == b"wait" {
                            judge.check(
                                matches!(result, Some(session::llm::Returned::Text {
                                text, error: false, replay: None
                            }) if text.as_ref() == b"waiting"),
                                "wait settles as exact canonical result before waiting",
                            );
                            self.wait_result = true;
                        }
                    }
                    Part::Text { .. } | Part::Opaque { .. } | Part::ToolOutput { .. } => {}
                }
            }
        }
    }
}

impl Expectations for Meeting {
    type Seen = Seen;

    type Name = &'static str;

    type Stimulus = ();

    fn observe(&mut self, seen: Seen, judge: &mut Judge<Self::Name, Self::Stimulus>) {
        if self.begun.is_none() {
            self.begun = Some(judge.now());
            judge.expect("chat answer", Duration::from_secs(3600));
        }
        judge.check(!self.answered, "no chat output or input follows its final answer");
        match seen {
            Seen::Admitted => {}
            Seen::Input { name, text } => self.queued.push_back((name, text)),
            Seen::Prompt { query } => self.prompt(&query, judge),
            Seen::Completed { parts } => {
                judge.check(self.calling && self.expected.is_none(), "provider terminal belongs to its actual request");
                self.calling = false;
                self.expected = Some(parts);
            }
            Seen::CompletionEnded => {
                judge.check(self.calling, "provider settlement consumes an actual request");
                self.calling = false;
            }
            Seen::Turn { number, read, turn, .. } => self.turn(number, read, &turn, judge),
            Seen::Waiting { read } => {
                judge.check(
                    !self.calling
                        && self.expected.is_none()
                        && self.queued.is_empty()
                        && self.offered.is_none()
                        && self.wait_result
                        && read == self.read,
                    "Waiting follows settled wait, turns and an empty inbox",
                );
                self.waiting = Some(judge.now());
                judge.rearm("idle park", self.idle.saturating_add(Duration::from_secs(1)));
            }
            Seen::Answer { turns, parked, read, .. } => {
                judge.check(
                    turns == self.count && read == self.read && !self.calling && self.expected.is_none(),
                    "final answer follows every actual turn and provider terminal",
                );
                if parked {
                    judge.check(
                        self.queued.is_empty()
                            && self.offered.is_none()
                            && self.waiting.is_some_and(|at| judge.now() >= at.saturating_add(self.idle)),
                        "Parked follows the actual empty-inbox waiting deadline",
                    );
                }
                let met = judge.meet(&"chat answer");
                judge.check(met, "final answer meets the actual chat obligation");
                judge.withdraw(&"idle park");
                self.answered = true;
            }
        }
    }
}

/// Project the actual root provider terminal into the fake's visible vocabulary.
/// Decoded application classifications are not provider input evidence.
/// Contract: domain/run.md, section 13; testing-strategy.md, section 7.
#[must_use]
pub fn completion_parts(content: &[llm::Said]) -> Vec<Part> {
    content
        .iter()
        .map(|part| match part {
            llm::Said::Text { text, .. } | llm::Said::Refusal { text, .. } => Part::Text { text: text.clone() },
            llm::Said::Opaque { bytes } => Part::Opaque { bytes: bytes.clone() },
            llm::Said::ToolCall { id, name, input, .. } => {
                Part::ToolCall { id: id.clone(), name: name.clone(), arguments: input.clone() }
            }
        })
        .collect()
}

fn recorded_parts(content: &[session::llm::Block]) -> Vec<Part> {
    content
        .iter()
        .map(|part| match part {
            session::llm::Block::Text { text, .. } | session::llm::Block::Refusal { text, .. } => {
                Part::Text { text: text.clone() }
            }
            session::llm::Block::Opaque { bytes } => Part::Opaque { bytes: bytes.clone() },
            session::llm::Block::ToolCall { id, name, input, .. } => {
                Part::ToolCall { id: id.clone(), name: name.clone(), arguments: input.clone() }
            }
            session::llm::Block::ToolResult { id, result } => Part::ToolOutput {
                id: id.clone(),
                output: smith_session_world::translate::render(result).0,
                is_error: smith_session_world::translate::render(result).1,
            },
        })
        .collect()
}
