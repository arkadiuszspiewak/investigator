use rmcp::{ErrorData, service::ServerInitializeError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("ALERTMANAGER_URL is required")]
    MissingUrl,
    #[error("ALERTMANAGER_URL is not a valid URL: {0}")]
    InvalidUrl(#[source] url::ParseError),
    #[error("ALERTMANAGER_TIMEOUT_SECONDS must be an integer: {0}")]
    InvalidTimeout(#[source] std::num::ParseIntError),
    #[error("ALERTMANAGER_TIMEOUT_SECONDS must be greater than zero")]
    ZeroTimeout,

    #[error("Alertmanager request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Transport(#[from] mcp_runtime::TransportError),

    #[error("failed to initialize MCP server: {0}")]
    ServerInitialize(#[source] Box<ServerInitializeError>),

    #[error("MCP server task failed: {0}")]
    ServerTask(#[from] tokio::task::JoinError),
}

impl From<ServerInitializeError> for AppError {
    fn from(error: ServerInitializeError) -> Self {
        Self::ServerInitialize(Box::new(error))
    }
}

impl From<AppError> for ErrorData {
    fn from(error: AppError) -> Self {
        ErrorData::internal_error(error.to_string(), None)
    }
}
