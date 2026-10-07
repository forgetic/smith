//! Opening entry points (protocol/channel.md, section 2).
use skein_channel::{Lower, StreamMode};
use skein_lib::{Queue, stream};

use crate::{Component, Limits, OpenEvent};

#[test]
fn the_host_sends_open_before_the_domain_hears_anything() {
    let limits = Limits {
        bodies: smith_channel::CEILINGS,
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
