//! Channel kind and body checks (protocol/channel.md, sections 2-4).

use skein_channel::Direction;
use skein_lib::Reader;
use smith_channel::{Answer, CEILINGS, Start, V1, schema};

#[test]
fn channel_goldens_decode_at_their_measured_length() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-channel/golden/v1");
    for name in ["record_start_smallest.bin", "record_start_full.bin"] {
        let bytes = std::fs::read(root.join(name)).expect("start golden");
        let value = Start::decode(&CEILINGS, &mut Reader::new(&bytes)).expect("start decodes");
        assert_eq!(u32::try_from(bytes.len()).expect("golden length fits"), value.measure());
    }
    for name in ["record_answer_smallest.bin", "record_answer_full.bin"] {
        let bytes = std::fs::read(root.join(name)).expect("answer golden");
        let value = Answer::decode(&CEILINGS, &mut Reader::new(&bytes)).expect("answer decodes");
        assert_eq!(u32::try_from(bytes.len()).expect("golden length fits"), value.measure());
    }
}

#[test]
fn every_required_kind_has_its_direction_and_bound_in_the_schema() {
    let table = schema(&CEILINGS).expect("schema bounds fit");
    assert_eq!(table.magic, *b"smth");
    assert_eq!(table.versions.len(), 1);
    let version = table.version(1).expect("version one");
    assert_eq!(version.kinds.len(), 17);
    for (index, rule) in V1.iter().enumerate() {
        assert!(rule.required, "all initial kinds are required");
        let kind = version.kind(rule.kind).expect("required kind is listed");
        assert_eq!(kind.direction, rule.direction);
        if [0x0105, 0x0106, 0x010c].contains(&rule.kind) {
            assert_eq!(kind.largest, 0);
        } else {
            assert!(kind.largest > 0);
        }
        assert_eq!(usize::from(rule.kind), 0x0100 + index);
        if index < 6 {
            assert_eq!(rule.direction, Direction::FromInitiator);
        } else {
            assert_eq!(rule.direction, Direction::FromResponder);
        }
    }
}
