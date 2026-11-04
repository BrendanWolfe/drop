use std::sync::{LazyLock, RwLock};

use crate::SecretHeader;

/// Retained for the process lifetime, including replaced tokens and failed connection probes:
/// old in-flight requests can still reflect them after a settings change.
#[derive(Default)]
pub struct Redactor(RwLock<Vec<String>>);

static SECRETS: LazyLock<Redactor> = LazyLock::new(Redactor::default);

impl Redactor {
    pub fn register(&self, value: &str) {
        if value.is_empty() {
            return;
        }
        let json = serde_json::to_string(value).unwrap();
        let debug = format!("{value:?}");
        let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
        let uri_encoded: String = value
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    char::from(byte).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        let html = value
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let mut patterns = self.0.write().unwrap();
        for pattern in [
            value.to_owned(),
            json[1..json.len() - 1].to_owned(),
            debug[1..debug.len() - 1].to_owned(),
            encoded,
            uri_encoded,
            html,
        ] {
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
            }
        }
        patterns.sort_by_key(|s| std::cmp::Reverse(s.len()));
    }

    pub fn redact(&self, message: &str) -> String {
        let patterns = self.0.read().unwrap();
        if patterns.is_empty() {
            return message.to_owned();
        }
        let mut result = String::with_capacity(message.len());
        let mut rest = message;
        while !rest.is_empty() {
            if let Some(pattern) = patterns
                .iter()
                .find(|pattern| rest.starts_with(pattern.as_str()))
            {
                result.push_str("[REDACTED]");
                rest = &rest[pattern.len()..];
            } else {
                let ch = rest.chars().next().unwrap();
                result.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
        result
    }
}

pub(crate) fn register(headers: &[SecretHeader]) {
    for header in headers {
        SECRETS.register(&header.value);
    }
}

pub fn redact(message: &str) -> String {
    SECRETS.redact(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_raw_formatted_encoded_and_overlapping_secrets_without_recursing() {
        let redactor = Redactor::default();
        redactor.register("abc");
        redactor.register("abcdef");
        redactor.register("a\"b\\c<&");
        redactor.register("token~ with spaces");
        assert_eq!(redactor.redact("token%7E+with+spaces"), "[REDACTED]");
        assert_eq!(redactor.redact("token~%20with%20spaces"), "[REDACTED]");
        assert_eq!(redactor.redact("abcdef abc"), "[REDACTED] [REDACTED]");
        for representation in [
            "a\"b\\c<&",
            "a\\\"b\\\\c<&",
            "a%22b%5Cc%3C%26",
            "a&quot;b\\c&lt;&amp;",
        ] {
            assert_eq!(redactor.redact(representation), "[REDACTED]");
        }
        assert_eq!(
            redactor.redact("ordinary Unicode: 🦀"),
            "ordinary Unicode: 🦀"
        );
    }
}
