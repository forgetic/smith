//! Small helpers for bounded structured stderr events.

use std::collections::BTreeMap;

use serde_json::Value;

/// Default bound for scalar event fields that originate outside Smith.
pub(crate) const FIELD_PREVIEW_CHARS: usize = 200;
/// Default bound for free-form model/operator text in events.
pub(crate) const REASON_PREVIEW_CHARS: usize = 200;

/// A stable JSON event renderer for Smith stderr logs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StructuredEvent {
    fields: BTreeMap<String, Value>,
}

impl StructuredEvent {
    /// Starts a new event with the stable `event` field.
    pub(crate) fn new(event: impl AsRef<str>) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert(
            "event".to_string(),
            Value::String(preview(event.as_ref(), FIELD_PREVIEW_CHARS)),
        );
        Self { fields }
    }

    /// Adds a bounded string field.
    pub(crate) fn str(mut self, key: &str, value: impl AsRef<str>) -> Self {
        self.fields.insert(
            key.to_string(),
            Value::String(preview(value.as_ref(), FIELD_PREVIEW_CHARS)),
        );
        self
    }

    /// Adds a bounded string field when the value is present.
    pub(crate) fn opt_str(mut self, key: &str, value: Option<&str>) -> Self {
        if let Some(value) = value {
            self.fields.insert(
                key.to_string(),
                Value::String(preview(value, FIELD_PREVIEW_CHARS)),
            );
        }
        self
    }

    /// Adds a boolean field.
    pub(crate) fn bool(mut self, key: &str, value: bool) -> Self {
        self.fields.insert(key.to_string(), Value::Bool(value));
        self
    }

    /// Adds a `usize` field.
    pub(crate) fn usize(mut self, key: &str, value: usize) -> Self {
        self.fields
            .insert(key.to_string(), serde_json::json!(value));
        self
    }

    /// Adds a `u64` field.
    pub(crate) fn u64(mut self, key: &str, value: u64) -> Self {
        self.fields
            .insert(key.to_string(), serde_json::json!(value));
        self
    }

    /// Adds a bounded string-array field.
    pub(crate) fn strings<I, S>(mut self, key: &str, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let values = values
            .into_iter()
            .map(|value| Value::String(preview(value.as_ref(), FIELD_PREVIEW_CHARS)))
            .collect();
        self.fields.insert(key.to_string(), Value::Array(values));
        self
    }

    /// Renders the event as one compact JSON object.
    pub(crate) fn render(&self) -> String {
        serde_json::to_string(&self.fields).expect("structured event fields serialize")
    }
}

/// Returns a single-line preview bounded by `max_chars` Unicode scalar values.
pub(crate) fn preview(input: &str, max_chars: usize) -> String {
    let collapsed = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }

    let mut output = collapsed.chars().take(max_chars).collect::<String>();
    output.push('…');
    output
}

/// Extracts a scalar JSON value as a bounded event string.
pub(crate) fn scalar_preview(value: Option<&Value>) -> Option<String> {
    let raw = match value? {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => return None,
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(preview(trimmed, FIELD_PREVIEW_CHARS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_stable_json_with_sorted_keys() {
        let rendered = StructuredEvent::new("smith.test")
            .str("zeta", "last")
            .usize("count", 2)
            .strings("ids", ["one", "two"])
            .render();

        assert_eq!(
            rendered,
            r#"{"count":2,"event":"smith.test","ids":["one","two"],"zeta":"last"}"#
        );
    }

    #[test]
    fn preview_collapses_whitespace_and_truncates_on_char_boundary() {
        let text = "first\nsecond\t🙂 third";
        assert_eq!(preview(text, 14), "first second 🙂…");
    }

    #[test]
    fn scalar_preview_tolerates_missing_or_non_scalar_json() {
        let value = serde_json::json!({"nested": true});
        assert_eq!(
            scalar_preview(Some(&serde_json::json!(42))).as_deref(),
            Some("42")
        );
        assert_eq!(
            scalar_preview(Some(&serde_json::json!("  id-1  "))).as_deref(),
            Some("id-1")
        );
        assert_eq!(scalar_preview(Some(&value)), None);
        assert_eq!(scalar_preview(None), None);
    }
}
