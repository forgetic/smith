use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySpec {
    pub repo: String,
    pub role: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerConfig {
    pub daemon_url: String,
    pub worker_id: String,
    pub capabilities: Vec<CapabilitySpec>,
    pub max_concurrent_jobs: u32,
    pub poll_wait: Duration,
    pub heartbeat_interval: Duration,
}

pub const USAGE: &str = "smith-worker --daemon-url <url> --worker-id <id> --capability <owner/name>:<role> [--capability ...] [--max-concurrent <n>] [--poll-wait-ms <n>] [--heartbeat-interval-ms <n>]";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseOutcome {
    Help,
    Run(WorkerConfig),
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<ParseOutcome, String> {
    let args: Vec<String> = args.into_iter().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(ParseOutcome::Help);
    }

    let mut daemon_url: Option<String> = None;
    let mut worker_id: Option<String> = None;
    let mut capabilities = Vec::new();
    let mut seen_capabilities = BTreeSet::new();
    let mut max_concurrent_jobs = 1;
    let mut poll_wait_ms = 30_000;
    let mut heartbeat_interval_ms = 10_000;

    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--daemon-url" => {
                let value = flag_value(&args, &mut index, "--daemon-url")?;
                let value = required_trimmed_value("--daemon-url", value)?;
                daemon_url = Some(value);
            }
            "--worker-id" => {
                let value = flag_value(&args, &mut index, "--worker-id")?;
                let value = required_trimmed_value("--worker-id", value)?;
                worker_id = Some(value);
            }
            "--capability" => {
                let value = flag_value(&args, &mut index, "--capability")?;
                let capability = parse_capability(value)?;
                let key = (capability.repo.clone(), capability.role.clone());
                if seen_capabilities.insert(key) {
                    capabilities.push(capability);
                }
            }
            "--max-concurrent" => {
                let value = flag_value(&args, &mut index, "--max-concurrent")?;
                max_concurrent_jobs = parse_non_zero_u32("--max-concurrent", value)?;
            }
            "--poll-wait-ms" => {
                let value = flag_value(&args, &mut index, "--poll-wait-ms")?;
                poll_wait_ms = parse_non_zero_u64("--poll-wait-ms", value)?;
            }
            "--heartbeat-interval-ms" => {
                let value = flag_value(&args, &mut index, "--heartbeat-interval-ms")?;
                heartbeat_interval_ms = parse_non_zero_u64("--heartbeat-interval-ms", value)?;
            }
            other if other.starts_with('-') => return Err(format!("unknown flag `{other}`")),
            other => return Err(format!("unexpected positional argument `{other}`")),
        }
        index += 1;
    }

    let daemon_url = daemon_url.ok_or_else(|| "--daemon-url is required".to_string())?;
    let worker_id = worker_id.ok_or_else(|| "--worker-id is required".to_string())?;
    if capabilities.is_empty() {
        return Err("--capability is required at least once".to_string());
    }

    Ok(ParseOutcome::Run(WorkerConfig {
        daemon_url,
        worker_id,
        capabilities,
        max_concurrent_jobs,
        poll_wait: Duration::from_millis(poll_wait_ms),
        heartbeat_interval: Duration::from_millis(heartbeat_interval_ms),
    }))
}

fn flag_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, String> {
    *index += 1;
    let value = args
        .get(*index)
        .ok_or_else(|| format!("{flag} requires a value"))?;
    if value.starts_with('-') {
        return Err(format!("{flag} requires a value"));
    }
    Ok(value)
}

fn required_trimmed_value(flag: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{flag} must not be empty"));
    }
    Ok(value.to_string())
}

fn parse_capability(value: &str) -> Result<CapabilitySpec, String> {
    let mut parts = value.splitn(2, ':');
    let repo = parts
        .next()
        .expect("splitn always returns the first part")
        .trim();
    let role = parts
        .next()
        .ok_or_else(|| format!("invalid --capability `{value}`; expected <owner/name>:<role>"))?
        .trim();

    validate_repo(repo).map_err(|message| format!("invalid --capability `{value}`: {message}"))?;
    if role.is_empty() {
        return Err(format!(
            "invalid --capability `{value}`: role must not be empty"
        ));
    }

    Ok(CapabilitySpec {
        repo: repo.to_string(),
        role: role.to_string(),
    })
}

fn validate_repo(repo: &str) -> Result<(), String> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err("repo must be owner/name with exactly two non-empty parts".to_string());
    }
    Ok(())
}

fn parse_non_zero_u32(flag: &str, value: &str) -> Result<u32, String> {
    let parsed: u32 = value
        .trim()
        .parse()
        .map_err(|error| format!("{flag} must be a positive integer: {error}"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(parsed)
}

fn parse_non_zero_u64(flag: &str, value: &str) -> Result<u64, String> {
    let parsed: u64 = value
        .trim()
        .parse()
        .map_err(|error| format!("{flag} must be a positive integer: {error}"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(args: &[&str]) -> WorkerConfig {
        match parse(args.iter().map(|arg| (*arg).to_string())).expect("parse succeeds") {
            ParseOutcome::Run(config) => config,
            ParseOutcome::Help => panic!("expected run config"),
        }
    }

    fn parse_err(args: &[&str]) -> String {
        parse(args.iter().map(|arg| (*arg).to_string())).expect_err("parse fails")
    }

    #[test]
    fn parses_defaults_and_trims_required_values() {
        let config = parse_ok(&[
            "--daemon-url",
            " https://temper.example/ ",
            "--worker-id",
            " worker-1 ",
            "--capability",
            " ai/temper : coder ",
        ]);

        assert_eq!(config.daemon_url, "https://temper.example/");
        assert_eq!(config.worker_id, "worker-1");
        assert_eq!(
            config.capabilities,
            vec![CapabilitySpec {
                repo: "ai/temper".to_string(),
                role: "coder".to_string(),
            }]
        );
        assert_eq!(config.max_concurrent_jobs, 1);
        assert_eq!(config.poll_wait, Duration::from_millis(30_000));
        assert_eq!(config.heartbeat_interval, Duration::from_millis(10_000));
    }

    #[test]
    fn singleton_flags_use_last_value_and_numeric_overrides() {
        let config = parse_ok(&[
            "--daemon-url",
            "http://old.example",
            "--daemon-url",
            "http://new.example",
            "--worker-id",
            "old-worker",
            "--worker-id",
            "new-worker",
            "--capability",
            "ai/temper:coder",
            "--max-concurrent",
            "2",
            "--poll-wait-ms",
            "500",
            "--heartbeat-interval-ms",
            "250",
        ]);

        assert_eq!(config.daemon_url, "http://new.example");
        assert_eq!(config.worker_id, "new-worker");
        assert_eq!(config.max_concurrent_jobs, 2);
        assert_eq!(config.poll_wait, Duration::from_millis(500));
        assert_eq!(config.heartbeat_interval, Duration::from_millis(250));
    }

    #[test]
    fn repeated_capabilities_are_deduplicated_preserving_order() {
        let config = parse_ok(&[
            "--daemon-url",
            "http://daemon.example",
            "--worker-id",
            "worker-1",
            "--capability",
            "ai/temper:coder",
            "--capability",
            " ai/temper : coder ",
            "--capability",
            "ai/smith:engineer",
            "--capability",
            "ai/temper:architect",
        ]);

        assert_eq!(
            config.capabilities,
            vec![
                CapabilitySpec {
                    repo: "ai/temper".to_string(),
                    role: "coder".to_string(),
                },
                CapabilitySpec {
                    repo: "ai/smith".to_string(),
                    role: "engineer".to_string(),
                },
                CapabilitySpec {
                    repo: "ai/temper".to_string(),
                    role: "architect".to_string(),
                },
            ]
        );
    }

    #[test]
    fn rejects_malformed_capabilities() {
        for capability in ["nope", "ai/temper", ":role", "ai/temper:"] {
            let error = parse_err(&[
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
                "--capability",
                capability,
            ]);
            assert!(
                error.contains("invalid --capability"),
                "unexpected error for {capability:?}: {error}"
            );
        }
    }

    #[test]
    fn rejects_missing_required_flags() {
        assert!(parse_err(&[]).contains("--daemon-url is required"));
        assert!(
            parse_err(&["--daemon-url", "http://daemon.example"])
                .contains("--worker-id is required")
        );
        assert!(
            parse_err(&[
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
            ])
            .contains("--capability is required")
        );
        assert!(
            parse_err(&[
                "--daemon-url",
                " ",
                "--worker-id",
                "worker-1",
                "--capability",
                "ai/temper:coder",
            ])
            .contains("--daemon-url must not be empty")
        );
    }

    #[test]
    fn rejects_zero_and_invalid_numerics() {
        for (flag, value) in [
            ("--max-concurrent", "0"),
            ("--max-concurrent", "nope"),
            ("--poll-wait-ms", "0"),
            ("--poll-wait-ms", "1.5"),
            ("--heartbeat-interval-ms", "0"),
            ("--heartbeat-interval-ms", "NaN"),
        ] {
            let error = parse_err(&[
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
                "--capability",
                "ai/temper:coder",
                flag,
                value,
            ]);
            assert!(error.contains(flag), "unexpected error for {flag}: {error}");
        }
    }

    #[test]
    fn rejects_unknown_flags_positionals_and_missing_values() {
        assert!(
            parse_err(&[
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
                "--capability",
                "ai/temper:coder",
                "--unknown",
            ])
            .contains("unknown flag")
        );
        assert!(parse_err(&["positional"]).contains("unexpected positional argument"));
        assert!(parse_err(&["--daemon-url"]).contains("--daemon-url requires a value"));
        assert!(
            parse_err(&["--daemon-url", "--worker-id"]).contains("--daemon-url requires a value")
        );
    }

    #[test]
    fn help_anywhere_returns_help_before_validation() {
        assert_eq!(
            parse(["--help".to_string()]).expect("help parses"),
            ParseOutcome::Help
        );
        assert_eq!(
            parse(["--daemon-url".to_string(), "-h".to_string()]).expect("help parses"),
            ParseOutcome::Help
        );
    }
}
