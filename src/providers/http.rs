use reqwest::Client;
use reqwest::header::HeaderMap;
use url::Url;

/// Shared HTTP utility for provider adapters.
pub struct ProviderHttp;

impl ProviderHttp {
    /// Build a GET request with the given headers and send it.
    pub async fn get(
        client: &Client,
        url: Url,
        headers: HeaderMap,
    ) -> Result<reqwest::Response, crate::providers::traits::ProviderError> {
        let req = client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(classify_http_error)?;
        Ok(req)
    }

    /// Build a POST request with JSON body and send it.
    pub async fn post_json(
        client: &Client,
        url: Url,
        headers: HeaderMap,
        body: &impl serde::Serialize,
    ) -> Result<reqwest::Response, crate::providers::traits::ProviderError> {
        let req = client
            .post(url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(classify_http_error)?;
        Ok(req)
    }

    /// Build a POST request with JSON body and return the streaming response.
    pub async fn post_json_stream(
        client: &Client,
        url: Url,
        headers: HeaderMap,
        body: &impl serde::Serialize,
    ) -> Result<reqwest::Response, crate::providers::traits::ProviderError> {
        let req = client
            .post(url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(classify_http_error)?;
        Ok(req)
    }
}

fn classify_http_error(e: reqwest::Error) -> crate::providers::traits::ProviderError {
    if e.is_timeout() || e.is_connect() {
        crate::providers::traits::ProviderError::Timeout
    } else if let Some(status) = e.status() {
        match status.as_u16() {
            429 => crate::providers::traits::ProviderError::RateLimited {
                retry_after: None,
                details: e.to_string(),
            },
            401 | 403 => crate::providers::traits::ProviderError::Auth(e.to_string()),
            s if s >= 500 => {
                crate::providers::traits::ProviderError::Other(format!("Upstream {}: {}", s, e))
            }
            _ => crate::providers::traits::ProviderError::Http(e.to_string()),
        }
    } else {
        crate::providers::traits::ProviderError::Http(e.to_string())
    }
}

/// Build default bearer auth headers.
pub fn bearer_auth(config: &crate::config::schema::ProviderConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    if let Some(ref key) = config.api_key {
        let value = format!("Bearer {}", key);
        headers.insert(reqwest::header::AUTHORIZATION, value.parse().unwrap());
    }
    headers
}

/// Build custom header auth (e.g., x-api-key).
pub fn custom_auth(config: &crate::config::schema::ProviderConfig, header_name: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    if let Some(ref key) = config.api_key {
        let name: reqwest::header::HeaderName = header_name.parse().unwrap();
        headers.insert(name, key.parse().unwrap());
    }
    headers
}
