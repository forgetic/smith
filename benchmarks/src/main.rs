//! Offline manifest checking; no attempt starts from this command yet.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [command] if command == "check" => check(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tasks")),
        [command, directory] if command == "check" => check(Path::new(directory)),
        _ => {
            eprintln!("usage: smith-bench check [TASKS_DIRECTORY]");
            ExitCode::from(2)
        }
    }
}

fn check(directory: &Path) -> ExitCode {
    let design = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/design");
    match smith_bench::check_tree(directory, &design) {
        Ok(count) => {
            println!("checked {count} tasks");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
