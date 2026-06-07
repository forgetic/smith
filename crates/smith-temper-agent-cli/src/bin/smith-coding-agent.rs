//! Smith pi-SDK coding-workspace agent binary.
//!
//! Implements Temper's external coding-workspace command
//! (`TEMPER_CODING_WORKSPACE_COMMAND`). Temper runs this binary in a prepared
//! checkout (the process cwd), having written the work-item context JSON to the
//! file named by `TEMPER_CODING_WORKSPACE_CONTEXT`. The binary builds a
//! capability/role-aware `pi` SDK agent (engineer ⇒ edit tools, architect /
//! reviewer ⇒ read-only), runs the agent loop, and writes the resulting
//! [`WorkspaceResult`] JSON to the file named by
//! `TEMPER_CODING_WORKSPACE_RESULT`. For the engineer head path the working-tree
//! diff is the work product (Temper commits/pushes it); a non-empty `verdict`
//! routes the transition instead.
//!
//! The `pi` file/bash tools use asupersync IO, so the agent loop runs under an
//! **asupersync** runtime (not tokio), mirroring `pi`'s own `src/main.rs`.
//!
//! Exit codes: `0` on success; `2` on any failure (bad flags, missing context,
//! provider/credential error, agent failure, no product diff), with a clear
//! message on stderr and no result file written.

use std::path::PathBuf;

use asupersync::runtime::RuntimeBuilder;
use asupersync::runtime::reactor::create_reactor;
use smith_temper_agent::{
    AuthChoice, CodingAgentError, DEFAULT_MAX_ITERATIONS, ProviderConfig, WorkspaceContext,
    WorkspaceResult, resolve_config_dir, run_coding_agent,
};

/// Env var naming the file Temper wrote the work-item context JSON to.
const CONTEXT_ENV: &str = "TEMPER_CODING_WORKSPACE_CONTEXT";
/// Env var naming the file the command must write its result JSON to.
const RESULT_ENV: &str = "TEMPER_CODING_WORKSPACE_RESULT";

#[cfg(feature = "test-provider-base-url-override")]
const TEST_PROVIDER_BASE_URL_ENV: &str = "SMITH_TEST_PROVIDER_BASE_URL";

#[cfg(feature = "test-provider-base-url-override")]
fn apply_test_provider_base_url_override(provider: ProviderConfig) -> ProviderConfig {
    apply_test_provider_base_url_override_value(
        provider,
        std::env::var(TEST_PROVIDER_BASE_URL_ENV).ok(),
    )
}

#[cfg(feature = "test-provider-base-url-override")]
fn apply_test_provider_base_url_override_value(
    provider: ProviderConfig,
    base_url: Option<String>,
) -> ProviderConfig {
    match base_url {
        Some(base_url) if !base_url.trim().is_empty() => provider.with_base_url_override(base_url),
        _ => provider,
    }
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(message) => {
            eprintln!("smith-coding-agent: {message}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<(), String> {
    let options = CodingAgentOptions::parse(std::env::args().skip(1).collect())?;
    if options.help {
        print_usage();
        return Ok(());
    }

    let context = read_context()?;

    // The checkout Temper prepared is this process's working directory.
    let cwd = std::env::current_dir()
        .map_err(|error| format!("resolving working directory failed: {error}"))?;

    // Resolve the operator config dir for prompt overlays + AGENTS.md injection.
    // A missing dir/files is a clean no-op inside `run_coding_agent`.
    let config_dir = resolve_config_dir(options.config_dir.as_deref());

    // Preflight credentials before booting the runtime so a missing key fails
    // fast with a clear setup error (and never writes a result file).
    let provider = ProviderConfig::from_auth(options.auth, options.codex_model, options.auth_file)
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "test-provider-base-url-override")]
    let provider = apply_test_provider_base_url_override(provider);

    // The pi file/bash tools require an asupersync reactor; mirror pi's main.rs.
    let reactor =
        create_reactor().map_err(|error| format!("creating asupersync reactor failed: {error}"))?;
    let runtime = RuntimeBuilder::multi_thread()
        .blocking_threads(1, 2)
        .with_reactor(reactor)
        .build()
        .map_err(|error| format!("building asupersync runtime failed: {error}"))?;

    let result = runtime
        .block_on(run_coding_agent(
            &provider,
            &context,
            &cwd,
            options.max_iterations,
            config_dir.as_deref(),
        ))
        .map_err(describe_agent_error)?;

    write_result(&result)?;
    Ok(())
}

/// Reads and parses the work-item context from `$TEMPER_CODING_WORKSPACE_CONTEXT`.
fn read_context() -> Result<WorkspaceContext, String> {
    let path = std::env::var_os(CONTEXT_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{CONTEXT_ENV} is not set; Temper must name the context file"))?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("reading context file {}: {error}", path.display()))?;
    serde_json::from_str::<WorkspaceContext>(&raw).map_err(|error| {
        format!(
            "invalid WorkspaceContext JSON in {}: {error}",
            path.display()
        )
    })
}

/// Writes the result to `$TEMPER_CODING_WORKSPACE_RESULT`. When the env var is
/// unset (e.g. a manual invocation) the result is written to stdout instead so
/// the run is still observable.
fn write_result(result: &WorkspaceResult) -> Result<(), String> {
    let json = serde_json::to_string(result)
        .map_err(|error| format!("serializing WorkspaceResult failed: {error}"))?;
    match std::env::var_os(RESULT_ENV) {
        Some(path) => {
            let path = PathBuf::from(path);
            std::fs::write(&path, format!("{json}\n"))
                .map_err(|error| format!("writing result file {}: {error}", path.display()))
        }
        None => {
            use std::io::Write;
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            writeln!(stdout, "{json}").map_err(|error| format!("writing stdout failed: {error}"))
        }
    }
}

/// Maps an agent error to a stderr message. The agent's errors already redact
/// secrets (provider errors carry no token bytes).
fn describe_agent_error(error: CodingAgentError) -> String {
    error.to_string()
}

#[derive(Debug)]
struct CodingAgentOptions {
    auth: AuthChoice,
    codex_model: Option<String>,
    auth_file: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    max_iterations: usize,
    help: bool,
}

impl CodingAgentOptions {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut auth = AuthChoice::ChatGptOAuth;
        let mut codex_model = None;
        let mut auth_file = None;
        let mut config_dir = None;
        let mut max_iterations = DEFAULT_MAX_ITERATIONS;
        let mut help = false;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--auth" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--auth requires a value".to_string())?;
                    auth = parse_auth_choice(&value)?;
                }
                "--codex-model" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--codex-model requires a value".to_string())?;
                    codex_model = Some(value);
                }
                "--auth-file" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--auth-file requires a value".to_string())?;
                    auth_file = Some(PathBuf::from(value));
                }
                "--config-dir" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--config-dir requires a value".to_string())?;
                    config_dir = Some(PathBuf::from(value));
                }
                "--max-iterations" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| "--max-iterations requires a value".to_string())?;
                    max_iterations = value.parse::<usize>().map_err(|error| {
                        format!("--max-iterations must be a positive integer: {error}")
                    })?;
                    if max_iterations == 0 {
                        return Err("--max-iterations must be greater than zero".to_string());
                    }
                }
                "--help" | "-h" | "help" => help = true,
                other => return Err(format!("unknown option `{other}`; run with --help")),
            }
        }
        Ok(Self {
            auth,
            codex_model,
            auth_file,
            config_dir,
            max_iterations,
            help,
        })
    }
}

fn parse_auth_choice(value: &str) -> Result<AuthChoice, String> {
    match value {
        "deepseek" => Ok(AuthChoice::DeepSeek),
        "chatgpt-oauth" => Ok(AuthChoice::ChatGptOAuth),
        "anthropic-oauth" => Ok(AuthChoice::AnthropicOAuth),
        other => Err(format!(
            "unsupported auth `{other}`; expected deepseek, chatgpt-oauth, or anthropic-oauth"
        )),
    }
}

fn print_usage() {
    println!(
        "Usage:\n  smith-coding-agent \\\n    [--auth deepseek|chatgpt-oauth|anthropic-oauth] \\\n    [--codex-model MODEL] [--auth-file PATH] [--config-dir PATH] [--max-iterations N]\n\nImplements Temper's external coding-workspace command. Reads the work-item context from the file named by {CONTEXT_ENV}, runs a capability/role-aware pi SDK agent in the current working directory (the prepared checkout), and writes a WorkspaceResult JSON value to the file named by {RESULT_ENV} (or to stdout when that variable is unset). The engineer role leaves a product diff in the working tree; architect and reviewer roles emit a verdict.\n\nOperator prompt overlays (prompts/architect.md, prompts/engineer.md, prompts/reviewer.md, prompts/coding-agent.md) are loaded from --config-dir if given, else $SMITH_CONFIG_DIR, else $XDG_CONFIG_HOME/smith, else ~/.config/smith; the checkout's root AGENTS.md is injected as context. Missing dir/files are a clean no-op. Logs and errors go to stderr; exits non-zero on failure."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_and_iteration_options() {
        let options = CodingAgentOptions::parse(vec![
            "--auth".into(),
            "anthropic-oauth".into(),
            "--codex-model".into(),
            "gpt-test".into(),
            "--auth-file".into(),
            "/tmp/auth.json".into(),
            "--config-dir".into(),
            "/tmp/smith-config".into(),
            "--max-iterations".into(),
            "12".into(),
        ])
        .expect("options parse");

        assert_eq!(options.auth, AuthChoice::AnthropicOAuth);
        assert_eq!(options.codex_model.as_deref(), Some("gpt-test"));
        assert_eq!(options.auth_file, Some(PathBuf::from("/tmp/auth.json")));
        assert_eq!(options.config_dir, Some(PathBuf::from("/tmp/smith-config")));
        assert_eq!(options.max_iterations, 12);
    }

    #[test]
    fn defaults_to_chatgpt_oauth_and_default_iterations() {
        let options = CodingAgentOptions::parse(Vec::new()).expect("defaults parse");
        assert_eq!(options.auth, AuthChoice::ChatGptOAuth);
        assert_eq!(options.max_iterations, DEFAULT_MAX_ITERATIONS);
        assert_eq!(options.config_dir, None);
        assert!(!options.help);
    }

    #[test]
    fn config_dir_requires_a_value() {
        let error = CodingAgentOptions::parse(vec!["--config-dir".into()])
            .expect_err("missing value fails");
        assert!(error.contains("--config-dir requires a value"));
    }

    #[test]
    fn rejects_unknown_auth() {
        let error = CodingAgentOptions::parse(vec!["--auth".into(), "unknown".into()])
            .expect_err("unknown auth fails");
        assert!(error.contains("unsupported auth"));
    }

    #[test]
    fn rejects_zero_iterations() {
        let error = CodingAgentOptions::parse(vec!["--max-iterations".into(), "0".into()])
            .expect_err("zero iterations fails");
        assert!(error.contains("greater than zero"));
    }

    #[test]
    fn rejects_non_numeric_iterations() {
        let error = CodingAgentOptions::parse(vec!["--max-iterations".into(), "lots".into()])
            .expect_err("non-numeric fails");
        assert!(error.contains("positive integer"));
    }

    #[cfg(feature = "test-provider-base-url-override")]
    #[test]
    fn test_provider_base_url_override_honors_non_empty_env() {
        let provider = ProviderConfig::new("test", "model", "http://original", "key");

        let provider = apply_test_provider_base_url_override_value(
            provider,
            Some("http://127.0.0.1:4100".to_string()),
        );

        assert_eq!(provider.base_url_for_test(), "http://127.0.0.1:4100");
    }

    #[cfg(feature = "test-provider-base-url-override")]
    #[test]
    fn test_provider_base_url_override_ignores_unset_and_empty_env() {
        let provider = ProviderConfig::new("test", "model", "http://original", "key");
        let provider = apply_test_provider_base_url_override_value(provider, None);
        assert_eq!(provider.base_url_for_test(), "http://original");

        let provider = ProviderConfig::new("test", "model", "http://original", "key");
        let provider =
            apply_test_provider_base_url_override_value(provider, Some("   ".to_string()));
        assert_eq!(provider.base_url_for_test(), "http://original");
    }
}
