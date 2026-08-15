mod client;
mod error;
mod server;
mod tools;

use client::AlertmanagerClient;
use error::AppError;
use rmcp::ServiceExt;
use server::AlertmanagerServer;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "alertmanager_mcp=info".into()),
        )
        .init();

    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(event = "application_failed", error = %error);
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), AppError> {
    let server = AlertmanagerServer::new(AlertmanagerClient::from_env()?);
    if std::env::args().any(|argument| argument == "--http") {
        tracing::info!(event = "mcp_server_starting", transport = "http");
        mcp_runtime::serve_http(server).await?;
    } else {
        tracing::info!(event = "mcp_server_starting", transport = "stdio");
        let service = server
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await?;
        service.waiting().await?;
    }
    tracing::info!(event = "mcp_server_stopped");
    Ok(())
}
