use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("Kubernetes client initialization failed: {0}")]
    Kubernetes(#[from] kube::Error),
    #[error("could not serialize the Investigation CRD: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid agent configuration: {0}")]
    Configuration(String),
}

#[derive(Debug, Error)]
pub(crate) enum ReconcileError {
    #[error("Kubernetes API request failed: {0}")]
    Kubernetes(#[from] kube::Error),
    #[error("Investigation is missing its namespace")]
    MissingNamespace,
    #[error("Investigation is missing its owner reference data")]
    MissingOwnerReference,
    #[error("could not serialize or deserialize pod configuration: {0}")]
    Serialization(#[from] serde_json::Error),
}
