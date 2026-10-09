//! The pre-events face keeps parsed facts, calls and content, never agent state.
//! `parse_debug` reads bounded Rust Debug notation; `parse_line` maps records
//! by field name and `Observer` identifies main's finish and the run's answer.
//! Loss, response identities and usage under `calls` remain unavailable;
//! `classify` reconciles the terminal with the legacy process exit, never its
//! terminal text (benchmarks.md, sections 6.2, 8.2, 8.3 and 14).

use std::collections::BTreeMap;
use std::fmt;

use crate::{BudgetLimit, Classification, End, Exit, FailureReason, Forced, Measure};

const MAX_BYTES: usize = 400_000;
const MAX_DEPTH: usize = 64;

/// A legacy shape refused by the reader, naming its record and field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub record: String,
    pub path: String,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "smith legacy {} at {}: {}", self.record, self.path, self.reason)
    }
}

impl std::error::Error for ParseError {}

fn bad(path: &str, reason: &str) -> ParseError {
    ParseError { record: "trace".into(), path: path.into(), reason: reason.into() }
}

/// One Debug value emitted by the legacy binary, preserving named fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A unit enum variant, boolean or other identifier.
    Name(String),
    /// An unsigned integer, as emitted by tokens, times and counters.
    Number(u64),
    /// A quoted Unicode string.
    String(String),
    /// A quoted byte string, retaining non-UTF-8 bytes.
    Bytes(Vec<u8>),
    /// A list or anonymous tuple of values.
    Sequence(Vec<Self>),
    /// A named tuple variant or newtype.
    Tuple { name: String, values: Vec<Self> },
    /// A struct or struct variant with fields looked up by name.
    Struct { name: String, fields: BTreeMap<String, Self> },
}

impl Value {
    fn name(&self) -> Option<&str> {
        match self {
            Self::Name(name) | Self::Tuple { name, .. } | Self::Struct { name, .. } => Some(name),
            Self::Number(_) | Self::String(_) | Self::Bytes(_) | Self::Sequence(_) => None,
        }
    }

    fn field(&self, field: &str) -> Result<&Self, ParseError> {
        if let Self::Struct { fields, .. } = self {
            fields.get(field).ok_or_else(|| bad(field, "missing Debug field"))
        } else {
            Err(bad(field, "expected Debug struct"))
        }
    }

    fn number(&self, path: &str) -> Result<u64, ParseError> {
        if let Self::Number(number) = self { Ok(*number) } else { Err(bad(path, "expected unsigned integer")) }
    }

    fn token(&self, field: &str) -> Result<u64, ParseError> {
        match self.field(field)? {
            Self::Tuple { name, values } if name == "Token" && values.len() == 1 => values[0].number(field),
            Self::Name(_)
            | Self::Number(_)
            | Self::String(_)
            | Self::Bytes(_)
            | Self::Sequence(_)
            | Self::Tuple { .. }
            | Self::Struct { .. } => Err(bad(field, "expected Token(integer)")),
        }
    }

    fn fields(&self, expected: &[&str]) -> Result<(), ParseError> {
        for field in expected {
            self.field(field)?;
        }
        if let Self::Struct { fields, .. } = self {
            for field in fields.keys() {
                if !expected.contains(&field.as_str()) {
                    return Err(bad(field, "unknown Debug field"));
                }
            }
        }
        Ok(())
    }
}

struct Parser<'a> {
    text: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.text.as_bytes().get(self.offset).is_some_and(u8::is_ascii_whitespace) {
            self.offset += 1;
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.text.as_bytes().get(self.offset) == Some(&byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn need(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.take(byte) { Ok(()) } else { Err(bad(&format!("Debug byte {}", self.offset), "missing delimiter")) }
    }

    fn identifier(&mut self) -> Result<String, ParseError> {
        self.space();
        let start = self.offset;
        while self.text.as_bytes().get(self.offset).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_') {
            self.offset += 1;
        }
        if self.offset == start {
            return Err(bad("Debug identifier", "expected name"));
        }
        Ok(self.text[start..self.offset].into())
    }

    fn quoted(&mut self, bytes: bool) -> Result<Value, ParseError> {
        self.need(b'"')?;
        let mut result = Vec::new();
        loop {
            let next = *self.text.as_bytes().get(self.offset).ok_or_else(|| bad("Debug string", "unterminated"))?;
            self.offset += 1;
            match next {
                b'"' => break,
                b'\\' => {
                    let escape =
                        *self.text.as_bytes().get(self.offset).ok_or_else(|| bad("Debug escape", "truncated"))?;
                    self.offset += 1;
                    match escape {
                        b'0' => result.push(0),
                        b'n' => result.push(b'\n'),
                        b'r' => result.push(b'\r'),
                        b't' => result.push(b'\t'),
                        b'\\' | b'"' | b'\'' => result.push(escape),
                        b'x' => {
                            let end = self.offset.checked_add(2).ok_or_else(|| bad("Debug escape", "overflow"))?;
                            let digits =
                                self.text.get(self.offset..end).ok_or_else(|| bad("Debug escape", "truncated hex"))?;
                            let value =
                                u8::from_str_radix(digits, 16).map_err(|_| bad("Debug escape", "invalid hex"))?;
                            if !bytes && !value.is_ascii() {
                                return Err(bad("Debug escape", "non-ASCII string hex escape"));
                            }
                            result.push(value);
                            self.offset = end;
                        }
                        b'u' if !bytes => {
                            self.need(b'{')?;
                            let start = self.offset;
                            while self.text.as_bytes().get(self.offset).is_some_and(u8::is_ascii_hexdigit) {
                                self.offset += 1;
                            }
                            let digits = &self.text[start..self.offset];
                            if digits.is_empty() || digits.len() > 6 {
                                return Err(bad("Debug escape", "invalid Unicode escape"));
                            }
                            let point =
                                u32::from_str_radix(digits, 16).map_err(|_| bad("Debug escape", "invalid Unicode"))?;
                            let character =
                                char::from_u32(point).ok_or_else(|| bad("Debug escape", "invalid Unicode scalar"))?;
                            self.need(b'}')?;
                            result.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        _ => return Err(bad("Debug escape", "unknown escape")),
                    }
                }
                byte if byte < 32 || (bytes && !byte.is_ascii()) => return Err(bad("Debug string", "unescaped byte")),
                byte => result.push(byte),
            }
        }
        if bytes {
            Ok(Value::Bytes(result))
        } else {
            String::from_utf8(result).map(Value::String).map_err(|_| bad("Debug string", "invalid UTF-8"))
        }
    }

    fn sequence(&mut self, close: u8, depth: usize) -> Result<Vec<Value>, ParseError> {
        let mut values = Vec::new();
        if self.take(close) {
            return Ok(values);
        }
        loop {
            values.push(self.value(depth + 1)?);
            if self.take(close) {
                return Ok(values);
            }
            self.need(b',')?;
            if self.take(close) {
                return Ok(values);
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth >= MAX_DEPTH {
            return Err(bad("Debug depth", "nesting limit exceeded"));
        }
        self.space();
        match self.text.as_bytes().get(self.offset) {
            Some(b'"') => self.quoted(false),
            Some(b'b') if self.text.as_bytes().get(self.offset + 1) == Some(&b'"') => {
                self.offset += 1;
                self.quoted(true)
            }
            Some(b'[') => {
                self.offset += 1;
                self.sequence(b']', depth).map(Value::Sequence)
            }
            Some(b'(') => {
                self.offset += 1;
                self.sequence(b')', depth).map(Value::Sequence)
            }
            Some(byte) if byte.is_ascii_digit() => {
                let start = self.offset;
                while self.text.as_bytes().get(self.offset).is_some_and(u8::is_ascii_digit) {
                    self.offset += 1;
                }
                self.text[start..self.offset]
                    .parse()
                    .map(Value::Number)
                    .map_err(|_| bad("Debug number", "integer overflow"))
            }
            Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => {
                let name = self.identifier()?;
                if self.take(b'(') {
                    return Ok(Value::Tuple { name, values: self.sequence(b')', depth)? });
                }
                if !self.take(b'{') {
                    return Ok(Value::Name(name));
                }
                let mut fields = BTreeMap::new();
                if !self.take(b'}') {
                    loop {
                        let field = self.identifier()?;
                        self.need(b':')?;
                        if fields.insert(field.clone(), self.value(depth + 1)?).is_some() {
                            return Err(bad(&field, "duplicate Debug field"));
                        }
                        if self.take(b'}') {
                            break;
                        }
                        self.need(b',')?;
                        if self.take(b'}') {
                            break;
                        }
                    }
                }
                Ok(Value::Struct { name, fields })
            }
            _ => Err(bad(&format!("Debug byte {}", self.offset), "expected Debug value")),
        }
    }
}

/// Parse the whole bounded Debug rendering, refusing prefixes and deep nesting.
pub fn parse_debug(text: &str) -> Result<Value, ParseError> {
    if text.len() > MAX_BYTES {
        return Err(bad("Debug bytes", "record limit exceeded"));
    }
    let mut parser = Parser { text, offset: 0 };
    let value = parser.value(0)?;
    parser.space();
    if parser.offset != text.len() {
        return Err(bad("Debug suffix", "unexpected trailing bytes"));
    }
    Ok(value)
}

/// A lifecycle observation mapped from a named legacy domain fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The agent admitted this run.
    RunAdmitted { run: u64 },
    /// The run opened its main or child conversation.
    ConversationOpened { run: u64, conversation: u64, child: bool },
    /// The run received a call from this conversation.
    RunCalled { run: u64, conversation: u64, call: u64, ask: String },
    /// The run returned the call's terminal.
    RunReturned { run: u64, call: u64, result: String },
    /// The run supplied its answer to its caller.
    RunAnswered { run: u64, answer: Value },
    /// The session admitted its opener.
    SessionOpened { opener: u64 },
    /// The session requested a completion with this retry number.
    CompletionStarted { opener: u64, attempt: u64, messages: u64, max_tokens: u64 },
    /// The provider supplied a completion terminal.
    CompletionAnswered { opener: u64, stop: Value, blocks: u64, calls: u64, invalid: u64 },
    /// The provider completion ended in a failure with transport evidence.
    CompletionFailed { opener: u64, failure: Value, evidence: Value },
    /// The session scheduled another provider attempt.
    CompletionRetried { opener: u64, attempt: u64, delay: Value },
    /// The session charged usage; this is not a per-response content record.
    SessionUsed { opener: u64, usage: Value },
    /// The session supplied its settled terminal and aggregate usage.
    SessionEnded { opener: u64, end: Value, turns: u64, usage: Value },
    /// The tool kit admitted this checkout operation.
    ToolStarted { opener: u64, session: u64, tool: String },
    /// The tool kit supplied the operation's verdict and payload length.
    ToolAnswered { opener: u64, session: u64, tool: String, verdict: Value, bytes: u64 },
    /// Another checked content-free fact retained for subsequent event checks.
    Other { domain: String, value: Value },
}

fn map_fact(domain: &str, value: Value) -> Result<Event, ParseError> {
    let number = |field: &str| value.field(field)?.number(field);
    let text =
        |field: &str| value.field(field)?.name().map(str::to_owned).ok_or_else(|| bad(field, "expected variant"));
    let copy = |field: &str| value.field(field).cloned();
    Ok(match (domain, value.name()) {
        ("Run", Some("Admitted")) => Event::RunAdmitted { run: value.token("run")? },
        ("Run", Some("Opened")) => Event::ConversationOpened {
            run: value.token("run")?,
            conversation: value.token("conversation")?,
            child: value.field("child")?.name() == Some("true"),
        },
        ("Run", Some("Called")) => Event::RunCalled {
            run: value.token("run")?,
            conversation: value.token("conversation")?,
            call: value.token("call")?,
            ask: text("ask")?,
        },
        ("Run", Some("Returned")) => {
            Event::RunReturned { run: value.token("run")?, call: value.token("call")?, result: text("result")? }
        }
        ("Run", Some("Answered")) => Event::RunAnswered { run: value.token("run")?, answer: copy("answer")? },
        ("Session", Some("Opened")) => Event::SessionOpened { opener: value.token("opener")? },
        ("Session", Some("CompletionStarted")) => Event::CompletionStarted {
            opener: value.token("opener")?,
            attempt: number("attempt")?,
            messages: number("messages")?,
            max_tokens: number("max_tokens")?,
        },
        ("Session", Some("CompletionAnswered")) => Event::CompletionAnswered {
            opener: value.token("opener")?,
            stop: copy("stop")?,
            blocks: number("blocks")?,
            calls: number("calls")?,
            invalid: number("invalid")?,
        },
        ("Session", Some("CompletionFailed")) => Event::CompletionFailed {
            opener: value.token("opener")?,
            failure: copy("failure")?,
            evidence: copy("evidence")?,
        },
        ("Session", Some("CompletionRetried")) => Event::CompletionRetried {
            opener: value.token("opener")?,
            attempt: number("attempt")?,
            delay: copy("delay")?,
        },
        ("Session", Some("Used")) => Event::SessionUsed { opener: value.token("opener")?, usage: copy("usage")? },
        ("Session", Some("Ended")) => Event::SessionEnded {
            opener: value.token("opener")?,
            end: copy("end")?,
            turns: number("turns")?,
            usage: copy("usage")?,
        },
        ("Session", Some("Tools")) => {
            let opener = value.token("opener")?;
            let fact = value.field("fact")?;
            match fact.name() {
                Some("Started") => Event::ToolStarted {
                    opener,
                    session: fact.token("session")?,
                    tool: fact.field("tool")?.name().expect("checked tool").into(),
                },
                Some("Answered") => Event::ToolAnswered {
                    opener,
                    session: fact.token("session")?,
                    tool: fact.field("tool")?.name().expect("checked tool").into(),
                    verdict: fact.field("verdict")?.clone(),
                    bytes: fact.field("bytes")?.number("bytes")?,
                },
                _ => Event::Other { domain: domain.into(), value },
            }
        }
        _ => Event::Other { domain: domain.into(), value },
    })
}

/// A neutral outside observation emitted by the legacy reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observation {
    /// A run or session fact, parsed by name without current domain types.
    Fact { at_ns: u64, event: Event },
    /// Exact provider call bytes, supplied under calls or everything capture.
    Call { at_ns: u64, owner: u64, id: Vec<u8>, name: Vec<u8>, input: Vec<u8> },
    /// The legacy tool content record, with no call identity or verdict.
    Tool { at_ns: u64, owner: u64, result_bytes: u64 },
    /// Exact request bytes, supplied only under everything capture.
    Prompt { at_ns: u64, owner: u64, bytes: Vec<u8> },
    /// Provider usage, supplied only under everything capture.
    Usage { at_ns: u64, owner: u64, value: Value },
    /// Completion text, supplied only under everything capture.
    Completion { at_ns: u64, owner: u64, text: String },
}

fn validate_fact(domain: &str, fact: &Value) -> Result<(), ParseError> {
    let name = fact.name().ok_or_else(|| bad("fact", "expected named fact"))?;
    let fields: &[&str] = match (domain, name) {
        ("Run", "Admitted") => &["run"],
        ("Run", "MessageReceived") => &["run", "name", "bytes"],
        ("Run", "MessageRefused") => &["run", "name", "bytes", "reason"],
        ("Run", "MessageRead") => &["run", "name", "turn"],
        ("Run", "MessageFence") => &["run", "turn", "read"],
        ("Run", "MessageUnread") => &["run", "name"],
        ("Run", "Prepared") => &["run", "guides", "checks"],
        ("Run", "Opened") => &["run", "conversation", "child"],
        ("Run", "Ended") => &["run", "conversation", "end"],
        ("Run", "Called") => &["run", "conversation", "call", "ask"],
        ("Run", "Returned") => &["run", "call", "result"],
        ("Run", "CheckStarted") => &["run", "deadline"],
        ("Run", "CheckFinished") => &["run", "exit"],
        ("Run", "Delivered") => &["run", "status"],
        ("Run", "Answered") => &["run", "answer"],
        ("Session", "Opened" | "CompletionCancelled" | "DelegateCancelled") => &["opener"],
        ("Session", "CompletionStarted") => &["opener", "attempt", "messages", "max_tokens"],
        ("Session", "CompletionAnswered") => &["opener", "stop", "blocks", "calls", "invalid"],
        ("Session", "CompletionFailed") => &["opener", "failure", "evidence"],
        ("Session", "CompletionRetried") => &["opener", "attempt", "delay"],
        ("Session", "Tools") => &["opener", "fact"],
        ("Session", "DelegateStarted") => &["opener", "block"],
        ("Session", "DelegateAnswered") => &["opener", "bytes", "error"],
        ("Session", "Yielded") => &["opener", "stop"],
        ("Session", "Used") => &["opener", "usage"],
        ("Session", "Ended") => &["opener", "end", "turns", "usage"],
        ("Tools", "Opened" | "Closed") => &["session"],
        ("Tools", "Refused") => &["session", "refusal"],
        ("Tools", "Started") => &["session", "tool"],
        ("Tools", "Answered") => &["session", "tool", "verdict", "bytes"],
        ("Tools", "Closing") => &["session", "running"],
        _ => return Err(bad("fact", "unknown legacy fact variant")),
    };
    fact.fields(fields)?;
    for field in fields {
        match *field {
            "run" | "conversation" | "call" | "opener" | "session" | "name" => {
                fact.token(field)?;
            }
            "guides" | "checks" | "attempt" | "messages" | "max_tokens" | "blocks" | "calls" | "invalid" | "block"
            | "bytes" | "turns" | "running" => {
                fact.field(field)?.number(field)?;
            }
            "turn" => {
                let turn = fact.field(field)?.number(field)?;
                if turn == 0 || u32::try_from(turn).is_err() {
                    return Err(bad(field, "expected positive u32 turn"));
                }
            }
            "read" => validate_read(fact.field(field)?)?,
            "reason" => unit(fact.field(field)?, field, &["TooLarge", "Full", "NameInUse", "Ending"])?,
            "child" | "error" => {
                if !matches!(fact.field(field)?, Value::Name(value) if value == "true" || value == "false") {
                    return Err(bad(field, "expected boolean"));
                }
            }
            "ask" => unit(fact.field(field)?, field, &["Wait", "Host", "Deliver", "Finish", "SubAgent"])?,
            "result" => unit(
                fact.field(field)?,
                field,
                &[
                    "Waiting",
                    "HostAnswered",
                    "HostTooLarge",
                    "HostUnknown",
                    "HostRejected",
                    "Delivered",
                    "Nothing",
                    "DeliveryRefused",
                    "Accepted",
                    "Rejected",
                    "ChecksFailed",
                    "Stale",
                    "DeliveryFailed",
                    "Cancelled",
                    "TimedOut",
                    "Busy",
                    "Answered",
                    "Unanswered",
                    "Refused",
                ],
            )?,
            "tool" => unit(fact.field(field)?, field, &["Read", "List", "Search", "Write", "Edit", "Shell"])?,
            "answer" => validate_answer(fact.field(field)?)?,
            "fact" => validate_fact("Tools", fact.field(field)?)?,
            "usage" => validate_usage(fact.field(field)?)?,
            _ => {
                if fact.field(field)?.name().is_none() {
                    return Err(bad(field, "expected named Debug value"));
                }
            }
        }
    }
    Ok(())
}

fn validate_read(value: &Value) -> Result<(), ParseError> {
    match value {
        Value::Name(name) if name == "None" => Ok(()),
        Value::Tuple { name, values } if name == "Some" && values.len() == 1 => match &values[0] {
            Value::Tuple { name, values } if name == "Token" && values.len() == 1 => {
                values[0].number("read")?;
                Ok(())
            }
            Value::Name(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Bytes(_)
            | Value::Sequence(_)
            | Value::Tuple { .. }
            | Value::Struct { .. } => Err(bad("read", "expected Some(Token(integer))")),
        },
        Value::Name(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Bytes(_)
        | Value::Sequence(_)
        | Value::Tuple { .. }
        | Value::Struct { .. } => Err(bad("read", "expected optional Token")),
    }
}

fn validate_usage(usage: &Value) -> Result<(), ParseError> {
    if usage.name() != Some("Usage") {
        return Err(bad("usage", "expected Usage"));
    }
    let fields = ["input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens"];
    usage.fields(&fields)?;
    for field in fields {
        usage.field(field)?.number(field)?;
    }
    Ok(())
}

fn hex(value: &str, path: &str) -> Result<Vec<u8>, ParseError> {
    if !value.len().is_multiple_of(2) {
        return Err(bad(path, "odd hexadecimal byte length"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| char::from(byte).to_digit(16).ok_or_else(|| bad(path, "invalid hexadecimal byte"));
            u8::try_from(digit(pair[0])? * 16 + digit(pair[1])?).map_err(|_| bad(path, "invalid hexadecimal byte"))
        })
        .collect()
}

/// Parse one complete JSON line, skipping unknown record types only.
pub fn parse_line(line: &str) -> Result<Option<Observation>, ParseError> {
    if line.len() > MAX_BYTES {
        return Err(bad("line bytes", "record limit exceeded"));
    }
    let json: serde_json::Value = serde_json::from_str(line).map_err(|_| bad("JSON line", "invalid JSON"))?;
    let object = json.as_object().ok_or_else(|| bad("record", "expected JSON object"))?;
    let string = |field: &str| {
        object.get(field).and_then(serde_json::Value::as_str).ok_or_else(|| bad(field, "expected string"))
    };
    let number = |field: &str| {
        object.get(field).and_then(serde_json::Value::as_u64).ok_or_else(|| bad(field, "expected unsigned integer"))
    };
    let kind = string("type")?;
    let expected: &[&str] = match kind {
        "fact" => &["type", "at_ns", "fact"],
        "call" => &["type", "at_ns", "owner", "encoding", "id", "name", "input"],
        "tool" => &["type", "at_ns", "owner", "result_bytes"],
        "prompt" => &["type", "at_ns", "owner", "encoding", "bytes"],
        "usage" => &["type", "at_ns", "owner", "usage"],
        "completion" => &["type", "at_ns", "owner", "text"],
        _ => return Ok(None),
    };
    for field in object.keys() {
        if !expected.contains(&field.as_str()) {
            return Err(bad(field, "unknown JSON field"));
        }
    }
    let at_ns = number("at_ns")?;
    if kind == "fact" {
        let root = parse_debug(string("fact")?)?;
        let domain = root.name().ok_or_else(|| bad("fact", "expected domain wrapper"))?;
        root.fields(&["fact"])?;
        let value = root.field("fact")?.clone();
        if domain != "Run" && domain != "Session" {
            return Err(bad("fact", "expected Run or Session wrapper"));
        }
        validate_fact(domain, &value)?;
        return Ok(Some(Observation::Fact { at_ns, event: map_fact(domain, value)? }));
    }
    let owner = number("owner")?;
    if (kind == "call" || kind == "prompt") && string("encoding")? != "hex" {
        return Err(bad("encoding", "expected hex"));
    }
    let observation = match kind {
        "call" => Observation::Call {
            at_ns,
            owner,
            id: hex(string("id")?, "id")?,
            name: hex(string("name")?, "name")?,
            input: hex(string("input")?, "input")?,
        },
        "tool" => Observation::Tool { at_ns, owner, result_bytes: number("result_bytes")? },
        "prompt" => Observation::Prompt { at_ns, owner, bytes: hex(string("bytes")?, "bytes")? },
        "usage" => {
            let value = parse_debug(string("usage")?)?;
            validate_usage(&value)?;
            Observation::Usage { at_ns, owner, value }
        }
        "completion" => Observation::Completion { at_ns, owner, text: string("text")?.into() },
        _ => unreachable!("known non-fact type"),
    };
    Ok(Some(observation))
}

/// Main's accepted finish observed through its run-issued call identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accepted {
    pub run: u64,
    pub conversation: u64,
    pub at_ns: u64,
}

/// Outside lifecycle evidence retained by the legacy adapter for one process.
#[derive(Clone, Debug, Default)]
pub struct Observer {
    mains: BTreeMap<u64, u64>,
    calls: BTreeMap<(u64, u64), (u64, bool)>,
    accepted: Option<Accepted>,
    answer: Option<Value>,
    error: Option<ParseError>,
}

impl Observer {
    /// Consume one line; a shape error poisons subsequent classification.
    pub fn observe_line(&mut self, line: &str) -> Result<Option<Observation>, ParseError> {
        let result = parse_line(line).and_then(|observation| {
            if let Some(Observation::Fact { at_ns, event }) = &observation {
                self.fact(*at_ns, event)?;
            }
            Ok(observation)
        });
        if let Err(error) = &result {
            self.error = Some(error.clone());
        }
        result
    }

    fn fact(&mut self, at_ns: u64, event: &Event) -> Result<(), ParseError> {
        match event {
            Event::ConversationOpened { run, conversation, child: false } => {
                if self.mains.insert(*run, *conversation).is_some_and(|old| old != *conversation) {
                    return Err(bad("conversation", "conflicting main conversation"));
                }
                self.accepted = None;
                self.answer = None;
            }
            Event::RunCalled { run, conversation, call, ask } => {
                let item = (*conversation, ask == "Finish");
                if self.calls.insert((*run, *call), item).is_some_and(|old| old != item) {
                    return Err(bad("call", "conflicting run call"));
                }
            }
            Event::RunReturned { run, call, result } if result == "Accepted" || result == "Delivered" => {
                let (conversation, finish) =
                    self.calls.get(&(*run, *call)).ok_or_else(|| bad("call", "accepted call lacks its Called fact"))?;
                if *finish && self.mains.get(run) == Some(conversation) {
                    self.accepted = Some(Accepted { run: *run, conversation: *conversation, at_ns });
                }
            }
            Event::RunAnswered { answer, .. } => {
                self.answer = Some(answer.clone());
                if answer != &Value::Name("Accepted".into()) {
                    self.accepted = None;
                }
            }
            Event::RunAdmitted { .. }
            | Event::ConversationOpened { .. }
            | Event::RunReturned { .. }
            | Event::SessionOpened { .. }
            | Event::CompletionStarted { .. }
            | Event::CompletionAnswered { .. }
            | Event::CompletionFailed { .. }
            | Event::CompletionRetried { .. }
            | Event::SessionUsed { .. }
            | Event::SessionEnded { .. }
            | Event::ToolStarted { .. }
            | Event::ToolAnswered { .. }
            | Event::Other { .. } => {}
        }
        Ok(())
    }

    /// Main's acceptance; a child's accepted finish never supplies this terminal.
    #[must_use]
    pub fn accepted(&self) -> Option<Accepted> {
        self.accepted
    }

    /// The run's terminal, where the lossy trace retained its Answered fact.
    #[must_use]
    pub fn answer(&self) -> Option<&Value> {
        self.answer.as_ref()
    }

    /// Calls capture lacks reliable per-response usage, even when Used facts survive.
    #[must_use]
    pub fn usage(&self) -> Measure<()> {
        Measure::unavailable("legacy calls capture has no per-response usage records")
    }

    /// The legacy face cannot observe loss; standard error is never parsed.
    #[must_use]
    pub fn loss(&self) -> Measure<u64> {
        Measure::unavailable("legacy trace has no loss record")
    }

    /// Reconcile main's terminal with the legacy binary's process exit and deadline.
    #[must_use]
    pub fn classify(&self, exit: Exit, forced: Forced) -> Classification {
        let accepted = self.accepted.is_some() || self.answer.as_ref() == Some(&Value::Name("Accepted".into()));
        let end = if let Some(error) = &self.error {
            End::HarnessError { what: error.to_string() }
        } else if accepted && (exit == Exit::Code(0) || forced != Forced::No) {
            End::Completed
        } else if forced != Forced::No {
            End::Timeout
        } else if accepted {
            End::HarnessError { what: "legacy accepted answer disagrees with exit".into() }
        } else if let Some(answer) = &self.answer {
            classify_answer(answer)
        } else if exit == Exit::Code(0) {
            End::HarnessError { what: "legacy exit 0 has no observed main acceptance or run answer".into() }
        } else {
            End::Failed { reason: FailureReason::Agent, detail: format!("legacy exited without an answer: {exit:?}") }
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

fn unit(value: &Value, path: &str, names: &[&str]) -> Result<(), ParseError> {
    if matches!(value, Value::Name(name) if names.contains(&name.as_str())) {
        Ok(())
    } else {
        Err(bad(path, "unknown unit variant or changed shape"))
    }
}

fn one<'a>(value: &'a Value, name: &str, path: &str) -> Result<&'a Value, ParseError> {
    match value {
        Value::Tuple { name: actual, values } if actual == name && values.len() == 1 => Ok(&values[0]),
        Value::Name(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Bytes(_)
        | Value::Sequence(_)
        | Value::Tuple { .. }
        | Value::Struct { .. } => Err(bad(path, "expected one-field tuple variant")),
    }
}

fn validate_answer(answer: &Value) -> Result<(), ParseError> {
    match answer.name() {
        Some("Accepted" | "Parked") => unit(answer, "answer", &["Accepted", "Parked"]),
        Some("Refused") => {
            let refusal = one(answer, "Refused", "answer")?;
            match refusal.name() {
                Some("Busy") => unit(refusal, "answer.refusal", &["Busy"]),
                Some("Invalid") => {
                    one(refusal, "Invalid", "answer.refusal")?;
                    Ok(())
                }
                _ => Err(bad("answer.refusal", "unknown refusal")),
            }
        }
        Some("Failed") => {
            let failure = one(answer, "Failed", "answer")?;
            match failure.name() {
                Some("Cancelled" | "Stale") => unit(failure, "answer.failure", &["Cancelled", "Stale"]),
                Some("Budget" | "Model" | "Policy" | "Transcript") => {
                    one(failure, failure.name().expect("named"), "answer.failure")?;
                    Ok(())
                }
                _ => Err(bad("answer.failure", "unknown failure")),
            }
        }
        _ => Err(bad("answer", "unknown answer")),
    }
}

fn classify_answer(answer: &Value) -> End {
    if let Ok(failure) = one(answer, "Failed", "answer") {
        if let Ok(limit) = one(failure, "Budget", "answer.failure") {
            let which = match limit {
                Value::Name(name) if name == "Turns" => Some(BudgetLimit::Turns),
                Value::Name(name) if name == "Time" => Some(BudgetLimit::Time),
                Value::Name(name) if name == "Spend" => Some(BudgetLimit::Spend),
                Value::Name(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Bytes(_)
                | Value::Sequence(_)
                | Value::Tuple { .. }
                | Value::Struct { .. } => None,
            };
            if let Some(which) = which {
                return End::Budget { which };
            }
        }
        if failure == &Value::Name("Cancelled".into()) {
            return End::Failed { reason: FailureReason::Cancelled, detail: "legacy run cancelled".into() };
        }
    }
    match answer.name() {
        Some("Parked") => End::Failed { reason: FailureReason::InputNeeded, detail: "legacy run parked".into() },
        Some("Refused") => End::Refused { setup: format!("legacy run refused {answer:?}") },
        _ => End::Failed { reason: FailureReason::Agent, detail: format!("legacy run answered {answer:?}") },
    }
}
