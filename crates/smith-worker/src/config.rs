use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::workspace::RoleGitIdentity;

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
    pub executor: ExecutorSelection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutorSelection {
    Stub,
    Coding(CodingSurface),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodingSurface {
    pub workspace_root: PathBuf,
    pub git_base_url: String,
    pub agent_command: Vec<String>,
}

pub const USAGE: &str = "smith-worker --daemon-url <url> --worker-id <id> --capability <owner/name>:<role> [--capability ...] [--max-concurrent <n>] [--poll-wait-ms <n>] [--heartbeat-interval-ms <n>] [--executor <stub|coding>] [--workspace-root <path>] [--git-base-url <url>] [--agent-command <program>] [--agent-arg <arg> ...]";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseOutcome {
    Help,
    Run(WorkerConfig),
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<ParseOutcome, String> {
    let args: Vec<String> = args.into_iter().collect();
    if contains_help_request(&args) {
        return Ok(ParseOutcome::Help);
    }

    let mut daemon_url: Option<String> = None;
    let mut worker_id: Option<String> = None;
    let mut capabilities = Vec::new();
    let mut seen_capabilities = BTreeSet::new();
    let mut max_concurrent_jobs = 1;
    let mut poll_wait_ms = 30_000;
    let mut heartbeat_interval_ms = 10_000;
    let mut executor = ExecutorKind::Stub;
    let mut workspace_root: Option<PathBuf> = None;
    let mut git_base_url: Option<String> = None;
    let mut agent_program: Option<String> = None;
    let mut agent_args = Vec::new();

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
            "--executor" => {
                let value = flag_value(&args, &mut index, "--executor")?;
                executor = parse_executor(value)?;
            }
            "--workspace-root" => {
                let value = flag_value(&args, &mut index, "--workspace-root")?;
                workspace_root = Some(PathBuf::from(required_trimmed_value(
                    "--workspace-root",
                    value,
                )?));
            }
            "--git-base-url" => {
                let value = flag_value(&args, &mut index, "--git-base-url")?;
                git_base_url = Some(required_trimmed_value("--git-base-url", value)?);
            }
            "--agent-command" => {
                let value = flag_value(&args, &mut index, "--agent-command")?;
                agent_program = Some(required_trimmed_value("--agent-command", value)?);
            }
            "--agent-arg" => {
                let value = positional_flag_value(&args, &mut index, "--agent-arg")?;
                agent_args.push(value.to_string());
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
    let executor = executor_selection(
        executor,
        workspace_root,
        git_base_url,
        agent_program,
        agent_args,
    )?;

    Ok(ParseOutcome::Run(WorkerConfig {
        daemon_url,
        worker_id,
        capabilities,
        max_concurrent_jobs,
        poll_wait: Duration::from_millis(poll_wait_ms),
        heartbeat_interval: Duration::from_millis(heartbeat_interval_ms),
        executor,
    }))
}

fn contains_help_request(args: &[String]) -> bool {
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return true,
            "--agent-arg" => index += 1,
            _ => {}
        }
        index += 1;
    }
    false
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

fn positional_flag_value<'a>(
    args: &'a [String],
    index: &mut usize,
    flag: &str,
) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn required_trimmed_value(flag: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{flag} must not be empty"));
    }
    Ok(value.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutorKind {
    Stub,
    Coding,
}

fn parse_executor(value: &str) -> Result<ExecutorKind, String> {
    match value.trim() {
        "stub" => Ok(ExecutorKind::Stub),
        "coding" => Ok(ExecutorKind::Coding),
        other => Err(format!(
            "--executor must be `stub` or `coding` (got `{other}`)"
        )),
    }
}

fn executor_selection(
    executor: ExecutorKind,
    workspace_root: Option<PathBuf>,
    git_base_url: Option<String>,
    agent_program: Option<String>,
    mut agent_args: Vec<String>,
) -> Result<ExecutorSelection, String> {
    match executor {
        ExecutorKind::Stub => {
            if workspace_root.is_some() {
                return Err("--workspace-root requires --executor coding".to_string());
            }
            if git_base_url.is_some() {
                return Err("--git-base-url requires --executor coding".to_string());
            }
            if agent_program.is_some() {
                return Err("--agent-command requires --executor coding".to_string());
            }
            if !agent_args.is_empty() {
                return Err("--agent-arg requires --executor coding".to_string());
            }
            Ok(ExecutorSelection::Stub)
        }
        ExecutorKind::Coding => {
            let workspace_root = workspace_root
                .ok_or_else(|| "--workspace-root is required when --executor coding".to_string())?;
            let git_base_url = git_base_url
                .ok_or_else(|| "--git-base-url is required when --executor coding".to_string())?;
            let agent_program = agent_program
                .ok_or_else(|| "--agent-command is required when --executor coding".to_string())?;
            let mut agent_command = Vec::with_capacity(agent_args.len() + 1);
            agent_command.push(agent_program);
            agent_command.append(&mut agent_args);

            Ok(ExecutorSelection::Coding(CodingSurface {
                workspace_root,
                git_base_url,
                agent_command,
            }))
        }
    }
}

pub fn role_identities_from_env(
    roles: impl IntoIterator<Item = String>,
    vars: impl IntoIterator<Item = (String, String)>,
) -> Result<BTreeMap<String, RoleGitIdentity>, String> {
    let roles: BTreeSet<String> = roles.into_iter().collect();
    let vars: BTreeMap<String, String> = vars.into_iter().collect();
    let mut identities = BTreeMap::new();

    for role in roles {
        let key = env_role_key(&role);
        let user_var = format!("TEMPER_FORGEJO_USER_{key}");
        let token_var = format!("TEMPER_FORGEJO_TOKEN_{key}");
        let email_var = format!("TEMPER_FORGEJO_EMAIL_{key}");

        let user = required_env_value(&vars, &user_var, &role)?;
        let token = required_env_value(&vars, &token_var, &role)?;
        let email = vars
            .get(&email_var)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{user}@noreply.localhost"));

        identities.insert(role, RoleGitIdentity { user, email, token });
    }

    Ok(identities)
}

fn env_role_key(role: &str) -> String {
    role.chars()
        .flat_map(char::to_uppercase)
        .map(|character| {
            if character.is_ascii_uppercase() || character.is_ascii_digit() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn required_env_value(
    vars: &BTreeMap<String, String>,
    var_name: &str,
    role: &str,
) -> Result<String, String> {
    vars.get(var_name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!("no {var_name} in the environment for role `{role}`; is roles.env provisioned?")
        })
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
        assert_eq!(config.executor, ExecutorSelection::Stub);
    }

    #[test]
    fn parses_coding_executor_surface_with_agent_args_in_order() {
        let config = parse_ok(&[
            "--daemon-url",
            "http://daemon.example",
            "--worker-id",
            "worker-1",
            "--capability",
            "ai/temper:coder",
            "--executor",
            "coding",
            "--workspace-root",
            " /var/lib/smith/workspaces ",
            "--git-base-url",
            " https://forgejo.example ",
            "--agent-command",
            " smith-coding-agent ",
            "--agent-arg",
            "--auth",
            "--agent-arg",
            "chatgpt-oauth",
        ]);

        assert_eq!(
            config.executor,
            ExecutorSelection::Coding(CodingSurface {
                workspace_root: PathBuf::from("/var/lib/smith/workspaces"),
                git_base_url: "https://forgejo.example".to_string(),
                agent_command: vec![
                    "smith-coding-agent".to_string(),
                    "--auth".to_string(),
                    "chatgpt-oauth".to_string(),
                ],
            })
        );
    }

    #[test]
    fn agent_arg_accepts_leading_dash_values() {
        let config = parse_ok(&[
            "--daemon-url",
            "http://daemon.example",
            "--worker-id",
            "worker-1",
            "--capability",
            "ai/temper:coder",
            "--executor",
            "coding",
            "--workspace-root",
            "/workspaces",
            "--git-base-url",
            "https://forgejo.example",
            "--agent-command",
            "smith-coding-agent",
            "--agent-arg",
            "--verbose",
        ]);

        let ExecutorSelection::Coding(surface) = config.executor else {
            panic!("expected coding executor");
        };
        assert_eq!(
            surface.agent_command,
            vec!["smith-coding-agent".to_string(), "--verbose".to_string()]
        );
    }

    #[test]
    fn coding_executor_requires_all_coding_flags() {
        for missing_flag in ["--workspace-root", "--git-base-url", "--agent-command"] {
            let mut args = vec![
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
                "--capability",
                "ai/temper:coder",
                "--executor",
                "coding",
            ];
            if missing_flag != "--workspace-root" {
                args.extend(["--workspace-root", "/workspaces"]);
            }
            if missing_flag != "--git-base-url" {
                args.extend(["--git-base-url", "https://forgejo.example"]);
            }
            if missing_flag != "--agent-command" {
                args.extend(["--agent-command", "smith-coding-agent"]);
            }

            let error = parse_err(&args);
            assert!(
                error.contains(missing_flag),
                "unexpected error for missing {missing_flag}: {error}"
            );
        }
    }

    #[test]
    fn rejects_bogus_executor_and_coding_flags_with_stub_executor() {
        assert!(
            parse_err(&[
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
                "--capability",
                "ai/temper:coder",
                "--executor",
                "bogus",
            ])
            .contains("--executor must be")
        );

        for flag in [
            "--workspace-root",
            "--git-base-url",
            "--agent-command",
            "--agent-arg",
        ] {
            let error = parse_err(&[
                "--daemon-url",
                "http://daemon.example",
                "--worker-id",
                "worker-1",
                "--capability",
                "ai/temper:coder",
                flag,
                "value",
            ]);
            assert!(
                error.contains(&format!("{flag} requires --executor coding")),
                "unexpected error for {flag}: {error}"
            );
        }
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
        assert!(
            parse([
                "--executor".to_string(),
                "coding".to_string(),
                "--agent-arg".to_string(),
                "--help".to_string(),
            ])
            .expect_err("agent arg help-looking value is not global help")
            .contains("--daemon-url is required")
        );
    }

    #[test]
    fn loads_role_identities_from_env_for_distinct_roles() {
        let identities = role_identities_from_env(
            [
                "engineer".to_string(),
                "code-reviewer".to_string(),
                "engineer".to_string(),
            ],
            [
                (
                    "TEMPER_FORGEJO_USER_ENGINEER".to_string(),
                    " engineer-user ".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_TOKEN_ENGINEER".to_string(),
                    " engineer-token ".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_USER_CODE_REVIEWER".to_string(),
                    "reviewer-user".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_TOKEN_CODE_REVIEWER".to_string(),
                    "reviewer-token".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_EMAIL_CODE_REVIEWER".to_string(),
                    "reviewer@example.test".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_USER_ARCHITECT".to_string(),
                    "ignored".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_TOKEN_ARCHITECT".to_string(),
                    "ignored".to_string(),
                ),
            ],
        )
        .expect("identities load");

        assert_eq!(identities.len(), 2);
        assert_eq!(
            identities.get("engineer"),
            Some(&RoleGitIdentity {
                user: "engineer-user".to_string(),
                email: "engineer-user@noreply.localhost".to_string(),
                token: "engineer-token".to_string(),
            })
        );
        assert_eq!(
            identities.get("code-reviewer"),
            Some(&RoleGitIdentity {
                user: "reviewer-user".to_string(),
                email: "reviewer@example.test".to_string(),
                token: "reviewer-token".to_string(),
            })
        );
        assert!(!identities.contains_key("architect"));
    }

    #[test]
    fn role_identity_errors_name_missing_user_or_token_and_role() {
        let missing_user = role_identities_from_env(
            ["engineer".to_string()],
            [(
                "TEMPER_FORGEJO_TOKEN_ENGINEER".to_string(),
                "token".to_string(),
            )],
        )
        .expect_err("missing user fails");
        assert!(missing_user.contains("TEMPER_FORGEJO_USER_ENGINEER"));
        assert!(missing_user.contains("role `engineer`"));

        let missing_token = role_identities_from_env(
            ["code-reviewer".to_string()],
            [
                (
                    "TEMPER_FORGEJO_USER_CODE_REVIEWER".to_string(),
                    "reviewer".to_string(),
                ),
                (
                    "TEMPER_FORGEJO_TOKEN_CODE_REVIEWER".to_string(),
                    " ".to_string(),
                ),
            ],
        )
        .expect_err("missing token fails");
        assert!(missing_token.contains("TEMPER_FORGEJO_TOKEN_CODE_REVIEWER"));
        assert!(missing_token.contains("role `code-reviewer`"));
    }
}
