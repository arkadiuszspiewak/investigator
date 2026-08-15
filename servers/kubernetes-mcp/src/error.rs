use rmcp::{ErrorData, service::ServerInitializeError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Kubernetes request failed: {0}")]
    Kubernetes(#[from] kube::Error),

    #[error("failed to serialize Kubernetes response: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error(
        "Kubernetes resource not found: {resource}{api_version} with {operation} access",
        api_version = api_version.as_deref().map(|value| format!(" ({value})")).unwrap_or_default()
    )]
    ResourceNotFound {
        resource: String,
        api_version: Option<String>,
        operation: String,
    },

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
        match error {
            error @ AppError::ResourceNotFound { .. } => {
                ErrorData::resource_not_found(error.to_string(), None)
            }
            error => ErrorData::internal_error(error.to_string(), None),
        }
    }
}
