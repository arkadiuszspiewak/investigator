use std::{env, time::Duration};

use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use serde_json::Value;

use crate::error::AppError;

#[derive(Clone)]
pub struct PrometheusClient {
    client: Client,
    base_url: String,
    bearer_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResponse {
    status: String,
    #[serde(default)]
    data: Value,
    #[serde(default)]
    error_type: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl PrometheusClient {
    pub fn from_env() -> Result<Self, AppError> {
        let base_url = env::var("PROMETHEUS_URL")
            .map_err(|_| AppError::Configuration("PROMETHEUS_URL is required".to_owned()))?;
        let base_url = base_url.trim_end_matches('/').to_owned();
        reqwest::Url::parse(&base_url).map_err(|error| {
            AppError::Configuration(format!("PROMETHEUS_URL is not a valid URL: {error}"))
        })?;

        let timeout = env::var("PROMETHEUS_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "30".to_owned())
            .parse::<u64>()
            .map_err(|error| {
                AppError::Configuration(format!(
                    "PROMETHEUS_TIMEOUT_SECONDS must be an integer: {error}"
                ))
            })?;
        if timeout == 0 {
            return Err(AppError::Configuration(
                "PROMETHEUS_TIMEOUT_SECONDS must be greater than zero".to_owned(),
            ));
        }

        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(timeout))
                .build()?,
            base_url,
            bearer_token: env::var("PROMETHEUS_BEARER_TOKEN").ok(),
        })
    }

    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, AppError> {
        let request = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .query(query);
        let response = self.authorize(request).send().await?.error_for_status()?;
        let response: ApiResponse = response.json().await?;
        if response.status == "success" {
            Ok(response.data)
        } else {
            let kind = response.error_type.unwrap_or_else(|| "unknown".to_owned());
            let message = response
                .error
                .unwrap_or_else(|| "Prometheus returned an unsuccessful response".to_owned());
            Err(AppError::Prometheus(format!("{kind}: {message}")))
        }
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.bearer_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_has_useful_message() {
        let response: ApiResponse = serde_json::from_value(serde_json::json!({
            "status": "error",
            "errorType": "bad_data",
            "error": "invalid expression"
        }))
        .unwrap();
        assert_eq!(response.status, "error");
        assert_eq!(response.error_type.as_deref(), Some("bad_data"));
        assert_eq!(response.error.as_deref(), Some("invalid expression"));
    }
}
