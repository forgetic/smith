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

fn repository_history() -> smith_domain::llm::Prompt {
    use smith_domain::{llm as domain, tools};
    let mut messages = vec![domain::Message {
        role: domain::Role::User,
        content: Box::new([domain::Block::Text { text: b"Read the repository".as_slice().into(), replay: None }]),
    }];
    // Twenty answers of four ordinary reads retain eighty bounded windows.
    // Each source window is within the actual standard read allowance.
    for batch in 0..20 {
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for index in 0..4 {
            let id = format!("read_{}", batch * 4 + index).into_bytes().into_boxed_slice();
            calls.push(domain::Block::ToolCall {
                id: id.clone(),
                name: b"read".as_slice().into(),
                input: br#"{"path":"source.rs"}"#.as_slice().into(),
                replay: None,
            });
            results.push(domain::Block::ToolResult {
                id,
                result: domain::Returned::Owned {
                    outcome: tools::Outcome::Read {
                        content: vec![b'x'; 4096].into_boxed_slice(),
                        skipped: 0,
                        lines: 1,
                        total: 1,
                        cut: false,
                    },
                },
            });
        }
        messages.push(domain::Message { role: domain::Role::Assistant, content: calls.into_boxed_slice() });
        messages.push(domain::Message { role: domain::Role::User, content: results.into_boxed_slice() });
    }
    domain::Prompt {
        endpoint: domain::Endpoint(0),
        model: b"fake".as_slice().into(),
        system: b"repository reading".as_slice().into(),
        tools: tools::Grants { inspect: true, modify: false, shell: false },
        served: Box::new([]),
        messages: messages.into_boxed_slice(),
        max_tokens: 4096,
    }
}

fn prepared_repository(
    limits: smith_protocol_llm::Limits,
    receiving: smith_protocol_llm::Receiving,
) -> Result<smith_protocol_llm::Prepared, llm::Error> {
    let input = smith_protocol_llm::Input {
        owner: Token::new(1),
        prompt: repository_history(),
        endpoint_name: smith_domain::llm::Endpoint(0),
        endpoint: llm::Endpoint::codex(),
        credential: call().credential,
        application: Box::new([]),
        results: Box::new([]),
        receiving,
    };
    let contract =
        smith_domain::run::outcome::OutcomeSpec { change: None, verdicts: Box::new([]), report: None, failure: None };
    smith_protocol_llm::prepare_for_contract(input, &contract, None, &limits)
}

#[test]
fn production_adapter_admits_repository_history_beyond_the_previous_request_cap() {
    let production = configuration().service.limits.llm;
    assert_eq!(production.adapter.client.dialect.request_bytes, 1_048_576);
    assert_eq!(production.adapter.client.dialect.parts, 256, "the observed failure is bytes, not block count");
    assert_eq!(production.adapter.client.dialect.opaque_bytes, 8192);
    prepared_repository(production.adapter, production.receiving)
        .expect("eighty standard read windows fit the production request allowance");
    let mut previous = production.adapter;
    previous.client.dialect.request_bytes = 262_144;
    assert!(
        matches!(prepared_repository(previous, production.receiving), Err(llm::Error::Limit)),
        "the actual adapter and codec reproduce the former unsent refusal"
    );
}

fn request_blocks(payload: usize) -> Box<[llm::Message]> {
    let mut messages = Vec::new();
    let mut remaining = payload;
    for _ in 0..32 {
        if remaining == 0 {
            break;
        }
        let size = remaining.min(60_000);
        messages.push(llm::Message {
            role: llm::Role::User,
            content: Box::new([llm::Block::Text { text: vec![b'x'; size].into_boxed_slice(), replay: None }]),
        });
        remaining -= size;
    }
    assert_eq!(remaining, 0, "bounded fixture payload");
    messages.into_boxed_slice()
}

fn request_call(payload: usize) -> llm::Call {
    let mut request = call();
    request.prompt.instructions = b"repository reading".as_slice().into();
    request.prompt.messages = request_blocks(payload);
    request
}

fn wire_size(payload: usize, limits: &llm::DocumentLimits) -> u32 {
    let request = request_call(payload);
    let input = request
        .prompt
        .messages
        .into_vec()
        .into_iter()
        .map(|message| {
            let [llm::Block::Text { text, .. }] = message.content.as_ref() else { panic!("fixture text") };
            llm::openai::Input::Message {
                role: llm::openai::Role::User,
                text: text.clone(),
                id: None,
                phase: None,
                refusal: false,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    llm::openai::measure_request(
        &llm::openai::Request {
            model: request.prompt.model,
            instructions: request.prompt.instructions,
            tools: Box::new([]),
            input,
            effort: None,
            prompt_cache_key: None,
        },
        limits,
    )
    .expect("measurement allowance")
}

#[test]
fn exact_production_request_boundary_replays_and_refuses_one_more_encoded_byte() {
    let limits = configuration().service.limits.llm.connection.llm;
    let mut measurement = limits.dialect;
    measurement.request_bytes = measurement.request_bytes.checked_add(4096).expect("fixture envelope room");
    let maximum = usize::try_from(limits.dialect.request_bytes).expect("request bound");
    let mut low = 0;
    let mut high = maximum;
    for _ in 0..32 {
        if low >= high {
            break;
        }
        let middle = low + (high - low).div_ceil(2);
        if wire_size(middle, &measurement) <= limits.dialect.request_bytes {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    assert_eq!(
        wire_size(low, &measurement),
        limits.dialect.request_bytes,
        "the fixture reaches the exact encoded boundary"
    );
    assert_eq!(wire_size(low + 1, &measurement), limits.dialect.request_bytes + 1);
    let meter = Meter::new();
    let bound = client::worst_case(&limits).expect("checked production client bound");
    meter.start();
    let accepted = client::Client::prepare(request_call(low), &limits).expect("at-boundary request admitted");
    let measured = meter.end();
    let peak = meter.check(measured, bound, &"production encoded request boundary with caller-owned input");
    drop(accepted);
    client::Client::prepare(request_call(low), &limits).expect("the boundary replays exactly");
    assert!(
        matches!(client::Client::prepare(request_call(low + 1), &limits), Err(llm::Error::Limit)),
        "one more encoded byte refuses before any stream exists"
    );
    eprintln!("production request boundary heap={peak}, client bound={bound}");
}
