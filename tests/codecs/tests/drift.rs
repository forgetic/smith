//! Frozen generated code and bytes follow codec.md, section 6.

use std::{fs, path::Path};

use skein_codegen::{generate, parse};

#[test]
fn charter_schema_matches_generated_code_and_all_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/smith-charter");
    let source = fs::read_to_string(root.join("schema/charter-v1.schema")).expect("charter schema");
    let schema = parse(&source).expect("schema valid");
    let output = generate(&schema);
    let committed = fs::read_to_string(root.join("src/generated/v1.rs")).expect("generated code");
    assert_eq!(committed, output.rust, "regenerate charter codec with skein-codegen");
    for golden in output.goldens {
        let committed = fs::read(root.join("golden/v1").join(&golden.name)).expect("golden bytes");
        assert_eq!(committed, golden.bytes, "golden {} drifted", golden.name);
    }
}
