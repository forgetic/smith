use std::process::{Command, Stdio};

#[test]
fn unreadable_configuration_exits_with_a_reason_and_no_channel_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_smith"))
        .arg("agent")
        .arg("")
        .stdin(Stdio::null())
        .output()
        .expect("start smith binary");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("configuration metadata"));
}
