//! Charter and result wire boundaries (protocol/charter.md, sections 3-6).

use skein_codec::Reason;
use skein_lib::Reader;
use smith_charter::v1::{CEILINGS, Charter, Path, RunResult};

#[test]
fn charter_and_result_golden_records_decode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-charter/golden/v1");
    for name in ["record_charter_smallest.bin", "record_charter_full.bin"] {
        let bytes = std::fs::read(root.join(name)).expect("charter golden");
        let value = Charter::decode(&CEILINGS, &mut Reader::new(&bytes)).expect("charter decodes");
        assert_eq!(u32::try_from(bytes.len()).expect("golden length fits"), value.measure());
    }
    for name in ["record_run_result_smallest.bin", "record_run_result_full.bin"] {
        let bytes = std::fs::read(root.join(name)).expect("result golden");
        let value = RunResult::decode(&CEILINGS, &mut Reader::new(&bytes)).expect("result decodes");
        assert_eq!(u32::try_from(bytes.len()).expect("golden length fits"), value.measure());
    }
}

#[test]
fn charter_rejects_an_unsupported_version_before_fields() {
    let problem = Charter::decode(&CEILINGS, &mut Reader::new(&[0, 2])).expect_err("version two");
    assert_eq!((problem.path, problem.reason), (Path::CharterVersion, Reason::Version));
}
