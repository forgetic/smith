//! One write guard keeps resolved user tool directories and no credentials.
//! `Guard::write_path` resolves existing ancestors before permitting a write;
//! `check_credential_record` refuses refresh tokens without returning their
//! contents (testing.md, section 2.3; benchmarks.md, section 10). Callers check
//! every destination immediately before writing; this guard never reads a login.

use std::path::{Component, Path, PathBuf};

/// A deliberately unusable refresh field for binaries predating access-only records.
/// The legacy converter may emit this sentinel, never an issued refresh token.
pub const UNUSABLE_REFRESH: &str = "smith-bench-unused-refresh";

/// User tool locations supplied by the environment or a deterministic test.
#[derive(Clone, Debug)]
pub struct Guard {
    protected: Vec<PathBuf>,
}

impl Guard {
    /// Protect the user's default tool directories and explicitly named tool homes.
    pub fn new(home: &Path, config: &Path, state: &Path, tool_homes: &[PathBuf]) -> Result<Self, String> {
        let mut protected = vec![
            home.join(".codex"),
            home.join(".claude"),
            home.join(".claude.json"),
            config.join("smith"),
            state.join("smith"),
        ];
        protected.extend_from_slice(tool_homes);
        let resolved: Vec<_> = protected.iter().map(|path| resolve(path)).collect::<Result<_, _>>()?;
        protected.extend(resolved);
        protected.sort();
        protected.dedup();
        Ok(Self { protected })
    }

    /// Read directory choices only, retaining custom XDG and tool home locations.
    pub fn from_environment() -> Result<Self, String> {
        let home =
            std::env::var_os("HOME").filter(|value| !value.is_empty()).ok_or("HOME is required by the write guard")?;
        let home = PathBuf::from(home);
        let config = environment_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let state = environment_path("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state"));
        let tools: Vec<_> = ["CODEX_HOME", "CLAUDE_CONFIG_DIR"].into_iter().filter_map(environment_path).collect();
        Self::new(&home, &config, &state, &tools)
    }

    /// Resolve a destination and refuse writes inside any user tool location.
    pub fn write_path(&self, path: &Path) -> Result<PathBuf, String> {
        let resolved = resolve(path)?;
        for protected in &self.protected {
            if resolved.starts_with(resolve(protected)?) {
                return Err("write refused inside the user's tool directories".into());
            }
        }
        Ok(resolved)
    }
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

/// Apply the shared guard to one live-test or harness destination.
pub fn write_path(path: &Path) -> Result<PathBuf, String> {
    Guard::from_environment()?.write_path(path)
}

fn resolve(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err("write destination must be absolute and contain no '..'".into());
    }
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(existing.file_name().ok_or("destination has no existing ancestor")?.to_owned());
                existing = existing.parent().ok_or("destination has no existing ancestor")?;
            }
            Err(error) => return Err(format!("cannot inspect write destination: {}", error.kind())),
        }
    }
    let mut resolved =
        existing.canonicalize().map_err(|error| format!("cannot resolve write destination: {}", error.kind()))?;
    for name in suffix.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

/// Refuse a proposed converted credential record if it contains a refresh token.
/// Empty/null fields and the explicit unusable legacy sentinel carry no token.
/// Issuer refresh tests keep their separately signed-in token store; this check
/// applies to conversion and copying, never to their issuer's own response.
pub fn check_credential_record(record: &serde_json::Value) -> Result<(), String> {
    match record {
        serde_json::Value::Object(fields) => {
            for (name, value) in fields {
                let normalized: String =
                    name.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect();
                if normalized == "refreshtoken" || normalized == "refresh" {
                    match value {
                        serde_json::Value::Null => {}
                        serde_json::Value::String(value) if value.is_empty() || value == UNUSABLE_REFRESH => {}
                        serde_json::Value::String(_)
                        | serde_json::Value::Bool(_)
                        | serde_json::Value::Number(_)
                        | serde_json::Value::Array(_)
                        | serde_json::Value::Object(_) => {
                            return Err("credential conversion cannot copy a refresh token".into());
                        }
                    }
                }
                check_credential_record(value)?;
            }
        }
        serde_json::Value::Array(records) => {
            for value in records {
                check_credential_record(value)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_login_directories_are_refused_before_any_write() {
        let root = std::env::temp_dir().join(format!("smith-bench-guard-{}", std::process::id()));
        std::fs::create_dir(&root).expect("fresh deterministic guard root");
        let home = root.join("home");
        let config = root.join("custom-config");
        let state = root.join("custom-state");
        std::fs::create_dir(&home).expect("fake home");
        let tools = [root.join("custom-codex"), root.join("custom-claude")];
        let guard = Guard::new(&home, &config, &state, &tools).expect("test guard");
        for protected in [
            home.join(".codex"),
            home.join(".claude"),
            home.join(".claude.json"),
            config.join("smith"),
            state.join("smith"),
            tools[0].clone(),
            tools[1].clone(),
        ] {
            assert!(guard.write_path(&protected).is_err());
            assert!(guard.write_path(&protected.join("nested")).is_err());
            assert!(!protected.exists(), "guard never creates a protected path");
        }
        assert!(guard.write_path(Path::new("relative")).is_err());
        assert!(guard.write_path(&root.join("home/../escape")).is_err());
        let allowed = guard.write_path(&state.join("smith-bench/arms/new")).expect("private harness sibling");
        assert_eq!(allowed, state.join("smith-bench/arms/new"));
        std::fs::create_dir(home.join(".codex")).expect("fake protected directory");
        std::os::unix::fs::symlink(home.join(".codex"), root.join("alias")).expect("alias into protected directory");
        assert!(guard.write_path(&root.join("alias/new/nested")).is_err());
        std::os::unix::fs::symlink(root.join("missing"), root.join("dangling")).expect("dangling alias");
        assert!(guard.write_path(&root.join("dangling/nested")).is_err());
        let linked_config = root.join("linked-config");
        std::fs::create_dir(&config).expect("fake XDG directory");
        std::os::unix::fs::symlink(&config, &linked_config).expect("XDG alias");
        let guard = Guard::new(&home, &linked_config, &state, &tools).expect("resolved XDG guard");
        assert!(guard.write_path(&config.join("smith/new")).is_err());
        let external = root.join("external-tool-state");
        std::fs::create_dir(&external).expect("fake tool target");
        std::fs::create_dir(&state).expect("fake state root");
        std::os::unix::fs::symlink(&external, state.join("smith"))
            .expect("tool alias installed after guard construction");
        assert!(guard.write_path(&external.join("new")).is_err(), "tool links are resolved on each write check");
        std::fs::remove_dir_all(root).expect("remove only the synthetic guard root");
    }

    #[test]
    fn credential_conversion_refuses_nested_refresh_values_without_disclosing_them() {
        for field in ["refresh_token", "refresh-token", "refreshToken", "REFRESH_TOKEN", "refresh"] {
            for value in [
                serde_json::json!("synthetic-private-value"),
                serde_json::json!({"nested":"synthetic-private-value"}),
                serde_json::json!(true),
            ] {
                let record = serde_json::json!({"tokens":[{field:value}]});
                let error = check_credential_record(&record).expect_err("refresh copy");
                assert!(!error.contains("synthetic-private-value"));
            }
        }
        for value in [serde_json::Value::Null, serde_json::json!(""), serde_json::json!(UNUSABLE_REFRESH)] {
            check_credential_record(&serde_json::json!({"access_token":"synthetic-access", "refresh_token":value}))
                .expect("no issued refresh token");
        }
    }
}
