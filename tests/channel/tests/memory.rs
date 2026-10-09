//! Both channel halves are measured under their ceiling configuration with
//! Skein's shared counting allocator (protocol/channel.md, section 7;
//! programming-model.md, section 6.3; testing-strategy.md, section 6).

use skein_channel::StreamMode;
use skein_lib::{List, Queue};
use skein_world::domain::heap::{Counting, Meter};
use smith_channel::CEILINGS;
use smith_host_protocol as host;
use smith_protocol_channel as agent;

#[global_allocator]
static HEAP: Counting = Counting;

fn channel_limits() -> skein_channel::Limits {
    let schema = smith_channel::schema(&CEILINGS).expect("ceiling body schema");
    let largest = schema.version(2).expect("version two").kinds.iter().map(|kind| kind.largest).max().expect("kinds");
    skein_channel::Limits {
        chunk: 4096,
        credential: 0,
        skip: 4096,
        output_bytes: largest.checked_add(8).expect("one largest frame"),
        output_frames: 4,
        kinds: 18,
    }
}

#[test]
fn ceiling_limit_halves_and_their_opening_fit_their_combined_worst_case() {
    let channel = channel_limits();
    let host_limits = host::Limits { bodies: CEILINGS, channel, calls: 8 };
    let agent_limits = agent::Limits {
        bodies: CEILINGS,
        charter: smith_charter::CEILINGS,
        transcript: smith_transcript::CEILINGS,
        channel,
        endpoints: 1,
        calls: 8,
        turns: 8,
        fact_reserve_frames: 1,
        fact_reserve_bytes: 128,
        grants: 8,
    };
    let mut entries = List::with_capacity(1);
    entries.push(agent::Endpoint { name: Box::default(), number: 0, dialect: 0, account: 0 }).expect("one endpoint");
    let endpoints = agent::Endpoints::new(entries);
    let bound = host::worst_case(&host_limits)
        .expect("checked host bound")
        .checked_add(agent::worst_case(&agent_limits).expect("checked agent bound"))
        .expect("combined bound");
    let meter = Meter::new();
    meter.start();
    let mut host = host::Component::new(&host_limits, StreamMode::Two).expect("ceiling host");
    let agent = agent::Component::new(&agent_limits, StreamMode::Two, endpoints).expect("ceiling agent");
    let mut host_events = Queue::with_capacity(host::max_out(&host_limits).to_domain);
    let mut below = Queue::with_capacity(host::max_out(&host_limits).below);
    host.open(&mut host_events, &mut below);
    let measurement = meter.end();
    let peak = meter.check(measurement, bound, &"both ceiling channel halves and opening");
    assert!(peak > 0, "the allocator saw the real channel construction");
    drop((host, agent, host_events, below));
}
