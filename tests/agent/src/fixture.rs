//! One scripted host workspace, copied from temper `25ac2ad` without forge
//! metadata (domain/run.md, section 14). The fake checkout owns bytes and
//! process scripts; neither this fixture nor the agent contacts a filesystem.

use skein_fake_checkout::{Checkout, Exit, Program};

pub(crate) const CHECKS: &[u8] = b".temper/pre-pr";

pub(crate) const CODE: &[u8] = b"src/lib.rs";

pub(crate) const INITIAL: &[u8] = b"pub fn answer() -> u32 { 42 }\n";

pub(crate) fn seed(disk: &mut Checkout) -> u64 {
    disk.mkdir(b"work");
    for (path, bytes) in [
        (CODE, INITIAL),
        (b"README.md", b"smith: the copied agent\n"),
        (b"AGENTS.md", b"The answer is in src/lib.rs; the checks want it to be 43.\n"),
        (CHECKS, b"#!checks\nsrc/lib.rs 43\n"),
    ] {
        disk.write(&[b"work/", path].concat(), bytes);
    }
    for (command, millis, output, code) in [
        (&b"cargo test"[..], 200, &b"test result: ok. 1 passed\n"[..], 0),
        (b"ls", 10, b"AGENTS.md README.md src\n", 0),
        (b"sleep", 3_600_000, b"", 0),
    ] {
        disk.program(
            command,
            Program {
                duration: std::time::Duration::from_millis(millis),
                output: output.to_vec(),
                exit: Exit::Code(code),
                changes: Vec::new(),
            },
        );
    }
    disk.root(b"work")
}

pub(crate) fn check(disk: &Checkout, root: u64) -> (bool, Vec<u8>) {
    let passed = disk.load(root, CODE, u64::MAX).is_ok_and(|(content, _)| content.windows(2).any(|part| part == b"43"));
    let output = if passed { b"ok: src/lib.rs\n".as_slice() } else { b"FAILED: src/lib.rs does not hold 43\n" };
    (passed, output.to_vec())
}
