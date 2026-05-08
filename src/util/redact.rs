/// Redact sensitive strings (API keys, tokens) from display/log output.
/// Replaces all but the last 4 characters with asterisks.
pub fn redact_secret(secret: &str) -> String {
    if secret.len() <= 8 {
        return "********".to_string();
    }
    let visible = &secret[secret.len() - 4..];
    format!("****{}", visible)
}

/// Return true when a submitted value looks like a display-only redaction mask.
///
/// Admin UI saves should preserve the existing secret for these values instead
/// of writing the mask back to durable config.
pub fn is_redacted_secret(value: &str) -> bool {
    if value == "********" {
        return true;
    }

    value.len() >= 8
        && value.starts_with("****")
        && value.chars().filter(|ch| *ch == '*').count() >= 4
        && value.chars().any(|ch| ch != '*')
}

/// Redact sensitive values from a JSON value tree in-place.
pub fn redact_json_value(mut value: serde_json::Value) -> serde_json::Value {
    match &mut value {
        serde_json::Value::Object(map) => {
            let sensitive_keys = [
                "api_key", "api-key", "apikey", "secret", "password", "token", "key",
            ];
            for (k, v) in map.iter_mut() {
                if sensitive_keys.contains(&k.as_str()) && v.is_string() {
                    if let Some(s) = v.as_str() {
                        *v = serde_json::Value::String(redact_secret(s));
                    }
                } else if v.is_object() || v.is_array() {
                    *v = redact_json_value(v.take());
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                *item = redact_json_value(item.take());
            }
        }
        _ => {}
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_secret_short() {
        assert_eq!(redact_secret("abc"), "********");
    }

    #[test]
    fn test_redact_secret_long() {
        let result = redact_secret("sk-abc123def456");
        assert_eq!(result, "****f456");
    }

    #[test]
    fn test_is_redacted_secret() {
        assert!(is_redacted_secret("********"));
        assert!(is_redacted_secret("****f456"));
        assert!(!is_redacted_secret("sk-real-secret"));
        assert!(!is_redacted_secret(""));
    }

    #[test]
    fn test_redact_json_api_key() {
        let json = serde_json::json!({"api_key": "sk-secret-key-1234"});
        let redacted = redact_json_value(json);
        assert_eq!(redacted["api_key"], "****1234");
    }

    #[test]
    fn test_redact_json_nested() {
        let json = serde_json::json!({
            "provider": {
                "api_key": "super-secret-key-9999",
                "base_url": "https://api.example.com"
            }
        });
        let redacted = redact_json_value(json);
        assert_eq!(redacted["provider"]["api_key"], "****9999");
        assert_eq!(redacted["provider"]["base_url"], "https://api.example.com");
    }

    #[test]
    fn test_redact_json_array() {
        let json = serde_json::json!([{"token": "abc123xyz"}]);
        let redacted = redact_json_value(json);
        assert_eq!(redacted[0]["token"], "****3xyz");
    }
}
