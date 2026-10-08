//! Opening entry points (protocol/channel.md, section 2).
use skein_channel::{Lower, StreamMode};
use skein_lib::{Queue, stream};

use crate::{Component, Limits, OpenEvent};

#[test]
fn a_start_keeps_host_bytes_and_service_values_in_order() {
    use alloc::boxed::Box;
    use skein_lib::{Duration, Reader, Token};
    use smith_host_domain::channel;

    let limits = Limits {
        bodies: smith_channel::CEILINGS,
        calls: 8,
        channel: skein_channel::Limits {
            chunk: 8,
            credential: 0,
            skip: 8,
            output_bytes: u32::MAX,
            output_frames: 2,
            kinds: 17,
        },
    };
    let start = channel::Start {
        logical_run: Token::new(2),
        activation: 3,
        workspace: Some(Token::new(4)),
        charter: Box::from(*b"charter"),
        transcript: Some(Box::from([Box::from(*b"turn")])),
        answered: Box::from([channel::AnsweredCall {
            name: channel::CallName { activation: 1, completion: 2, position: 3 },
            tool: Box::from(*b"check"),
            reply: channel::SavedReply::Host { error: false, body: Box::from(*b"ok") },
        }]),
        directories: Box::from([channel::Directory {
            name: Box::from(*b"src"),
            writable: true,
            git: true,
            conflicts: Box::from([Box::from(*b"file")]),
        }]),
        grants: Box::from([channel::Grant { account: 5, generation: 6, valid: Duration::from_nanos(7) }]),
    };
    let values =
        crate::Values { paths: Box::from([Box::from(*b"/tmp/src")]), credentials: Box::from([Box::from(*b"secret")]) };
    let frame = crate::encode_start(start, channel::Window { turns: 1, bytes: 4096 }, values, &limits.bodies)
        .expect("bounded Start");
    assert_eq!(frame.kind(), 0x0100);
    let mut reader = Reader::new(&frame.bytes()[8..]);
    let start = smith_channel::Start::decode(&limits.bodies, &mut reader).expect("Start decodes");
    assert_eq!(start.activation(), 3);
    assert_eq!(start.charter(), b"charter");
    assert_eq!(start.transcript().get(0).expect("saved turn").as_ref(), b"turn");
    assert_eq!(start.answered().get(0).expect("answered call").tool(), b"check");
    assert_eq!(
        start.workspace().as_ref().expect("workspace").directories().get(0).expect("directory").path(),
        b"/tmp/src"
    );
    assert_eq!(start.grants().get(0).expect("grant").value().credential(), b"secret");
}

#[test]
fn the_host_sends_open_before_the_domain_hears_anything() {
    let limits = Limits {
        bodies: smith_channel::CEILINGS,
        calls: 8,
        channel: skein_channel::Limits {
            chunk: 8,
            credential: 0,
            skip: 8,
            output_bytes: u32::MAX,
            output_frames: 2,
            kinds: 17,
        },
    };
    let mut component = Component::new(&limits, StreamMode::Two).expect("checked schema");
    let mut to_service = Queue::<OpenEvent>::with_capacity(2);
    let mut below = Queue::<Lower>::with_capacity(8);
    component.open(&mut to_service, &mut below);
    assert!(to_service.is_empty());
    match below.pop() {
        Some(Lower::Read(stream::Down::Demand { .. })) => {}
        other => panic!("expected opening read: {other:?}"),
    }
    match below.pop() {
        Some(Lower::Write(stream::OutputDown::Room { .. })) => {}
        other => panic!("expected Open write room: {other:?}"),
    }
}

fn process_limits() -> Limits {
    Limits {
        bodies: smith_channel::CEILINGS,
        calls: 8,
        channel: skein_channel::Limits {
            chunk: 8,
            credential: 0,
            skip: 8,
            output_bytes: u32::MAX,
            output_frames: 2,
            kinds: 17,
        },
    }
}

fn launch() -> crate::Launch {
    use alloc::boxed::Box;
    crate::Launch {
        program: Box::from(&b"smith"[..]),
        arguments: Box::from([Box::from(&b"agent"[..])]),
        environment: Box::from([Box::from(&b"LANG=C"[..])]),
        root: skein_io::kernel::Fd::new(4),
        directory: Box::from(&b"."[..]),
    }
}

#[test]
fn spawn_has_three_credential_free_standard_pipes() {
    use skein_io::{Request as IoRequest, kernel};
    use skein_lib::{Time, Token};

    let agent = Token::new(7);
    let mut process = crate::Process::new(agent, &process_limits(), 8).expect("channel");
    let mut below = Queue::with_capacity(8);
    process.spawn(launch(), Time::from_nanos(100), &mut below);
    assert_eq!(process.next_deadline(), Some(Time::from_nanos(100)));
    let Some(IoRequest::Spawn { owner, spawn }) = below.pop() else {
        panic!("one lower spawn");
    };
    assert_eq!(owner, process.io_owner());
    assert_eq!(&*spawn.program, b"smith");
    assert_eq!(&*spawn.args[0], b"agent");
    assert_eq!(&*spawn.env[0], b"LANG=C");
    assert_eq!(spawn.pipes.len(), 3);
    assert_eq!(spawn.pipes[0], kernel::Pipe { child: 0, way: kernel::Way::In });
    assert_eq!(spawn.pipes[1], kernel::Pipe { child: 1, way: kernel::Way::Out });
    assert_eq!(spawn.pipes[2], kernel::Pipe { child: 2, way: kernel::Way::Out });
}

#[test]
#[expect(clippy::wildcard_enum_match_arm, reason = "test rejects every other terminal")]
fn refused_spawn_reports_one_unspawned_terminal() {
    use skein_io::{Error as IoError, Event as IoEvent};
    use skein_lib::{Time, Token};

    let agent = Token::new(7);
    let mut process = crate::Process::new(agent, &process_limits(), 8).expect("channel");
    let io_owner = process.io_owner();
    let mut below = Queue::with_capacity(8);
    let mut above = Queue::with_capacity(8);
    process.spawn(launch(), Time::from_nanos(100), &mut below);
    below.pop();
    process.from_io(
        Time::from_nanos(2),
        IoEvent::Failed { owner: io_owner, error: IoError::Busy },
        &mut above,
        &mut below,
    );
    process.from_io(Time::from_nanos(2), IoEvent::Closed { owner: io_owner }, &mut above, &mut below);
    match above.pop().expect("one terminal") {
        crate::ProcessEvent::Unspawned { agent: observed, detail } => {
            assert_eq!(observed, agent);
            assert!(detail.is_empty());
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(above.is_empty());
}

#[test]
#[expect(clippy::wildcard_enum_match_arm, reason = "test rejects every other terminal")]
fn expired_opening_kills_child_and_keeps_errors_tail() {
    use alloc::boxed::Box;
    use skein_io::{Event as IoEvent, Request as IoRequest};
    use skein_lib::{Time, Token, stream};

    let agent = Token::new(7);
    let child = Token::new(8);
    let input = Token::new(9);
    let output = Token::new(10);
    let error = Token::new(11);
    let mut process = crate::Process::new(agent, &process_limits(), 4).expect("channel");
    let io_owner = process.io_owner();
    let mut below = Queue::with_capacity(16);
    let mut above = Queue::with_capacity(8);
    process.spawn(launch(), Time::from_nanos(100), &mut below);
    below.pop();
    process.from_io(
        Time::from_nanos(101),
        IoEvent::Spawned { owner: io_owner, child, pipes: Box::from([input, output, error]) },
        &mut above,
        &mut below,
    );
    assert_eq!(process.next_deadline(), None);
    assert_eq!(below.pop(), Some(IoRequest::Abort { entity: child }));
    assert_eq!(below.pop(), Some(IoRequest::Abort { entity: input }));
    assert_eq!(below.pop(), Some(IoRequest::Abort { entity: output }));
    assert_eq!(
        below.pop(),
        Some(IoRequest::Stream { stream: error, down: stream::Down::Demand { read: stream::Read::Fill(1), room: 0 } })
    );
    process.from_io(
        Time::from_nanos(102),
        IoEvent::Stream { owner: error, up: stream::Up::Bytes(Box::from(&b"abcdef"[..])) },
        &mut above,
        &mut below,
    );
    below.pop();
    process.from_io(
        Time::from_nanos(102),
        IoEvent::Stream { owner: error, up: stream::Up::End },
        &mut above,
        &mut below,
    );
    assert_eq!(below.pop(), Some(IoRequest::Close { entity: error }));
    process.from_io(
        Time::from_nanos(103),
        IoEvent::Exited { owner: io_owner, exit: skein_io::kernel::Exit::Code(1) },
        &mut above,
        &mut below,
    );
    below.pop();
    below.pop();
    for owner in [input, output, error, io_owner] {
        process.from_io(Time::from_nanos(104), IoEvent::Closed { owner }, &mut above, &mut below);
    }
    match above.pop().expect("unspawned") {
        crate::ProcessEvent::Unspawned { agent: observed, detail } => {
            assert_eq!(observed, agent);
            assert_eq!(&*detail, b"cdef");
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(above.is_empty());
}

#[test]
fn host_stop_signals_keep_the_child_identity() {
    use alloc::boxed::Box;
    use skein_io::{Event as IoEvent, Request as IoRequest, kernel};
    use skein_lib::{Time, Token};

    let agent = Token::new(7);
    let child = Token::new(8);
    let mut process = crate::Process::new(agent, &process_limits(), 4).expect("channel");
    let io_owner = process.io_owner();
    let mut below = Queue::with_capacity(16);
    let mut above = Queue::with_capacity(8);
    process.spawn(launch(), Time::from_nanos(100), &mut below);
    below.pop();
    process.from_io(
        Time::from_nanos(1),
        IoEvent::Spawned { owner: io_owner, child, pipes: Box::from([Token::new(9), Token::new(10), Token::new(11)]) },
        &mut above,
        &mut below,
    );
    below.pop();
    process.signal(kernel::Signal::Terminate, &mut below);
    process.signal(kernel::Signal::Kill, &mut below);
    assert_eq!(below.pop(), Some(IoRequest::Signal { child, signal: kernel::Signal::Terminate }));
    assert_eq!(below.pop(), Some(IoRequest::Signal { child, signal: kernel::Signal::Kill }));
}
