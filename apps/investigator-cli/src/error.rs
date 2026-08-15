use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("HOME is not set; pass --config PATH")]
    MissingHome,
    #[error("could not read configuration {path}: {source}. Create it or pass --config PATH")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse configuration: {0}")]
    ParseConfig(#[from] serde_json::Error),
    #[error("config auth must contain exactly one of apiKeySecretRef or authJsonSecretRef")]
    InvalidAuth,
    #[error("Kubernetes request failed: {0}")]
    Kubernetes(#[from] kube::Error),
    #[error("terminal I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("investigation conversation state is missing")]
    MissingConversationState,
    #[error("completed investigation has no status")]
    MissingCompletedStatus,
}
