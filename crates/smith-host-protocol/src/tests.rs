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
