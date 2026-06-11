//! The pure agent loop, as a sans-IO state machine.
//!
//! [`AgentMachine`] is the functional core of an LLM agent turn: it owns the
//! conversation state and the iteration budget, and decides — purely — when to
//! call the model, which tools to run, when to inject steering, and when to
//! stop. It performs no I/O. The actual model streaming and tool execution are
//! done by the shell ([`crate::shell`]), which reuses pi-SDK providers and
//! tools and feeds results back as [`AgentCompletion`]s.
//!
//! This mirrors the [`smith_io_engine::Machine`] discipline used by the worker:
//! `(state, completion) -> [request]`, deterministic and replayable, so the
//! whole loop — tool orchestration, max-iteration cutoff, stop-reason handling,
//! steering at turn boundaries — is unit-testable with synthetic completions and
//! drivable under the asupersync lab for simulation/fuzz testing.
//!
//! Design note (steering): steering messages are injected at **turn
//! boundaries** — after a model turn and its tool batch complete, before the
//! next model call — not mid-tool-batch. This keeps the machine simple while
//! still supporting live interaction (the user's stated control goal); pi's
//! finer-grained mid-batch steering is deliberately not reproduced.

use pi::model::{
    AssistantMessage, ContentBlock, Message, StopReason, ToolCall, ToolResultMessage,
};
use pi::tools::ToolOutput;

/// An observability event the machine emits as data (the shell renders/records
/// it). Keeping events as machine output — rather than callbacks fired from
/// inside the loop, as pi does — preserves purity and makes the event stream
/// itself testable.
#[derive(Clone, Debug)]
pub enum AgentEvent {
    /// A model turn is starting (about to call the LLM).
    TurnStart { turn: usize },
    /// The model produced an assistant message.
    AssistantMessage { content: Vec<ContentBlock> },
    /// A tool is about to run.
    ToolStart { id: String, name: String },
    /// A tool finished.
    ToolEnd { id: String, is_error: bool },
    /// Steering messages were injected at a turn boundary.
    Steered { count: usize },
    /// The agent run ended (with the reason it stopped).
    AgentEnd { reason: AgentStop },
}

/// Why the agent loop stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentStop {
    /// The model returned a non-tool-use stop (normal completion).
    Completed,
    /// The model signalled an error stop reason.
    ModelError,
    /// The run was aborted (cancellation/steering-to-stop).
    Aborted,
    /// The tool-iteration budget was exhausted.
    BudgetExhausted,
}

/// A finished I/O event delivered to the machine.
pub enum AgentCompletion {
    /// The model stream completed, yielding the full assistant message.
    LlmResponded(AssistantMessage),
    /// The model call failed at the transport/provider layer.
    LlmFailed(String),
    /// A tool the machine requested finished.
    ToolFinished { id: String, output: ToolOutput },
    /// Steering messages arrived from the controller; inject at the next turn
    /// boundary.
    Steer(Vec<Message>),
    /// The run was asked to abort.
    Abort,
}

/// An I/O request the shell must perform.
pub enum AgentRequest {
    /// Stream a model response over the current message history. The shell
    /// builds the provider `Context` from these messages + the agent's system
    /// prompt and tool defs (held by the shell), and replies with
    /// [`AgentCompletion::LlmResponded`] / [`AgentCompletion::LlmFailed`].
    CallLlm { messages: Vec<Message> },
    /// Run one tool call; reply with [`AgentCompletion::ToolFinished`].
    RunTool(ToolCall),
    /// Emit an observability event.
    Emit(AgentEvent),
    /// The run is finished; `final_message` is the last assistant message (or a
    /// synthesized terminal message). The shell resolves the run with it.
    Finished {
        stop: AgentStop,
        final_message: AssistantMessage,
        messages: Vec<Message>,
    },
}

/// Where the loop is in the call/tool cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Phase {
    /// Waiting for a model response.
    AwaitingLlm,
    /// Waiting for the in-flight tool batch to finish.
    AwaitingTools,
    /// Terminal.
    Done,
}

/// The pure agent loop.
pub struct AgentMachine {
    messages: Vec<Message>,
    max_iterations: usize,
    iterations: usize,
    phase: Phase,
    turn: usize,
    /// Tool calls still outstanding in the current batch (id -> name), and the
    /// results collected so far, kept in the model's tool-call order so the
    /// tool-result messages are appended deterministically.
    pending_tools: Vec<PendingTool>,
    /// The most recent assistant message (the run's product on completion).
    last_assistant: Option<AssistantMessage>,
    /// Steering messages to inject at the next turn boundary.
    queued_steering: Vec<Message>,
    aborted: bool,
}

struct PendingTool {
    call: ToolCall,
    result: Option<ToolResultMessage>,
}

impl AgentMachine {
    /// Build a machine seeded with the initial conversation (typically a single
    /// user message), bounded to `max_iterations` tool rounds.
    pub fn new(initial_messages: Vec<Message>, max_iterations: usize) -> Self {
        Self {
            messages: initial_messages,
            max_iterations,
            iterations: 0,
            phase: Phase::AwaitingLlm,
            turn: 0,
            pending_tools: Vec::new(),
            last_assistant: None,
            queued_steering: Vec::new(),
            aborted: false,
        }
    }

    /// The current conversation (test/observability accessor).
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    fn finish(&mut self, stop: AgentStop) -> Vec<AgentRequest> {
        self.phase = Phase::Done;
        let final_message = self
            .last_assistant
            .clone()
            .unwrap_or_else(|| error_assistant("agent ended before producing a message"));
        vec![
            AgentRequest::Emit(AgentEvent::AgentEnd { reason: stop }),
            AgentRequest::Finished {
                stop,
                final_message,
                messages: std::mem::take(&mut self.messages),
            },
        ]
    }

    /// Begin the next model turn: inject any queued steering, then call the LLM.
    fn begin_turn(&mut self) -> Vec<AgentRequest> {
        let mut requests = Vec::new();
        if !self.queued_steering.is_empty() {
            let steering = std::mem::take(&mut self.queued_steering);
            requests.push(AgentRequest::Emit(AgentEvent::Steered {
                count: steering.len(),
            }));
            self.messages.extend(steering);
        }
        self.phase = Phase::AwaitingLlm;
        requests.push(AgentRequest::Emit(AgentEvent::TurnStart { turn: self.turn }));
        requests.push(AgentRequest::CallLlm {
            messages: self.messages.clone(),
        });
        self.turn += 1;
        requests
    }

    fn on_llm_responded(&mut self, assistant: AssistantMessage) -> Vec<AgentRequest> {
        let mut requests = vec![AgentRequest::Emit(AgentEvent::AssistantMessage {
            content: assistant.content.clone(),
        })];
        self.messages
            .push(Message::Assistant(std::sync::Arc::new(assistant.clone())));
        self.last_assistant = Some(assistant.clone());

        if matches!(assistant.stop_reason, StopReason::Error) {
            requests.extend(self.finish(AgentStop::ModelError));
            return requests;
        }
        if matches!(assistant.stop_reason, StopReason::Aborted) {
            requests.extend(self.finish(AgentStop::Aborted));
            return requests;
        }

        let tool_calls = extract_tool_calls(&assistant.content);
        if tool_calls.is_empty() {
            // No tools requested ⇒ the model is done.
            requests.extend(self.finish(AgentStop::Completed));
            return requests;
        }

        // Tool round: enforce the iteration budget before dispatching.
        self.iterations += 1;
        if self.iterations > self.max_iterations {
            requests.extend(self.finish(AgentStop::BudgetExhausted));
            return requests;
        }

        self.phase = Phase::AwaitingTools;
        self.pending_tools = tool_calls
            .iter()
            .map(|call| PendingTool {
                call: call.clone(),
                result: None,
            })
            .collect();
        for call in &tool_calls {
            requests.push(AgentRequest::Emit(AgentEvent::ToolStart {
                id: call.id.clone(),
                name: call.name.clone(),
            }));
            requests.push(AgentRequest::RunTool(call.clone()));
        }
        requests
    }

    fn on_tool_finished(&mut self, id: String, output: ToolOutput) -> Vec<AgentRequest> {
        let mut requests = vec![AgentRequest::Emit(AgentEvent::ToolEnd {
            id: id.clone(),
            is_error: output.is_error,
        })];

        if let Some(pending) = self.pending_tools.iter_mut().find(|p| p.call.id == id) {
            let tool_name = pending.call.name.clone();
            pending.result = Some(tool_result_message(&id, &tool_name, output));
        }

        // When every tool in the batch has a result, append them in order and
        // start the next model turn.
        if self.pending_tools.iter().all(|p| p.result.is_some()) {
            for pending in self.pending_tools.drain(..) {
                if let Some(result) = pending.result {
                    self.messages
                        .push(Message::ToolResult(std::sync::Arc::new(result)));
                }
            }
            if self.aborted {
                requests.extend(self.finish(AgentStop::Aborted));
            } else {
                requests.extend(self.begin_turn());
            }
        }
        requests
    }
}

impl smith_io_engine::Machine for AgentMachine {
    type Completion = AgentCompletion;
    type Request = AgentRequest;

    fn on_start(&mut self, _now: smith_io_engine::EngineTime) -> Vec<AgentRequest> {
        self.begin_turn()
    }

    fn on_completion(
        &mut self,
        _now: smith_io_engine::EngineTime,
        completion: AgentCompletion,
    ) -> Vec<AgentRequest> {
        match completion {
            AgentCompletion::LlmResponded(assistant) => self.on_llm_responded(assistant),
            AgentCompletion::LlmFailed(message) => {
                self.last_assistant = Some(error_assistant(&message));
                self.finish(AgentStop::ModelError)
            }
            AgentCompletion::ToolFinished { id, output } => self.on_tool_finished(id, output),
            AgentCompletion::Steer(messages) => {
                // Queue for the next turn boundary. If we are idle between turns
                // (shouldn't normally happen — the shell only delivers steering
                // while a run is active), it will be picked up on begin_turn.
                self.queued_steering.extend(messages);
                Vec::new()
            }
            AgentCompletion::Abort => {
                self.aborted = true;
                // If we're mid-LLM or between turns, stop now; if mid-tools, let
                // the in-flight batch drain (on_tool_finished checks `aborted`).
                if matches!(self.phase, Phase::AwaitingTools) {
                    Vec::new()
                } else {
                    self.finish(AgentStop::Aborted)
                }
            }
        }
    }

    fn is_stopped(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }
}

/// Pulls the tool-call blocks out of an assistant message, in order.
fn extract_tool_calls(content: &[ContentBlock]) -> Vec<ToolCall> {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall(call) => Some(call.clone()),
            _ => None,
        })
        .collect()
}

/// Builds the tool-result message appended to the conversation after a tool runs.
fn tool_result_message(
    tool_call_id: &str,
    tool_name: &str,
    output: ToolOutput,
) -> ToolResultMessage {
    ToolResultMessage {
        tool_call_id: tool_call_id.to_string(),
        tool_name: tool_name.to_string(),
        content: output.content,
        details: output.details,
        is_error: output.is_error,
        timestamp: 0,
    }
}

/// Synthesizes a terminal assistant message carrying an error string, for the
/// paths where the run ends without a real model message.
fn error_assistant(message: &str) -> AssistantMessage {
    AssistantMessage {
        content: vec![ContentBlock::Text(pi::model::TextContent {
            text: message.to_string(),
            text_signature: None,
        })],
        api: String::new(),
        provider: String::new(),
        model: String::new(),
        usage: pi::model::Usage::default(),
        stop_reason: StopReason::Error,
        error_message: Some(message.to_string()),
        timestamp: 0,
    }
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
