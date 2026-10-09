//! Charter and result wire boundaries (protocol/charter.md, sections 3-6).

use skein_codec::Reason;
use skein_lib::Reader;
use smith_charter::v2::{CEILINGS, Charter, Path, RunResult};

#[test]
fn charter_and_result_golden_records_decode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-charter/golden/v2");
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
    let problem = Charter::decode(&CEILINGS, &mut Reader::new(&[0, 1])).expect_err("version one");
    assert_eq!((problem.path, problem.reason), (Path::CharterVersion, Reason::Version));
}

#[test]
fn llm_goldens_keep_the_window_and_output_as_separate_quantities() {
    use smith_charter::v2::Llm;
    let full = include_bytes!("../../../crates/smith-charter/golden/v2/record_llm_full.bin");
    let value = Llm::decode(&CEILINGS, &mut Reader::new(full)).expect("full LLM entry");
    assert_eq!((value.window(), value.output()), (u32::MAX, u32::MAX));
    let smallest = include_bytes!("../../../crates/smith-charter/golden/v2/record_llm_smallest.bin");
    let value = Llm::decode(&CEILINGS, &mut Reader::new(smallest)).expect("smallest LLM entry");
    assert_eq!((value.window(), value.output()), (0, 0));
}
