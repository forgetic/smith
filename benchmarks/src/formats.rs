//! Strict TOML reads retain no state beyond their returned document.
//! Each refusal names its file and key path (benchmarks.md, section 5).

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::Refusal;

pub(crate) fn read_document<T: DeserializeOwned>(file: &Path) -> Result<T, Refusal> {
    let text = std::fs::read_to_string(file).map_err(|error| Refusal::new(file, "document", error.to_string()))?;
    parse_document(file, &text)
}

pub(crate) fn parse_document<T: DeserializeOwned>(file: &Path, text: &str) -> Result<T, Refusal> {
    let deserializer =
        toml::de::Deserializer::parse(text).map_err(|error| Refusal::new(file, "document", error.to_string()))?;
    serde_path_to_error::deserialize(deserializer).map_err(|error| {
        let mut key = error.path().to_string();
        let reason = error.inner().to_string();
        if let Some(rest) = reason.split("unknown field `").nth(1)
            && let Some(field) = rest.split('`').next()
        {
            if key == "." {
                key.clear();
            }
            if key != field && !key.ends_with(&format!(".{field}")) {
                if !key.is_empty() {
                    key.push('.');
                }
                key.push_str(field);
            }
        }
        Refusal::new(file, &key, reason)
    })
}
