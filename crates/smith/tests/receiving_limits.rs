//! Production receiving bounds exercised through the real HTTP/SSE client.

use skein_lib::stream::{Down, Held, Up};
use skein_lib::{Env, Queue, Time, Token, Wall};
use skein_llm::{self as llm, client};
use skein_world::domain::heap::{Counting, Meter};

#[global_allocator]
static HEAP: Counting = Counting;

fn configuration() -> smith::config::Configuration {
    smith::config::parse(
        br#"{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,"endpoints":[],"environment":[]}"#,
    )
    .expect("production standard profile")
}

fn call() -> llm::Call {
    llm::Call {
        owner: Token::new(1),
        endpoint: llm::Endpoint::codex(),
        credential: llm::Credential {
            access_token: b"fake-token".as_slice().into(),
            account_id: b"fake-account".as_slice().into(),
        },
        prompt: llm::Prompt {
            model: b"fake".as_slice().into(),
            instructions: b"Write the source file".as_slice().into(),
            tools: Box::new([]),
            messages: Box::new([]),
            reasoning_effort: None,
            cache_key: None,
            max_output_tokens: None,
        },
    }
}

fn coding_arguments(bytes: usize) -> Vec<u8> {
    let mut arguments = br#"{"path":"source.rs","content":""#.to_vec();
    let padding = bytes.checked_sub(arguments.len() + 2).expect("room for the argument envelope");
    arguments.extend(std::iter::repeat_n(b'x', padding));
    arguments.extend_from_slice(b"\"}");
    assert_eq!(arguments.len(), bytes);
    arguments
}

fn receive(limits: client::Limits, arguments: &[u8]) -> Vec<client::Event> {
    let item = serde_json::json!({
        "type": "function_call", "id": "item", "call_id": "call", "name": "write",
        "arguments": std::str::from_utf8(arguments).expect("UTF-8 arguments")
    });
    let mut body = Vec::new();
    for event in [
        serde_json::json!({"type":"response.output_item.added","output_index":0,"item":{
            "type":"function_call","id":"item","call_id":"call","name":"write","arguments":""
        }}),
        serde_json::json!({"type":"response.function_call_arguments.delta","output_index":0,
            "delta":std::str::from_utf8(arguments).expect("UTF-8 arguments")}),
        serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}),
        serde_json::json!({"type":"response.completed","response":{
            "status":"completed","output":[item],"usage":{"input_tokens":1,"output_tokens":1}
        }}),
    ] {
        body.extend_from_slice(b"data: ");
        body.extend_from_slice(&serde_json::to_vec(&event).expect("provider event"));
        body.extend_from_slice(b"\n\n");
    }
    let mut response =
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n", body.len())
            .into_bytes();
    response.extend_from_slice(&body);
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut client = client::Client::prepare(call(), &limits).expect("prepared call");
    let mut above = Queue::with_capacity(16);
    let mut below = Queue::with_capacity(16);
    client::down(&mut client, &env, client::Request::Start, &mut above, &mut below);
    client::down(&mut client, &env, client::Request::Next, &mut above, &mut below);
    let mut response = Held::new(response.into());
    let mut events = Vec::new();
    for _ in 0..100_000 {
        if let Some(event) = above.pop() {
            let next = matches!(event, client::Event::Delta { .. } | client::Event::Block { .. });
            events.push(event);
            if next {
                client::down(&mut client, &env, client::Request::Next, &mut above, &mut below);
            }
        }
        if let Some(down) = below.pop() {
            match down {
                Down::Demand { room, .. } if room > 0 => {
                    client::up(&mut client, &env, Up::Room, &mut above, &mut below);
                }
                Down::Demand { read, room: 0 } => {
                    if let Some(answer) = response.answer(read) {
                        client::up(&mut client, &env, answer, &mut above, &mut below);
                    }
                }
                Down::Demand { .. } => {}
                Down::Send(_) | Down::Finish => {}
            }
        }
        if client.has_work() {
            client::resume(&mut client, &env, &mut above, &mut below);
        }
        if above.is_empty() && matches!(client.waiting(), client::Waiting::Idle | client::Waiting::Closing) {
            break;
        }
    }
    assert!(matches!(client.waiting(), client::Waiting::Idle | client::Waiting::Closing), "bounded fixture settles");
    events
}

#[test]
fn production_argument_boundary_completes_replays_and_refuses_one_extra_byte() {
    let limits = configuration().service.limits.llm.connection.llm;
    let bytes = usize::try_from(limits.dialect.input_bytes).expect("argument bound");
    assert_eq!(bytes, 32_768);
    let arguments = coding_arguments(bytes);
    let first = receive(limits, &arguments);
    assert_eq!(first, receive(limits, &arguments), "the receiving boundary replays exactly");
    let completion = first
        .iter()
        .find_map(|event| match event {
            client::Event::Completed { completion, .. } => Some(completion),
            _ => None,
        })
        .expect("a full coding argument completes");
    assert!(matches!(&completion.content[0], llm::Block::ToolCall { arguments: actual, replay: Some(_), .. }
        if actual.as_ref() == arguments));
    assert_eq!(completion.stop, llm::Stop::ToolUse);
    let mut replay = call();
    replay.prompt.messages = Box::new([
        llm::Message { role: llm::Role::Assistant, content: completion.content.clone() },
        llm::Message {
            role: llm::Role::User,
            content: Box::new([llm::Block::ToolResult {
                id: b"call".as_slice().into(),
                text: b"written".as_slice().into(),
                is_error: false,
            }]),
        },
    ]);
    client::Client::prepare(replay, &limits).expect("the accepted argument and replay fit a follow-up request");
    let excessive = receive(limits, &coding_arguments(bytes + 1));
    assert!(excessive.iter().any(|event| matches!(
        event,
        client::Event::Failed { failure: llm::Failure::Limit, evidence: client::Evidence::Response, .. }
    )));
    let mut old = limits;
    old.dialect.input_bytes = 2048;
    let old_failure = receive(old, &coding_arguments(3000));
    assert!(
        old_failure.iter().any(|event| matches!(
            event,
            client::Event::Failed { failure: llm::Failure::Limit, evidence: client::Evidence::Response, .. }
        )),
        "the former profile reproduces the benchmark's exact typed failure"
    );
}

#[test]
fn production_startup_reserves_a_practical_heap_within_its_checked_bound() {
    let configuration = configuration();
    let bound = smith_agent_service::worst_case(&configuration.service.limits).expect("checked service bound");
    assert!(bound <= 1_099_511_627_776, "standard service fits the existing accounting budget");
    let meter = Meter::new();
    meter.start();
    let service = smith_agent_service::Service::new(configuration.service, 1).expect("production startup");
    let measured = meter.end();
    let peak = meter.check(measured, bound, &"production standard service startup");
    assert!(peak < 64 * 1024 * 1024, "eager reservations remain practical: {peak} bytes");
    eprintln!("production standard service eager heap={peak}, checked worst case={bound}");
    drop(service);
}

#[test]
fn an_argument_at_the_byte_boundary_fits_after_worst_case_json_escaping() {
    let limits = configuration().service.limits.llm.connection.llm;
    let bytes = usize::try_from(limits.dialect.input_bytes).expect("argument bound");
    // The shared client preserves even malformed JSON argument strings for
    // application validation. A NUL expands to six bytes in the SSE document.
    let arguments = vec![0; bytes];
    let meter = Meter::new();
    let bound = client::worst_case(&limits).expect("checked client receiving bound");
    meter.start();
    let events = receive(limits, &arguments);
    let measured = meter.end();
    let peak = meter.check(measured, bound, &"escaped boundary response including fixture buffers");
    assert!(events.iter().any(|event| matches!(event, client::Event::Completed { .. })));
    eprintln!("escaped argument boundary heap including fixtures={peak}, client bound={bound}");
}
