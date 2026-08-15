use rmcp::{ErrorData, service::ServerInitializeError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid Alertmanager configuration: {0}")]
    Configuration(String),

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
