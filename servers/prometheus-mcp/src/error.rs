use rmcp::{ErrorData, service::ServerInitializeError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid Prometheus configuration: {0}")]
    Configuration(String),

    #[error("Prometheus request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Prometheus API error: {0}")]
    Prometheus(String),

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
