//! Deterministic arbitrary bytes through each top-level family record
//! (protocol/charter.md, section 8; protocol/transcript.md, section 9;
//! protocol/channel.md, section 10).

use skein_lib::{Reader, Rng, Writer};

macro_rules! check {
    ($bytes:expr, $family:ident, $record:ident) => {
        if let Ok(value) = $family::$record::decode(&$family::CEILINGS, &mut Reader::new($bytes)) {
            let mut writer = Writer::new(usize::try_from(value.measure()).expect("measured length"));
            value.encode(&mut writer).expect("measured room");
            assert_eq!(writer.finish().as_ref(), $bytes);
        }
    };
}

fn check_all(bytes: &[u8]) {
    check!(bytes, smith_charter, Charter);
    check!(bytes, smith_charter, RunResult);
    check!(bytes, smith_transcript, Turn);
    check!(bytes, smith_channel, Start);
    check!(bytes, smith_channel, Message);
    check!(bytes, smith_channel, HostAnswer);
    check!(bytes, smith_channel, Acknowledge);
    check!(bytes, smith_channel, GrantRefresh);
    check!(bytes, smith_channel, Cancel);
    check!(bytes, smith_channel, Admitted);
    check!(bytes, smith_channel, Call);
    check!(bytes, smith_channel, Withdraw);
    check!(bytes, smith_channel, Turn);
    check!(bytes, smith_channel, Waiting);
    check!(bytes, smith_channel, Long);
    check!(bytes, smith_channel, LongDone);
    check!(bytes, smith_channel, Rejected);
    check!(bytes, smith_channel, Exhausted);
    check!(bytes, smith_channel, Fact);
    check!(bytes, smith_channel, Answer);
}

#[test]
fn arbitrary_and_mutated_wires_never_panic_and_round_trip_when_valid() {
    let mut rng = Rng::new(0x51_17_0c_0d_ec_01);
    let mut buffer = [0_u8; 128];
    for _round in 0_u32..10_000 {
        let length = usize::try_from(rng.below(129)).expect("bounded length");
        for byte in buffer.get_mut(..length).expect("within buffer") {
            *byte = u8::try_from(rng.next_u64() & 0xff).expect("low byte");
        }
        check_all(buffer.get(..length).expect("within buffer"));
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    for (family, version, record) in [
        ("smith-charter", "v1", "charter"),
        ("smith-charter", "v1", "run_result"),
        ("smith-transcript", "v2", "turn"),
        ("smith-channel", "v1", "start"),
        ("smith-channel", "v1", "message"),
        ("smith-channel", "v1", "host_answer"),
        ("smith-channel", "v1", "acknowledge"),
        ("smith-channel", "v1", "grant_refresh"),
        ("smith-channel", "v1", "cancel"),
        ("smith-channel", "v1", "admitted"),
        ("smith-channel", "v1", "call"),
        ("smith-channel", "v1", "withdraw"),
        ("smith-channel", "v1", "turn"),
        ("smith-channel", "v1", "waiting"),
        ("smith-channel", "v1", "long"),
        ("smith-channel", "v1", "long_done"),
        ("smith-channel", "v1", "rejected"),
        ("smith-channel", "v1", "exhausted"),
        ("smith-channel", "v1", "fact"),
        ("smith-channel", "v1", "answer"),
    ] {
        let name = format!("record_{record}_full.bin");
        let golden = std::fs::read(root.join(family).join("golden").join(version).join(name)).expect("golden readable");
        check_all(&golden);
        for _round in 0_u32..100 {
            let mut mutated = golden.clone();
            if !mutated.is_empty() {
                let index =
                    usize::try_from(rng.below(u64::try_from(mutated.len()).expect("length fits"))).expect("index fits");
                *mutated.get_mut(index).expect("chosen byte") = u8::try_from(rng.next_u64() & 0xff).expect("low byte");
            }
            check_all(&mutated);
        }
    }
}
