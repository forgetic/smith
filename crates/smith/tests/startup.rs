use std::process::{Command, Stdio};

#[test]
fn a_configuration_that_does_not_read_exits_with_failure_and_opens_no_channel() {
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

#[test]
fn the_last_line_of_standard_error_says_why_a_run_could_not_answer() {
    let path = std::env::temp_dir().join(format!("smith-agent-config-{}", std::process::id()));
    std::fs::write(
        &path,
        r#"{"profile":"standard","memory_bytes":1099511627776,"grace_ms":10,"endpoints":[],"environment":[]}"#,
    )
    .expect("write valid agent configuration");
    let output = Command::new(env!("CARGO_BIN_EXE_smith"))
        .arg("agent")
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .expect("start smith binary");
    std::fs::remove_file(path).expect("remove agent configuration");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 diagnostics");
    assert_eq!(stderr.lines().last(), Some("smith: the run could not answer: Some(ChannelEnded)"));
}
