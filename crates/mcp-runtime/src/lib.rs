//! Shared Streamable HTTP transport for MCP server binaries.

use std::{env, net::SocketAddr};

use axum::Router;
use rmcp::{
    ServerHandler,
    transport::{
        StreamableHttpServerConfig, StreamableHttpService,
        streamable_http_server::session::local::LocalSessionManager,
    },
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("MCP_PATH must begin with '/'")]
    InvalidPath,
    #[error("MCP_ALLOWED_HOSTS must contain at least one host")]
    EmptyAllowedHosts,
    #[error("{name} must be true or false, got {value:?}")]
    InvalidBoolean { name: String, value: String },
    #[error("could not read {name}: {source}")]
    Environment {
        name: String,
        #[source]
        source: env::VarError,
    },
    #[error("invalid listen address: {0}")]
    Address(#[from] std::net::AddrParseError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn serve_http<S>(server: S) -> Result<(), TransportError>
where
    S: ServerHandler + Clone + Send + Sync + 'static,
{
    let address: SocketAddr = env::var("MCP_BIND_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse()?;
    let path = env::var("MCP_PATH").unwrap_or_else(|_| "/mcp".to_owned());
    if !path.starts_with('/') {
        return Err(TransportError::InvalidPath);
    }

    let stateless = env_flag("MCP_STATELESS", false)?;
    let mut config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(!stateless)
        .with_json_response(stateless);
    if let Ok(hosts) = env::var("MCP_ALLOWED_HOSTS") {
        let hosts: Vec<_> = hosts
            .split(',')
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .map(str::to_owned)
            .collect();
        if hosts.is_empty() {
            return Err(TransportError::EmptyAllowedHosts);
        }
        config = config.with_allowed_hosts(hosts);
    }

    let service: StreamableHttpService<S, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(server.clone()), Default::default(), config);
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, Router::new().nest_service(&path, service))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn env_flag(name: &str, default: bool) -> Result<bool, TransportError> {
    match env::var(name) {
        Ok(value) if matches!(value.as_str(), "1" | "true" | "TRUE") => Ok(true),
        Ok(value) if matches!(value.as_str(), "0" | "false" | "FALSE") => Ok(false),
        Ok(value) => Err(TransportError::InvalidBoolean {
            name: name.to_owned(),
            value,
        }),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(source) => Err(TransportError::Environment {
            name: name.to_owned(),
            source,
        }),
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
