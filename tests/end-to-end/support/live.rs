//! Caller-selected live endpoints and private, durable test credentials
//! (testing.md, section 2.3; protocol/hosts.md, section 5.4).

use smith_real_world::Scratch;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[must_use]
pub fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn required(name: &str) -> String {
    setting(name).unwrap_or_else(|| panic!("live suite requires {name}"))
}

#[must_use]
pub fn enabled() -> bool {
    setting("NEXTEST_PROFILE").as_deref() == Some("live")
}

/// One real provider and its caller-supplied public OAuth registration.
pub struct Backend {
    pub name: String,
    pub model: String,
    account: serde_json::Value,
    pub tokens: PathBuf,
}

fn small_model(provider_name: &str) -> String {
    let models_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benchmarks/agents/models.toml");
    let models = smith_bench::read_model_tiers(&models_file).expect("pinned live models");
    let provider =
        if provider_name == "codex" { smith_bench::Provider::Codex } else { smith_bench::Provider::Anthropic };
    models.lookup(&models_file, "small", provider).expect("small live model").model.clone().expect("resolved model")
}

/// Public endpoint choices and a token directory belonging only to these tests.
pub struct Environment {
    pub backends: Vec<Backend>,
    pub remote: Option<String>,
}

impl Environment {
    pub fn load(bootstrap: bool) -> Self {
        let root = smith_bench::guard::write_path(Path::new(&required("SMITH_TEST_LIVE_TOKEN_DIR")))
            .expect("guarded live token directory");
        if bootstrap {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&root).expect("create private test tokens");
        }
        let metadata =
            std::fs::metadata(&root).expect("SMITH_TEST_LIVE_TOKEN_DIR must exist; bootstrap and sign in first");
        assert!(
            metadata.is_dir() && metadata.permissions().mode().trailing_zeros() >= 6,
            "live token directory must be private (0700)"
        );
        let marker = smith_bench::guard::write_path(&root.join(".smith-live-test-tokens")).expect("guarded marker");
        if bootstrap {
            std::fs::write(&marker, b"Dedicated Smith live-test token directory\n").expect("mark test-only directory");
        }
        assert!(marker.is_file(), "SMITH_TEST_LIVE_TOKEN_DIR needs the bootstrap's test-only marker");
        let providers = required("SMITH_TEST_LIVE_PROVIDERS");
        let mut backends = Vec::new();
        for name in providers.split(',').map(str::trim) {
            assert!(matches!(name, "codex" | "anthropic"), "SMITH_TEST_LIVE_PROVIDERS accepts codex,anthropic");
            assert!(!backends.iter().any(|backend: &Backend| backend.name == name), "each live provider appears once");
            let prefix = format!("SMITH_TEST_LIVE_{}", name.to_ascii_uppercase());
            let account_file = required(&format!("{prefix}_ACCOUNT_FILE"));
            assert!(
                std::fs::metadata(&account_file).expect("public account registration file").len() <= 32_768,
                "bounded public registration"
            );
            let mut account: serde_json::Value =
                serde_json::from_slice(&std::fs::read(account_file).expect("public account registration"))
                    .expect("account registration must be JSON");
            assert!(
                account.get("account_id").is_some_and(serde_json::Value::is_string)
                    && account.get("oauth").is_some_and(serde_json::Value::is_object),
                "registration requires account_id and oauth"
            );
            assert!(
                account
                    .as_object()
                    .expect("account object")
                    .keys()
                    .all(|key| matches!(key.as_str(), "number" | "account_id" | "oauth")),
                "public account registration has unknown fields"
            );
            assert!(
                account.get("oauth").expect("OAuth registration").as_object().expect("OAuth object").keys().all(
                    |key| matches!(
                        key.as_str(),
                        "authorization_url"
                            | "token_endpoint"
                            | "client_id"
                            | "redirect_uri"
                            | "scope"
                            | "address"
                            | "server_name"
                            | "json"
                    )
                ),
                "public OAuth registration cannot contain credentials or unknown fields"
            );
            smith_bench::guard::check_credential_record(&account)
                .expect("public registration cannot copy refresh tokens");
            account.as_object_mut().expect("account object").insert("number".into(), serde_json::json!(0));
            let tokens = smith_bench::guard::write_path(&root.join(name)).expect("guarded provider directory");
            if bootstrap {
                std::fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&tokens)
                    .or_else(
                        |error| if error.kind() == std::io::ErrorKind::AlreadyExists { Ok(()) } else { Err(error) },
                    )
                    .expect("private provider token directory");
            }
            assert!(
                tokens.canonicalize().expect("provider token directory; bootstrap first").starts_with(&root),
                "provider tokens must stay in the dedicated directory"
            );
            if !bootstrap {
                let saved = smith_local_shell::local_tokens::Tokens::new(
                    &tokens,
                    smith_local_shell::local_host::token_limits(),
                )
                .expect("private test token store")
                .load(0)
                .expect("valid private token record");
                assert!(saved.is_some(), "provider must be signed in by hand; missing dedicated 0.json record");
            }
            backends.push(Backend {
                name: name.into(),
                model: setting(&format!("{prefix}_MODEL")).unwrap_or_else(|| small_model(name)),
                account,
                tokens,
            });
        }
        Self { backends, remote: setting("SMITH_TEST_LIVE_GIT_REMOTE") }
    }
}

impl Backend {
    #[must_use]
    pub fn settings(&self, directory: &Path, change: bool) -> serde_json::Value {
        let (host, identity, headers) = if self.name == "codex" {
            ("chatgpt.com", "plain", skein_llm::openai::identity::headers().iter().map(|header| serde_json::json!({"name":String::from_utf8_lossy(&header.name),"value":String::from_utf8_lossy(&header.value)})).collect::<Vec<_>>())
        } else {
            let mut headers = skein_llm::anthropic::identity::claude_code_headers();
            for header in &mut headers {
                if header.name.eq_ignore_ascii_case(b"anthropic-beta") {
                    header.value = b"claude-code-20250219,oauth-2025-04-20".as_slice().into();
                }
            }
            ("api.anthropic.com", "claude-code", headers.iter().map(|header| serde_json::json!({"name":String::from_utf8_lossy(&header.name),"value":String::from_utf8_lossy(&header.value)})).collect::<Vec<_>>())
        };
        let mut settings = serde_json::json!({
            "agent":{"profile":"standard","memory_bytes":1_099_511_627_776_u64,"grace_ms":1000,
                "endpoints":[{"name":self.name,"number":0,"dialect":0,"account":0,"provider":self.name,"address":format!("{host}:443"),"server_name":host,"transport":"tls","identity":identity,"headers":headers,"reasoning_effort":if self.name == "codex" { Some("low") } else { None }}],
                "environment":[],"trace":{"path":directory.join("agent-trace.jsonl"),"capture":"calls"}},
            "chat":"chat","instructions":"@local-shell Follow the user's requested outcome exactly. Use the tools and call finish when done.",
            "models":[{"endpoint":self.name,"name":self.model,"max_tokens":4096,"input_price":0,"cached_price":0,"output_price":0,"price_unit":1}],
            "budget":{"turns":12,"spend":1,"seconds":120},"waiting_seconds":30,
            "contract":{"form":"report","max":4096},"token_directory":self.tokens,"accounts":[self.account]
        });
        if change {
            settings.as_object_mut().expect("settings object").insert(
                "directories".into(),
                serde_json::json!([{"name":"repo","path":directory.join("repo"),"writable":true,"git":true}]),
            );
            settings.as_object_mut().expect("settings object").insert(
                "conventions".into(),
                serde_json::json!({"guide":".smith-test/guide","checks":".smith-test/check"}),
            );
            settings.as_object_mut().expect("settings object").insert("contract".into(), serde_json::json!({"form":"change","checks_must_pass":true,"fields":[{"name":"title","max":256},{"name":"body","max":4096}]}));
        }
        settings
    }

    pub fn configure(&self, scratch: &Scratch, change: bool, push: Option<(&str, &str)>) {
        let mut settings = self.settings(scratch.path(), change);
        if let Some((remote, branch)) = push {
            settings
                .as_object_mut()
                .expect("settings object")
                .insert("push".into(), serde_json::json!([{"remote":remote,"branch":branch}]));
            settings.as_object_mut().expect("settings object").insert(
                "delivery_environment".into(),
                serde_json::json!(
                    ["HOME", "PATH", "SSH_AUTH_SOCK"]
                        .into_iter()
                        .filter_map(|name| setting(name).map(|value| format!("{name}={value}")))
                        .collect::<Vec<_>>()
                ),
            );
        }
        std::fs::write(
            smith_bench::guard::write_path(&scratch.path().join("settings.json")).expect("guarded live settings"),
            serde_json::to_vec(&settings).expect("live settings JSON"),
        )
        .expect("live settings");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_provider_settings_use_machine_verified_tls_and_their_identity() {
        let scratch = Scratch::new();
        for name in ["codex", "anthropic"] {
            let backend = Backend {
                name: name.into(),
                model: "configured-model".into(),
                account: serde_json::json!({"number":0,"account_id":"public-account","oauth":{}}),
                tokens: scratch.path().join("dedicated-tokens"),
            };
            let settings = backend.settings(scratch.path(), true);
            let endpoint = &settings["agent"]["endpoints"][0];
            assert_eq!(endpoint["transport"], "tls");
            assert!(endpoint.get("trust_der").is_none());
            assert_eq!(endpoint["identity"], if name == "anthropic" { "claude-code" } else { "plain" });
            assert_eq!(settings["token_directory"], serde_json::json!(backend.tokens));
            assert!(settings.get("push").is_none());
            assert_eq!(settings["contract"]["checks_must_pass"], true);
        }
    }
}
