//! Smith application argument menu, separate from Skein's provider-neutral fake.
//! These retained source fixtures know checkout paths and commands; the generic
//! fake only selects bounded opaque menu entries. Contract: domain/session.md,
//! sections 9 and 12; testing-strategy.md, sections 4 and 7.

use skein_fake_llm_domain::api::{InvalidInput, Menu};

/// Complete application-owned random inputs, including the preserved deliberate
/// unknown name, malformed object and missing fields. Script and menu wrappers
/// and bytes jointly obey the actual provider `Config.script_bytes` admission.
/// Contract: domain/session.md, sections 9 and 12; testing-strategy.md, section 4.
#[must_use]
pub fn menu() -> Menu {
    let arguments: [&[u8]; 5] = [
        br#"{"path":"src/lib.rs","content":"src/lib.rs: rewritten","command":"cargo test","old":"42","new":"43","pattern":"answer"}"#,
        br#"{"path":"./README.md","content":"README.md: rewritten","command":"ls","old":"hello","new":"goodbye","pattern":"hello"}"#,
        br#"{"path":"../outside.txt","content":"outside.txt: rewritten","command":"true","old":"a","new":"b","pattern":"outside"}"#,
        br#"{"path":"docs","content":"docs: rewritten","command":"cat README.md","old":"guide","new":"manual","pattern":"guide"}"#,
        br#"{"path":"notes.md","content":"notes.md: noted","command":"false","old":"noted","new":"kept","pattern":"noted"}"#,
    ];
    Menu {
        arguments: arguments.into_iter().map(Into::into).collect(),
        invalid: Box::new([
            InvalidInput { name: Some(b"delete_repository".as_slice().into()), arguments: arguments[0].into() },
            InvalidInput { name: None, arguments: br#"{"path":"#.as_slice().into() },
            InvalidInput { name: None, arguments: b"{}".as_slice().into() },
        ]),
    }
}
