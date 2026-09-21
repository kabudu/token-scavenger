//! Apple Silicon MLX runtime detection and status.
//!
//! This module is strictly read-only: it detects whether the `mlx-lm`
//! Python runtime is installed and whether an `mlx_lm.server` instance is
//! reachable. It never installs, downloads, starts, or stops anything —
//! managing the MLX server process stays an explicit operator action
//! (see `documentation/mlx.md`).

use serde::Serialize;
use std::time::Duration;

/// Default base URL of `mlx_lm.server`.
pub const DEFAULT_MLX_BASE_URL: &str = "http://127.0.0.1:8080/v1";
/// Default port of `mlx_lm.server`.
pub const DEFAULT_MLX_PORT: u16 = 8080;
/// Curated MLX model seeded in the catalog.
pub const BONSAI_MLX_MODEL_ID: &str = "prism-ml/Ternary-Bonsai-27B-mlx-2bit";

const RUNTIME_PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SERVED_MODELS: usize = 100;

/// Point-in-time MLX runtime/server status.
#[derive(Debug, Clone, Serialize)]
pub struct MlxStatus {
    /// True on Apple Silicon macOS, the only platform MLX supports.
    pub supported_platform: bool,
    /// True when `mlx_lm` is importable by `python3`.
    pub runtime_installed: bool,
    /// Installed version, or a hint explaining why detection failed.
    pub runtime_detail: String,
    /// True when `GET {base_url}/models` succeeds.
    pub server_reachable: bool,
    /// Model ids reported by the server (empty when unreachable).
    pub served_models: Vec<String>,
    /// Base URL that was probed.
    pub base_url: String,
    /// Suggested command to serve the curated model (informational only).
    pub serve_command: String,
}

/// MLX runs on Apple Silicon macOS only.
pub fn is_supported_platform() -> bool {
    cfg!(target_arch = "aarch64") && cfg!(target_os = "macos")
}

/// Resolve the MLX server base URL: the `mlx` (or `mlx-lm`) provider
/// `base_url` override wins, otherwise the `mlx_lm.server` default.
pub fn mlx_base_url(config: &crate::config::schema::Config) -> String {
    config
        .providers
        .iter()
        .find(|provider| provider.id == "mlx" || provider.id == "mlx-lm")
        .and_then(|provider| provider.base_url.clone())
        .unwrap_or_else(|| DEFAULT_MLX_BASE_URL.to_string())
}

/// Extract the port from a base URL, falling back to the server default.
pub fn mlx_port(base_url: &str) -> u16 {
    url::Url::parse(base_url)
        .ok()
        .and_then(|url| url.port())
        .unwrap_or(DEFAULT_MLX_PORT)
}

/// Suggested (informational only) command to serve a model.
pub fn suggested_serve_command(model: &str, port: u16) -> String {
    format!("python3 -m mlx_lm server --model {model} --host 127.0.0.1 --port {port}")
}

/// Leniently parse an OpenAI-style `/models` payload into model ids.
/// Accepts `{"data": [{"id": ...}]}` as well as a bare array of ids or
/// objects; anything else yields an empty list.
pub fn parse_models_payload(body: &str) -> Vec<String> {
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let data = value.get("data").unwrap_or(&value);
    let items: Vec<&serde_json::Value> = match data {
        serde_json::Value::Array(items) => items.iter().collect(),
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| match item {
            serde_json::Value::String(id) => Some(id.clone()),
            serde_json::Value::Object(_) => item
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string),
            _ => None,
        })
        .take(MAX_SERVED_MODELS)
        .collect()
}

/// Probe whether `mlx_lm` is importable, returning the installed version
/// or a hint when it is not.
pub async fn probe_runtime() -> (bool, String) {
    if !is_supported_platform() {
        return (
            false,
            "MLX requires Apple Silicon (macOS arm64)".to_string(),
        );
    }
    let output = tokio::time::timeout(
        RUNTIME_PROBE_TIMEOUT,
        tokio::process::Command::new("python3")
            .args([
                "-c",
                "import mlx_lm; from importlib.metadata import version; print(version('mlx-lm'))",
            ])
            .output(),
    )
    .await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                (true, "mlx-lm installed (unknown version)".to_string())
            } else {
                (true, format!("mlx-lm {version}"))
            }
        }
        _ => (
            false,
            "mlx_lm not importable by python3; see documentation/mlx.md".to_string(),
        ),
    }
}

/// Probe `GET {base_url}/models` for a reachable server and its model ids.
pub async fn probe_server(client: &reqwest::Client, base_url: &str) -> (bool, Vec<String>) {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let response = tokio::time::timeout(SERVER_PROBE_TIMEOUT, client.get(&url).send()).await;
    match response {
        Ok(Ok(response)) if response.status().is_success() => match response.text().await {
            Ok(body) => (true, parse_models_payload(&body)),
            Err(_) => (false, Vec::new()),
        },
        _ => (false, Vec::new()),
    }
}

/// Full read-only detection pass.
pub async fn detect(client: &reqwest::Client, base_url: &str) -> MlxStatus {
    let (runtime_installed, runtime_detail) = probe_runtime().await;
    let (server_reachable, served_models) = probe_server(client, base_url).await;
    let port = mlx_port(base_url);
    let model = served_models
        .first()
        .cloned()
        .unwrap_or_else(|| BONSAI_MLX_MODEL_ID.to_string());
    MlxStatus {
        supported_platform: is_supported_platform(),
        runtime_installed,
        runtime_detail,
        server_reachable,
        served_models,
        base_url: base_url.to_string(),
        serve_command: suggested_serve_command(&model, port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_models_openai_shape() {
        let body = r#"{"object":"list","data":[{"id":"m1","object":"model"},{"id":"m2"}]}"#;
        assert_eq!(parse_models_payload(body), vec!["m1", "m2"]);
    }

    #[test]
    fn test_parse_models_bare_array() {
        assert_eq!(
            parse_models_payload(r#"["a", {"id": "b"}]"#),
            vec!["a", "b"]
        );
    }

    #[test]
    fn test_parse_models_malformed_is_empty() {
        assert!(parse_models_payload("not json").is_empty());
        assert!(parse_models_payload(r#"{"data": {"id": "x"}}"#).is_empty());
        assert!(parse_models_payload(r#"{"other": []}"#).is_empty());
    }

    #[test]
    fn test_mlx_port_extraction() {
        assert_eq!(mlx_port("http://127.0.0.1:8080/v1"), 8080);
        assert_eq!(mlx_port("http://localhost:9090/v1"), 9090);
        assert_eq!(mlx_port("not a url"), DEFAULT_MLX_PORT);
    }

    #[test]
    fn test_serve_command() {
        assert_eq!(
            suggested_serve_command("some-model", 8080),
            "python3 -m mlx_lm server --model some-model --host 127.0.0.1 --port 8080"
        );
    }

    #[test]
    fn test_mlx_base_url_defaults_and_override() {
        let config = crate::config::schema::Config::default();
        assert_eq!(mlx_base_url(&config), DEFAULT_MLX_BASE_URL);

        let mut config = crate::config::schema::Config::default();
        config.providers = vec![crate::config::schema::ProviderConfig {
            id: "mlx".into(),
            enabled: true,
            base_url: Some("http://127.0.0.1:9090/v1".into()),
            api_key: None,
            free_only: true,
            discover_models: true,
            embedding_support: Default::default(),
        }];
        assert_eq!(mlx_base_url(&config), "http://127.0.0.1:9090/v1");
    }
}
