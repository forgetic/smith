//! Codex JSON lines retain typed root-thread observations, never child usage.
//! `parse_line` skips unknown record kinds and refuses a known persisted shape
//! that changed (benchmarks.md, sections 6, 8.2 and 14). Login and rollouts are
//! outside this observer; its version is pinned by the adapter configuration.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::{
    Classification, Convention, Counts, End, Exit, FailureReason, Forced, Measure, ScopeKind, TokenCounts, TokenRecord,
    Usage,
};

/// A persisted record refused by the observer, naming the record and field path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub record: String,
    pub path: String,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Codex {} at {}: {}", self.record, self.path, self.reason)
    }
}

impl std::error::Error for ParseError {}

/// A turn's reported root usage, retained before convention normalisation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_input_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
}

/// A fatal turn error supplied by Codex; it has no provider-specific class.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TurnError {
    pub message: String,
}

/// The item lifecycle supplied by Codex's JSON stream.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// Codex has started the operation.
    InProgress,
    /// Codex has completed the operation successfully.
    Completed,
    /// Codex completed the operation with a failure.
    Failed,
}

/// A candidate file change reported by Codex, retained for scope observation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

/// A file operation reported by Codex's item observer.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Codex added a file.
    Add,
    /// Codex changed an existing file.
    Update,
    /// Codex deleted a file.
    Delete,
}

/// A known item supplied by the stream to the attempt observer.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Item {
    /// A textual message from the agent; its turn terminal decides acceptance.
    AgentMessage { id: String, text: String },
    /// A shell operation started or completed by Codex.
    CommandExecution {
        id: String,
        command: String,
        aggregated_output: String,
        exit_code: Option<i32>,
        status: ItemStatus,
    },
    /// Candidate file changes started or completed by Codex.
    FileChange { id: String, changes: Vec<FileChange>, status: ItemStatus },
    /// A collaboration operation whose child usage is absent from JSON lines.
    CollabToolCall {
        id: String,
        tool: String,
        sender_thread_id: String,
        receiver_thread_ids: Vec<String>,
        prompt: Option<String>,
        agents_states: BTreeMap<String, serde_json::Value>,
        status: ItemStatus,
    },
}

/// A neutral record supplied by the parser to the attempt observer.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Observation {
    /// The stream names its root thread.
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: String },
    /// Codex starts another root turn.
    #[serde(rename = "turn.started")]
    TurnStarted,
    /// Codex starts an item in this turn.
    #[serde(rename = "item.started")]
    ItemStarted { item: Item },
    /// Codex completes an item in this turn.
    #[serde(rename = "item.completed")]
    ItemCompleted { item: Item },
    /// Codex updates an existing item's lifecycle.
    #[serde(rename = "item.updated")]
    ItemUpdated { item: Item },
    /// Codex completes its root turn with aggregate root usage.
    #[serde(rename = "turn.completed")]
    TurnCompleted { usage: TurnUsage },
    /// Codex ends this turn without an answer.
    #[serde(rename = "turn.failed")]
    TurnFailed { error: TurnError },
    /// Codex reports an unrecoverable stream error.
    #[serde(rename = "error")]
    Error { message: String },
}

/// Parse a complete JSON line without accepting a truncated prefix.
/// Unknown record and item kinds are skipped; known shapes are read strictly.
pub fn parse_line(line: &str) -> Result<Option<Observation>, ParseError> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|error| ParseError {
        record: "JSON line".into(),
        path: format!("line {}, column {}", error.line(), error.column()),
        reason: "invalid JSON".into(),
    })?;
    let kind = value.get("type").and_then(serde_json::Value::as_str).ok_or_else(|| ParseError {
        record: "record".into(),
        path: "type".into(),
        reason: "record needs a string type".into(),
    })?;
    match kind {
        "thread.started" | "turn.started" | "turn.completed" | "turn.failed" | "error" => {}
        "item.started" | "item.completed" | "item.updated" => {
            let item = value.get("item").ok_or_else(|| ParseError {
                record: kind.into(),
                path: "item".into(),
                reason: "item record needs its item".into(),
            })?;
            let item_kind = item.get("type").and_then(serde_json::Value::as_str).ok_or_else(|| ParseError {
                record: kind.into(),
                path: "item.type".into(),
                reason: "item needs a string type".into(),
            })?;
            if !matches!(item_kind, "agent_message" | "command_execution" | "file_change" | "collab_tool_call") {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    match kind {
        "thread.started" => {
            decode::<String>(field(&value, "thread_id", kind)?, kind, "thread_id")?;
        }
        "turn.completed" => {
            decode::<TurnUsage>(field(&value, "usage", kind)?, kind, "usage")?;
        }
        "turn.failed" => {
            decode::<TurnError>(field(&value, "error", kind)?, kind, "error")?;
        }
        "error" => {
            decode::<String>(field(&value, "message", kind)?, kind, "message")?;
        }
        "item.started" | "item.completed" | "item.updated" => {
            let item = field(&value, "item", kind)?;
            decode::<String>(field(&item, "id", kind)?, kind, "item.id")?;
            let item_type = item["type"].as_str().expect("item type already checked");
            let strings: &[&str] = match item_type {
                "agent_message" => &["text"],
                "command_execution" => &["command", "aggregated_output"],
                "file_change" => &[],
                "collab_tool_call" => &["tool", "sender_thread_id"],
                _ => unreachable!("unknown item already skipped"),
            };
            for name in strings {
                decode::<String>(field(&item, name, kind)?, kind, &format!("item.{name}"))?;
            }
            if item_type != "agent_message" {
                decode::<ItemStatus>(field(&item, "status", kind)?, kind, "item.status")?;
            }
            match item_type {
                "command_execution" => {
                    decode::<Option<i32>>(field(&item, "exit_code", kind)?, kind, "item.exit_code")?;
                }
                "file_change" => {
                    decode::<Vec<FileChange>>(field(&item, "changes", kind)?, kind, "item.changes")?;
                }
                "collab_tool_call" => {
                    decode::<Vec<String>>(
                        field(&item, "receiver_thread_ids", kind)?,
                        kind,
                        "item.receiver_thread_ids",
                    )?;
                }
                _ => {}
            }
            decode::<Item>(item, kind, "item")?;
        }
        _ => {}
    }
    let record = kind.to_owned();
    decode(value, &record, "").map(Some)
}

fn field(value: &serde_json::Value, name: &str, record: &str) -> Result<serde_json::Value, ParseError> {
    value.get(name).cloned().ok_or_else(|| ParseError {
        record: record.into(),
        path: name.into(),
        reason: "required field missing".into(),
    })
}

fn decode<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    record: &str,
    prefix: &str,
) -> Result<T, ParseError> {
    serde_path_to_error::deserialize(value).map_err(|error| {
        let mut path = error.path().to_string();
        if path == "." {
            path.clear();
        }
        let reason = error.inner().to_string();
        for marker in ["unknown field `", "missing field `"] {
            if let Some(field) = reason.split(marker).nth(1).and_then(|rest| rest.split('`').next()) {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(field);
            }
        }
        if !prefix.is_empty() {
            path = if path.is_empty() { prefix.into() } else { format!("{prefix}.{path}") };
        }
        ParseError { record: record.into(), path, reason: "known record shape refused".into() }
    })
}

impl TurnUsage {
    fn normalise(&self) -> Result<TokenCounts, String> {
        Usage::Codex {
            input: self.input_tokens,
            cached_input: self.cached_input_tokens,
            cache_write: self.cache_write_input_tokens,
            output: self.output_tokens,
            reasoning: self.reasoning_output_tokens,
        }
        .normalise()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum LastTurn {
    #[default]
    NotStarted,
    Active,
    Completed,
    Failed(String),
}

/// Attempt-local root observations supplied by Codex's JSON-lines stream.
/// Turn aggregates have no response ids; `counts` keeps response counts unavailable.
/// Rollout collection later replaces this evidence with per-response scope usage.
#[derive(Clone, Debug, Default)]
pub struct Observer {
    thread: Option<String>,
    turn: u64,
    last: LastTurn,
    answer: Option<String>,
    usages: BTreeMap<u64, TurnUsage>,
    items: BTreeMap<String, Item>,
    fatal: Option<String>,
    error: Option<ParseError>,
}

impl Observer {
    /// Observe a line and return its typed record, retaining any harness error.
    pub fn observe_line(&mut self, line: &str) -> Result<Option<Observation>, ParseError> {
        let result = parse_line(line).and_then(|observation| {
            if let Some(record) = &observation {
                self.observe(record)?;
            }
            Ok(observation)
        });
        if let Err(error) = &result {
            self.error = Some(error.clone());
        }
        result
    }

    fn observe(&mut self, observation: &Observation) -> Result<(), ParseError> {
        let bad =
            |path: &str, reason: &str| ParseError { record: "stream".into(), path: path.into(), reason: reason.into() };
        match observation {
            Observation::ThreadStarted { thread_id } => {
                if thread_id.is_empty() || self.thread.is_some() {
                    return Err(bad("thread_id", "root thread must be named exactly once"));
                }
                self.thread = Some(thread_id.clone());
            }
            Observation::TurnStarted => {
                if self.thread.is_none() || self.last == LastTurn::Active {
                    return Err(bad("turn.started", "turn needs a root and a terminal for the previous turn"));
                }
                self.turn = self.turn.checked_add(1).ok_or_else(|| bad("turn", "turn counter overflow"))?;
                self.last = LastTurn::Active;
                self.answer = None;
            }
            Observation::ItemStarted { item }
            | Observation::ItemUpdated { item }
            | Observation::ItemCompleted { item } => {
                if self.last != LastTurn::Active {
                    return Err(bad("item", "item needs an active turn"));
                }
                if let Observation::ItemCompleted { item: _ } = observation {
                    let id = match item {
                        Item::AgentMessage { id, text } => {
                            self.answer = Some(text.clone());
                            id
                        }
                        Item::CommandExecution { id, .. }
                        | Item::FileChange { id, .. }
                        | Item::CollabToolCall { id, .. } => id,
                    };
                    if id.is_empty() {
                        return Err(bad("item.id", "item needs an id"));
                    }
                    if let Some(previous) = self.items.get(id) {
                        if previous != item {
                            return Err(bad("item.id", "conflicting repeated item"));
                        }
                    } else {
                        self.items.insert(id.clone(), item.clone());
                    }
                }
            }
            Observation::TurnCompleted { usage } => {
                if let Some(previous) = self.usages.get(&self.turn) {
                    if previous != usage {
                        return Err(bad("usage", "conflicting repeated turn usage"));
                    }
                    return Ok(());
                }
                if self.last != LastTurn::Active {
                    return Err(bad("turn.completed", "completion needs an active turn"));
                }
                usage.normalise().map_err(|reason| bad("usage", &reason))?;
                self.usages.insert(self.turn, usage.clone());
                self.last = LastTurn::Completed;
            }
            Observation::TurnFailed { error } => {
                if self.last != LastTurn::Active {
                    return Err(bad("turn.failed", "failure needs an active turn"));
                }
                self.answer = None;
                self.last = LastTurn::Failed(error.message.clone());
            }
            Observation::Error { message } => {
                self.fatal = Some(message.clone());
                self.answer = None;
            }
        }
        Ok(())
    }

    /// The root thread named by the stream, for subsequent read-only collection.
    #[must_use]
    pub fn thread(&self) -> Option<&str> {
        self.thread.as_deref()
    }

    /// The answer from the last completed turn, before a fatal or parser error.
    #[must_use]
    pub fn answer(&self) -> Option<&str> {
        if self.last == LastTurn::Completed && self.fatal.is_none() && self.error.is_none() {
            self.answer.as_deref()
        } else {
            None
        }
    }

    /// Root aggregate evidence and a total naming the absent non-root scopes.
    /// An identical repeated terminal does not add its aggregate again.
    pub fn token_records(&self) -> Result<Vec<TokenRecord>, String> {
        let scope = self.thread.as_ref().ok_or("Codex stream has no root thread")?;
        let mut usages = self.usages.values();
        let mut root = usages
            .next()
            .map_or_else(|| Ok(TokenCounts::unavailable("root turn usage not observed")), TurnUsage::normalise)?;
        for usage in usages {
            root = root.add(&usage.normalise()?)?;
        }
        let mut total = root.clone();
        for metric in
            [&mut total.fresh, &mut total.cache_read, &mut total.cache_write, &mut total.output, &mut total.reasoning]
        {
            if let Measure::Observed { value } = metric {
                *metric = Measure::LowerBound {
                    value: *value,
                    missing: vec![
                        "JSON lines omit child, compaction and helper usage; rollout collection required".into(),
                    ],
                };
            }
        }
        Ok(vec![
            TokenRecord { scope: scope.clone(), role: ScopeKind::Root, convention: Convention::Codex, usage: root },
            TokenRecord { scope: "total".into(), role: ScopeKind::Total, convention: Convention::Codex, usage: total },
        ])
    }

    /// Stream-visible tool counts; turn aggregates cannot count provider responses.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut tools: BTreeMap<String, u64> = BTreeMap::new();
        for item in self.items.values() {
            let name = match item {
                Item::AgentMessage { .. } => None,
                Item::CommandExecution { .. } => Some("command_execution"),
                Item::FileChange { .. } => Some("file_change"),
                Item::CollabToolCall { .. } => Some("collab_tool_call"),
            };
            if let Some(name) = name {
                *tools.entry(name.into()).or_default() += 1;
            }
        }
        Counts {
            responses: Measure::unavailable("turn aggregate has no provider response ids"),
            provider_attempts: Measure::unavailable("JSON lines omit provider attempts"),
            retries: Measure::unavailable("JSON lines omit provider retries"),
            completions: Measure::unavailable("turn aggregate does not count provider completions"),
            tool_calls: tools
                .into_iter()
                .map(|(name, value)| {
                    (
                        name,
                        Measure::LowerBound {
                            value,
                            missing: vec!["non-root tool calls absent from JSON lines".into()],
                        },
                    )
                })
                .collect(),
            tool_failures: BTreeMap::new(),
            children: Measure::unavailable("collaboration items do not enumerate every child"),
            conversations: self.thread.as_ref().map_or_else(
                || Measure::unavailable("root thread not observed"),
                |_| Measure::LowerBound {
                    value: 1,
                    missing: vec!["non-root conversations absent from JSON lines".into()],
                },
            ),
            compactions: Measure::unavailable("JSON lines omit compaction scopes"),
            message_terminals: BTreeMap::new(),
        }
    }

    /// Reconcile the last answer, stream errors, settled exit and forced teardown.
    #[must_use]
    pub fn classify(&self, exit: Exit, forced: Forced) -> Classification {
        let accepted = self.answer().is_some();
        let end = if let Some(error) = &self.error {
            End::HarnessError { what: error.to_string() }
        } else if accepted && (exit == Exit::Code(0) || forced != Forced::No) {
            End::Completed
        } else if forced != Forced::No {
            End::Timeout
        } else if accepted {
            End::HarnessError { what: "Codex answer disagrees with exit".into() }
        } else if let Some(detail) = &self.fatal {
            End::Failed { reason: FailureReason::Agent, detail: detail.clone() }
        } else if let LastTurn::Failed(detail) = &self.last {
            End::Failed { reason: FailureReason::Agent, detail: detail.clone() }
        } else if exit == Exit::Code(0) {
            End::HarnessError { what: "Codex exited 0 without a completed answer".into() }
        } else {
            End::Failed { reason: FailureReason::Agent, detail: format!("Codex ended without an answer: {exit:?}") }
        };
        Classification {
            end,
            warnings: if accepted && forced != Forced::No {
                vec![format!("forced teardown after accepted answer: {forced:?}")]
            } else {
                Vec::new()
            },
        }
    }
}
