//! Turn wire boundaries (protocol/transcript.md, sections 2, 6 and 7).

use skein_codec::Reason;
use skein_lib::Reader;
use smith_transcript::v3::{CEILINGS, Path, Turn};

#[test]
fn turn_goldens_decode_at_their_measured_length() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-transcript/golden/v3");
    for name in ["record_turn_smallest.bin", "record_turn_full.bin"] {
        let bytes = std::fs::read(root.join(name)).expect("turn golden");
        let value = Turn::decode(&CEILINGS, &mut Reader::new(&bytes)).expect("turn decodes");
        assert_eq!(u32::try_from(bytes.len()).expect("golden length fits"), value.measure());
    }
}

#[test]
fn turn_rejects_an_unsupported_version_before_fields() {
    let problem = Turn::decode(&CEILINGS, &mut Reader::new(&[0, 2])).expect_err("version two");
    assert_eq!((problem.path, problem.reason), (Path::TurnVersion, Reason::Version));
}
