use std::{env, time::Duration};

use reqwest::{Client, RequestBuilder};
use serde_json::Value;

use crate::error::AppError;

#[derive(Clone)]
pub struct AlertmanagerClient {
    client: Client,
    base_url: String,
    bearer_token: Option<String>,
}

impl AlertmanagerClient {
    pub fn from_env() -> Result<Self, AppError> {
        let base_url = env::var("ALERTMANAGER_URL")
            .map_err(|_| AppError::Configuration("ALERTMANAGER_URL is required".to_owned()))?;
        let base_url = base_url.trim_end_matches('/').to_owned();
        reqwest::Url::parse(&base_url).map_err(|error| {
            AppError::Configuration(format!("ALERTMANAGER_URL is not a valid URL: {error}"))
        })?;

        let timeout = env::var("ALERTMANAGER_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "30".to_owned())
            .parse::<u64>()
            .map_err(|error| {
                AppError::Configuration(format!(
                    "ALERTMANAGER_TIMEOUT_SECONDS must be an integer: {error}"
                ))
            })?;
        if timeout == 0 {
            return Err(AppError::Configuration(
                "ALERTMANAGER_TIMEOUT_SECONDS must be greater than zero".to_owned(),
            ));
        }

        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(timeout))
                .build()?,
            base_url,
            bearer_token: env::var("ALERTMANAGER_BEARER_TOKEN").ok(),
        })
    }

    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, AppError> {
        let request = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .query(query);
        Ok(self
            .authorize(request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.bearer_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}
