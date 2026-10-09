//! Frozen generated code and bytes follow codec.md, section 6.

use std::{fs, path::Path};

use skein_codegen::{generate, parse};

#[test]
fn charter_schema_matches_generated_code_and_all_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-charter");
    let source = fs::read_to_string(root.join("schema/charter-v2.schema")).expect("charter schema");
    let schema = parse(&source).expect("schema valid");
    let output = generate(&schema);
    let committed = fs::read_to_string(root.join("src/generated/v2.rs")).expect("generated code");
    assert_eq!(committed, output.rust, "regenerate charter codec with skein-codegen");
    for golden in output.goldens {
        let committed = fs::read(root.join("golden/v2").join(&golden.name)).expect("golden bytes");
        assert_eq!(committed, golden.bytes, "golden {} drifted", golden.name);
    }
}

#[test]
fn transcript_schema_matches_generated_code_and_all_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-transcript");
    let source = fs::read_to_string(root.join("schema/transcript-v2.schema")).expect("transcript schema");
    let schema = parse(&source).expect("schema valid");
    let output = generate(&schema);
    let committed = fs::read_to_string(root.join("src/generated/v2.rs")).expect("generated code");
    assert_eq!(committed, output.rust, "regenerate transcript codec with skein-codegen");
    for golden in output.goldens {
        let committed = fs::read(root.join("golden/v2").join(&golden.name)).expect("golden bytes");
        assert_eq!(committed, golden.bytes, "golden {} drifted", golden.name);
    }
}

#[test]
fn channel_schema_matches_generated_code_and_all_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-channel");
    let source = fs::read_to_string(root.join("schema/channel-v2.schema")).expect("channel schema");
    let schema = parse(&source).expect("schema valid");
    let output = generate(&schema);
    let committed = fs::read_to_string(root.join("src/generated/v2.rs")).expect("generated code");
    assert_eq!(committed, output.rust, "regenerate channel codec with skein-codegen");
    for golden in output.goldens {
        let committed = fs::read(root.join("golden/v2").join(&golden.name)).expect("golden bytes");
        assert_eq!(committed, golden.bytes, "golden {} drifted", golden.name);
    }
}

#[path = "support/events.rs"]
mod events;

#[test]
fn event_records_and_every_listed_value_match_frozen_version_one_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-events/golden/v1");
    let limits = smith_events::Limits { string: 64, content: 1024, items: 8 };
    let samples = events::samples();
    assert_eq!(fs::read_dir(&root).expect("event goldens").count(), samples.len(), "every golden is covered");
    for (name, record, capture) in samples {
        let frozen = fs::read(root.join(format!("{name}.jsonl"))).expect("frozen event golden");
        let encoded = smith_events::write(&record, &capture, &limits).expect("bounded fixture").expect("event");
        assert_eq!(encoded.as_ref(), frozen, "event {name} drifted");
        let decoded = smith_events::read(&frozen, &limits).expect("valid golden").expect("known record");
        let rewritten =
            smith_events::write(&decoded, &capture, &limits).expect("bounded decoded fixture").expect("event");
        assert_eq!(rewritten.as_ref(), frozen, "golden {name} reads back");
    }
}
