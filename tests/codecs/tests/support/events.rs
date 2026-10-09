//! Typed fixture values for frozen event goldens.

use skein_lib::List;
use smith_events::{
    Agent, AnswerClass, AnswerFailure, Block, Budget, Capture, CheckCompleted, CheckStarted, CommandExit, Completion,
    CompletionFailure, ConversationClosed, ConversationEnd, ConversationKind, ConversationOpened, Count, Cpu,
    Delivered, Delivery, Effect, Event, Evidence, FailureClass, Family, Field, Form, Level, Loss, Message, Mode, Model,
    Notice, NoticeKind, Outcome, PeakRss, Prompt, Record, ResponseCompleted, ResponseStarted, Role, RunCompleted,
    RunResult, RunStarted, SessionEnded, SessionStarted, Source, Status, Stop, TextDelta, ToolCompleted, ToolStarted,
    Tools, Usage, Verdict, Versions,
};

fn value_capture(name: &str, spelling: &str) -> Capture {
    if name != "Capture" {
        return Capture::None;
    }
    match spelling {
        "none" => Capture::None,
        "calls" => Capture::Calls,
        "everything" => Capture::Everything,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_delivery(name: &str, spelling: &str) -> Delivery {
    if name != "Delivery" {
        return Delivery::Complete;
    }
    match spelling {
        "complete" => Delivery::Complete,
        "best_effort" => Delivery::BestEffort,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_mode(name: &str, spelling: &str) -> Mode {
    if name != "Mode" {
        return Mode::Agent;
    }
    match spelling {
        "agent" => Mode::Agent,
        "exec" => Mode::Exec,
        "chat" => Mode::Chat,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_family(name: &str, spelling: &str) -> Family {
    if name != "Family" {
        return Family::Inspect;
    }
    match spelling {
        "inspect" => Family::Inspect,
        "modify" => Family::Modify,
        "shell" => Family::Shell,
        "sub_agents" => Family::SubAgents,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_form(name: &str, spelling: &str) -> Form {
    if name != "Form" {
        return Form::Report;
    }
    match spelling {
        "report" => Form::Report,
        "verdict" => Form::Verdict,
        "change" => Form::Change,
        "failure" => Form::Failure,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_status(name: &str, spelling: &str) -> Status {
    if name != "Status" {
        return Status::Accepted;
    }
    match spelling {
        "accepted" => Status::Accepted,
        "parked" => Status::Parked,
        "failed" => Status::Failed,
        "refused" => Status::Refused,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_conversation_kind(name: &str, spelling: &str) -> ConversationKind {
    if name != "ConversationKind" {
        return ConversationKind::Main;
    }
    match spelling {
        "main" => ConversationKind::Main,
        "child" => ConversationKind::Child,
        "compaction" => ConversationKind::Compaction,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_conversation_end(name: &str, spelling: &str) -> ConversationEnd {
    if name != "ConversationEnd" {
        return ConversationEnd::Closed;
    }
    match spelling {
        "closed" => ConversationEnd::Closed,
        "budget" => ConversationEnd::Budget,
        "failed" => ConversationEnd::Failed,
        "refused" => ConversationEnd::Refused,
        "transcript" => ConversationEnd::Transcript,
        "overflow" => ConversationEnd::Overflow,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_outcome(name: &str, spelling: &str) -> Outcome {
    if name != "Outcome" {
        return Outcome::Completed;
    }
    match spelling {
        "completed" => Outcome::Completed,
        "failed" => Outcome::Failed,
        "cancelled" => Outcome::Cancelled,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_stop(name: &str, spelling: &str) -> Stop {
    if name != "Stop" {
        return Stop::End;
    }
    match spelling {
        "end" => Stop::End,
        "tools" => Stop::Tools,
        "max_tokens" => Stop::MaxTokens,
        "refusal" => Stop::Refusal,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_source(name: &str, spelling: &str) -> Source {
    if name != "Source" {
        return Source::Workspace;
    }
    match spelling {
        "workspace" => Source::Workspace,
        "run" => Source::Run,
        "host" => Source::Host,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_effect(name: &str, spelling: &str) -> Effect {
    if name != "Effect" {
        return Effect::Read;
    }
    match spelling {
        "read" => Effect::Read,
        "write" => Effect::Write,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_verdict(name: &str, spelling: &str) -> Verdict {
    if name != "Verdict" {
        return Verdict::Read;
    }
    match spelling {
        "read" => Verdict::Read,
        "listed" => Verdict::Listed,
        "found" => Verdict::Found,
        "written" => Verdict::Written,
        "edited" => Verdict::Edited,
        "exited" => Verdict::Exited,
        "conflict" => Verdict::Conflict,
        "missing" => Verdict::Missing,
        "too_large" => Verdict::TooLarge,
        "timed_out" => Verdict::TimedOut,
        "failed" => Verdict::Failed,
        "cancelled" => Verdict::Cancelled,
        "ambiguous" => Verdict::Ambiguous,
        "result" => Verdict::Result,
        "error" => Verdict::Error,
        "invalid" => Verdict::Invalid,
        "not_run" => Verdict::NotRun,
        "withdrawn" => Verdict::Withdrawn,
        "not_granted" => Verdict::NotGranted,
        "outside" => Verdict::Outside,
        "read_only" => Verdict::ReadOnly,
        "too_long" => Verdict::TooLong,
        "not_found" => Verdict::NotFound,
        "not_file" => Verdict::NotFile,
        "linked" => Verdict::Linked,
        "protected" => Verdict::Protected,
        "not_directory" => Verdict::NotDirectory,
        "not_read" => Verdict::NotRead,
        "stale" => Verdict::Stale,
        "no_match" => Verdict::NoMatch,
        "unchanged" => Verdict::Unchanged,
        "busy" => Verdict::Busy,
        "nul_byte" => Verdict::NulByte,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_delivered(name: &str, spelling: &str) -> Delivered {
    if name != "Delivered" {
        return Delivered::Delivered;
    }
    match spelling {
        "delivered" => Delivered::Delivered,
        "nothing" => Delivered::Nothing,
        "refused" => Delivered::Refused,
        "failed" => Delivered::Failed,
        "stale" => Delivered::Stale,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_failure_class(name: &str, spelling: &str) -> FailureClass {
    if name != "FailureClass" {
        return FailureClass::Overloaded;
    }
    match spelling {
        "overloaded" => FailureClass::Overloaded,
        "rate_limited" => FailureClass::RateLimited,
        "exhausted" => FailureClass::Exhausted,
        "unavailable" => FailureClass::Unavailable,
        "timed_out" => FailureClass::TimedOut,
        "context_too_long" => FailureClass::ContextTooLong,
        "invalid" => FailureClass::Invalid,
        "unauthorized" => FailureClass::Unauthorized,
        "limit" => FailureClass::Limit,
        "protocol" => FailureClass::Protocol,
        "cancelled" => FailureClass::Cancelled,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_evidence(name: &str, spelling: &str) -> Evidence {
    if name != "Evidence" {
        return Evidence::Unsent;
    }
    match spelling {
        "unsent" => Evidence::Unsent,
        "maybe_sent" => Evidence::MaybeSent,
        "response" => Evidence::Response,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_answer_class(name: &str, spelling: &str) -> AnswerClass {
    if name != "AnswerClass" {
        return AnswerClass::Model;
    }
    match spelling {
        "model" => AnswerClass::Model,
        "budget" => AnswerClass::Budget,
        "policy" => AnswerClass::Policy,
        "cancelled" => AnswerClass::Cancelled,
        "stale" => AnswerClass::Stale,
        "transcript" => AnswerClass::Transcript,
        "busy" => AnswerClass::Busy,
        "invalid" => AnswerClass::Invalid,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_level(name: &str, spelling: &str) -> Level {
    if name != "Level" {
        return Level::Info;
    }
    match spelling {
        "info" => Level::Info,
        "warning" => Level::Warning,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_notice_kind(name: &str, spelling: &str) -> NoticeKind {
    if name != "NoticeKind" {
        return NoticeKind::CredentialRejected;
    }
    match spelling {
        "credential_rejected" => NoticeKind::CredentialRejected,
        "account_exhausted" => NoticeKind::AccountExhausted,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_role(name: &str, spelling: &str) -> Role {
    if name != "Role" {
        return Role::User;
    }
    match spelling {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        _ => panic!("fixture enum spelling"),
    }
}

fn value_agent(_name: &str, _spelling: &str) -> Agent {
    Agent {
        name: Box::from("sample\n\"🌍".as_bytes()),
        version: Box::from("sample\n\"🌍".as_bytes()),
        build: Box::from("sample\n\"🌍".as_bytes()),
    }
}

fn value_versions(_name: &str, _spelling: &str) -> Versions {
    Versions { events: u64::MAX, channel: u64::MAX, charter: u64::MAX, transcript: u64::MAX }
}

fn value_loss(_name: &str, _spelling: &str) -> Loss {
    Loss { events: u64::MAX, channel: Some(u64::MAX) }
}

fn value_cpu(_name: &str, _spelling: &str) -> Cpu {
    Cpu { user: u64::MAX, system: u64::MAX, children_user: u64::MAX, children_system: u64::MAX }
}

fn value_peak_rss(_name: &str, _spelling: &str) -> PeakRss {
    PeakRss { self_bytes: u64::MAX, children: u64::MAX }
}

fn value_model(_name: &str, _spelling: &str) -> Model {
    Model {
        endpoint: Box::from("sample\n\"🌍".as_bytes()),
        model: Box::from("sample\n\"🌍".as_bytes()),
        output_tokens: u64::MAX,
        effort: Some(Box::from("sample\n\"🌍".as_bytes())),
    }
}

fn value_usage(_name: &str, _spelling: &str) -> Usage {
    Usage {
        input_tokens: Some(u64::MAX),
        cache_read_tokens: Some(u64::MAX),
        cache_write_tokens: Some(u64::MAX),
        output_tokens: Some(u64::MAX),
        reasoning_tokens: Some(u64::MAX),
    }
}

fn value_budget(_name: &str, _spelling: &str) -> Budget {
    Budget { turns: u64::MAX, spend: u64::MAX, time_ms: u64::MAX }
}

fn value_tools(name: &str, spelling: &str) -> Tools {
    Tools { families: list_family(name, spelling), wait: true, deliver: true, host: list_text(name, spelling) }
}

fn value_completion_failure(name: &str, spelling: &str) -> CompletionFailure {
    CompletionFailure {
        class: value_failure_class(name, spelling),
        evidence: value_evidence(name, spelling),
        retry_after_ms: Some(u64::MAX),
    }
}

fn value_answer_failure(name: &str, spelling: &str) -> AnswerFailure {
    AnswerFailure {
        class: value_answer_class(name, spelling),
        completion: Some(value_completion_failure(name, spelling)),
        account: Some(u64::MAX),
        which: Some(Box::from("sample\n\"🌍".as_bytes())),
        reason: Some(Box::from("sample\n\"🌍".as_bytes())),
    }
}

fn value_field(_name: &str, _spelling: &str) -> Field {
    Field { name: Box::from("sample\n\"🌍".as_bytes()), text: Box::from("sample\n\"🌍".as_bytes()) }
}

fn value_run_result(name: &str, spelling: &str) -> RunResult {
    RunResult {
        form: value_form(name, spelling),
        label: Some(Box::from("sample\n\"🌍".as_bytes())),
        text: Some(Box::from("sample\n\"🌍".as_bytes())),
        fields: Some(list_field(name, spelling)),
    }
}

fn value_message(name: &str, spelling: &str) -> Message {
    Message { role: value_role(name, spelling), blocks: list_block(name, spelling) }
}

fn value_prompt(name: &str, spelling: &str) -> Prompt {
    Prompt {
        system: Some(Box::from("sample\n\"🌍".as_bytes())),
        tools: Some(list_text(name, spelling)),
        messages: list_message(name, spelling),
    }
}

fn value_completion(name: &str, spelling: &str) -> Completion {
    Completion { blocks: list_block(name, spelling) }
}

fn value_command_exit(_name: &str, _spelling: &str) -> CommandExit {
    CommandExit { code: i64::MIN }
}

fn value_session_started(name: &str, spelling: &str) -> SessionStarted {
    SessionStarted {
        wall_ms: u64::MAX,
        agent: value_agent(name, spelling),
        mode: value_mode(name, spelling),
        pid: u64::MAX,
        profile: Box::from("sample\n\"🌍".as_bytes()),
        capture: value_capture(name, spelling),
        delivery: value_delivery(name, spelling),
        versions: value_versions(name, spelling),
    }
}

fn value_session_ended(name: &str, spelling: &str) -> SessionEnded {
    SessionEnded {
        exit: i64::MIN,
        answered: true,
        teardown_ms: Some(u64::MAX),
        emitted: list_count(name, spelling),
        loss: value_loss(name, spelling),
        cpu_ms: value_cpu(name, spelling),
        peak_rss_bytes: value_peak_rss(name, spelling),
    }
}

fn value_run_started(name: &str, spelling: &str) -> RunStarted {
    RunStarted {
        run: u64::MAX,
        resumed: true,
        main: value_model(name, spelling),
        budget: value_budget(name, spelling),
        tools: value_tools(name, spelling),
        contract: list_form(name, spelling),
    }
}

fn value_run_completed(name: &str, spelling: &str) -> RunCompleted {
    RunCompleted {
        run: u64::MAX,
        status: value_status(name, spelling),
        failure: Some(value_answer_failure(name, spelling)),
        result: Some(value_run_result(name, spelling)),
        turns: u64::MAX,
        spent: u64::MAX,
        usage: value_usage(name, spelling),
        duration_ms: u64::MAX,
    }
}

fn value_conversation_opened(name: &str, spelling: &str) -> ConversationOpened {
    ConversationOpened {
        run: u64::MAX,
        conversation: u64::MAX,
        kind: value_conversation_kind(name, spelling),
        parent: Some(u64::MAX),
        call: Some(Box::from("sample\n\"🌍".as_bytes())),
        model: value_model(name, spelling),
    }
}

fn value_conversation_closed(name: &str, spelling: &str) -> ConversationClosed {
    ConversationClosed {
        run: u64::MAX,
        conversation: u64::MAX,
        end: value_conversation_end(name, spelling),
        which: Some(Box::from("sample\n\"🌍".as_bytes())),
        failure: Some(value_completion_failure(name, spelling)),
        turns: u64::MAX,
        usage: value_usage(name, spelling),
        spent: u64::MAX,
        duration_ms: u64::MAX,
    }
}

fn value_response_started(name: &str, spelling: &str) -> ResponseStarted {
    ResponseStarted {
        run: u64::MAX,
        conversation: u64::MAX,
        response: u64::MAX,
        turn: u64::MAX,
        attempt: u64::MAX,
        model: value_model(name, spelling),
        messages: u64::MAX,
        output_tokens: u64::MAX,
        prompt: Some(value_prompt(name, spelling)),
    }
}

fn value_response_completed(name: &str, spelling: &str) -> ResponseCompleted {
    ResponseCompleted {
        run: u64::MAX,
        conversation: u64::MAX,
        response: u64::MAX,
        outcome: value_outcome(name, spelling),
        stop: Some(value_stop(name, spelling)),
        blocks: u64::MAX,
        calls: u64::MAX,
        invalid: u64::MAX,
        usage: value_usage(name, spelling),
        spent: u64::MAX,
        request_bytes: Some(u64::MAX),
        first_byte_ms: Some(u64::MAX),
        largest_gap_ms: Some(u64::MAX),
        duration_ms: u64::MAX,
        failure: Some(value_completion_failure(name, spelling)),
        retry_ms: Some(u64::MAX),
        completion: Some(value_completion(name, spelling)),
    }
}

fn value_text_delta(_name: &str, _spelling: &str) -> TextDelta {
    TextDelta {
        run: u64::MAX,
        conversation: u64::MAX,
        response: u64::MAX,
        block: u64::MAX,
        text: Box::from("sample\n\"🌍".as_bytes()),
    }
}

fn value_tool_started(name: &str, spelling: &str) -> ToolStarted {
    ToolStarted {
        run: u64::MAX,
        conversation: u64::MAX,
        call: Box::from("sample\n\"🌍".as_bytes()),
        tool: Box::from("sample\n\"🌍".as_bytes()),
        source: value_source(name, spelling),
        effect: value_effect(name, spelling),
        deadline_ms: Some(u64::MAX),
        input_bytes: u64::MAX,
        input: Some(Box::from("sample\n\"🌍".as_bytes())),
    }
}

fn value_tool_completed(name: &str, spelling: &str) -> ToolCompleted {
    ToolCompleted {
        run: u64::MAX,
        conversation: u64::MAX,
        call: Box::from("sample\n\"🌍".as_bytes()),
        tool: Box::from("sample\n\"🌍".as_bytes()),
        verdict: value_verdict(name, spelling),
        exit: Some(value_command_exit(name, spelling)),
        bytes: u64::MAX,
        duration_ms: u64::MAX,
        delivery: Some(value_delivered(name, spelling)),
        result: Some(Box::from("sample\n\"🌍".as_bytes())),
    }
}

fn value_check_started(_name: &str, _spelling: &str) -> CheckStarted {
    CheckStarted { run: u64::MAX, deadline_ms: Some(u64::MAX) }
}

fn value_check_completed(name: &str, spelling: &str) -> CheckCompleted {
    CheckCompleted { run: u64::MAX, exit: value_command_exit(name, spelling), passed: true, duration_ms: u64::MAX }
}

fn value_notice(name: &str, spelling: &str) -> Notice {
    Notice {
        level: value_level(name, spelling),
        kind: value_notice_kind(name, spelling),
        run: Some(u64::MAX),
        account: Some(u64::MAX),
        bytes: None,
        wait_ms: Some(u64::MAX),
    }
}

fn list_block(_name: &str, _spelling: &str) -> List<Block> {
    let mut values = List::with_capacity(7);
    values.push(Block::Text { text: Box::from("sample\n\"🌍".as_bytes()) }).expect("block room");
    values.push(Block::Refusal { text: Box::from("sample\n\"🌍".as_bytes()) }).expect("block room");
    values
        .push(Block::Call {
            call: Box::from("sample\n\"🌍".as_bytes()),
            tool: Box::from("sample\n\"🌍".as_bytes()),
            input: Box::from("sample\n\"🌍".as_bytes()),
        })
        .expect("block room");
    values
        .push(Block::Oversized {
            call: Box::from("sample\n\"🌍".as_bytes()),
            tool: Box::from("sample\n\"🌍".as_bytes()),
            bytes: u64::MAX,
        })
        .expect("block room");
    values
        .push(Block::Cut {
            call: Box::from("sample\n\"🌍".as_bytes()),
            tool: Box::from("sample\n\"🌍".as_bytes()),
            input: Box::from("sample\n\"🌍".as_bytes()),
        })
        .expect("block room");
    values
        .push(Block::Result {
            call: Box::from("sample\n\"🌍".as_bytes()),
            error: true,
            text: Box::from("sample\n\"🌍".as_bytes()),
        })
        .expect("block room");
    values.push(Block::Opaque { bytes: u64::MAX }).expect("block room");
    values
}

fn list_count(_name: &str, _spelling: &str) -> List<Count> {
    let mut values = List::with_capacity(1);
    values.push(Count { record: Box::from(b"notice".as_slice()), count: u64::MAX }).expect("one count");
    values
}

fn list_family(name: &str, spelling: &str) -> List<Family> {
    let mut values = List::with_capacity(1);
    values.push(value_family(name, spelling)).expect("fixture room");
    values
}

fn list_field(name: &str, spelling: &str) -> List<Field> {
    let mut values = List::with_capacity(1);
    values.push(value_field(name, spelling)).expect("fixture room");
    values
}

fn list_form(name: &str, spelling: &str) -> List<Form> {
    let mut values = List::with_capacity(1);
    values.push(value_form(name, spelling)).expect("fixture room");
    values
}

fn list_message(name: &str, spelling: &str) -> List<Message> {
    let mut values = List::with_capacity(1);
    values.push(value_message(name, spelling)).expect("fixture room");
    values
}

fn list_text(_name: &str, _spelling: &str) -> List<Box<[u8]>> {
    let mut values = List::with_capacity(1);
    values.push(Box::from("sample\n\"🌍".as_bytes())).expect("fixture room");
    values
}

fn samples_0() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "session_started_none",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Capture", "none")) },
            Capture::None,
        ),
        (
            "session_started_calls",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "session_started_everything",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "session_ended_none",
            Record { t_ms: u64::MAX, event: Event::SessionEnded(value_session_ended("Capture", "none")) },
            Capture::None,
        ),
        (
            "session_ended_calls",
            Record { t_ms: u64::MAX, event: Event::SessionEnded(value_session_ended("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "session_ended_everything",
            Record { t_ms: u64::MAX, event: Event::SessionEnded(value_session_ended("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "run_started_none",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Capture", "none")) },
            Capture::None,
        ),
        (
            "run_started_calls",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "run_started_everything",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "run_completed_none",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Capture", "none")) },
            Capture::None,
        ),
    ]
}

fn samples_1() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "run_completed_calls",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "run_completed_everything",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "conversation_opened_none",
            Record { t_ms: u64::MAX, event: Event::ConversationOpened(value_conversation_opened("Capture", "none")) },
            Capture::None,
        ),
        (
            "conversation_opened_calls",
            Record { t_ms: u64::MAX, event: Event::ConversationOpened(value_conversation_opened("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "conversation_opened_everything",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationOpened(value_conversation_opened("Capture", "everything")),
            },
            Capture::Everything,
        ),
        (
            "conversation_closed_none",
            Record { t_ms: u64::MAX, event: Event::ConversationClosed(value_conversation_closed("Capture", "none")) },
            Capture::None,
        ),
        (
            "conversation_closed_calls",
            Record { t_ms: u64::MAX, event: Event::ConversationClosed(value_conversation_closed("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "conversation_closed_everything",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("Capture", "everything")),
            },
            Capture::Everything,
        ),
        (
            "response_started_none",
            Record { t_ms: u64::MAX, event: Event::ResponseStarted(value_response_started("Capture", "none")) },
            Capture::None,
        ),
        (
            "response_started_calls",
            Record { t_ms: u64::MAX, event: Event::ResponseStarted(value_response_started("Capture", "calls")) },
            Capture::Calls,
        ),
    ]
}

fn samples_2() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "response_started_everything",
            Record { t_ms: u64::MAX, event: Event::ResponseStarted(value_response_started("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "response_completed_none",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Capture", "none")) },
            Capture::None,
        ),
        (
            "response_completed_calls",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "response_completed_everything",
            Record {
                t_ms: u64::MAX,
                event: Event::ResponseCompleted(value_response_completed("Capture", "everything")),
            },
            Capture::Everything,
        ),
        (
            "text_delta_everything",
            Record { t_ms: u64::MAX, event: Event::TextDelta(value_text_delta("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "tool_started_none",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Capture", "none")) },
            Capture::None,
        ),
        (
            "tool_started_calls",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "tool_started_everything",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "tool_completed_none",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Capture", "none")) },
            Capture::None,
        ),
        (
            "tool_completed_calls",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Capture", "calls")) },
            Capture::Calls,
        ),
    ]
}

fn samples_3() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "tool_completed_everything",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "check_started_none",
            Record { t_ms: u64::MAX, event: Event::CheckStarted(value_check_started("Capture", "none")) },
            Capture::None,
        ),
        (
            "check_started_calls",
            Record { t_ms: u64::MAX, event: Event::CheckStarted(value_check_started("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "check_started_everything",
            Record { t_ms: u64::MAX, event: Event::CheckStarted(value_check_started("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "check_completed_none",
            Record { t_ms: u64::MAX, event: Event::CheckCompleted(value_check_completed("Capture", "none")) },
            Capture::None,
        ),
        (
            "check_completed_calls",
            Record { t_ms: u64::MAX, event: Event::CheckCompleted(value_check_completed("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "check_completed_everything",
            Record { t_ms: u64::MAX, event: Event::CheckCompleted(value_check_completed("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "notice_none",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("Capture", "none")) },
            Capture::None,
        ),
        (
            "notice_calls",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("Capture", "calls")) },
            Capture::Calls,
        ),
        (
            "notice_everything",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("Capture", "everything")) },
            Capture::Everything,
        ),
    ]
}

fn samples_4() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "capture_none",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Capture", "none")) },
            Capture::Everything,
        ),
        (
            "capture_calls",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Capture", "calls")) },
            Capture::Everything,
        ),
        (
            "capture_everything",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Capture", "everything")) },
            Capture::Everything,
        ),
        (
            "delivery_complete",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Delivery", "complete")) },
            Capture::Everything,
        ),
        (
            "delivery_best_effort",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Delivery", "best_effort")) },
            Capture::Everything,
        ),
        (
            "mode_agent",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Mode", "agent")) },
            Capture::Everything,
        ),
        (
            "mode_exec",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Mode", "exec")) },
            Capture::Everything,
        ),
        (
            "mode_chat",
            Record { t_ms: u64::MAX, event: Event::SessionStarted(value_session_started("Mode", "chat")) },
            Capture::Everything,
        ),
        (
            "family_modify",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Family", "modify")) },
            Capture::Everything,
        ),
        (
            "family_shell",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Family", "shell")) },
            Capture::Everything,
        ),
    ]
}

fn samples_5() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "family_sub_agents",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Family", "sub_agents")) },
            Capture::Everything,
        ),
        (
            "form_report",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Form", "report")) },
            Capture::Everything,
        ),
        (
            "form_verdict",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Form", "verdict")) },
            Capture::Everything,
        ),
        (
            "form_change",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Form", "change")) },
            Capture::Everything,
        ),
        (
            "form_failure",
            Record { t_ms: u64::MAX, event: Event::RunStarted(value_run_started("Form", "failure")) },
            Capture::Everything,
        ),
        (
            "status_accepted",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Status", "accepted")) },
            Capture::Everything,
        ),
        (
            "status_parked",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Status", "parked")) },
            Capture::Everything,
        ),
        (
            "status_failed",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Status", "failed")) },
            Capture::Everything,
        ),
        (
            "status_refused",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Status", "refused")) },
            Capture::Everything,
        ),
        (
            "conversation_kind_main",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationOpened(value_conversation_opened("ConversationKind", "main")),
            },
            Capture::Everything,
        ),
    ]
}

fn samples_6() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "conversation_kind_child",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationOpened(value_conversation_opened("ConversationKind", "child")),
            },
            Capture::Everything,
        ),
        (
            "conversation_kind_compaction",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationOpened(value_conversation_opened("ConversationKind", "compaction")),
            },
            Capture::Everything,
        ),
        (
            "conversation_end_closed",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("ConversationEnd", "closed")),
            },
            Capture::Everything,
        ),
        (
            "conversation_end_budget",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("ConversationEnd", "budget")),
            },
            Capture::Everything,
        ),
        (
            "conversation_end_failed",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("ConversationEnd", "failed")),
            },
            Capture::Everything,
        ),
        (
            "conversation_end_refused",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("ConversationEnd", "refused")),
            },
            Capture::Everything,
        ),
        (
            "conversation_end_transcript",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("ConversationEnd", "transcript")),
            },
            Capture::Everything,
        ),
        (
            "conversation_end_overflow",
            Record {
                t_ms: u64::MAX,
                event: Event::ConversationClosed(value_conversation_closed("ConversationEnd", "overflow")),
            },
            Capture::Everything,
        ),
        (
            "outcome_completed",
            Record {
                t_ms: u64::MAX,
                event: Event::ResponseCompleted(value_response_completed("Outcome", "completed")),
            },
            Capture::Everything,
        ),
        (
            "outcome_failed",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Outcome", "failed")) },
            Capture::Everything,
        ),
    ]
}

fn samples_7() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "outcome_cancelled",
            Record {
                t_ms: u64::MAX,
                event: Event::ResponseCompleted(value_response_completed("Outcome", "cancelled")),
            },
            Capture::Everything,
        ),
        (
            "stop_end",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Stop", "end")) },
            Capture::Everything,
        ),
        (
            "stop_tools",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Stop", "tools")) },
            Capture::Everything,
        ),
        (
            "stop_max_tokens",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Stop", "max_tokens")) },
            Capture::Everything,
        ),
        (
            "stop_refusal",
            Record { t_ms: u64::MAX, event: Event::ResponseCompleted(value_response_completed("Stop", "refusal")) },
            Capture::Everything,
        ),
        (
            "source_workspace",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Source", "workspace")) },
            Capture::Everything,
        ),
        (
            "source_run",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Source", "run")) },
            Capture::Everything,
        ),
        (
            "source_host",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Source", "host")) },
            Capture::Everything,
        ),
        (
            "effect_read",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Effect", "read")) },
            Capture::Everything,
        ),
        (
            "effect_write",
            Record { t_ms: u64::MAX, event: Event::ToolStarted(value_tool_started("Effect", "write")) },
            Capture::Everything,
        ),
    ]
}

fn samples_8() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "verdict_read",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "read")) },
            Capture::Everything,
        ),
        (
            "verdict_listed",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "listed")) },
            Capture::Everything,
        ),
        (
            "verdict_found",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "found")) },
            Capture::Everything,
        ),
        (
            "verdict_written",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "written")) },
            Capture::Everything,
        ),
        (
            "verdict_edited",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "edited")) },
            Capture::Everything,
        ),
        (
            "verdict_exited",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "exited")) },
            Capture::Everything,
        ),
        (
            "verdict_conflict",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "conflict")) },
            Capture::Everything,
        ),
        (
            "verdict_missing",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "missing")) },
            Capture::Everything,
        ),
        (
            "verdict_too_large",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "too_large")) },
            Capture::Everything,
        ),
        (
            "verdict_timed_out",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "timed_out")) },
            Capture::Everything,
        ),
    ]
}

fn samples_9() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "verdict_failed",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "failed")) },
            Capture::Everything,
        ),
        (
            "verdict_cancelled",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "cancelled")) },
            Capture::Everything,
        ),
        (
            "verdict_ambiguous",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "ambiguous")) },
            Capture::Everything,
        ),
        (
            "verdict_result",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "result")) },
            Capture::Everything,
        ),
        (
            "verdict_error",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "error")) },
            Capture::Everything,
        ),
        (
            "verdict_invalid",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "invalid")) },
            Capture::Everything,
        ),
        (
            "verdict_not_run",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "not_run")) },
            Capture::Everything,
        ),
        (
            "verdict_withdrawn",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "withdrawn")) },
            Capture::Everything,
        ),
        (
            "verdict_not_granted",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "not_granted")) },
            Capture::Everything,
        ),
        (
            "verdict_outside",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "outside")) },
            Capture::Everything,
        ),
    ]
}

fn samples_10() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "verdict_read_only",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "read_only")) },
            Capture::Everything,
        ),
        (
            "verdict_too_long",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "too_long")) },
            Capture::Everything,
        ),
        (
            "verdict_not_found",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "not_found")) },
            Capture::Everything,
        ),
        (
            "verdict_not_file",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "not_file")) },
            Capture::Everything,
        ),
        (
            "verdict_linked",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "linked")) },
            Capture::Everything,
        ),
        (
            "verdict_protected",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "protected")) },
            Capture::Everything,
        ),
        (
            "verdict_not_directory",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "not_directory")) },
            Capture::Everything,
        ),
        (
            "verdict_not_read",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "not_read")) },
            Capture::Everything,
        ),
        (
            "verdict_stale",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "stale")) },
            Capture::Everything,
        ),
        (
            "verdict_no_match",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "no_match")) },
            Capture::Everything,
        ),
    ]
}

fn samples_11() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "verdict_unchanged",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "unchanged")) },
            Capture::Everything,
        ),
        (
            "verdict_busy",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "busy")) },
            Capture::Everything,
        ),
        (
            "verdict_nul_byte",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Verdict", "nul_byte")) },
            Capture::Everything,
        ),
        (
            "delivered_delivered",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Delivered", "delivered")) },
            Capture::Everything,
        ),
        (
            "delivered_nothing",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Delivered", "nothing")) },
            Capture::Everything,
        ),
        (
            "delivered_refused",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Delivered", "refused")) },
            Capture::Everything,
        ),
        (
            "delivered_failed",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Delivered", "failed")) },
            Capture::Everything,
        ),
        (
            "delivered_stale",
            Record { t_ms: u64::MAX, event: Event::ToolCompleted(value_tool_completed("Delivered", "stale")) },
            Capture::Everything,
        ),
        (
            "failure_class_rate_limited",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "rate_limited")) },
            Capture::Everything,
        ),
        (
            "failure_class_exhausted",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "exhausted")) },
            Capture::Everything,
        ),
    ]
}

fn samples_12() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "failure_class_unavailable",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "unavailable")) },
            Capture::Everything,
        ),
        (
            "failure_class_timed_out",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "timed_out")) },
            Capture::Everything,
        ),
        (
            "failure_class_context_too_long",
            Record {
                t_ms: u64::MAX,
                event: Event::RunCompleted(value_run_completed("FailureClass", "context_too_long")),
            },
            Capture::Everything,
        ),
        (
            "failure_class_invalid",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "invalid")) },
            Capture::Everything,
        ),
        (
            "failure_class_unauthorized",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "unauthorized")) },
            Capture::Everything,
        ),
        (
            "failure_class_limit",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "limit")) },
            Capture::Everything,
        ),
        (
            "failure_class_protocol",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "protocol")) },
            Capture::Everything,
        ),
        (
            "failure_class_cancelled",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("FailureClass", "cancelled")) },
            Capture::Everything,
        ),
        (
            "evidence_maybe_sent",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Evidence", "maybe_sent")) },
            Capture::Everything,
        ),
        (
            "evidence_response",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("Evidence", "response")) },
            Capture::Everything,
        ),
    ]
}

fn samples_13() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "answer_class_budget",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "budget")) },
            Capture::Everything,
        ),
        (
            "answer_class_policy",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "policy")) },
            Capture::Everything,
        ),
        (
            "answer_class_cancelled",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "cancelled")) },
            Capture::Everything,
        ),
        (
            "answer_class_stale",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "stale")) },
            Capture::Everything,
        ),
        (
            "answer_class_transcript",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "transcript")) },
            Capture::Everything,
        ),
        (
            "answer_class_busy",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "busy")) },
            Capture::Everything,
        ),
        (
            "answer_class_invalid",
            Record { t_ms: u64::MAX, event: Event::RunCompleted(value_run_completed("AnswerClass", "invalid")) },
            Capture::Everything,
        ),
        (
            "level_info",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("Level", "info")) },
            Capture::Everything,
        ),
        (
            "level_warning",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("Level", "warning")) },
            Capture::Everything,
        ),
        (
            "notice_kind_credential_rejected",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("NoticeKind", "credential_rejected")) },
            Capture::Everything,
        ),
    ]
}

fn samples_14() -> Vec<(&'static str, Record, Capture)> {
    vec![
        (
            "notice_kind_account_exhausted",
            Record { t_ms: u64::MAX, event: Event::Notice(value_notice("NoticeKind", "account_exhausted")) },
            Capture::Everything,
        ),
        (
            "role_assistant",
            Record { t_ms: u64::MAX, event: Event::ResponseStarted(value_response_started("Role", "assistant")) },
            Capture::Everything,
        ),
    ]
}

pub fn samples() -> Vec<(&'static str, Record, Capture)> {
    let mut samples = vec![("notice_kind_reasoning_dropped", Record { t_ms: u64::MAX, event: Event::Notice(Notice {
        level: Level::Warning, kind: NoticeKind::ReasoningDropped, run: Some(u64::MAX), account: None, wait_ms: None, bytes: Some(u64::MAX),
    }) }, Capture::Everything)];
    samples.extend(samples_0());
    samples.extend(samples_1());
    samples.extend(samples_2());
    samples.extend(samples_3());
    samples.extend(samples_4());
    samples.extend(samples_5());
    samples.extend(samples_6());
    samples.extend(samples_7());
    samples.extend(samples_8());
    samples.extend(samples_9());
    samples.extend(samples_10());
    samples.extend(samples_11());
    samples.extend(samples_12());
    samples.extend(samples_13());
    samples.extend(samples_14());
    samples.extend(null_samples());
    samples
}

fn null_samples() -> Vec<(&'static str, Record, Capture)> {
    let mut response = value_response_completed("", "");
    response.stop = None;
    response.usage = Usage {
        input_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        output_tokens: None,
        reasoning_tokens: None,
    };
    response.request_bytes = None;
    response.first_byte_ms = None;
    response.largest_gap_ms = None;
    response.failure = None;
    response.retry_ms = None;
    response.completion = None;
    vec![(
        "response_unknown_measures",
        Record { t_ms: u64::MAX, event: Event::ResponseCompleted(response) },
        Capture::Everything,
    )]
}
