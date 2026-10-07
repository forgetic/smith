//! Peers: what the top level keeps of each conversation the run opens, from
//! its `Open` to its session's `Ended`, and the tickets that session carries.
//!
//! The session cannot name run asks, so each live decoded application call gets
//! one bounded peer ticket until dispatch or yield. Fixed offered descriptors
//! are finish/deliver/sub-agent/wait plus admitted opaque host declarations.
//! Aggregate decoded owning bytes and completion block count bound this map.
//! Concrete V2 results move through the ready list into session history; no
//! persistent root answer-ticket inventory survives for prompt replay. Historical
//! calls are concrete provider id/name/input/replay with Historical classification.
//! The peer never knows provider syntax, credential secrets or host effects.
//!
//! Contract: domain/run.md, sections 5.2 and 13; programming-model.md, sections 4.4 and 6.3.

use core::mem::size_of;

use skein_lib::{List, Map, Token};
use smith_domain_run::{self as run, Ask};
use smith_domain_session::{self as session, llm as sllm};

use crate::llm::{self, Block, Decoded, Message, Prompt, Returned, Said, Served};
use crate::translate::{self, DELIVER, FINISH, FIRST, Offered, SUB_AGENT, WAIT};

/// A conversation the run opened, and its session's tickets.
#[derive(Debug)]
pub(crate) struct Peer {
    /// Host-supplied activation used in every name made by this conversation.
    pub(crate) activation: u64,
    /// The run's token for the conversation, which is its session's opener.
    pub(crate) conversation: Token,
    pub(crate) account: u32,
    /// The session's token for itself, once it has opened.
    pub(crate) session: Option<Token>,
    offered: Offered,
    /// The asks of the last completion's calls to the tools the run serves,
    /// by ticket, until the session dispatches them; and what they hold.
    asks: Map<u64, Ask>,
    held: u64,
    /// The next ticket to give.
    next: u64,
}

impl Peer {
    pub(crate) fn new(
        conversation: Token,
        activation: u64,
        account: u32,
        offered: Offered,
        limits: &session::Limits,
    ) -> Peer {
        Peer {
            conversation,
            activation,
            account,
            session: None,
            offered,
            asks: Map::with_capacity(asks(limits)),
            held: 0,
            next: FIRST,
        }
    }

    pub(crate) const fn is_main(&self) -> bool {
        self.offered.finish
    }

    /// The tickets it holds.
    pub(crate) fn tickets(&self) -> u32 {
        self.asks.len()
    }

    /// The session's view of `completion`: each call to a tool the run serves
    /// becomes a ticket for its ask, while what the asks hold fits `limit`; a
    /// call to one the session was not offered is no call.
    pub(crate) fn completion(&mut self, completion: llm::Completion, limit: u64) -> sllm::Completion {
        let llm::Completion { content, stop, usage } = completion;
        let mut blocks = List::with_capacity(u32::try_from(content.len()).expect("a completion fits in memory"));
        // Every call keeps a classification cell even when decoding is refused.
        // Reserve the complete batch's cells first, so an oversized first call
        // cannot spend the allowance needed by a later TooLarge classification.
        let classification_bytes = u64::try_from(size_of::<Decoded>()).expect("a classification cell fits a u64");
        let mut decoded_bytes = 0_u64;
        for said in &content {
            match said {
                Said::ToolCall { .. } => {
                    decoded_bytes =
                        decoded_bytes.checked_add(classification_bytes).expect("completion cells fit in memory");
                }
                Said::Text { .. } | Said::Refusal { .. } | Said::Opaque { .. } => {}
            }
        }
        for said in content {
            let block = match said {
                Said::Text { text, replay } => sllm::Block::Text { text, replay },
                Said::Refusal { text, replay } => sllm::Block::Refusal { text, replay },
                Said::Opaque { bytes } => sllm::Block::Opaque { bytes },
                Said::ToolCall { id, name, input, call, replay } => {
                    let total = match call.owned_bytes() {
                        Some(bytes) => match bytes.checked_sub(classification_bytes) {
                            Some(payload) => decoded_bytes.checked_add(payload),
                            None => None,
                        },
                        None => None,
                    };
                    let call = match total {
                        Some(bytes) if bytes <= limit => {
                            decoded_bytes = bytes;
                            self.decoded(call, limit)
                        }
                        Some(_) | None => sllm::Decoded::Invalid { problem: sllm::Problem::TooLarge },
                    };
                    sllm::Block::ToolCall { id, name, input, call, replay }
                }
            };
            blocks.push(block).expect("room for every block");
        }
        sllm::Completion { content: blocks.into_boxed(), stop, usage }
    }

    fn host_offered(&self, name: &[u8], effect: run::HostEffect) -> bool {
        for declaration in &self.offered.host_tools {
            if declaration.name.as_ref() == name {
                return declaration.effect == effect;
            }
        }
        false
    }

    fn decoded(&mut self, call: Decoded, limit: u64) -> sllm::Decoded {
        let ask = match call {
            Decoded::Owned { call } => return sllm::Decoded::Owned { call },
            Decoded::Invalid { problem } => return sllm::Decoded::Invalid { problem },
            Decoded::Served { ask } => ask,
        };
        let offered = match &ask {
            Ask::Wait => self.offered.wait,
            Ask::Host { tool, effect, .. } => self.host_offered(tool, *effect),
            Ask::Finish { .. } => self.offered.finish,
            Ask::Deliver { .. } => self.offered.deliver,
            Ask::SubAgent { .. } => self.offered.agents,
        };
        if !offered {
            return sllm::Decoded::Invalid { problem: sllm::Problem::UnknownTool };
        }
        let held = match ask_cost(&ask) {
            Some(cost) => self.held.checked_add(cost),
            None => None,
        };
        let held = match held {
            Some(held) if held <= limit && self.asks.len() < self.asks.capacity() => held,
            Some(_) | None => return sllm::Decoded::Invalid { problem: sllm::Problem::TooLarge },
        };
        let effect = translate::effect(&ask);
        let ticket = self.ticket();
        let fresh = self.asks.insert(ticket, ask).expect("checked for room above");
        assert!(fresh.is_none(), "tickets are not reused");
        self.held = held;
        sllm::Decoded::Delegated { ticket: Token::new(ticket), effect }
    }

    /// The ask the session dispatches as `ticket`.
    pub(crate) fn take(&mut self, ticket: Token) -> Ask {
        self.asks.remove(&ticket.raw()).expect("a session dispatches a call once, by the ticket it was given")
    }

    /// The session goes on without the calls it has not dispatched: it
    /// yielded, or called the LLM again. How many tickets went.
    pub(crate) fn forget_asks(&mut self, limits: &session::Limits) -> u32 {
        let forgotten = self.asks.len();
        self.asks = Map::with_capacity(asks(limits));
        self.held = 0;
        forgotten
    }

    /// The protocol layer's view of the session's `prompt`: every ticket in it
    /// resolved, the answers copied.
    pub(crate) fn prompt(&self, prompt: sllm::Prompt) -> Prompt {
        let sllm::Prompt { endpoint, model, system, tools, delegated, messages, max_tokens } = prompt;
        let mut served = List::with_capacity(u32::try_from(delegated.len()).expect("bounded declaration inventory"));
        for descriptor in &delegated {
            let tool = match descriptor.ticket {
                WAIT => Served::Wait,
                DELIVER => Served::Deliver,
                FINISH => Served::Finish,
                SUB_AGENT => Served::SubAgent,
                ticket => {
                    let index = usize::try_from(ticket.raw().checked_sub(FIRST).expect("host descriptor range"))
                        .expect("bounded host descriptor");
                    Served::Host(
                        self.offered.host_tools.get(index).expect("descriptor names admitted declaration").clone(),
                    )
                }
            };
            served.push(tool).expect("room for every descriptor");
        }
        let mut resolved = List::with_capacity(u32::try_from(messages.len()).expect("a transcript fits its limit"));
        for sllm::Message { role, content } in messages {
            let mut blocks = List::with_capacity(u32::try_from(content.len()).expect("a message fits in memory"));
            for block in content {
                blocks.push(Self::block(block)).expect("room for every block");
            }
            resolved.push(Message { role, content: blocks.into_boxed() }).expect("room for every message");
        }
        Prompt {
            endpoint,
            model,
            system,
            tools,
            served: served.into_boxed(),
            messages: resolved.into_boxed(),
            max_tokens,
        }
    }

    /// Bytes of opaque host declarations copied into the protocol prompt.
    /// They are absent from the session's ticket-only prompt count.
    pub(crate) fn declaration_bytes(&self) -> Option<u64> {
        let mut bytes = 0_u64;
        for tool in &self.offered.host_tools {
            bytes = bytes
                .checked_add(u64::try_from(tool.name.len()).ok()?)?
                .checked_add(u64::try_from(tool.description.len()).ok()?)?
                .checked_add(u64::try_from(tool.schema.len()).ok()?)?;
        }
        Some(bytes)
    }

    fn block(block: sllm::Block) -> Block {
        match block {
            sllm::Block::Text { text, replay } => Block::Text { text, replay },
            sllm::Block::Refusal { text, replay } => Block::Refusal { text, replay },
            sllm::Block::Opaque { bytes } => Block::Opaque { bytes },
            sllm::Block::ToolCall { id, name, input, call: _, replay } => Block::ToolCall { id, name, input, replay },
            sllm::Block::ToolResult { id, result } => {
                let result = match result {
                    sllm::Returned::Owned { outcome } => Returned::Owned { outcome },
                    sllm::Returned::Invalid { problem } => Returned::Invalid { problem },
                    sllm::Returned::NotRun => Returned::NotRun,
                    sllm::Returned::Text { text, error, replay } => Returned::Text { text, error, replay },
                    sllm::Returned::Withdrawn => Returned::Withdrawn,
                };
                Block::ToolResult { id, result }
            }
        }
    }

    fn ticket(&mut self) -> u64 {
        let ticket = self.next;
        self.next = ticket.checked_add(1).expect("a session makes fewer calls than a u64 counts");
        ticket
    }
}

/// The most asks a peer holds: as many as fit the session's byte limit at
/// their fixed size.
pub(crate) fn asks(limits: &session::Limits) -> u32 {
    per(limits.session_bytes, size_of::<Ask>()).min(limits.completion_blocks)
}

fn per(bytes: u64, size: usize) -> u32 {
    let size = u64::try_from(size).expect("a size fits in a u64").max(1);
    u32::try_from(bytes.checked_div(size).unwrap_or(0)).unwrap_or(u32::MAX)
}

/// What an ask holds: its fixed size, and each part held in a box at its
/// fixed size plus its payload.
pub(crate) fn ask_cost(ask: &Ask) -> Option<u64> {
    let payload = match ask {
        Ask::Wait => 0,
        Ask::Host { tool, input, .. } => size(tool.len())?.checked_add(size(input.bytes().len())?)?,
        Ask::Deliver { change } => change.owned_bytes()?,
        Ask::Finish { outcome } => run::outcome::owned_bytes(outcome)?,
        Ask::SubAgent { brief, families: _, llm, share: _ } => {
            let llm = match llm {
                Some(llm) => len(llm)?,
                None => 0,
            };
            len(brief)?.checked_add(llm)?
        }
    };
    size(size_of::<Ask>())?.checked_add(payload)
}

/// The most an answer holds beyond its fixed size, as [`returned_cost`]
/// counts it, under the run's `limits`, or `None` past a `u64`: a sub-agent's
/// answer; a failed check's tail and its repository's name, which the charter
/// holds; or the problems listed of an outcome rejected, each naming a field
/// of the outcome spec, which the charter holds, or of the outcome declared.
pub(crate) fn payload(limits: &run::Limits) -> Option<u64> {
    let answered = u64::from(limits.answer_bytes);
    let failed = u64::from(limits.check_tail).checked_add(limits.run_bytes)?;
    let problem = size(size_of::<run::outcome::Problem>())?.checked_add(limits.run_bytes.max(limits.outcome_bytes))?;
    let rejected = u64::from(run::outcome::Problems::LISTED).checked_mul(problem)?;
    let refused =
        u64::try_from(run::Marker::CAPACITY).ok()?.checked_add(u64::try_from(run::DeliveryRefusal::CAPACITY).ok()?)?;
    Some(
        answered
            .max(failed)
            .max(rejected)
            .max(run::Delivered::worst_case())
            .max(refused)
            .max(u64::from(limits.host_reply_bytes)),
    )
}

fn len(bytes: &[u8]) -> Option<u64> {
    size(bytes.len())
}

fn size(size: usize) -> Option<u64> {
    u64::try_from(size).ok()
}
