use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("invalid listen address: {0}")]
    Address(#[from] std::net::AddrParseError),
    #[error("Kubernetes request failed: {0}")]
    Kubernetes(#[from] kube::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("could not serialize or deserialize alert data: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("investigation {name} failed")]
    InvestigationFailed { name: String },
    #[error("Slack chat.postMessage failed: {code}")]
    SlackApi { code: String },
    #[error("Slack chat.postMessage response did not contain ts")]
    MissingSlackTimestamp,
    #[error("relay mode has no configured Slack App target")]
    MissingRelayTarget,
    #[error("Slack App delivery did not return a message reference")]
    MissingSlackMessage,
    #[error(transparent)]
    Configuration(#[from] ConfigurationError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum ConfigurationError {
    #[error(
        "RELAY_MODE requires SLACK_BOT_TOKEN and SLACK_CHANNEL; Slack webhooks cannot create reliable threads"
    )]
    RelayRequiresSlackApp,
    #[error("configure either SLACK_WEBHOOK_URL or the Slack App settings, not both")]
    ConflictingSlackSettings,
    #[error("Slack App delivery requires both SLACK_BOT_TOKEN and SLACK_CHANNEL")]
    IncompleteSlackAppSettings,
    #[error("exactly one investigation credential env var must contain NAME:KEY")]
    InvalidAuthSelection,
    #[error("credential must use NAME:KEY")]
    InvalidSecretReference,
}
